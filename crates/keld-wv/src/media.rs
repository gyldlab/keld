//! Webview media-capture permission policy (KEL-59, KEL-73).
//!
//! One policy, two install mechanisms:
//!
//! - **macOS + Linux (wry interim)**: wry's `with_permission_handler` is the
//!   same builder call on both — omitting it means auto-grant on macOS
//!   (`WKPermissionDecision::Grant` in
//!   [`wry_web_view_ui_delegate.rs`](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/wkwebview/class/wry_web_view_ui_delegate.rs))
//!   or `WebKitGTK`'s own prompt on Linux
//!   ([`connect_permission_request`](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/webkitgtk/mod.rs#L585),
//!   KEL-28) — different platform defaults, same wrong-for-Keld direction.
//!   Keld installs `with_guarded_media_permissions` on both (cfg-gated, so no
//!   intra-doc link). Vendored locally: `competitors/wry` @ this same commit.
//! - **Windows (direct COM, KEL-65)**: without a handler `WebView2` falls back
//!   to its own user prompt — default-ask, not default-deny. The backend
//!   registers `add_PermissionRequested` before the first navigation and maps
//!   kinds through `webview2_media_kind` (cfg-gated to Windows, likewise).
//!
//! Both funnel into [`media_permission_allowed`], which default-denies via
//! [`keld_guard::evaluate`] on `web.camera` / `web.microphone` — the mapping is
//! the security decision, so it lives here rather than once per platform.
//!
//! Platform callbacks still do not pass an origin. The host *does* mint a
//! [`WebviewId`] at create time, so capture evaluates as that
//! [`Principal::Webview`]. Missing identity and [`Principal::AppProcess`] fail
//! closed ([`DenyReason::MediaPrincipalRequired`], `KELD-GUARD007`) — they
//! must not inherit `/app` media grants. A minted webview principal is still
//! `KELD-GUARD006` until window-level grants exist. Deny has no side effect
//! (no capture start, no principal mint, no manifest write). Requested
//! resource remains [`WEB_MEDIA_ORIGIN`] (`*`).

use keld_guard::{Decision, DenyReason, PermissionsManifest, Principal, evaluate};

use crate::WebviewId;

/// Capability id for camera capture (`getUserMedia` video).
pub const WEB_CAMERA: &str = "web.camera";

/// Capability id for microphone capture (`getUserMedia` audio).
pub const WEB_MICROPHONE: &str = "web.microphone";

/// Requested-resource sentinel for v0 media checks.
///
/// Exact-match only — this is not a glob. Origin-scoped grants are not
/// enforceable until a platform callback names the requesting origin.
pub const WEB_MEDIA_ORIGIN: &str = "*";

/// v0 generation for media principals. Rotation on navigation is not in this
/// slice (spec 03).
const WEBVIEW_MEDIA_GENERATION: u32 = 0;

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

/// Host-minted webview principal used for media checks (KEL-73).
///
/// Generation stays `0` until navigation rotation lands (spec 03).
#[must_use]
pub fn webview_media_principal(id: WebviewId) -> Principal {
    Principal::Webview {
        id: id.0,
        generation: WEBVIEW_MEDIA_GENERATION,
    }
}

/// Guard decision for `capability` from `principal`.
///
/// Only a minted [`Principal::Webview`] proceeds to [`evaluate`]. Anything
/// else — including omitted identity and [`Principal::AppProcess`] — is
/// [`DenyReason::MediaPrincipalRequired`] (`KELD-GUARD007`). Deny allocates
/// only the reason; it does not start capture or mutate the manifest.
#[must_use]
pub fn media_permission_decision(
    manifest: &PermissionsManifest,
    principal: Option<Principal>,
    capability: &str,
) -> Decision {
    match principal {
        Some(webview @ Principal::Webview { .. }) => {
            evaluate(manifest, webview, capability, WEB_MEDIA_ORIGIN)
        }
        presented => Decision::Deny(DenyReason::MediaPrincipalRequired {
            capability: capability.to_owned(),
            presented,
        }),
    }
}

