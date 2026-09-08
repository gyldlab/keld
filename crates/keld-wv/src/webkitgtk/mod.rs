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
//! concern: [`prepare_gpu_safe_mode_process`] applies the documented
//! NVIDIA+Wayland DMA-BUF safe-mode mitigation by exact-self re-exec before
//! any `WebKit`/GTK object is constructed. [`WebKitGtkEngine::new`] then
//! validates that preparation before init. Crate `AGENTS.md`: do this
//! programmatically, never by instructing users to export an environment
//! variable.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeMap;
use std::ffi::{CString, OsStr, OsString, c_char};
use std::fmt;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::WebviewId;
use crate::engine::{
    AppWindowCommand, AppWindowEvent, DevtoolsAction, NavTarget, Rect, WebEngine,
    WebKitGtkEngineExt, WebviewSpec,
};
use crate::error::WvError;
use crate::media::guarded_default_media_builder;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::{Window, WindowBuilder};

const INITIAL_NAVIGATION_DEADLINE: Duration = Duration::from_secs(5);
const GPU_SAFE_MODE_ENV: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
const SELF_EXE: &str = "/proc/self/exe";

unsafe extern "C" {
    #[link_name = "execve"]
    fn execve_raw(
        path: *const c_char,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> std::ffi::c_int;
}

/// Outcome of Linux GPU-stack detection and process preparation: whether Keld
/// silently degraded rendering to avoid a known `WebKitGTK` crash/flicker class.
///
/// `keld doctor` and apps read this as the structured `degraded-rendering`
/// fact crate `AGENTS.md` requires — not a log line to grep for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuSafeMode {
    /// No known-risky driver/session combination detected.
    Normal,
    /// NVIDIA proprietary driver + Wayland require process preparation, but
    /// the DMA-BUF mitigation is not present yet.
    NvidiaWaylandPreparationRequired,
    /// NVIDIA proprietary driver + Wayland session: `WebKitGTK`'s DMA-BUF
    /// compositor path crashes and flickers on this combination, on every
    /// `WebKitGTK` release through 2.54 (no fix shipped as of that release —
    /// `research/06` §Linux). Upstream reports:
    /// [tauri-apps/tauri#9394](https://github.com/tauri-apps/tauri/issues/9394),
    /// [#14924](https://github.com/tauri-apps/tauri/issues/14924).
    /// `WEBKIT_DISABLE_DMABUF_RENDERER=1` remains the documented mitigation.
    NvidiaWaylandDmabufDisabled,
}

impl GpuSafeMode {
    /// Whether this probe result means rendering is running in a degraded
    /// (safe) mode rather than the driver's normal accelerated path.
    #[must_use]
    pub const fn is_degraded(self) -> bool {
        matches!(self, Self::NvidiaWaylandDmabufDisabled)
    }

    /// Whether process entry must exact-self re-exec before engine creation.
    #[must_use]
    pub const fn requires_preparation(self) -> bool {
        matches!(self, Self::NvidiaWaylandPreparationRequired)
    }

