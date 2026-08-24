//! The platform-neutral `WebEngine` contract (v0) and its supporting types.
//!
//! This is a documented subset of the normative trait sketch in
//! `docs/architecture/05-webview-and-native.md` §1 — only the methods the
//! Phase 1 macOS hello slice can actually implement today. Deviations from
//! the sketch, and why:
//!
//! - `post` and `register_scheme` are omitted: the kipc control-frame types
//!   (`ControlFrame`, `SchemeHandler`) do not exist yet, and wry custom
//!   protocols are builder-time options, so a post-creation `register_scheme`
//!   cannot be honored by the interim wry scaffolding. Both return with the
//!   kipc integration milestone.
//! - `eval` takes a plain `&str` and returns `Result` instead of
//!   `ScriptRef<'_>` + `EvalCallback`: the callback plumbing belongs to the
//!   command-queue design keld-core will own.
//! - `create` has no `HostHooks` parameter and `set_bounds` has no `Anchor`:
//!   both types depend on the multi-webview composition work; v0 is one
//!   window-filling webview per window.
//! - No `Send` supertrait: backends hold UI-thread-only platform handles
//!   (`WKWebView` today), and the crate invariant is "all engine/window
//!   mutations on the UI thread via command queue" (crate `AGENTS.md`). The
//!   `Send` bound returns with the command-queue design review.
//! - `set_bounds`, `devtools`, and `destroy` return `Result`: a stale
//!   [`WebviewId`] must surface as a typed error, never a panic (root
//!   `AGENTS.md`: no panics in libs).

use crate::WebviewId;
use crate::error::WvError;

/// Logical (DPI-independent) size in points.
///
/// On macOS these are `NSWindow` points. `backingScaleFactor` maps them to
/// device pixels (2.0 on Retina), so 960×640 points is 1920×1280 px on a
/// 2× display. macOS 10.7+; Apple:
/// <https://developer.apple.com/documentation/appkit/nswindow/backingscalefactor>
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalSize {
    /// Width in logical points.
    pub width: f64,
    /// Height in logical points.
    pub height: f64,
}

/// Logical rectangle: position plus size, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge in logical points.
    pub x: f64,
    /// Top edge in logical points.
    pub y: f64,
    /// Width in logical points.
    pub width: f64,
    /// Height in logical points.
    pub height: f64,
}

/// What a webview should display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    /// An inline HTML document, rendered as-is.
    Html(String),
    /// A URL to load (scheme included, e.g. `https://…`).
    Url(String),
}

/// A devtools request for [`WebEngine::devtools`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevtoolsAction {
    /// Open (or focus) the inspector attached to the webview.
    Open,
    /// Close the inspector if it is open.
    Close,
}

/// Creation parameters for a webview and its host window.
///
/// v0 is one window-filling webview per window, so the spec describes both.
/// Fields are the minimum the hello slice needs — no speculative options.
#[derive(Debug, Clone, PartialEq)]
pub struct WebviewSpec {
    /// Host window title.
    pub title: String,
    /// Initial inner size of the host window.
    pub size: LogicalSize,
    /// Content to display once the webview exists.
    pub initial: NavTarget,
}

impl Default for WebviewSpec {
    /// A 960x640 window titled "Keld" on a blank page — the Phase 1 hello
    /// slice geometry.
    fn default() -> Self {
        Self {
            title: String::from("Keld"),
            size: LogicalSize {
                width: 960.0,
                height: 640.0,
            },
            initial: NavTarget::Html(String::new()),
        }
    }
}

/// The engine contract every platform backend implements.
///
/// Backends: `crate::wkwebview` (macOS, live), `crate::webview2` (Windows,
/// live — KEL-27/KEL-65) and `crate::webkitgtk` (Linux, live; KEL-28 runtime
/// product acceptance still open). See the module docs above for how this v0
/// surface relates to the spec 05 §1 sketch.
/// Trait changes require design review (crate `AGENTS.md`).
pub trait WebEngine {
    /// Creates a webview (and, in v0, its host window) from `spec`.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::Window`] or [`WvError::Webview`] when platform
    /// window/webview creation fails, or [`WvError::EventLoop`] when the
    /// backend can no longer create views (e.g. its run loop already owns
    /// the thread).
    fn create(&mut self, spec: &WebviewSpec) -> Result<WebviewId, WvError>;

    /// Points the webview at new content.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`, or
    /// [`WvError::Navigate`] when the platform load call fails.
    fn navigate(&mut self, id: WebviewId, target: NavTarget) -> Result<(), WvError>;

    /// Evaluates `script` in the webview's top frame, fire-and-forget.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`, or
    /// [`WvError::Script`] when the platform refuses the script.
    fn eval(&mut self, id: WebviewId, script: &str) -> Result<(), WvError>;

    /// Repositions/resizes the webview. In v0 the webview fills its host
    /// window, so `rect` applies to the window (outer position, inner size).
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`.
    fn set_bounds(&mut self, id: WebviewId, rect: Rect) -> Result<(), WvError>;

    /// Opens or closes the inspector for the webview.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`.
    fn devtools(&mut self, id: WebviewId, action: DevtoolsAction) -> Result<(), WvError>;

    /// Tears down the webview and, in v0, its host window.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`.
    fn destroy(&mut self, id: WebviewId) -> Result<(), WvError>;
}

/// macOS (`WKWebView`) engine extensions.
///
/// Marker-level in v0: it fixes the shape wry proves out with
/// `WebViewExtMacOS` (`competitors/wry/src/lib.rs`) — native handle access,
/// reparenting, traffic-light insets, print options. Methods land as the
/// backend grows direct objc2 bindings (spec 05 §1); the trait definition is
/// platform-neutral so every platform's clippy pass sees it.
pub trait WkWebViewEngineExt: WebEngine {}

/// Windows (`WebView2`) engine extensions — implemented under KEL-27.
///
/// Marker-level in v0: fixes the shape wry proves out with
/// `WebViewExtWindows`/`WebViewBuilderExtWindows` — controller/environment
/// options, additional browser arguments, memory-usage target. The trait
/// definition is platform-neutral so every platform's clippy pass sees it.
pub trait WebView2EngineExt: WebEngine {}

/// Linux (`WebKitGTK`) engine extensions — implemented under KEL-28.
///
/// Marker-level in v0: fixes the shape wry proves out with
/// `WebViewExtUnix`/`WebViewBuilderExtUnix` — attaching the webview into an
/// existing GTK container instead of owning the window, plus the
/// degraded-rendering probe required by crate `AGENTS.md`. The trait
/// definition is platform-neutral so every platform's clippy pass sees it.
pub trait WebKitGtkEngineExt: WebEngine {}

#[cfg(test)]
mod tests {
    use super::{LogicalSize, NavTarget, WebviewSpec};

    #[test]
    fn webview_spec_default_matches_hello_slice() {
        let spec = WebviewSpec::default();
        assert_eq!(spec.title, "Keld");
        assert_eq!(
            spec.size,
            LogicalSize {
                width: 960.0,
                height: 640.0
            }
        );
        assert_eq!(spec.initial, NavTarget::Html(String::new()));
    }

    #[test]
    fn nav_target_variants_carry_their_payloads() {
        let html = NavTarget::Html(String::from("<h1>hi</h1>"));
        let url = NavTarget::Url(String::from("https://keld.dev"));
        assert_eq!(html, NavTarget::Html(String::from("<h1>hi</h1>")));
        assert_eq!(url, NavTarget::Url(String::from("https://keld.dev")));
        assert_ne!(html, url);
    }
}
