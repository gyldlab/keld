//! Webview layer errors. Messages state the fix per arch/07 §2.

use core::fmt;

/// Failure in the webview engine layer.
#[derive(Debug)]
pub enum WvError {
    /// This OS backend is not implemented yet.
    UnsupportedPlatform {
        /// Host triple at runtime.
        os: &'static str,
        /// Tracking issue for the missing backend (`KEL-27` Windows, `KEL-28` Linux).
        issue: &'static str,
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
    /// The `WebView2` Evergreen runtime is absent or too old (Windows).
    ///
    /// Defined on every platform so the enum stays platform-neutral and
    /// workspace-wide clippy sees it; only `crate::webview2` constructs it.
    WebView2RuntimeMissing {
        /// Probe failure text from `GetAvailableCoreWebView2BrowserVersionString`.
        detail: String,
    },
    /// Linux NVIDIA+Wayland safe-mode process preparation was skipped or failed.
    GpuSafeModePreparation {
        /// Exact preparation or re-exec failure.
        detail: String,
    },
}

impl fmt::Display for WvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform { os, issue } => write!(
                f,
                "KELD-WV-001: no webview backend for `{os}` yet. \
                 Track {issue} or run on macOS, Windows, or Linux."
            ),
            Self::Window(msg) => write!(
                f,
                "KELD-WV-002: failed to create window — {msg}. \
                 Check display permissions and that a window server is available."
            ),
            Self::Webview(msg) => write!(
                f,
                "KELD-WV-003: failed to create webview — {msg}. \
                 On macOS ensure WKWebView is available (10.13+); \
                 on Windows ensure the WebView2 runtime is installed."
            ),
            Self::EventLoop(msg) => write!(
                f,
                "KELD-WV-004: event loop error — {msg}. \
                 Check the window server is running and the event loop was started on the UI thread."
            ),
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
            Self::WebView2RuntimeMissing { detail } => write!(
                f,
                "KELD-WV-008: the WebView2 runtime is unavailable — {detail}. \
                 Install the Evergreen Runtime from \
                 https://developer.microsoft.com/microsoft-edge/webview2/ and re-run. \
                 Keld will not download or execute an installer for you."
            ),
            Self::GpuSafeModePreparation { detail } => write!(
                f,
                "KELD-WV-010: Linux GPU safe-mode preparation failed — {detail}. \
                 Launch through Keld's process entry before creating a webview; \
                 do not mutate or export the process environment manually."
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
        let cases: [(WvError, &str, &str); 9] = [
            (
                WvError::UnsupportedPlatform {
                    os: "freebsd",
                    issue: "architecture spec 05 §1 (no backend planned)",
                },
                "KELD-WV-001",
                "run on macOS, Windows, or Linux",
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
                "UI thread",
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
            (
                WvError::WebView2RuntimeMissing {
                    detail: String::from("boom"),
                },
                "KELD-WV-008",
                "Evergreen Runtime",
            ),
            (
                WvError::GpuSafeModePreparation {
                    detail: String::from("boom"),
                },
                "KELD-WV-010",
                "process entry",
            ),
        ];
        for (err, code, fix_hint) in cases {
            let msg = err.to_string();
            assert!(msg.contains(code), "missing code {code} in: {msg}");
            assert!(msg.contains(fix_hint), "missing fix hint in: {msg}");
        }
    }
}
