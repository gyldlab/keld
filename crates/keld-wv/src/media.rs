//! Webview media-capture permission policy (KEL-59).
//!
//! One policy, two install mechanisms:
//!
//! - **macOS (wry interim)**: wry 0.56 auto-grants camera and microphone when
//!   `WebViewBuilder::with_permission_handler` is omitted
//!   (`WKPermissionDecision::Grant` in `wry_web_view_ui_delegate.rs`). Keld
//!   installs `with_guarded_media_permissions` (cfg-gated to macOS, so no
//!   intra-doc link).
//! - **Windows (direct COM, KEL-65)**: without a handler `WebView2` falls back
//!   to its own user prompt — default-ask, not default-deny. The backend
//!   registers `add_PermissionRequested` before the first navigation and maps
//!   kinds through `webview2_media_kind` (cfg-gated to Windows, likewise).
//!
//! Both funnel into [`media_permission_allowed`], which default-denies via
//! [`keld_guard::evaluate`] on `web.camera` / `web.microphone` — the mapping is
//! the security decision, so it lives here rather than once per platform.
//!
//! Neither platform callback passes an origin or a webview principal. v0
//! therefore evaluates as [`Principal::AppProcess`] with requested resource
//! [`WEB_MEDIA_ORIGIN`] (`*`). Origin-scoped and window-principal grants are
//! not enforceable until the callbacks grow those arguments.

use keld_guard::{Decision, PermissionsManifest, Principal, evaluate};

/// Capability id for camera capture (`getUserMedia` video).
pub const WEB_CAMERA: &str = "web.camera";

/// Capability id for microphone capture (`getUserMedia` audio).
pub const WEB_MICROPHONE: &str = "web.microphone";

/// Requested-resource sentinel for v0 media checks.
///
/// Exact-match only — this is not a glob. A grant of `["*"]` allows any origin
/// because the live wry callback cannot name the requesting origin.
pub const WEB_MEDIA_ORIGIN: &str = "*";

/// Media kinds the platform backends map their permission callbacks onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPermission {
    /// Camera capture.
    Camera,
    /// Microphone capture.
    Microphone,
    /// Any other platform permission (geolocation, display-capture, unknown, …).
    ///
    /// Fail closed: there is no v0 capability for these.
    Other,
}

impl MediaPermission {
    /// Capability id this kind evaluates, if Keld has one.
    #[must_use]
    pub const fn capability(self) -> Option<&'static str> {
        match self {
            Self::Camera => Some(WEB_CAMERA),
            Self::Microphone => Some(WEB_MICROPHONE),
            Self::Other => None,
        }
    }
}

/// Whether `kind` is allowed by `manifest`.
///
/// Unknown kinds fail closed without consulting the manifest. Camera and
/// microphone use [`evaluate`] as [`Principal::AppProcess`] against
/// [`WEB_MEDIA_ORIGIN`] — no platform callback can name the requesting webview
/// yet.
#[must_use]
pub fn media_permission_allowed(manifest: &PermissionsManifest, kind: MediaPermission) -> bool {
    let Some(capability) = kind.capability() else {
        return false;
    };
    evaluate(
        manifest,
        Principal::AppProcess,
        capability,
        WEB_MEDIA_ORIGIN,
    ) == Decision::Allow
}

/// Maps `WebView2` permission kinds onto Keld capabilities. Unknown kinds fail
/// closed.
///
/// Plain data mapping — no COM object is touched, so this stays in the shared
/// policy module while the `unsafe` handler installation lives in the backend
/// (root `AGENTS.md`: `unsafe` only in platform backends).
#[cfg(target_os = "windows")]
#[must_use]
pub fn webview2_media_kind(
    kind: webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PERMISSION_KIND,
) -> MediaPermission {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
    };
    match kind {
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => MediaPermission::Camera,
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => MediaPermission::Microphone,
        _ => MediaPermission::Other,
    }
}

/// Maps wry's permission kinds onto Keld capabilities. Unknown kinds fail closed.
///
/// macOS-only since KEL-65: Windows maps `WebView2` kinds directly via
/// `webview2_media_kind` (cfg-gated, so no intra-doc link) and no longer
/// links wry.
#[cfg(target_os = "macos")]
#[must_use]
pub fn wry_media_kind(kind: wry::PermissionKind) -> MediaPermission {
    match kind {
        wry::PermissionKind::Camera => MediaPermission::Camera,
        wry::PermissionKind::Microphone => MediaPermission::Microphone,
        _ => MediaPermission::Other,
    }
}

/// Guard decision for one wry permission request. Deny is fail-closed.
#[cfg(target_os = "macos")]
#[must_use]
pub fn media_permission_response(
    manifest: &PermissionsManifest,
    kind: wry::PermissionKind,
) -> wry::PermissionResponse {
    if media_permission_allowed(manifest, wry_media_kind(kind)) {
        wry::PermissionResponse::Allow
    } else {
        wry::PermissionResponse::Deny
    }
}

