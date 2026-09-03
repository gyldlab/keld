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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::unix::WindowExtUnix;
use tao::window::{Window, WindowBuilder};
use wry::WebViewBuilderExtUnix;

use keld_guard::PermissionsManifest;

use crate::WebviewId;
use crate::engine::{
    AppWindowCommand, AppWindowEvent, DevtoolsAction, NavTarget, Rect, WebEngine,
    WebKitGtkEngineExt, WebviewSpec,
};
use crate::error::WvError;
use crate::media::{webview_media_principal, with_guarded_media_permissions};

const INITIAL_NAVIGATION_DEADLINE: Duration = Duration::from_secs(5);

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
    /// compositor path crashes and flickers on this combination, on every
    /// `WebKitGTK` release through 2.54 (no fix shipped as of that release —
    /// `research/06` §Linux). Upstream reports:
    /// [tauri-apps/tauri#9394](https://github.com/tauri-apps/tauri/issues/9394),
    /// [#14924](https://github.com/tauri-apps/tauri/issues/14924).
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

/// Detects the known-risky NVIDIA+Wayland combination. Pure: reads the
/// session type and driver presence, mutates nothing. Safe to call from
/// anywhere (`keld doctor`, tests, repeatedly) without side effects — unlike
/// [`probe_gpu_stack`], which additionally applies the mitigation.
///
/// Best-effort: an unreadable `/proc` entry (e.g. a sandboxed container) is
/// read as "no NVIDIA driver," not an error — a missed degradation defaults
/// to the driver's normal path, which is no worse than not probing at all.
#[must_use]
pub fn detect_gpu_safe_mode() -> GpuSafeMode {
    if is_wayland_session() && nvidia_driver_loaded() {
        GpuSafeMode::NvidiaWaylandDmabuf
    } else {
        GpuSafeMode::Normal
    }
}

