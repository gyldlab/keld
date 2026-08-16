//! Linux backend: `WebKitGTK` via tao + wry scaffolding (KEL-28).
//!
//! Interim implementation — replace with direct webkit6/gtk4 bindings per
//! `docs/architecture/05-webview-and-native.md` §1, the same "wry now, direct
//! bindings later" policy macOS and (until KEL-65) Windows started with
//! (`docs/engineering/decisions.md` §2). Layout mirrors wry's
//! `competitors/wry/src/webkitgtk/` per-platform module pattern. wry 0.56.1's
//! Linux backend targets GTK3 + `WebKit2GTK` 4.1 — the spec's webkit6/gtk4
//! destination is a later rewrite, not this slice.
//!
//! Unlike the other two backends this module owns one extra Linux-only
//! concern: [`probe_gpu_stack`] applies the documented NVIDIA+Wayland
//! DMA-BUF safe-mode mitigation internally before any `WebKit`/GTK object is
//! constructed. Crate `AGENTS.md`: agents MUST probe the GPU stack and apply
//! safe-mode before init, MUST NOT instruct env-var exports — this is that
//! probe, not a doc telling a developer to set a shell variable.
//!
// SAFETY: `probe_gpu_stack` calls `std::env::set_var`, `unsafe` in edition
// 2024 because concurrent env reads on other threads are a real soundness
// gap on some platforms. It runs at the very start of `WebKitGtkEngine::new`
// — before the tao event loop, GTK main loop, or any WebKit object exists —
// so no other thread in this process is reading or writing the environment
// concurrently. Every mutation of engine and window state after that happens
// on the same UI thread inside tao's event loop, satisfying the crate
// `AGENTS.md` "UI-thread-only mutations" invariant.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeMap;
use std::fmt;

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Window, WindowBuilder};

use keld_guard::PermissionsManifest;

use crate::WebviewId;
use crate::engine::{DevtoolsAction, NavTarget, Rect, WebEngine, WebKitGtkEngineExt, WebviewSpec};
use crate::error::WvError;
use crate::media::with_guarded_media_permissions;

/// Outcome of [`probe_gpu_stack`]: whether Keld silently degraded rendering
/// to avoid a known `WebKitGTK` crash/flicker class.
///
/// `keld doctor` and apps read this as the structured `degraded-rendering`
/// fact crate `AGENTS.md` requires — not a log line to grep for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuSafeMode {
    /// No known-risky driver/session combination detected.
    Normal,
    /// NVIDIA proprietary driver + Wayland session: `WebKitGTK`'s DMA-BUF
    /// compositor path crashes and flickers on this combination (Tauri
    /// #9394, #14924; no fix as of `WebKitGTK` 2.54 — `research/06` §Linux).
    /// `WEBKIT_DISABLE_DMABUF_RENDERER=1` remains the documented mitigation.
    NvidiaWaylandDmabuf,
}

impl GpuSafeMode {
    /// Whether this probe result means rendering is running in a degraded
    /// (safe) mode rather than the driver's normal accelerated path.
    #[must_use]
    pub const fn is_degraded(self) -> bool {
        !matches!(self, Self::Normal)
    }

    /// Short, stable reason string for `keld doctor` / app-surfaced diagnostics.
    #[must_use]
    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::NvidiaWaylandDmabuf => Some(
                "NVIDIA driver + Wayland: DMA-BUF renderer disabled \
                 (WEBKIT_DISABLE_DMABUF_RENDERER=1) to avoid known crashes/flicker",
            ),
        }
    }
}

fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// True when the NVIDIA kernel module is loaded.
///
/// Reads `/proc/driver/nvidia/version`, which exists only while the
/// proprietary driver is loaded — no subprocess (`nvidia-smi`), no requiring
/// the probe itself to have a GPU context.
fn nvidia_driver_loaded() -> bool {
    std::path::Path::new("/proc/driver/nvidia/version").exists()
}