/// Whether `kind` is allowed by `manifest` for `principal`.
///
/// Unknown kinds fail closed without consulting the manifest. Camera and
/// microphone require a minted webview principal; see
/// [`media_permission_decision`].
#[must_use]
pub fn media_permission_allowed(
    manifest: &PermissionsManifest,
    principal: Option<Principal>,
    kind: MediaPermission,
) -> bool {
    let Some(capability) = kind.capability() else {
        return false;
    };
    media_permission_decision(manifest, principal, capability) == Decision::Allow
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
/// macOS and Linux only since KEL-65: Windows maps `WebView2` kinds directly
/// via `webview2_media_kind` (cfg-gated, so no intra-doc link) and no longer
/// links wry.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[must_use]
pub fn wry_media_kind(kind: wry::PermissionKind) -> MediaPermission {
    match kind {
        wry::PermissionKind::Camera => MediaPermission::Camera,
        wry::PermissionKind::Microphone => MediaPermission::Microphone,
        _ => MediaPermission::Other,
    }
}

/// Guard decision for one wry permission request. Deny is fail-closed.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[must_use]
pub fn media_permission_response(
    manifest: &PermissionsManifest,
    principal: Principal,
    kind: wry::PermissionKind,
) -> wry::PermissionResponse {
    if media_permission_allowed(manifest, Some(principal), wry_media_kind(kind)) {
        wry::PermissionResponse::Allow
    } else {
        wry::PermissionResponse::Deny
    }
}

/// Installs a default-deny media-capture handler backed by `keld-guard`.
///
/// Omitting wry's handler means wry 0.56.1 auto-grants on macOS
/// ([`wry_web_view_ui_delegate.rs`](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/wkwebview/class/wry_web_view_ui_delegate.rs)
/// returns `Grant` unconditionally) or shows `WebKitGTK`'s own prompt on Linux
/// ([`webkitgtk/mod.rs`](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/webkitgtk/mod.rs#L642):
/// an unhandled request "let[s] `WebKitGTK` show default prompt"). The
/// manifest is the authority (`docs/architecture/03-security.md` §1), so
/// `Deny` here is deliberate on both: default-deny, not default-ask.
///
/// `principal` MUST be the webview this builder will become. Presenting
/// [`Principal::AppProcess`] is `KELD-GUARD007`, not an allow.
///
/// The `backends_install_guarded_handler` test asserts every live backend
/// wires its platform mechanism; dropping the call silently restores the
/// platform default. Windows registers `add_PermissionRequested` directly in
/// `webview2/mod.rs` (`install_guarded_media_permissions`) since KEL-65.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[must_use]
pub fn with_guarded_media_permissions(
    builder: wry::WebViewBuilder<'_>,
    manifest: PermissionsManifest,
    principal: Principal,
) -> wry::WebViewBuilder<'_> {
    builder
        .with_permission_handler(move |kind| media_permission_response(&manifest, principal, kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keld_guard::{DenyReason, parse_manifest};

    fn other_webview() -> Principal {
        Principal::Webview {
            id: 99,
            generation: 0,
        }
    }

    fn camera_grant() -> PermissionsManifest {
        parse_manifest(r#"{"app":{"web":{"camera":["*"]}}}"#).expect("camera grant")
    }

    #[test]
    fn webview_media_principal_uses_host_id() {
        assert_eq!(
            webview_media_principal(WebviewId(7)),
            Principal::Webview {
                id: 7,
                generation: 0
            }
        );
    }

    #[test]
    fn empty_manifest_denies_camera_and_microphone() {
        let manifest = parse_manifest("{}").expect("empty object");
        let webview = webview_media_principal(WebviewId(1));
        assert!(
            !media_permission_allowed(&manifest, Some(webview), MediaPermission::Camera),
            "empty manifest must default-deny camera"
        );
        assert!(
            !media_permission_allowed(&manifest, Some(webview), MediaPermission::Microphone),
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
    fn remote_webview_does_not_inherit_app_process_media_grant() {
        let manifest = camera_grant();
        assert_eq!(
            evaluate(
                &manifest,
                Principal::AppProcess,
                WEB_CAMERA,
                WEB_MEDIA_ORIGIN
            ),
            Decision::Allow,
            "control: /app web.camera still allows AppProcess — the media path must not use that"
        );
        let other = other_webview();
        match media_permission_decision(&manifest, Some(other), WEB_CAMERA) {
            Decision::Deny(reason) => {
                assert_eq!(
                    reason.code(),
                    "KELD-GUARD006",
                    "minted webview must hit evaluate's non-AppProcess deny, got {}",
                    reason.code()
                );
                assert!(
                    !media_permission_allowed(&manifest, Some(other), MediaPermission::Camera),
                    "other webview must not start capture on an AppProcess camera grant"
                );
            }
            Decision::Allow => {
                panic!("other webview must not inherit AppProcess camera grant, got Allow")
            }
        }
        assert!(
            !media_permission_allowed(&manifest, Some(other), MediaPermission::Microphone),
            "camera grant must not imply microphone"
        );
        assert!(
            !media_permission_allowed(&manifest, Some(other), MediaPermission::Other),
            "unknown kinds must fail closed even when camera is granted to app"
        );
    }

    #[test]
    fn missing_or_app_process_media_principal_is_guard007() {
        let manifest = camera_grant();
        for presented in [None, Some(Principal::AppProcess)] {
            match media_permission_decision(&manifest, presented, WEB_CAMERA) {
                Decision::Deny(reason) => {
                    assert_eq!(reason.code(), "KELD-GUARD007", "{presented:?}: {reason}");
                    assert_eq!(reason.kind(), "media_principal_required");
                    assert!(
                        !reason.fix().contains("/app/web"),
                        "must not recommend applying app media grants: {}",
                        reason.fix()
                    );
                    assert!(
                        !media_permission_allowed(&manifest, presented, MediaPermission::Camera),
                        "{presented:?} must not start capture"
                    );
                }
                Decision::Allow => {
                    panic!("expected KELD-GUARD007, got Allow for {presented:?}")
                }
            }
        }
    }

    #[test]
    fn origin_looking_grant_does_not_match_sentinel() {
        let manifest = parse_manifest(r#"{"app":{"web":{"camera":["https://evil.example"]}}}"#)
            .expect("origin grant");
        assert!(
            !media_permission_allowed(
                &manifest,
                Some(webview_media_principal(WebviewId(1))),
                MediaPermission::Camera
            ),
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
        assert!(
            wkwebview.contains("webview_media_principal"),
            "KEL-73: wkwebview must mint a webview principal, not fall back to AppProcess"
        );
        // The wry helper must still reach wry and the guard, or the backends
        // above and below would be calling a no-op.
        let helper = include_str!("media.rs");
        assert!(
            helper.contains("with_permission_handler"),
            "KEL-59: the helper must call wry's permission handler, not a no-op wrapper"
        );
        assert!(
            helper.contains("media_permission_response"),
            "KEL-59: the handler must call media_permission_response, not a constant Allow"
        );

        // Linux (wry interim, KEL-28): same wry mechanism as macOS.
        let webkitgtk = include_str!("webkitgtk/mod.rs");
        assert!(
            webkitgtk.contains("with_guarded_media_permissions"),
            "KEL-28/KEL-59: webkitgtk omits the guarded handler, restoring `WebKitGTK`'s default prompt"
        );
        assert!(
            webkitgtk.contains("webview_media_principal"),
            "KEL-73: webkitgtk must mint a webview principal, not fall back to AppProcess"
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
            webview2.contains("webview_media_principal"),
            "KEL-73: webview2 must mint a webview principal, not fall back to AppProcess"
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
    use super::{
        MediaPermission, WebviewId, media_permission_allowed, webview_media_principal,
        webview2_media_kind,
    };
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
    /// denies every kind; an AppProcess camera grant must not allow a webview.
    #[test]
    fn empty_manifest_denies_every_webview2_kind() {
        let empty = parse_manifest("{}").expect("empty");
        let principal = Some(webview_media_principal(WebviewId(1)));
        for kind in [
            COREWEBVIEW2_PERMISSION_KIND_CAMERA,
            COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
            COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION,
            COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION,
        ] {
            assert!(
                !media_permission_allowed(&empty, principal, webview2_media_kind(kind)),
                "KEL-59: empty manifest must deny WebView2 kind {kind:?}"
            );
        }
    }

    #[test]
    fn camera_grant_does_not_allow_webview2_camera_kind() {
        let granted = parse_manifest(r#"{"app":{"web":{"camera":["*"]}}}"#).expect("grant");
        let principal = Some(webview_media_principal(WebviewId(1)));
        assert!(
            !media_permission_allowed(
                &granted,
                principal,
                webview2_media_kind(COREWEBVIEW2_PERMISSION_KIND_CAMERA)
            ),
            "KEL-73: /app camera grant must not start capture for a webview"
        );
        assert!(!media_permission_allowed(
            &granted,
            principal,
            webview2_media_kind(COREWEBVIEW2_PERMISSION_KIND_MICROPHONE)
        ));
    }
}

/// wry-facing tests for the shared helpers. macOS and Linux since KEL-65:
/// Windows no longer links wry.
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod wry_tests {
    use super::{
        MediaPermission, WebviewId, media_permission_response, webview_media_principal,
        with_guarded_media_permissions, wry_media_kind,
    };
    use keld_guard::parse_manifest;

    fn view() -> keld_guard::Principal {
        webview_media_principal(WebviewId(1))
    }

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
        let principal = view();
        assert_eq!(
            media_permission_response(&empty, principal, wry::PermissionKind::Camera),
            wry::PermissionResponse::Deny
        );
        assert_eq!(
            media_permission_response(&empty, principal, wry::PermissionKind::Microphone),
            wry::PermissionResponse::Deny
        );
        assert_eq!(
            media_permission_response(&empty, principal, wry::PermissionKind::DisplayCapture),
            wry::PermissionResponse::Deny
        );
        assert_ne!(
            media_permission_response(&empty, principal, wry::PermissionKind::Camera),
            wry::PermissionResponse::Allow,
            "KEL-59: Allow here is wry's unfixed auto-grant"
        );
        assert_ne!(
            media_permission_response(&empty, principal, wry::PermissionKind::Camera),
            wry::PermissionResponse::Default,
            "Default continues the platform behaviour — macOS auto-grants, Linux/Windows prompt. \n             v0 must Deny on all three."
        );
    }

    #[test]
    fn camera_grant_does_not_start_wry_capture_for_webview() {
        let granted = parse_manifest(r#"{"app":{"web":{"camera":["*"]}}}"#).expect("grant");
        let principal = view();
        assert_eq!(
            media_permission_response(&granted, principal, wry::PermissionKind::Camera),
            wry::PermissionResponse::Deny,
            "KEL-73: /app camera grant must not start capture (wry Deny = no capture start)"
        );
        assert_eq!(
            media_permission_response(&granted, principal, wry::PermissionKind::Microphone),
            wry::PermissionResponse::Deny
        );
        let _ = with_guarded_media_permissions(wry::WebViewBuilder::new(), granted, principal);
    }
}