/// Installs a default-deny media-capture handler backed by `keld-guard`.
///
/// Omitting wry's handler on macOS means wry 0.56.1
/// (`wry_web_view_ui_delegate.rs`) returns `Grant` unconditionally — camera
/// and mic with no prompt at all. The manifest is the authority
/// (`docs/architecture/03-security.md` §1), so `Deny` here is deliberate:
/// default-deny, not default-ask.
///
/// The `backends_install_guarded_handler` test asserts every live backend
/// wires its platform mechanism; dropping the call silently restores the
/// platform default. Windows registers `add_PermissionRequested` directly in
/// `webview2/mod.rs` (`install_guarded_media_permissions`) since KEL-65.
#[cfg(target_os = "macos")]
#[must_use]
pub fn with_guarded_media_permissions(
    builder: wry::WebViewBuilder<'_>,
    manifest: PermissionsManifest,
) -> wry::WebViewBuilder<'_> {
    builder.with_permission_handler(move |kind| media_permission_response(&manifest, kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keld_guard::{DenyReason, parse_manifest};

    #[test]
    fn empty_manifest_denies_camera_and_microphone() {
        let manifest = parse_manifest("{}").expect("empty object");
        assert!(
            !media_permission_allowed(&manifest, MediaPermission::Camera),
            "empty manifest must default-deny camera"
        );
        assert!(
            !media_permission_allowed(&manifest, MediaPermission::Microphone),
            "empty manifest must default-deny microphone"
        );
        match evaluate(
            &manifest,
            Principal::AppProcess,
            WEB_CAMERA,
            WEB_MEDIA_ORIGIN,
        ) {
            Decision::Deny(DenyReason::NotGranted {
                capability,
                json_pointer,
                requested,
            }) => {
                assert_eq!(capability, WEB_CAMERA);
                assert_eq!(json_pointer, "/app/web/camera");
                assert_eq!(requested, WEB_MEDIA_ORIGIN);
            }
            other => panic!("expected NotGranted for web.camera, got {other:?}"),
        }
    }

    #[test]
    fn camera_grant_does_not_allow_microphone_or_other() {
        let manifest = parse_manifest(r#"{"app":{"web":{"camera":["*"]}}}"#).expect("camera grant");
        assert!(
            media_permission_allowed(&manifest, MediaPermission::Camera),
            "in-scope web.camera grant must allow camera — inverted deny would fail this"
        );
        assert!(
            !media_permission_allowed(&manifest, MediaPermission::Microphone),
            "camera grant must not imply microphone"
        );
        assert!(
            !media_permission_allowed(&manifest, MediaPermission::Other),
            "unknown wry kinds must fail closed even when camera is granted"
        );
    }

    #[test]
    fn origin_looking_grant_does_not_match_sentinel() {
        let manifest = parse_manifest(r#"{"app":{"web":{"camera":["https://evil.example"]}}}"#)
            .expect("origin grant");
        assert!(
            !media_permission_allowed(&manifest, MediaPermission::Camera),
            "v0 requested resource is `*`; an https origin grant must not allow"
        );
    }

    #[test]
    fn capability_ids_are_stable() {
        assert_eq!(MediaPermission::Camera.capability(), Some("web.camera"));
        assert_eq!(
            MediaPermission::Microphone.capability(),
            Some("web.microphone")
        );
        assert_eq!(MediaPermission::Other.capability(), None);
        assert_eq!(WEB_MEDIA_ORIGIN, "*");
    }

    /// Every live backend must wire its platform's guarded handler. Dropping
    /// the call is silent: macOS falls back to unconditional `Grant`, Windows
    /// to a user prompt — neither is default-deny, and neither fails any other
    /// test.
    ///
    /// Source-text assertions because the alternative is driving a live
    /// `getUserMedia` request through a real webview, which needs a GUI session
    /// and a camera. This at least fails loudly if the wiring is deleted.
    #[test]
    fn backends_install_guarded_handler() {
        // macOS (wry interim): the builder must pass through the shared helper.
        let wkwebview = include_str!("wkwebview/mod.rs");
        assert!(
            wkwebview.contains("with_guarded_media_permissions"),
            "KEL-59: wkwebview omits the guarded handler, restoring wry's auto-grant"
        );
        // The macOS helper must still reach wry and the guard, or the backend
        // above would be calling a no-op.
        let helper = include_str!("media.rs");
        assert!(
            helper.contains("with_permission_handler"),
            "KEL-59: the helper must call wry's permission handler, not a no-op wrapper"
        );
        assert!(
            helper.contains("media_permission_response"),
            "KEL-59: the handler must call media_permission_response, not a constant Allow"
        );

        // Windows (direct COM, KEL-65): the backend must register the guarded
        // `PermissionRequested` handler and route it through the shared policy.
        let webview2 = include_str!("webview2/mod.rs");
        assert!(
            webview2.contains("install_guarded_media_permissions"),
            "KEL-59: webview2 omits the guarded handler, restoring WebView2's default prompt"
        );
        assert!(
            webview2.contains("add_PermissionRequested"),
            "KEL-59: webview2 guard must register the COM PermissionRequested handler"
        );
        assert!(
            webview2.contains("media_permission_allowed"),
            "KEL-59: webview2 guard must consult the shared policy, not a constant"
        );
        assert!(
            webview2.contains("navigate_initial(&view.webview, &guard"),
            "KEL-65: the first navigation must present the GuardInstalled proof"
        );
    }
}

/// `WebView2`-facing tests for the Windows kind mapping. Pure data — the COM
/// permission-kind values are plain constants, so the mapping that decides
/// deny-vs-allow is fully testable without a GUI session.
#[cfg(all(test, target_os = "windows"))]
mod webview2_tests {
    use super::{MediaPermission, media_permission_allowed, webview2_media_kind};
    use keld_guard::parse_manifest;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION,
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION,
    };

    #[test]
    fn webview2_kinds_map_to_keld_media_permissions() {
        assert_eq!(
            webview2_media_kind(COREWEBVIEW2_PERMISSION_KIND_CAMERA),
            MediaPermission::Camera
        );
        assert_eq!(
            webview2_media_kind(COREWEBVIEW2_PERMISSION_KIND_MICROPHONE),
            MediaPermission::Microphone
        );
        assert_eq!(
            webview2_media_kind(COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION),
            MediaPermission::Other
        );
        assert_eq!(
            webview2_media_kind(COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION),
            MediaPermission::Other
        );
    }

    /// The end-to-end deny decision the COM handler applies: empty manifest
    /// denies every kind; a camera grant allows exactly camera.
    #[test]
    fn empty_manifest_denies_every_webview2_kind() {
        let empty = parse_manifest("{}").expect("empty");
        for kind in [
            COREWEBVIEW2_PERMISSION_KIND_CAMERA,
            COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
            COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION,
            COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION,
        ] {
            assert!(
                !media_permission_allowed(&empty, webview2_media_kind(kind)),
                "KEL-59: empty manifest must deny WebView2 kind {kind:?}"
            );
        }
    }

    #[test]
    fn camera_grant_allows_only_camera_kind() {
        let granted = parse_manifest(r#"{"app":{"web":{"camera":["*"]}}}"#).expect("grant");
        assert!(media_permission_allowed(
            &granted,
            webview2_media_kind(COREWEBVIEW2_PERMISSION_KIND_CAMERA)
        ));
        assert!(!media_permission_allowed(
            &granted,
            webview2_media_kind(COREWEBVIEW2_PERMISSION_KIND_MICROPHONE)
        ));
    }
}

/// wry-facing tests for the shared helpers. macOS-only since KEL-65: Windows
/// no longer links wry.
#[cfg(all(test, target_os = "macos"))]
mod wry_tests {
    use super::{
        MediaPermission, media_permission_response, with_guarded_media_permissions, wry_media_kind,
    };
    use keld_guard::parse_manifest;

    #[test]
    fn wry_kinds_map_to_keld_media_permissions() {
        assert_eq!(
            wry_media_kind(wry::PermissionKind::Camera),
            MediaPermission::Camera
        );
        assert_eq!(
            wry_media_kind(wry::PermissionKind::Microphone),
            MediaPermission::Microphone
        );
        assert_eq!(
            wry_media_kind(wry::PermissionKind::DisplayCapture),
            MediaPermission::Other
        );
        assert_eq!(
            wry_media_kind(wry::PermissionKind::Other),
            MediaPermission::Other
        );
    }

    #[test]
    fn empty_manifest_returns_wry_deny_not_allow_or_default() {
        let empty = parse_manifest("{}").expect("empty");
        assert_eq!(
            media_permission_response(&empty, wry::PermissionKind::Camera),
            wry::PermissionResponse::Deny
        );
        assert_eq!(
            media_permission_response(&empty, wry::PermissionKind::Microphone),
            wry::PermissionResponse::Deny
        );
        assert_eq!(
            media_permission_response(&empty, wry::PermissionKind::DisplayCapture),
            wry::PermissionResponse::Deny
        );
        assert_ne!(
            media_permission_response(&empty, wry::PermissionKind::Camera),
            wry::PermissionResponse::Allow,
            "KEL-59: Allow here is wry's unfixed auto-grant"
        );
        assert_ne!(
            media_permission_response(&empty, wry::PermissionKind::Camera),
            wry::PermissionResponse::Default,
            "Default continues the platform behaviour — macOS auto-grants, Windows prompts. \n             v0 must Deny on both."
        );
    }

    #[test]
    fn camera_grant_allows_only_camera() {
        let granted = parse_manifest(r#"{"app":{"web":{"camera":["*"]}}}"#).expect("grant");
        assert_eq!(
            media_permission_response(&granted, wry::PermissionKind::Camera),
            wry::PermissionResponse::Allow
        );
        assert_eq!(
            media_permission_response(&granted, wry::PermissionKind::Microphone),
            wry::PermissionResponse::Deny
        );
        let _ = with_guarded_media_permissions(wry::WebViewBuilder::new(), granted);
    }
}
