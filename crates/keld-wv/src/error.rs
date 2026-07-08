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
        }
    }
}

impl std::error::Error for WvError {}