    /// Short, stable reason string for `keld doctor` / app-surfaced diagnostics.
    #[must_use]
    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::NvidiaWaylandPreparationRequired => Some(
                "NVIDIA driver + Wayland: DMA-BUF safe-mode preparation is required before WebKitGTK initialization",
            ),
            Self::NvidiaWaylandDmabufDisabled => Some(
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

fn detect_gpu_safe_mode_with(wayland: bool, nvidia: bool, value: Option<&OsStr>) -> GpuSafeMode {
    if !wayland || !nvidia {
        GpuSafeMode::Normal
    } else if value == Some(OsStr::new("1")) {
        GpuSafeMode::NvidiaWaylandDmabufDisabled
    } else {
        GpuSafeMode::NvidiaWaylandPreparationRequired
    }
}

/// Detects the NVIDIA+Wayland safe-mode state. Pure: reads the session,
/// driver presence, and mitigation environment, but mutates nothing. Safe to call from
/// anywhere (`keld doctor`, tests, repeatedly) without side effects. Process
/// preparation is owned separately by [`prepare_gpu_safe_mode_process`].
///
/// Best-effort: an unreadable `/proc` entry (e.g. a sandboxed container) is
/// read as "no NVIDIA driver," not an error — a missed degradation defaults
/// to the driver's normal path, which is no worse than not probing at all.
#[must_use]
pub fn detect_gpu_safe_mode() -> GpuSafeMode {
    let value = std::env::var_os(GPU_SAFE_MODE_ENV);
    detect_gpu_safe_mode_with(
        is_wayland_session(),
        nvidia_driver_loaded(),
        value.as_deref(),
    )
}

fn nul_error(kind: &'static str) -> impl FnOnce(std::ffi::NulError) -> io::Error {
    move |_| io::Error::new(io::ErrorKind::InvalidInput, format!("{kind} contains NUL"))
}

fn build_execve_vectors<I, E>(
    arguments: I,
    environment: E,
) -> io::Result<(Vec<CString>, Vec<CString>)>
where
    I: IntoIterator<Item = OsString>,
    E: IntoIterator<Item = (OsString, OsString)>,
{
    let mut argument_strings = arguments
        .into_iter()
        .map(|arg| CString::new(arg.into_vec()).map_err(nul_error("process argument")))
        .collect::<io::Result<Vec<_>>>()?;
    if argument_strings.is_empty() {
        argument_strings.push(CString::new(SELF_EXE).map_err(nul_error("fallback argv0"))?);
    }

    let mut environment_strings = Vec::new();
    for (key, value) in environment {
        if key == OsStr::new(GPU_SAFE_MODE_ENV) {
            continue;
        }
        let mut entry = key.into_vec();
        entry.push(b'=');
        entry.extend(value.into_vec());
        environment_strings
            .push(CString::new(entry).map_err(nul_error("process environment entry"))?);
    }
    environment_strings.push(
        CString::new(format!("{GPU_SAFE_MODE_ENV}=1"))
            .map_err(nul_error("GPU safe-mode environment entry"))?,
    );
    Ok((argument_strings, environment_strings))
}

fn exact_self_execve() -> io::Error {
    let (argv, envp) = match build_execve_vectors(std::env::args_os(), std::env::vars_os()) {
        Ok(vectors) => vectors,
        Err(error) => return error,
    };
    let mut argv_ptrs: Vec<*const c_char> = argv.iter().map(|value| value.as_ptr()).collect();
    argv_ptrs.push(std::ptr::null());
    let mut envp_ptrs: Vec<*const c_char> = envp.iter().map(|value| value.as_ptr()).collect();
    envp_ptrs.push(std::ptr::null());

    // SAFETY: Linux execve(2) takes NUL-terminated strings in NULL-terminated
    // pointer arrays and does not return on success:
    // https://man7.org/linux/man-pages/man2/execve.2.html
    // The path is a static C string. Every argv/envp entry is an owned
    // `CString`; neither the arrays nor their backing strings move or drop
    // during the call. `execve` receives envp directly and never requires us
    // to assign process-global `environ`.
    let result = unsafe {
        execve_raw(
            c"/proc/self/exe".as_ptr(),
            argv_ptrs.as_ptr(),
            envp_ptrs.as_ptr(),
        )
    };
    if result == -1 {
        io::Error::last_os_error()
    } else {
        io::Error::other(format!("execve returned unexpected status {result}"))
    }
}

fn prepare_gpu_safe_mode_process_with<F>(
    mode: GpuSafeMode,
    reexec: F,
) -> Result<GpuSafeMode, WvError>
where
    F: FnOnce() -> io::Error,
{
    if !mode.requires_preparation() {
        return Ok(mode);
    }

    let error = reexec();
    Err(WvError::GpuSafeModePreparation {
        detail: format!("exact-self re-exec through `{SELF_EXE}` failed: {error}"),
    })
}

/// Applies the NVIDIA+Wayland safe-mode mitigation before webview startup.
///
/// On the risky stack, an unprepared process is replaced through
/// `/proc/self/exe` with the same arguments and
/// `WEBKIT_DISABLE_DMABUF_RENDERER=1`. `exec` preserves the process identity
/// while avoiding edition-2024's unsound concurrent mutation of the live
/// environment. The Linux [`execve(2)` contract](https://man7.org/linux/man-pages/man2/execve.2.html)
/// receives explicit null-terminated `argv` and `envp`, preserves the PID, and
/// does not return on success. Call this from a process-entry dispatcher before
/// creating threads or other state that cannot safely be repeated. It returns
/// normally only when no mitigation is needed or the replacement process is prepared.
///
/// # Errors
///
/// Returns [`WvError`] when exact-self re-exec fails. It never falls back to
/// initializing `WebKitGTK` on the risky unprepared path.
pub fn prepare_gpu_safe_mode_process() -> Result<GpuSafeMode, WvError> {
    prepare_gpu_safe_mode_process_with(detect_gpu_safe_mode(), exact_self_execve)
}

fn require_prepared_gpu_safe_mode_with(mode: GpuSafeMode) -> Result<GpuSafeMode, WvError> {
    if mode.requires_preparation() {
        Err(WvError::GpuSafeModePreparation {
            detail: String::from(
                "NVIDIA+Wayland engine construction was reached before exact-self re-exec",
            ),
        })
    } else {
        Ok(mode)
    }
}

fn require_prepared_gpu_safe_mode() -> Result<GpuSafeMode, WvError> {
    require_prepared_gpu_safe_mode_with(detect_gpu_safe_mode())
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
    ///
    /// # Errors
    ///
    /// Returns [`WvError`] before GTK/WebKit initialization when a risky
    /// NVIDIA+Wayland process skipped [`prepare_gpu_safe_mode_process`].
    pub fn new() -> Result<Self, WvError> {
        let gpu_safe_mode = require_prepared_gpu_safe_mode()?;
        Ok(Self {
            event_loop: Some(EventLoopBuilder::with_user_event().build()),
            gpu_safe_mode,
            views: BTreeMap::new(),
            next_id: 1,
            navigation_ready: Arc::new(AtomicBool::new(false)),
            app_window_created: false,
        })
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

        let ready = Arc::clone(&self.navigation_ready);
        let id = self.next_id;
        let builder = guarded_default_media_builder(WebviewId(id), move |event, _url| {
            if matches!(event, wry::PageLoadEvent::Finished)
                && !ready.swap(true, Ordering::AcqRel)
                && let Some(events) = app_events.as_ref()
            {
                let _ = events.send(AppWindowEvent::NavigationReady);
            }
        });
        // KEL-59/KEL-132: Linux defaults an unhandled request to deny, but
        // that is not proof Keld evaluated the right manifest/principal. The
        // guarded witness mints the webview principal and is required to apply
        // initial content and build the live WebKitGTK view.
        // wry's plain `build(&window)` wires only X11. The witness owns the
        // `build_gtk` call, so the guarded builder cannot be recovered to
        // replace its callback before the Wayland/X11 build.
        let webview = builder.build_initial_gtk(&spec.initial, &window)?;

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
/// loop. Process entry must call [`prepare_gpu_safe_mode_process`] first.
///
/// # Errors
///
/// Returns [`WvError`] if window or webview creation fails.
pub fn run_hello(spec: &WebviewSpec) -> Result<(), WvError> {
    let mut engine = WebKitGtkEngine::new()?;
    engine.create(spec)?;
    engine.run_until_closed()
}

#[cfg(test)]
mod tests {
    use std::ffi::{CString, OsStr, OsString};
    use std::io;
    use std::os::unix::ffi::OsStringExt;
    use std::time::{Duration, Instant};

    use super::{
        GPU_SAFE_MODE_ENV, GpuSafeMode, WebKitGtkEngine, build_execve_vectors,
        detect_gpu_safe_mode_with, is_wayland_session, navigation_deadline_expired,
        nvidia_driver_loaded, prepare_gpu_safe_mode_process_with,
        require_prepared_gpu_safe_mode_with,
    };
    use crate::error::WvError;

    #[test]
    fn gpu_safe_mode_normal_is_not_degraded_and_has_no_reason() {
        assert!(!GpuSafeMode::Normal.is_degraded());
        assert_eq!(GpuSafeMode::Normal.reason(), None);
    }

    #[test]
    fn gpu_safe_mode_nvidia_wayland_is_degraded_with_a_reason() {
        let mode = GpuSafeMode::NvidiaWaylandDmabufDisabled;
        assert!(mode.is_degraded());
        assert!(!mode.requires_preparation());
        let reason = mode.reason().expect("degraded mode must explain itself");
        assert!(
            reason.contains("WEBKIT_DISABLE_DMABUF_RENDERER"),
            "{reason}"
        );
        assert!(reason.contains("NVIDIA"), "{reason}");
        assert!(reason.contains("Wayland"), "{reason}");
    }

    #[test]
    fn unprepared_nvidia_wayland_is_risky_not_degraded() {
        let mode = GpuSafeMode::NvidiaWaylandPreparationRequired;
        assert!(mode.requires_preparation());
        assert!(!mode.is_degraded());
        let reason = mode.reason().expect("unprepared risk must explain itself");
        assert!(reason.contains("preparation is required"), "{reason}");
        assert!(!reason.contains("renderer disabled"), "{reason}");
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
    /// effects. No backend path mutates the live process environment.
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

    #[test]
    fn gpu_safe_mode_preparation_matrix_fails_closed() {
        assert_eq!(
            detect_gpu_safe_mode_with(false, true, None),
            GpuSafeMode::Normal
        );
        assert_eq!(
            detect_gpu_safe_mode_with(true, false, None),
            GpuSafeMode::Normal
        );
        assert_eq!(
            detect_gpu_safe_mode_with(true, true, None),
            GpuSafeMode::NvidiaWaylandPreparationRequired
        );
        assert_eq!(
            detect_gpu_safe_mode_with(true, true, Some(OsStr::new("0"))),
            GpuSafeMode::NvidiaWaylandPreparationRequired
        );
        assert_eq!(
            detect_gpu_safe_mode_with(true, true, Some(OsStr::new("1"))),
            GpuSafeMode::NvidiaWaylandDmabufDisabled
        );

        assert_eq!(
            require_prepared_gpu_safe_mode_with(GpuSafeMode::Normal)
                .expect("normal stack needs no preparation"),
            GpuSafeMode::Normal
        );
        assert_eq!(
            require_prepared_gpu_safe_mode_with(GpuSafeMode::NvidiaWaylandDmabufDisabled)
                .expect("prepared risky stack"),
            GpuSafeMode::NvidiaWaylandDmabufDisabled
        );
        let error =
            require_prepared_gpu_safe_mode_with(GpuSafeMode::NvidiaWaylandPreparationRequired)
                .expect_err("unprepared risky stack must fail before GTK/WebKit");
        let message = error.to_string();
        assert!(message.contains("KELD-WV-010"), "{message}");
        assert!(message.contains("exact-self re-exec"), "{message}");
    }

    #[test]
    fn execve_vectors_preserve_argv0_args_and_one_mitigation_override() {
        let (argv, envp) = build_execve_vectors(
            [OsString::from("multicall-keld"), OsString::from("--hello")],
            [
                (OsString::from("KEEP"), OsString::from("value")),
                (OsString::from(GPU_SAFE_MODE_ENV), OsString::from("stale")),
                (
                    OsString::from(GPU_SAFE_MODE_ENV),
                    OsString::from("duplicate"),
                ),
            ],
        )
        .expect("valid execve vectors");
        let argv: Vec<&[u8]> = argv.iter().map(CString::as_bytes).collect();
        assert_eq!(argv, [b"multicall-keld".as_slice(), b"--hello".as_slice()]);
        let envp: Vec<&[u8]> = envp.iter().map(CString::as_bytes).collect();
        assert!(envp.contains(&b"KEEP=value".as_slice()));
        let overrides: Vec<_> = envp
            .iter()
            .filter(|entry| entry.starts_with(GPU_SAFE_MODE_ENV.as_bytes()))
            .copied()
            .collect();
        assert_eq!(overrides, [b"WEBKIT_DISABLE_DMABUF_RENDERER=1"]);

        let invalid = OsString::from_vec(b"bad\0argument".to_vec());
        let error = build_execve_vectors([invalid], std::iter::empty())
            .expect_err("NUL-bearing argv cannot cross execve");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn process_preparation_reexec_failure_is_typed_and_never_falls_back() {
        let error = prepare_gpu_safe_mode_process_with(
            GpuSafeMode::NvidiaWaylandPreparationRequired,
            || io::Error::other("fixture re-exec failure"),
        )
        .expect_err("a returned exec error must fail closed");
        let message = error.to_string();
        assert!(message.contains("KELD-WV-010"), "{message}");
        assert!(message.contains("fixture re-exec failure"), "{message}");

        let normal = prepare_gpu_safe_mode_process_with(GpuSafeMode::Normal, || {
            panic!("normal stack must not re-exec")
        })
        .expect("normal stack");
        assert_eq!(normal, GpuSafeMode::Normal);

        let prepared =
            prepare_gpu_safe_mode_process_with(GpuSafeMode::NvidiaWaylandDmabufDisabled, || {
                panic!("prepared stack must not re-exec")
            })
            .expect("prepared risky stack");
        assert_eq!(prepared, GpuSafeMode::NvidiaWaylandDmabufDisabled);
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
