//! Webview media-capture permission policy (KEL-59, KEL-73).
//!
//! One policy, two install mechanisms:
//!
//! - **macOS + Linux (wry interim)**: wry's `with_permission_handler` is the
//!   same builder call on both — omitting it means auto-grant on macOS 12+
//!   (`WKPermissionDecision::Grant` in
//!   [`wry_web_view_ui_delegate.rs`](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/wkwebview/class/wry_web_view_ui_delegate.rs))
//!   while `WebKitGTK` 2.52.6 and wry 0.56.1 default-deny an unhandled Linux
//!   request. The Linux fallback still cannot prove Keld evaluated the right
//!   principal and manifest. Keld therefore installs an explicit guarded
//!   callback on both through a build witness (cfg-gated, so no intra-doc
//!   link). Wry cfg-removes that delegate method below macOS 12 on debug
//!   hosts; oldest-supported-macOS proof remains open. Vendored locally:
//!   `competitors/wry` @ this same commit.
//!   `WebKitGTK`'s user-media default is documented by
//!   [`UserMediaPermissionRequest`](https://webkitgtk.org/reference/webkit2gtk/stable/class.UserMediaPermissionRequest.html);
//!   wry's OS gate is in its pinned
//!   [`build.rs`](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/build.rs).
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

#[cfg(all(target_os = "linux", debug_assertions))]
use std::fs::OpenOptions;
#[cfg(all(target_os = "linux", debug_assertions))]
use std::io::Write;

use keld_guard::{Decision, DenyReason, PermissionsManifest, Principal, evaluate};

