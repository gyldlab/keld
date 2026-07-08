//! keld-core — the host runtime.
//!
//! Owns the platform event loop, the window/webview registries, application
//! lifecycle, and dispatch between kipc links and native modules. Normative
//! spec: `docs/architecture/01-overview.md`.

mod hello;

pub use hello::run_hello_window;

/// Crate version, re-exported for host handshake reporting.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
