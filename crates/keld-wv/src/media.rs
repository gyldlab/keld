//! Webview media-capture permission policy (KEL-59).
//!
//! wry 0.56 still auto-grants camera and microphone when
//! `WebViewBuilder::with_permission_handler` is omitted (`WKPermissionDecision::Grant`
//! in `wry_web_view_ui_delegate.rs`). Keld installs a handler that default-denies via
//! [`keld_guard::evaluate`] on `web.camera` / `web.microphone`.
//!
//! wry's handler is `Fn(PermissionKind) -> PermissionResponse` and does not pass
//! origin. v0 therefore evaluates the requested resource as [`WEB_MEDIA_ORIGIN`]
//! (`*`). Origin-scoped grants are not enforceable until the handler grows an
//! origin argument.

use keld_guard::{Decision, PermissionsManifest, evaluate};

/// Capability id for camera capture (`getUserMedia` video).
pub const WEB_CAMERA: &str = "web.camera";

/// Capability id for microphone capture (`getUserMedia` audio).
pub const WEB_MICROPHONE: &str = "web.microphone";

/// Requested-resource sentinel for v0 media checks.
///
/// Exact-match only — this is not a glob. A grant of `["*"]` allows any origin
/// because the live wry callback cannot name the requesting origin.
pub const WEB_MEDIA_ORIGIN: &str = "*";

/// Media kinds the macOS backend maps from wry `PermissionKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPermission {
    /// Camera capture.
    Camera,
    /// Microphone capture.
    Microphone,
    /// Any other wry permission (geolocation, display-capture, unknown, …).
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
/// microphone use [`evaluate`] against [`WEB_MEDIA_ORIGIN`].
#[must_use]
pub fn media_permission_allowed(manifest: &PermissionsManifest, kind: MediaPermission) -> bool {
    let Some(capability) = kind.capability() else {
        return false;
    };
    evaluate(manifest, capability, WEB_MEDIA_ORIGIN) == Decision::Allow
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
        match evaluate(&manifest, WEB_CAMERA, WEB_MEDIA_ORIGIN) {
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

    #[test]
    fn macos_backend_installs_guarded_handler() {
        let src = include_str!("wkwebview/mod.rs");
        assert!(
            src.contains("with_guarded_media_permissions"),
            "KEL-59: omitting wry with_permission_handler restores auto-grant"
        );
        assert!(
            src.contains("with_permission_handler"),
            "KEL-59: the helper must call wry's permission handler, not a no-op wrapper"
        );
        assert!(
            src.contains("media_permission_response"),
            "KEL-59: the handler must call media_permission_response, not a constant Allow"
        );
    }
}
