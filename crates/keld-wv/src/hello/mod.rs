//! Hello-world window entry (Phase 1 vertical slice).
//!
//! Thin dispatch to the per-OS backend modules: `crate::wkwebview` on macOS,
//! `crate::webview2` on Windows, `crate::webkitgtk` on Linux.

use crate::engine::{NavTarget, WebviewSpec};
use crate::error::WvError;

/// Default HTML for the scaffold hello window.
pub const DEFAULT_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>Keld</title>
  <style>
    body { font-family: system-ui, sans-serif; margin: 0; display: grid;
           place-items: center; min-height: 100vh; background: #0b0f14; color: #e8eef5; }
    h1 { font-weight: 600; letter-spacing: -0.02em; }
    p { opacity: 0.75; max-width: 32rem; text-align: center; line-height: 1.5; }
  </style>
</head>
<body>
  <div>
    <h1>Keld</h1>
    <p>Hello from Keld — Phase 1 window-on-screen vertical slice.</p>
  </div>
</body>
</html>"#;

/// Creation spec for the hello slice: `title`, inline HTML, default 960×640.
#[must_use]
pub(crate) fn hello_spec(title: &str, html: &str) -> WebviewSpec {
    WebviewSpec {
        title: title.to_owned(),
        initial: NavTarget::Html(html.to_owned()),
        ..WebviewSpec::default()
    }
}

/// Run the hello-world window until closed.
///
/// # Errors
///
/// Returns [`WvError`] if the platform backend is unavailable or window/webview creation fails.
pub fn run(title: &str, html: &str) -> Result<(), WvError> {
    run_with_ready(title, html, || {})
}

/// Like [`run`], but calls `on_ready` once the window has been created —
/// before the run loop starts (KEL-72: the real, host-backed signal
/// `app.whenReady()` blocks on).
///
/// # Errors
///
/// Returns [`WvError`] if the platform backend is unavailable or window/webview creation fails.
pub fn run_with_ready(title: &str, html: &str, on_ready: impl FnOnce()) -> Result<(), WvError> {
    let spec = hello_spec(title, html);
    #[cfg(target_os = "macos")]
    {
        crate::wkwebview::run_hello_with_ready(&spec, on_ready)
    }
    #[cfg(target_os = "windows")]
    {
        crate::webview2::run_hello_with_ready(&spec, on_ready)
    }
    #[cfg(target_os = "linux")]
    {
        crate::webkitgtk::run_hello_with_ready(&spec, on_ready)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = spec;
        let _ = on_ready;
        // macOS, Windows, and Linux all have live backends (KEL-26/27/28);
        // this arm is the true "nothing planned" case, so it names the spec
        // rather than citing tickets that would mislead a reader into
        // thinking there is one to follow.
        Err(WvError::UnsupportedPlatform {
            os: std::env::consts::OS,
            issue: "architecture spec 05 §1 (no backend planned)",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_HTML, hello_spec};
    use crate::engine::{LogicalSize, NavTarget};

    #[test]
    fn hello_html_is_local_document() {
        assert!(DEFAULT_HTML.contains("<!DOCTYPE html>"));
        // Engine-neutral on purpose: the same const backs WKWebView, WebView2,
        // and WebKitGTK, so it must not name one engine.
        assert!(DEFAULT_HTML.contains("Hello from Keld"));
        assert!(!DEFAULT_HTML.contains("http://"));
        assert!(!DEFAULT_HTML.contains("https://"));
    }

    #[test]
    fn hello_spec_carries_title_html_and_default_geometry() {
        let spec = hello_spec("Acme", DEFAULT_HTML);
        assert_eq!(spec.title, "Acme");
        assert_eq!(spec.initial, NavTarget::Html(DEFAULT_HTML.to_owned()));
        assert_eq!(
            spec.size,
            LogicalSize {
                width: 960.0,
                height: 640.0
            }
        );
    }
}

// macOS (KEL-26), Windows (KEL-27), and Linux (KEL-28) all have live
// backends now, so `run` opens a window on every platform this workspace
// targets and cannot be asserted headlessly. This module keeps the shape for
// whatever platform is next in line to lose its backend (BSD, etc.) —
// nothing in this workspace's CI matrix hits it today.
#[cfg(all(
    test,
    not(any(target_os = "macos", target_os = "windows", target_os = "linux"))
))]
mod platform_tests {
    use super::{DEFAULT_HTML, run};

    #[test]
    fn hello_window_unsupported_without_a_backend() {
        let err = run("Keld", DEFAULT_HTML).unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(err, crate::error::WvError::UnsupportedPlatform { .. }),
            "expected UnsupportedPlatform, got: {msg}"
        );
        assert!(msg.contains("KELD-WV-001"), "missing code in: {msg}");
    }
}
