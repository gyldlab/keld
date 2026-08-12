//! keld-core — the host runtime.
//!
//! Owns the platform event loop, the window/webview registries, application
//! lifecycle, and dispatch between kipc links and native modules. Normative
//! spec: `docs/architecture/01-overview.md`.

mod hello;

pub use hello::{
    DEFAULT_HELLO_TITLE, hello_title_from_args, read_config_title, resolve_hello_title,
    run_hello_window, run_hello_window_titled, title_from_config_ts,
};

/// Crate version, re-exported for host handshake reporting.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