use crate::WebviewId;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::{engine::NavTarget, error::WvError};

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
/// microphone require a minted webview principal; omitted identity and
/// [`Principal::AppProcess`] fail closed as `KELD-GUARD007`.
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
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[must_use]
fn media_permission_response(
    manifest: &PermissionsManifest,
    principal: Principal,
    kind: wry::PermissionKind,
) -> wry::PermissionResponse {
    let decision = wry_media_decision(manifest, principal, wry_media_kind(kind));
    wry_response(decision.as_ref())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn wry_media_decision(
    manifest: &PermissionsManifest,
    principal: Principal,
    kind: MediaPermission,
) -> Option<Decision> {
    kind.capability()
        .map(|capability| media_permission_decision(manifest, Some(principal), capability))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn wry_response(decision: Option<&Decision>) -> wry::PermissionResponse {
    if matches!(decision, Some(Decision::Allow)) {
        wry::PermissionResponse::Allow
    } else {
        wry::PermissionResponse::Deny
    }
}

/// Boxed callback the wry adapter installs before initial content.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) type WryPermissionCallback =
    Box<dyn Fn(wry::PermissionKind) -> wry::PermissionResponse + Send + Sync + 'static>;

/// Adapter boundary that turns an unguarded builder into a guarded witness.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) trait WryPermissionInstaller: Sized {
    /// Witness type produced only after the callback is installed.
    type Guarded;

    /// Installs `callback` and returns the guarded witness.
    fn install_permission_handler(self, callback: WryPermissionCallback) -> Self::Guarded;
}

/// Opaque witness retaining the wry builder after Keld installs its callback.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) struct GuardedWryBuilder<B> {
    inner: B,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl WryPermissionInstaller for wry::WebViewBuilder<'_> {
    type Guarded = GuardedWryBuilder<Self>;

    fn install_permission_handler(self, callback: WryPermissionCallback) -> Self::Guarded {
        GuardedWryBuilder {
            inner: self.with_permission_handler(callback),
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl<'a> GuardedWryBuilder<wry::WebViewBuilder<'a>> {
    fn with_initial_target(self, target: &NavTarget) -> wry::WebViewBuilder<'a> {
        match target {
            NavTarget::Html(html) => self.inner.with_html(html),
            NavTarget::Url(url) => self.inner.with_url(url),
        }
    }
}

#[cfg(target_os = "linux")]
impl<'a> GuardedWryBuilder<wry::WebViewBuilder<'a>> {
    /// Applies initial content and performs the only Linux build operation
    /// without exposing the guarded builder for callback replacement.
    pub(crate) fn build_initial_gtk(
        self,
        target: &NavTarget,
        window: &'a tao::window::Window,
    ) -> Result<wry::WebView, WvError> {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;

        let vbox = window.default_vbox().ok_or_else(|| {
            WvError::Webview(String::from(
                "tao window has no default GTK vbox (WindowBuilderExtUnix::with_default_vbox(false) was set)",
            ))
        })?;
        self.with_initial_target(target)
            .build_gtk(vbox)
            .map_err(|error| WvError::Webview(error.to_string()))
    }
}

#[cfg(target_os = "macos")]
impl<'a> GuardedWryBuilder<wry::WebViewBuilder<'a>> {
    /// Applies initial content and performs the only macOS build operation
    /// without exposing the guarded builder for callback replacement.
    pub(crate) fn build_initial_window(
        self,
        target: &NavTarget,
        window: &'a tao::window::Window,
    ) -> Result<wry::WebView, WvError> {
        self.with_initial_target(target)
            .build(window)
            .map_err(|error| WvError::Webview(error.to_string()))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn guarded_default_media_builder(
    id: WebviewId,
    on_page_load: impl Fn(wry::PageLoadEvent, String) + 'static,
) -> GuardedWryBuilder<wry::WebViewBuilder<'static>> {
    let builder = wry::WebViewBuilder::new();
    #[cfg(debug_assertions)]
    let builder = builder.with_devtools(true);
    let builder = builder.with_on_page_load_handler(on_page_load);
    with_guarded_media_permissions(builder, PermissionsManifest::default(), id)
}

/// Installs a default-deny media-capture handler backed by `keld-guard`.
///
/// Omitting wry's handler means wry 0.56.1 auto-grants on macOS 12+
/// ([`wry_web_view_ui_delegate.rs`](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/wkwebview/class/wry_web_view_ui_delegate.rs)
/// returns `Grant` unconditionally). Linux's unhandled default is already
/// deny, but only this explicit callback proves the host evaluated a new
/// request against the minted webview principal and immutable manifest. Saved
/// browser permission preferences can bypass wry's callback; KEL-135 owns the
/// required ephemeral-dev/persistent-profile lifecycle boundary. The manifest
/// remains the authority (`docs/architecture/03-security.md` §1).
///
/// This helper accepts a [`WebviewId`], not an arbitrary [`Principal`], and
/// mints the media principal itself. The opaque witness gates initial content
/// and platform build. Windows keeps its fallible COM-specific
/// `GuardInstalled` owner in `webview2/mod.rs`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[must_use]
pub(crate) fn with_guarded_media_permissions<I>(
    installer: I,
    manifest: PermissionsManifest,
    id: WebviewId,
) -> I::Guarded
where
    I: WryPermissionInstaller,
{
    let principal = webview_media_principal(id);
    let callback = Box::new(move |kind| {
        let media_kind = wry_media_kind(kind);
        let decision = wry_media_decision(&manifest, principal, media_kind);
        let response = wry_response(decision.as_ref());
        trace_linux_policy_decision(
            principal,
            media_kind,
            decision.as_ref(),
            response,
            &manifest,
        );
        response
    });
    installer.install_permission_handler(callback)
}

#[cfg(all(target_os = "linux", debug_assertions))]
fn trace_linux_policy_decision(
    principal: Principal,
    kind: MediaPermission,
    decision: Option<&Decision>,
    response: wry::PermissionResponse,
    manifest: &PermissionsManifest,
) {
    let Some(path) = std::env::var_os("KELD_MEDIA_POLICY_TRACE") else {
        return;
    };
    let Some(capability) = kind.capability() else {
        return;
    };
    let decision = match decision {
        Some(Decision::Allow) => "allow",
        Some(Decision::Deny(reason)) => reason.code(),
        None => return,
    };
    let response = match response {
        wry::PermissionResponse::Allow => "allow",
        wry::PermissionResponse::Deny => "deny",
        wry::PermissionResponse::Default => "default",
    };
    let Principal::Webview { id, generation } = principal else {
        return;
    };
    let nonce = std::env::var("KELD_MEDIA_NONCE").unwrap_or_else(|_| String::from("missing"));
    let Ok(mut trace) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let manifest_fingerprint = format!("{manifest:?}")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        });
    let _ = writeln!(
        trace,
        "policy nonce={nonce} capability={capability} principal=webview:{id}:{generation} manifest_fnv1a64={manifest_fingerprint:016x} decision={decision} response={response} pid={}",
        std::process::id()
    );
}

#[cfg(all(
    any(target_os = "macos", target_os = "linux"),
    not(all(target_os = "linux", debug_assertions))
))]
fn trace_linux_policy_decision(
    _principal: Principal,
    _kind: MediaPermission,
    _decision: Option<&Decision>,
    _response: wry::PermissionResponse,
    _manifest: &PermissionsManifest,
) {
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
    /// denies every kind; an `AppProcess` camera grant must not allow a webview.
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
    use std::sync::{Arc, Mutex};

    use super::{
        MediaPermission, WebviewId, WryPermissionCallback, WryPermissionInstaller,
        media_permission_response, webview_media_principal, with_guarded_media_permissions,
        wry_media_kind,
    };
    use keld_guard::parse_manifest;

    struct FakeInstaller {
        installed: Arc<Mutex<Option<WryPermissionCallback>>>,
    }

    struct FakeGuardInstalled;

    impl WryPermissionInstaller for FakeInstaller {
        type Guarded = FakeGuardInstalled;

        fn install_permission_handler(self, callback: WryPermissionCallback) -> Self::Guarded {
            let mut installed = match self.installed.lock() {
                Ok(installed) => installed,
                Err(poisoned) => poisoned.into_inner(),
            };
            *installed = Some(callback);
            FakeGuardInstalled
        }
    }

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
            "Default delegates platform policy — macOS auto-grants and Linux defaults deny without Keld provenance. v0 must explicitly Deny on both."
        );
    }

    #[test]
    fn adapter_installs_the_exact_default_deny_callback() {
        let installed = Arc::new(Mutex::new(None));
        let _guard = with_guarded_media_permissions(
            FakeInstaller {
                installed: Arc::clone(&installed),
            },
            parse_manifest("{}").expect("empty manifest"),
            WebviewId(7),
        );
        let callback = installed
            .lock()
            .expect("fake callback slot")
            .take()
            .expect("adapter must install a callback");
        for kind in [
            wry::PermissionKind::Camera,
            wry::PermissionKind::Microphone,
            wry::PermissionKind::Other,
        ] {
            assert_eq!(
                callback(kind),
                wry::PermissionResponse::Deny,
                "installed callback must explicitly deny {kind:?}"
            );
        }
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
    }
}
