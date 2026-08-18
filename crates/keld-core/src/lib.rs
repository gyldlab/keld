//! keld-core — the host runtime.
//!
//! Owns the platform event loop, the window/webview registries, application
//! lifecycle, and dispatch between kipc links and native modules. Normative
//! spec: `docs/architecture/01-overview.md`.

mod hello;
mod lifecycle;

pub use hello::{
    DEFAULT_HELLO_TITLE, DEFAULT_RENDERER, hello_title_from_args, host_hello_unknown_arg,
    read_config_renderer, read_config_title, renderer_from_config_ts, resolve_hello_title,
    run_hello_window, run_hello_window_html, run_hello_window_titled, title_from_config_ts,
};
pub use lifecycle::LifecycleSession;

/// Crate version, re-exported for host handshake reporting.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
