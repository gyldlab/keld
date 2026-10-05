//! keld-wv — the webview engine layer.
//!
//! One [`WebEngine`] trait with platform backends for `WKWebView`, `WebView2`, and
//! `WebKitGTK`. Platform extension traits ([`WkWebViewEngineExt`],
//! [`WebView2EngineExt`], [`WebKitGtkEngineExt`]) are platform-neutral
//! definitions compiled everywhere. Normative spec:
//! `docs/architecture/05-webview-and-native.md`. Methods: `src/engine.rs`.
//! Repository maturity and evidence live in `docs/engineering/product-status.tsv`.
//!
//! Platform backends may use `unsafe` (see crate `AGENTS.md`).

#[cfg(all(feature = "profile-test-hooks", not(debug_assertions)))]
compile_error!("KEL-135 profile test hooks are available only in debug builds");

mod engine;
mod error;
mod hello;
mod media;
pub mod profile;
#[cfg(target_os = "macos")]
mod startup;
#[cfg(test)]
mod view_drop_order;
/// How long a backend waits for the initial renderer navigation before startup fails
/// with KELD-WV-005.
///
/// This is a hang guard, not a performance budget: a renderer that never finishes its
/// first navigation becomes a typed startup failure instead of a window that never
/// becomes ready. It must exceed real cold and relaunch starts. Measured 2026-10-05 on
/// Windows 10.0.26300 with `WebView2` runtime 154.0.4258.53: the first navigation took
/// a median of 1.3 s and at most 1.9 s idle, and at most 3.8 s under the full workspace
/// test suite plus extra load (source: the measurement table in
/// <https://github.com/gyldlab/keld/pull/370>). Hosted `windows-latest` runners exceeded
/// the previous 5 s bound in runs 37237832288, 37256252923 and 37265644118, mostly when
/// relaunching right after a previous instance. 15 s keeps at least 3x margin over every
/// observed healthy start while every test harness that waits for `Ready` allows longer,
/// so a slow but healthy start is never cut short by either side.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) const INITIAL_NAVIGATION_DEADLINE: std::time::Duration =
    std::time::Duration::from_secs(15);

#[cfg(target_os = "linux")]
pub mod webkitgtk;
#[cfg(target_os = "windows")]
pub mod webview2;
#[cfg(target_os = "macos")]
pub mod wkwebview;
#[doc(hidden)]
pub mod wv_link;

pub use engine::{
    AppWindowCommand, AppWindowEvent, DevtoolsAction, LogicalSize, NavTarget, Rect, WebEngine,
    WebKitGtkEngineExt, WebView2EngineExt, WebviewSpec, WkWebViewEngineExt,
};
pub use error::WvError;
pub use hello::{DEFAULT_HTML as HELLO_HTML, run as run_hello_window};
pub use media::{
    MediaPermission, WEB_CAMERA, WEB_MEDIA_ORIGIN, WEB_MICROPHONE, media_permission_allowed,
};
pub use profile::{ProfileError, ProfileErrorKind, ProfileIdentity, WebProfileSelection};

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
