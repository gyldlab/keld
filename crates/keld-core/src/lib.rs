//! keld-core — the host runtime.
//!
//! Owns the platform event loop, the window/webview registries, application
//! lifecycle, and dispatch between kipc links and native modules. Normative
//! spec: `docs/architecture/01-overview.md`.

pub mod app_session;
mod argv;
mod echo_link;
mod hello;
mod hello_session;
mod lifecycle;
#[cfg(target_os = "macos")]
mod macos_profile_identity;

pub use argv::{NonUnicodeArgument, unicode_args};
#[cfg(windows)]
pub use echo_link::EchoEndpoint;
pub use echo_link::{EchoServer, echo_roundtrip};
pub use hello::{
    ApplicationNameForUserAgentConfigError, AuthConfigError, DEFAULT_ENTRY, DEFAULT_HELLO_TITLE,
    DEFAULT_RENDERER, InitialWindowConfigError, LiteralConfigValue, SingleInstanceConfigError,
    SingleInstanceDeclaration, StaticConfigError, application_name_for_user_agent_from_config_ts,
    auth_configuration_from_config_ts, entry_from_config_ts, hello_title_from_args,
    host_hello_unknown_arg, initial_window_from_config_ts, literal_config_from_ts,
    prepare_webview_process, read_config_entry, read_config_renderer, read_config_title,
    renderer_from_config_ts, resolve_hello_title, run_hello_window, run_hello_window_html,
    run_hello_window_titled, single_instance_from_config_ts, title_from_config_ts,
};
pub use hello_session::{HelloSessionError, HostOwnedHelloSession};
pub use keld_wv::profile::validate_canonical_app_id;
pub use lifecycle::LifecycleSession;

/// Crate version, re-exported for host handshake reporting.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
