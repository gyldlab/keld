//! `WebKitGTK` backend slot (Linux) — not implemented yet, tracked as KEL-28.
//!
//! Compiled on every platform so workspace-wide clippy keeps the layout
//! honest. The real backend (webkit6/gtk4 bindings per
//! `docs/architecture/05-webview-and-native.md` §1, with the GPU-stack probe
//! and safe-mode behavior from crate `AGENTS.md`) lands under KEL-28 and
//! must implement [`crate::WebEngine`] plus [`crate::WebKitGtkEngineExt`].
//! Reference layout: `competitors/wry/src/webkitgtk/`.

use crate::error::WvError;

/// Returns the typed "no backend here yet" error (KELD-WV-001).
///
/// Kept as the single source for this backend's unavailability so callers
/// get fix guidance (track KEL-28, or run on macOS) instead of a panic or a
/// fake implementation.
#[must_use]
pub fn unavailable() -> WvError {
    WvError::UnsupportedPlatform {
        os: std::env::consts::OS,
        issue: "KEL-28",
    }
}

#[cfg(test)]
mod tests {
    use super::unavailable;
    use crate::error::WvError;

    #[test]
    fn unavailable_is_keld_wv_001_for_kel_28() {
        let err = unavailable();
        assert!(
            matches!(
                err,
                WvError::UnsupportedPlatform {
                    issue: "KEL-28",
                    os,
                } if os == std::env::consts::OS
            ),
            "webkitgtk slot must name KEL-28, got: {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("KELD-WV-001"), "missing code in: {msg}");
        assert!(msg.contains("KEL-28"), "missing tracking issue in: {msg}");
        assert!(
            !msg.contains("KEL-27"),
            "webkitgtk must not point at the Windows issue: {msg}"
        );
    }
}