/// Detects the known-risky NVIDIA+Wayland combination ([`detect_gpu_safe_mode`])
/// and, if found, applies the documented safe-mode mitigation on this
/// process's own environment.
///
/// Callers MUST invoke this before any `WebKit`/GTK object is constructed —
/// [`WebKitGtkEngine::new`] is the only current call site, at the very top of
/// engine construction, before the tao event loop or any GTK/WebKit call.
/// Calling it a second time is safe but pointless (idempotent env write); do
/// not call it from `keld doctor` or anywhere that only wants to *read* the
/// state — use [`detect_gpu_safe_mode`] there instead, which cannot mutate
/// anything.
#[must_use]
pub fn probe_gpu_stack() -> GpuSafeMode {
    let mode = detect_gpu_safe_mode();
    if mode.is_degraded() {
        // SAFETY: this function's own contract requires the caller to invoke
        // it before the tao event loop or any GTK/WebKit call exists — see
        // the module SAFETY note for why no concurrent env access exists yet.
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
    mode
}

/// One live webview and the host window it fills (v0: one per window).
#[repr(C)]
struct View {
    webview: wry::WebView,
    window: Window,
}

/// The Linux [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WebKitGtkEngine::run_until_closed`]
/// consumes it. Uses tao `run_return` so the host can reap supervised children
/// after the last window closes (KEL-30 concurrent hello app-link).
pub struct WebKitGtkEngine {
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<AppWindowCommand>>,
    /// Result of the startup GPU-stack probe — surfaced via [`Self::gpu_safe_mode`].
    gpu_safe_mode: GpuSafeMode,
    views: BTreeMap<u32, View>,
    next_id: u32,
    navigation_ready: Arc<AtomicBool>,
    app_window_created: bool,
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
            event_loop: Some(EventLoopBuilder::with_user_event().build()),
            gpu_safe_mode,
            views: BTreeMap::new(),
            next_id: 1,
            navigation_ready: Arc::new(AtomicBool::new(false)),
            app_window_created: false,
        }
    }

    /// Result of the startup GPU-stack probe, for `keld doctor` and
    /// app-surfaced `degraded-rendering` diagnostics.
    #[must_use]
    pub const fn gpu_safe_mode(&self) -> GpuSafeMode {
        self.gpu_safe_mode
    }

    /// Runs the event loop until the user closes the last window, then
    /// returns so the caller can tear down host-owned app-link state.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::EventLoop`] if the run loop was already started, or if
    /// tao reports a non-zero `run_return` status (e.g. display disconnect).
    pub fn run_until_closed(mut self) -> Result<(), WvError> {
        let Some(mut event_loop) = self.event_loop.take() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; call run_until_closed once",
            )));
        };
        let mut views = std::mem::take(&mut self.views);
        let code = event_loop.run_return(move |event, _, control_flow| {
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
        if code == 0 {
            Ok(())
        } else {
            Err(WvError::EventLoop(format!(
                "event loop exited with status {code}"
            )))
        }
    }

    /// Creates the initial app window and emits live navigation readiness.
    ///
    /// # Errors
    ///
    /// Returns [`WvError`] when the window or webview cannot be created, or
    /// when the v0 app-window slot was already consumed.
    pub fn create_app(
        &mut self,
        spec: &WebviewSpec,
        events: Sender<AppWindowEvent>,
    ) -> Result<WebviewId, WvError> {
        if self.app_window_created || !self.views.is_empty() {
            return Err(WvError::EventLoop(String::from(
                "the v0 app window was already created",
            )));
        }
        self.navigation_ready.store(false, Ordering::Release);
        let id = self.create_internal(spec, Some(events))?;
        self.app_window_created = true;
        Ok(id)
    }

    /// Runs the Linux UI loop until Quit or a fatal app-session command.
    ///
    /// Commands cross tao's [`EventLoopProxy`], so app-link threads never
    /// mutate GTK or `WebKit` objects. Closing the last window reports the
    /// lifecycle event and keeps the loop alive until Bun sends Quit.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::Navigate`] when initial navigation exceeds its
    /// deadline, and [`WvError::EventLoop`] for duplicate run, a fatal app
    /// command, bridge failure, or a non-zero tao exit.
    pub fn run_app_until_quit(
        mut self,
        commands: Receiver<AppWindowCommand>,
        events: Sender<AppWindowEvent>,
    ) -> Result<(), WvError> {
        let Some(mut event_loop) = self.event_loop.take() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; call run_app_until_quit once",
            )));
        };
        let stop_bridge = Arc::new(AtomicBool::new(false));
        let terminal_intent = Arc::new(AtomicBool::new(false));
        let bridge = spawn_app_wake_bridge(
            commands,
            event_loop.create_proxy(),
            Arc::clone(&stop_bridge),
            Arc::clone(&terminal_intent),
        )?;
        let fatal = Arc::new(AtomicBool::new(false));
        let fatal_in_loop = Arc::clone(&fatal);
        let navigation_timed_out = Arc::new(AtomicBool::new(false));
        let navigation_timed_out_in_loop = Arc::clone(&navigation_timed_out);
        let navigation_ready = Arc::clone(&self.navigation_ready);
        let navigation_deadline = Instant::now() + INITIAL_NAVIGATION_DEADLINE;
        let terminal_intent_in_loop = Arc::clone(&terminal_intent);
        let mut views = std::mem::take(&mut self.views);
        let code = event_loop.run_return(move |event, _, control_flow| {
            let terminal = matches!(
                event,
                Event::UserEvent(AppWindowCommand::Quit | AppWindowCommand::Fatal)
            ) || terminal_intent_in_loop.load(Ordering::Acquire);
            if navigation_ready.load(Ordering::Acquire) {
                *control_flow = ControlFlow::Wait;
            } else if views.is_empty() {
                // A pre-navigation user close is a lifecycle event, not a
                // renderer timeout. Keep the loop alive for Bun's Quit.
                *control_flow = ControlFlow::Wait;
            } else if navigation_deadline_expired(
                false,
                !views.is_empty(),
                terminal,
                Instant::now(),
                navigation_deadline,
            ) {
                navigation_timed_out_in_loop.store(true, Ordering::Release);
                views.clear();
                *control_flow = ControlFlow::Exit;
                return;
            } else {
                *control_flow = ControlFlow::WaitUntil(navigation_deadline);
            }
            match event {
                Event::UserEvent(AppWindowCommand::Quit) => {
                    views.clear();
                    *control_flow = ControlFlow::Exit;
                }
                Event::UserEvent(AppWindowCommand::Fatal) => {
                    fatal_in_loop.store(true, Ordering::Release);
                    views.clear();
                    *control_flow = ControlFlow::Exit;
                }
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    views.retain(|_, view| view.window.id() != window_id);
                    if views.is_empty() {
                        let _ = events.send(AppWindowEvent::LastWindowClosed);
                    }
                }
                _ => {}
            }
        });
        stop_bridge.store(true, Ordering::Release);
        let _ = bridge.join();
        if navigation_timed_out.load(Ordering::Acquire) {
            return Err(WvError::Navigate(String::from(
                "initial renderer navigation did not finish before the startup deadline",
            )));
        }
        if fatal.load(Ordering::Acquire) {
            return Err(WvError::EventLoop(String::from(
                "primary app session failed while the Linux event loop was live",
            )));
        }
        if code == 0 {
            Ok(())
        } else {
            Err(WvError::EventLoop(format!(
                "event loop exited with status {code}"
            )))
        }
    }

    fn create_internal(
        &mut self,
        spec: &WebviewSpec,
        app_events: Option<Sender<AppWindowEvent>>,
    ) -> Result<WebviewId, WvError> {
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
            .with_resizable(true)
            .with_minimizable(true)
            .with_closable(true)
            .build(event_loop)
            .map_err(|e| WvError::Window(e.to_string()))?;

        let builder = wry::WebViewBuilder::new();
        #[cfg(debug_assertions)]
        let builder = builder.with_devtools(true);
        // KEL-59 parity: without the guard WebKitGTK falls back to its own
        // permission prompt. The empty manifest is the default-deny policy;
        // mint the webview id first so it cannot inherit app-process grants.
        let id = self.next_id;
        let builder = with_guarded_media_permissions(
            builder,
            PermissionsManifest::default(),
            webview_media_principal(WebviewId(id)),
        );
        let ready = Arc::clone(&self.navigation_ready);
        let builder = builder.with_on_page_load_handler(move |event, _url| {
            if matches!(event, wry::PageLoadEvent::Finished)
                && !ready.swap(true, Ordering::AcqRel)
                && let Some(events) = app_events.as_ref()
            {
                let _ = events.send(AppWindowEvent::NavigationReady);
            }
        });
        let builder = match &spec.initial {
            NavTarget::Html(html) => builder.with_html(html),
            NavTarget::Url(url) => builder.with_url(url),
        };
        // wry's plain `build(&window)` wires only X11. `build_gtk` with tao's
        // existing default vbox is required for both Wayland and X11; passing
        // the GtkApplicationWindow itself silently leaves the webview detached.
        let vbox = window.default_vbox().ok_or_else(|| {
            WvError::Webview(String::from(
                "tao window has no default GTK vbox (WindowBuilderExtUnix::with_default_vbox(false) was set)",
            ))
        })?;
        let webview = builder
            .build_gtk(vbox)
            .map_err(|e| WvError::Webview(e.to_string()))?;

        self.next_id += 1;
        self.views.insert(id, View { webview, window });
        Ok(WebviewId(id))
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
        self.create_internal(spec, None)
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

fn spawn_app_wake_bridge(
    commands: Receiver<AppWindowCommand>,
    proxy: EventLoopProxy<AppWindowCommand>,
    stop: Arc<AtomicBool>,
    terminal_intent: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, WvError> {
    thread::Builder::new()
        .name("keld-wv-linux-app-wake".to_owned())
        .spawn(move || {
            loop {
                match commands.recv_timeout(Duration::from_millis(100)) {
                    Ok(command) => {
                        if stop.load(Ordering::Acquire) {
                            return;
                        }
                        let terminal =
                            matches!(command, AppWindowCommand::Quit | AppWindowCommand::Fatal);
                        if terminal {
                            terminal_intent.store(true, Ordering::Release);
                        }
                        if proxy.send_event(command).is_err() || terminal {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) if stop.load(Ordering::Acquire) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        terminal_intent.store(true, Ordering::Release);
                        let _ = proxy.send_event(AppWindowCommand::Fatal);
                        return;
                    }
                }
            }
        })
        .map_err(|error| WvError::EventLoop(format!("failed to start app wake bridge: {error}")))
}

fn navigation_deadline_expired(
    ready: bool,
    has_views: bool,
    terminal: bool,
    now: Instant,
    deadline: Instant,
) -> bool {
    !ready && has_views && !terminal && now >= deadline
}

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
    use std::time::{Duration, Instant};

    use super::{
        GpuSafeMode, WebKitGtkEngine, is_wayland_session, navigation_deadline_expired,
        nvidia_driver_loaded,
    };
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

    /// `detect_gpu_safe_mode` must never touch the process environment —
    /// `keld doctor` (and this test) can call it freely without side
    /// effects. Only `probe_gpu_stack` may mutate.
    #[test]
    fn detect_gpu_safe_mode_does_not_mutate_the_environment() {
        // `env::var_os` (read) is safe in edition 2024; only `set_var` /
        // `remove_var` (write) are unsafe.
        let before = std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER");
        let _ = super::detect_gpu_safe_mode();
        let after = std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER");
        assert_eq!(
            before, after,
            "detect_gpu_safe_mode must be pure — no env mutation"
        );
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

    #[test]
    fn closed_or_terminal_window_never_becomes_a_navigation_timeout() {
        let deadline = Instant::now();
        let expired = deadline + Duration::from_millis(1);
        assert!(navigation_deadline_expired(
            false, true, false, expired, deadline
        ));
        assert!(!navigation_deadline_expired(
            false, false, false, expired, deadline
        ));
        assert!(!navigation_deadline_expired(
            false, true, true, expired, deadline
        ));
        assert!(!navigation_deadline_expired(
            true, true, false, expired, deadline
        ));
    }

    #[test]
    fn view_declares_webview_before_window() {
        use super::View;
        use crate::view_drop_order::assert_wry_view_field_order;

        assert_wry_view_field_order!(View);
    }
}