/// Detects the known-risky NVIDIA+Wayland combination and applies the
/// documented safe-mode mitigation on this process's own environment,
/// before any `WebKit`/GTK object is constructed.
///
/// # Errors
///
/// Never fails. The probe is best-effort: an unreadable `/proc` entry (e.g. a
/// sandboxed container) is read as "no NVIDIA driver," not an error — a
/// missed degradation defaults to the driver's normal path, which is no
/// worse than not probing at all.
#[must_use]
pub fn probe_gpu_stack() -> GpuSafeMode {
    if is_wayland_session() && nvidia_driver_loaded() {
        // SAFETY: called once at the top of `WebKitGtkEngine::new`, before
        // the tao event loop or any GTK/WebKit call — see the module SAFETY
        // note for why no concurrent env access exists yet.
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        return GpuSafeMode::NvidiaWaylandDmabuf;
    }
    GpuSafeMode::Normal
}

/// One live webview and the host window it fills (v0: one per window).
struct View {
    window: Window,
    webview: wry::WebView,
}

/// The Linux [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WebKitGtkEngine::run_until_closed`]
/// consumes it, mirroring `crate::wkwebview::WkWebViewEngine`; keld-core
/// takes ownership of the loop once the command-queue design lands (crate
/// `AGENTS.md`).
pub struct WebKitGtkEngine {
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<()>>,
    /// Result of the startup GPU-stack probe — surfaced via [`Self::gpu_safe_mode`].
    gpu_safe_mode: GpuSafeMode,
    views: BTreeMap<u32, View>,
    next_id: u32,
}

