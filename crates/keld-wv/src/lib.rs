//! keld-wv — the webview engine layer.
//!
//! One [`WebEngine`] trait. Live backend: `wkwebview` (macOS). Windows
//! (`webview2`, KEL-27) and Linux (`webkitgtk`, KEL-28) are not in this
//! crate yet; `cef` later behind a feature flag. Platform extension traits
//! ([`WkWebViewEngineExt`], [`WebView2EngineExt`], [`WebKitGtkEngineExt`])
//! are platform-neutral definitions compiled everywhere. Normative spec:
//! `docs/architecture/05-webview-and-native.md`. v0 methods: `src/engine.rs`.
//!
//! Platform backends may use `unsafe` (see crate `AGENTS.md`).

mod engine;
mod error;
mod hello;
#[cfg(target_os = "macos")]
pub mod wkwebview;

pub use engine::{
    DevtoolsAction, LogicalSize, NavTarget, Rect, WebEngine, WebKitGtkEngineExt, WebView2EngineExt,
    WebviewSpec, WkWebViewEngineExt,
};
pub use error::WvError;
pub use hello::{DEFAULT_HTML as HELLO_HTML, run as run_hello_window};

/// Identifies a webview instance owned by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WebviewId(pub u32);

/// Engine selection policy, configurable globally or per platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnginePolicy {
    /// Use the operating system's webview (default).
    #[default]
    System,
    /// Use the bundled pinned engine (CEF today; Verso tracked).
    Pinned,
}
