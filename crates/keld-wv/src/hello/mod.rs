//! Hello-world window entry (Phase 1 vertical slice).
//!
//! Thin dispatch to the per-OS backend modules: `crate::wkwebview` on
//! macOS; Windows/Linux return [`WvError::UnsupportedPlatform`] until
//! KEL-27/KEL-28 land their backends.

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
    <p>Hello from WKWebView — Phase 1 window-on-screen vertical slice.</p>
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

/// Run the hello-world window until closed. macOS only for now.
///
/// # Errors
///
/// Returns [`WvError`] if the platform backend is unavailable or window/webview creation fails.
pub fn run(title: &str, html: &str) -> Result<(), WvError> {
    let spec = hello_spec(title, html);
    #[cfg(target_os = "macos")]
    {
        crate::wkwebview::run_hello(&spec)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = spec;
        Err(WvError::UnsupportedPlatform {
            os: std::env::consts::OS,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_HTML, hello_spec};
    use crate::engine::{NavTarget, WebviewSpec};

    #[test]
    fn hello_html_is_local_document() {
        assert!(DEFAULT_HTML.contains("<!DOCTYPE html>"));
        assert!(DEFAULT_HTML.contains("Hello from WKWebView"));
        assert!(!DEFAULT_HTML.contains("http://"));
        assert!(!DEFAULT_HTML.contains("https://"));
    }

    #[test]
    fn hello_spec_carries_title_html_and_default_geometry() {
        let spec = hello_spec("Acme", DEFAULT_HTML);
        assert_eq!(spec.title, "Acme");
        assert_eq!(spec.initial, NavTarget::Html(DEFAULT_HTML.to_owned()));
        assert_eq!(spec.size, WebviewSpec::default().size);
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod platform_tests {
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
