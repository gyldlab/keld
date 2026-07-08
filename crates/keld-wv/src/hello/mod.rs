//! Hello-world window entry (Phase 1 vertical slice).

#[cfg(target_os = "macos")]
mod macos;

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
    <p>Hello from WKWebView — Phase 1 window-on-screen vertical slice.</p>
  </div>
</body>
</html>"#;

/// Run the hello-world window until closed. macOS only for now.
///
/// # Errors
///
/// Returns [`WvError`] if the platform backend is unavailable or window/webview creation fails.
pub fn run(title: &str, html: &str) -> Result<(), WvError> {
    #[cfg(target_os = "macos")]
    {
        macos::run(title, html)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (title, html);
        Err(WvError::UnsupportedPlatform {
            os: std::env::consts::OS,
        })
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::{DEFAULT_HTML, run};

    #[test]
    fn hello_window_unsupported_off_macos() {
        let err = run("Keld", DEFAULT_HTML).unwrap_err();
        assert!(matches!(
            err,
            crate::error::WvError::UnsupportedPlatform { .. }
        ));
    }
}
