//! `WebView2` backend slot (Windows) — not implemented yet, tracked as KEL-27.
//!
//! Compiled on every platform so workspace-wide clippy keeps the layout
//! honest. The real backend (windows-rs + `WebView2` COM per
//! `docs/architecture/05-webview-and-native.md` §1) lands under KEL-27 and
//! must implement [`crate::WebEngine`] plus [`crate::WebView2EngineExt`].
//! Reference layout: `competitors/wry/src/webview2/`.

use crate::error::WvError;

/// Returns the typed "no backend here yet" error (KELD-WV-001).
///
/// Kept as the single source for this backend's unavailability so callers
/// get fix guidance (track KEL-27, or run on macOS) instead of a panic or a
/// fake implementation.
#[must_use]
pub fn unavailable() -> WvError {
    WvError::UnsupportedPlatform {
        os: std::env::consts::OS,
        issue: "KEL-27",
    }
}

#[cfg(test)]
mod tests {
    use super::unavailable;
    use crate::error::WvError;

    #[test]
    fn unavailable_is_keld_wv_001_for_kel_27() {
        let err = unavailable();
        assert!(
            matches!(
                err,
                WvError::UnsupportedPlatform {
                    issue: "KEL-27",
                    os,
                } if os == std::env::consts::OS
            ),
            "webview2 slot must name KEL-27, got: {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("KELD-WV-001"), "missing code in: {msg}");
        assert!(msg.contains("KEL-27"), "missing tracking issue in: {msg}");
        assert!(
            !msg.contains("KEL-28"),
            "webview2 must not point at the Linux issue: {msg}"
        );
    }
}
