//! Webview layer errors. Messages state the fix per arch/07 §2.

use core::fmt;

/// Failure in the webview engine layer.
#[derive(Debug)]
pub enum WvError {
    /// This OS backend is not implemented yet.
    UnsupportedPlatform {
        /// Host triple at runtime.
        os: &'static str,
    },
    /// Window creation failed.
    Window(String),
    /// Webview attach failed.
    Webview(String),
    /// Event loop exited with an error.
    EventLoop(String),
    /// Navigation (load HTML/URL) failed.
    Navigate(String),
    /// Script evaluation failed.
    Script(String),
    /// The operation referenced a webview id the engine does not own.
    UnknownWebview {
        /// The stale or never-issued id.
        id: u32,
    },
}

impl fmt::Display for WvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform { os } => write!(
                f,
                "KELD-WV-001: no webview backend for `{os}` yet. \
                 Track KEL-27 (Windows) / KEL-28 (Linux) or run on macOS."
            ),
            Self::Window(msg) => write!(
                f,
                "KELD-WV-002: failed to create window — {msg}. \
                 Check display permissions and that a window server is available."
            ),
            Self::Webview(msg) => write!(
                f,
                "KELD-WV-003: failed to create webview — {msg}. \
                 On macOS ensure WKWebView is available (10.13+)."
            ),
            Self::EventLoop(msg) => write!(f, "KELD-WV-004: event loop error — {msg}."),
            Self::Navigate(msg) => write!(
                f,
                "KELD-WV-005: navigation failed — {msg}. \
                 Check the target URL scheme and that the webview still exists."
            ),
            Self::Script(msg) => write!(
                f,
                "KELD-WV-006: script evaluation failed — {msg}. \
                 Verify the script parses and the webview finished creating."
            ),
            Self::UnknownWebview { id } => write!(
                f,
                "KELD-WV-007: no webview with id {id}. \
                 Create one with `WebEngine::create` and drop stale ids after `destroy`."
            ),
        }
    }
}

impl std::error::Error for WvError {}

#[cfg(test)]
mod tests {
    use super::WvError;

    #[test]
    fn display_messages_carry_error_codes_and_fix_guidance() {
        let cases: [(WvError, &str, &str); 7] = [
            (
                WvError::UnsupportedPlatform { os: "freebsd" },
                "KELD-WV-001",
                "KEL-27",
            ),
            (
                WvError::Window(String::from("boom")),
                "KELD-WV-002",
                "window server",
            ),
            (
                WvError::Webview(String::from("boom")),
                "KELD-WV-003",
                "WKWebView",
            ),
            (
                WvError::EventLoop(String::from("boom")),
                "KELD-WV-004",
                "boom",
            ),
            (
                WvError::Navigate(String::from("boom")),
                "KELD-WV-005",
                "URL scheme",
            ),
            (
                WvError::Script(String::from("boom")),
                "KELD-WV-006",
                "parses",
            ),
            (
                WvError::UnknownWebview { id: 7 },
                "KELD-WV-007",
                "WebEngine::create",
            ),
        ];
        for (err, code, fix_hint) in cases {
            let msg = err.to_string();
            assert!(msg.contains(code), "missing code {code} in: {msg}");
            assert!(msg.contains(fix_hint), "missing fix hint in: {msg}");
        }
    }
}