impl fmt::Debug for WebKitGtkEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebKitGtkEngine")
            .field("views", &self.views.len())
            .field("running", &self.event_loop.is_none())
            .field("gpu_safe_mode", &self.gpu_safe_mode)
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl WebKitGtkEngine {
    /// Probes the GPU stack, then creates the engine and its event loop.
    ///
    /// Must be called on the process main thread — GTK's platform contract,
    /// enforced by tao (which aborts otherwise).
    #[must_use]
    pub fn new() -> Self {
        let gpu_safe_mode = probe_gpu_stack();
        Self {
            event_loop: Some(EventLoop::new()),
            gpu_safe_mode,
            views: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Result of the startup GPU-stack probe, for `keld doctor` and
    /// app-surfaced `degraded-rendering` diagnostics.
    #[must_use]
    pub const fn gpu_safe_mode(&self) -> GpuSafeMode {
        self.gpu_safe_mode
    }

    /// Runs the event loop until the user closes a window, then exits the
    /// process (tao's `EventLoop::run` owns the thread and never returns).
    ///
    /// # Errors
    ///
    /// Returns [`WvError::EventLoop`] if the run loop was already started.
    pub fn run_until_closed(mut self) -> Result<(), WvError> {
        let Some(event_loop) = self.event_loop.take() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; call run_until_closed once",
            )));
        };
        let mut views = std::mem::take(&mut self.views);
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            if let Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } = event
            {
                // v0 hello is one window. Drop by id so a second window would
                // not tear down every view; exit when the map is empty.
                views.retain(|_, view| view.window.id() != window_id);
                if views.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }
            }
        });
    }

    fn view(&self, id: WebviewId) -> Result<&View, WvError> {
        self.views
            .get(&id.0)
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

impl Default for WebKitGtkEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WebEngine for WebKitGtkEngine {
    fn create(&mut self, spec: &WebviewSpec) -> Result<WebviewId, WvError> {
        let Some(event_loop) = self.event_loop.as_ref() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; create webviews before run_until_closed",
            )));
        };
        let window = WindowBuilder::new()
            .with_title(&spec.title)
            .with_inner_size(tao::dpi::LogicalSize::new(
                spec.size.width,
                spec.size.height,
            ))
            // KEL-25/KEL-28: standard window chrome — resize, minimize, close.
            .with_resizable(true)
            .with_minimizable(true)
            .with_closable(true)
            .build(event_loop)
            .map_err(|e| WvError::Window(e.to_string()))?;

        // Developer extras are debug-only until keld-guard owns `web.devtools`.
        let builder = wry::WebViewBuilder::new();
        #[cfg(debug_assertions)]
        let builder = builder.with_devtools(true);
        // KEL-59 parity. Without this, WebKitGTK falls back to its own
        // permission prompt on a `PermissionRequested` event it can't
        // handle — default-ask, not default-deny. Empty manifest → deny
        // everything.
        let builder = with_guarded_media_permissions(builder, PermissionsManifest::default());
        let builder = match &spec.initial {
            NavTarget::Html(html) => builder.with_html(html),
            NavTarget::Url(url) => builder.with_url(url),
        };
        let webview = builder
            .build(&window)
            .map_err(|e| WvError::Webview(e.to_string()))?;

        let id = self.next_id;
        self.next_id += 1;
        self.views.insert(id, View { window, webview });
        Ok(WebviewId(id))
    }

    fn navigate(&mut self, id: WebviewId, target: NavTarget) -> Result<(), WvError> {
        let view = self.view(id)?;
        match target {
            NavTarget::Html(html) => view.webview.load_html(&html),
            NavTarget::Url(url) => view.webview.load_url(&url),
        }
        .map_err(|e| WvError::Navigate(e.to_string()))
    }

    fn eval(&mut self, id: WebviewId, script: &str) -> Result<(), WvError> {
        self.view(id)?
            .webview
            .evaluate_script(script)
            .map_err(|e| WvError::Script(e.to_string()))
    }

    fn set_bounds(&mut self, id: WebviewId, rect: Rect) -> Result<(), WvError> {
        let view = self.view(id)?;
        // v0 webviews fill their host window, so bounds apply to the window.
        view.window
            .set_outer_position(tao::dpi::LogicalPosition::new(rect.x, rect.y));
        view.window
            .set_inner_size(tao::dpi::LogicalSize::new(rect.width, rect.height));
        Ok(())
    }

    fn devtools(&mut self, id: WebviewId, action: DevtoolsAction) -> Result<(), WvError> {
        let view = self.view(id)?;
        match action {
            DevtoolsAction::Open => view.webview.open_devtools(),
            DevtoolsAction::Close => view.webview.close_devtools(),
        }
        Ok(())
    }

    fn destroy(&mut self, id: WebviewId) -> Result<(), WvError> {
        // Dropping the `View` releases the webview, then closes the window.
        self.views
            .remove(&id.0)
            .map(|_| ())
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

impl WebKitGtkEngineExt for WebKitGtkEngine {}

/// Opens a window from `spec` and runs until the user closes it.
///
/// Thin wrapper for the Phase 1 hello slice: probes the GPU stack, builds a
/// [`WebKitGtkEngine`], creates one webview, and hands the thread to the run
/// loop.
///
/// # Errors
///
/// Returns [`WvError`] if window or webview creation fails.
pub fn run_hello(spec: &WebviewSpec) -> Result<(), WvError> {
    let mut engine = WebKitGtkEngine::new();
    engine.create(spec)?;
    engine.run_until_closed()
}

#[cfg(test)]
mod tests {
    use super::{GpuSafeMode, WebKitGtkEngine, is_wayland_session, nvidia_driver_loaded};
    use crate::error::WvError;

    #[test]
    fn gpu_safe_mode_normal_is_not_degraded_and_has_no_reason() {
        assert!(!GpuSafeMode::Normal.is_degraded());
        assert_eq!(GpuSafeMode::Normal.reason(), None);
    }

    #[test]
    fn gpu_safe_mode_nvidia_wayland_is_degraded_with_a_reason() {
        let mode = GpuSafeMode::NvidiaWaylandDmabuf;
        assert!(mode.is_degraded());
        let reason = mode.reason().expect("degraded mode must explain itself");
        assert!(
            reason.contains("WEBKIT_DISABLE_DMABUF_RENDERER"),
            "{reason}"
        );
        assert!(reason.contains("NVIDIA"), "{reason}");
        assert!(reason.contains("Wayland"), "{reason}");
    }

    /// Pure probes: no GTK/WebKit call, no env mutation, safe to run in any
    /// test harness regardless of what the actual CI runner's GPU looks like.
    #[test]
    fn session_and_driver_probes_do_not_panic_on_a_headless_runner() {
        let _ = is_wayland_session();
        let _ = nvidia_driver_loaded();
    }

    /// Keeps the engine type named from the test module so a rename fails the
    /// build here rather than silently orphaning these tests. Constructing
    /// one needs a tao `EventLoop`, which must be built on the process main
    /// thread and (on Linux) a live GTK main loop — the harness gives each
    /// test its own thread, so the engine itself is exercised by the GUI
    /// pass in KEL-28, not here (same constraint as `wkwebview`'s tests).
    fn _assert_engine_type(_: Option<&WebKitGtkEngine>) {}

    #[test]
    fn unknown_webview_id_is_typed_not_panic() {
        let err = WvError::UnknownWebview { id: 3 };
        assert!(err.to_string().contains("KELD-WV-007"));
    }
}
