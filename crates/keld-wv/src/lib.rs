//! keld-wv — the webview engine layer.
//!
//! One `WebEngine` trait, four backends: `wkwebview` (macOS), `webview2`
//! (Windows), `webkitgtk` (Linux) compiled always; `cef` behind a feature flag
//! for per-platform pinned-engine policy. Normative spec:
//! `docs/architecture/05-webview-and-native.md`.
//!
//! Platform backends may use `unsafe` (see crate `AGENTS.md`).

mod error;
mod hello;

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
