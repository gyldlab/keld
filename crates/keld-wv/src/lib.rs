//! keld-wv — the webview engine layer.
//!
//! One [`WebEngine`] trait. Live backends: `wkwebview` (macOS), `webview2`
//! (Windows, KEL-27), `webkitgtk` (Linux, KEL-28). `cef` later behind a
//! feature flag. Platform extension traits ([`WkWebViewEngineExt`],
//! [`WebView2EngineExt`], [`WebKitGtkEngineExt`]) are platform-neutral
//! definitions compiled everywhere. Normative spec:
//! `docs/architecture/05-webview-and-native.md`. v0 methods: `src/engine.rs`.
//!
//! Platform backends may use `unsafe` (see crate `AGENTS.md`).

mod engine;
mod error;
mod hello;
mod media;
#[cfg(test)]
mod view_drop_order;
#[cfg(target_os = "linux")]
pub mod webkitgtk;
#[cfg(target_os = "windows")]
pub mod webview2;
#[cfg(target_os = "macos")]
pub mod wkwebview;

pub use engine::{
    DevtoolsAction, LogicalSize, NavTarget, Rect, WebEngine, WebKitGtkEngineExt, WebView2EngineExt,
    WebviewSpec, WkWebViewEngineExt,
};
pub use error::WvError;
pub use hello::{DEFAULT_HTML as HELLO_HTML, run as run_hello_window};
pub use media::{
    MediaPermission, WEB_CAMERA, WEB_MEDIA_ORIGIN, WEB_MICROPHONE, media_permission_allowed,
};

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
