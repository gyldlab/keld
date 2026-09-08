//! macOS backend: `WKWebView` via tao + wry scaffolding.
//!
//! Interim implementation — replace with direct objc2 bindings per
//! `docs/architecture/05-webview-and-native.md` §1 (tracked in
//! `docs/agents/learnings.md`). Layout mirrors wry's
//! `competitors/wry/src/wkwebview/` per-platform module pattern.
//!
// SAFETY: wry/tao platform backends call Objective-C WebKit/AppKit APIs that
// are only valid on the process main thread. This module is macOS-only and
// single-threaded by construction: `WkWebViewEngine::new` must run on the main
// thread (tao enforces this), and every mutation happens either before the run
// loop starts or inside tao's event loop on that same UI thread — satisfying
// the crate `AGENTS.md` "UI-thread-only mutations" invariant. No `unsafe`
// blocks appear directly in this module today; the allow sanctions the
// transitive platform calls and future direct objc2 bindings, which must each
// carry their own `// SAFETY:` proof.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::{Window, WindowBuilder};

use crate::WebviewId;
pub use crate::engine::{AppWindowCommand, AppWindowEvent};
use crate::engine::{DevtoolsAction, NavTarget, Rect, WebEngine, WebviewSpec, WkWebViewEngineExt};
use crate::error::WvError;
use crate::media::guarded_default_media_builder;
use crate::startup::{PageLoad, StartupPhase, StartupTrace, trace_enabled};

const INITIAL_NAVIGATION_DEADLINE: Duration = Duration::from_secs(5);

/// One live webview and the host window it fills (v0: one per window).
#[repr(C)]
struct View {
    webview: wry::WebView,
    window: Window,
}

/// The macOS [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WkWebViewEngine::run_until_closed`]
/// consumes it. Uses tao `run_return` so the host can reap supervised children
/// after the last window closes (KEL-30 concurrent hello app-link).
pub struct WkWebViewEngine {
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<AppWindowCommand>>,
    views: BTreeMap<u32, View>,
    next_id: u32,
    /// KEL-62: navigation completion (`PageLoadEvent::Finished`), not window create.
    startup: Arc<Mutex<StartupTrace>>,
}

impl fmt::Debug for WkWebViewEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WkWebViewEngine")
            .field("views", &self.views.len())
            .field("running", &self.event_loop.is_none())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl WkWebViewEngine {
    /// Creates the engine and its event loop.
    ///
    /// Must be called on the process main thread — `AppKit`'s platform
    /// contract, enforced by tao (which aborts otherwise).
    #[must_use]
    pub fn new() -> Self {
        let startup = Arc::new(Mutex::new(StartupTrace::new()));
        let event_loop = EventLoopBuilder::with_user_event().build();
        mark_startup(&startup, StartupPhase::EventLoop);
        Self {
            event_loop: Some(event_loop),
            views: BTreeMap::new(),
            next_id: 1,
            startup,
        }
    }

    /// Runs the event loop until the user closes the last window, then
    /// returns so the caller can reap supervised children after the last window closes.
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
        match self.startup.lock() {
            Ok(guard) => guard.emit_if_nav_never_finished(),
            Err(poisoned) => poisoned.into_inner().emit_if_nav_never_finished(),
        }
        if code == 0 {
            Ok(())
        } else {
            Err(WvError::EventLoop(format!(
                "event loop exited with status {code}"
            )))
        }
    }

    /// Creates the initial app window and emits navigation readiness from the
    /// live `WKWebView` page-load callback.
    ///
    /// # Errors
    ///
    /// Returns [`WvError`] when the window or webview cannot be created.
    pub fn create_app(
        &mut self,
        spec: &WebviewSpec,
        events: Sender<AppWindowEvent>,
    ) -> Result<WebviewId, WvError> {
        self.create_internal(spec, Some(events))
    }

    /// Runs the live macOS event loop until a Quit or fatal app-session command.
    ///
    /// `commands` is bridged through tao's [`tao::event_loop::EventLoopProxy`],
    /// so I/O threads never touch a window handle and the UI thread does not
    /// poll. Closing the last window emits [`AppWindowEvent::LastWindowClosed`]
    /// but keeps the event loop alive until the app sends Quit.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::EventLoop`] if the loop already ran, the bridge/UI
    /// reports a fatal session command, or tao exits non-zero.
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
        let proxy = event_loop.create_proxy();
        let stop_bridge = Arc::new(AtomicBool::new(false));
        let stop_for_bridge = Arc::clone(&stop_bridge);
        let terminal_intent = Arc::new(AtomicBool::new(false));
        let terminal_intent_for_bridge = Arc::clone(&terminal_intent);
        let bridge =
            spawn_app_wake_bridge(commands, proxy, stop_for_bridge, terminal_intent_for_bridge)?;
        let fatal = Arc::new(AtomicBool::new(false));
        let fatal_in_loop = Arc::clone(&fatal);
        let navigation_timed_out = Arc::new(AtomicBool::new(false));
        let navigation_timed_out_in_loop = Arc::clone(&navigation_timed_out);
        let navigation_deadline = Instant::now() + INITIAL_NAVIGATION_DEADLINE;
        let startup_in_loop = Arc::clone(&self.startup);
        let terminal_intent_in_loop = Arc::clone(&terminal_intent);
        let mut views = std::mem::take(&mut self.views);
        let code = event_loop.run_return(move |event, _, control_flow| {
            let terminal =
                is_terminal_app_event(&event) || terminal_intent_in_loop.load(Ordering::Acquire);
            if navigation_finished(&startup_in_loop) {
                *control_flow = ControlFlow::Wait;
            } else if navigation_deadline_expired(&startup_in_loop, navigation_deadline, terminal) {
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
                "primary app session failed while the macOS event loop was live",
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
                "run loop already started; create webviews before running it",
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
            .map_err(|error| WvError::Window(error.to_string()))?;
        mark_startup(&self.startup, StartupPhase::WindowCreated);
        let id = self.next_id;
        let builder = guarded_default_media_builder(
            WebviewId(id),
            page_load_trace_handler(Arc::clone(&self.startup), app_events),
        );
        let webview = builder.build_initial_window(&spec.initial, &window)?;
        mark_startup(&self.startup, StartupPhase::WebviewAttached);
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

impl Default for WkWebViewEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WebEngine for WkWebViewEngine {
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

impl WkWebViewEngineExt for WkWebViewEngine {}

fn mark_startup(startup: &Mutex<StartupTrace>, phase: StartupPhase) {
    let mut guard = match startup.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.mark(phase);
}

fn navigation_finished(startup: &Mutex<StartupTrace>) -> bool {
    match startup.lock() {
        Ok(guard) => guard.offset(StartupPhase::NavFinished).is_some(),
        Err(poisoned) => poisoned
            .into_inner()
            .offset(StartupPhase::NavFinished)
            .is_some(),
    }
}

fn spawn_app_wake_bridge(
    commands: Receiver<AppWindowCommand>,
    proxy: EventLoopProxy<AppWindowCommand>,
    stop: Arc<AtomicBool>,
    terminal_intent: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, WvError> {
    thread::Builder::new()
        .name("keld-wv-macos-app-wake".to_owned())
        .spawn(move || {
            loop {
                match commands.recv_timeout(Duration::from_millis(100)) {
                    Ok(command) => {
                        let terminal = mark_terminal_intent(&terminal_intent, command);
                        let _ = proxy.send_event(command);
                        if terminal {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) if stop.load(Ordering::Acquire) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        mark_terminal_intent(&terminal_intent, AppWindowCommand::Fatal);
                        let _ = proxy.send_event(AppWindowCommand::Fatal);
                        return;
                    }
                }
            }
        })
        .map_err(|error| WvError::EventLoop(format!("failed to start app wake bridge: {error}")))
}

fn is_terminal_app_event(event: &Event<'_, AppWindowCommand>) -> bool {
    matches!(
        event,
        Event::UserEvent(AppWindowCommand::Quit | AppWindowCommand::Fatal)
    )
}

fn mark_terminal_intent(intent: &AtomicBool, command: AppWindowCommand) -> bool {
    let terminal = matches!(command, AppWindowCommand::Quit | AppWindowCommand::Fatal);
    if terminal {
        intent.store(true, Ordering::Release);
    }
    terminal
}

fn navigation_deadline_expired(
    startup: &Mutex<StartupTrace>,
    deadline: Instant,
    terminal_command: bool,
) -> bool {
    !terminal_command && !navigation_finished(startup) && Instant::now() >= deadline
}

/// wry `PageLoadEvent::Finished` → navigation completion (`nav_finished`).
fn page_load_trace_handler(
    startup: Arc<Mutex<StartupTrace>>,
    app_events: Option<Sender<AppWindowEvent>>,
) -> impl Fn(wry::PageLoadEvent, String) + 'static {
    move |event, _url| {
        let mut guard = match startup.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let had_nav = guard.offset(StartupPhase::NavFinished).is_some();
        guard.on_page_load(wry_page_load(&event));
        if !had_nav && guard.offset(StartupPhase::NavFinished).is_some() {
            if let Some(events) = app_events.as_ref() {
                let _ = events.send(AppWindowEvent::NavigationReady);
            }
            if trace_enabled() {
                eprintln!("{}", guard.report());
            }
        }
    }
}

/// KEL-62: wry Finished → navigation completion. Started is navigation begin.
fn wry_page_load(event: &wry::PageLoadEvent) -> PageLoad {
    match event {
        wry::PageLoadEvent::Started => PageLoad::Started,
        wry::PageLoadEvent::Finished => PageLoad::Finished,
    }
}

#[cfg(test)]
mod startup_tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::{page_load_trace_handler, wry_page_load};
    use crate::startup::{PageLoad, StartupPhase, StartupTrace, phase_for_page_load};

    #[test]
    fn wry_finished_maps_to_nav_finished_started_does_not() {
        assert_eq!(
            phase_for_page_load(wry_page_load(&wry::PageLoadEvent::Finished)),
            Some(StartupPhase::NavFinished)
        );
        assert_eq!(
            phase_for_page_load(wry_page_load(&wry::PageLoadEvent::Started)),
            None,
            "KEL-62: Started is navigation begin, not completion"
        );
    }

    #[test]
    fn page_load_handler_marks_nav_finished_on_finished() -> Result<(), String> {
        let startup = Arc::new(Mutex::new(StartupTrace::new()));
        let handler = page_load_trace_handler(Arc::clone(&startup), None);
        handler(wry::PageLoadEvent::Finished, String::new());
        let guard = startup
            .lock()
            .map_err(|e| format!("startup lock poisoned: {e}"))?;
        if guard.offset(StartupPhase::NavFinished).is_none() {
            return Err(String::from(
                "Finished must mark nav_finished; omitting the handler leaves it unset",
            ));
        }
        Ok(())
    }

    #[test]
    fn page_load_handler_ignores_started_for_nav_finished() -> Result<(), String> {
        let startup = Arc::new(Mutex::new(StartupTrace::new()));
        let handler = page_load_trace_handler(Arc::clone(&startup), None);
        handler(wry::PageLoadEvent::Started, String::new());
        let guard = startup
            .lock()
            .map_err(|e| format!("startup lock poisoned: {e}"))?;
        if guard.offset(StartupPhase::NavFinished).is_some() {
            return Err(String::from("Started must not mark nav_finished"));
        }
        Ok(())
    }

    #[test]
    fn app_page_load_handler_emits_navigation_ready_once() {
        let startup = Arc::new(Mutex::new(StartupTrace::new()));
        let (events_tx, events_rx) = std::sync::mpsc::channel();
        let handler = page_load_trace_handler(Arc::clone(&startup), Some(events_tx));
        handler(wry::PageLoadEvent::Started, String::new());
        assert!(events_rx.try_recv().is_err());
        handler(wry::PageLoadEvent::Finished, String::new());
        assert_eq!(
            events_rx.recv().expect("navigation-ready event"),
            super::AppWindowEvent::NavigationReady
        );
        handler(wry::PageLoadEvent::Finished, String::new());
        assert!(
            events_rx.try_recv().is_err(),
            "Ready must be first-transition only"
        );
    }

    #[test]
    fn nav_never_finished_report_condition() {
        let mut trace = StartupTrace::new();
        trace.mark(StartupPhase::EventLoop);
        assert!(
            trace.offset(StartupPhase::NavFinished).is_none(),
            "nav_finished unset before Finished"
        );
        let report = trace.report();
        assert!(report.contains("nav_finished=never"), "{report}");
        trace.on_page_load(PageLoad::Finished);
        assert!(
            trace.offset(StartupPhase::NavFinished).is_some(),
            "Finished must set nav_finished for terminal-path guard"
        );
    }

    #[test]
    fn initial_navigation_deadline_expires_only_while_pending() {
        let startup = Mutex::new(StartupTrace::new());
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("one millisecond before now");
        assert!(super::navigation_deadline_expired(&startup, expired, false));
        let terminal_intent = std::sync::atomic::AtomicBool::new(false);
        assert!(super::mark_terminal_intent(
            &terminal_intent,
            super::AppWindowCommand::Quit
        ));
        assert!(
            !super::navigation_deadline_expired(
                &startup,
                expired,
                terminal_intent.load(std::sync::atomic::Ordering::Acquire)
            ),
            "a queued terminal UI command must win at the navigation deadline"
        );
        startup
            .lock()
            .expect("startup lock")
            .on_page_load(PageLoad::Finished);
        assert!(!super::navigation_deadline_expired(
            &startup, expired, false
        ));
    }
}

/// Opens a window from `spec` and runs until the user closes it.
///
/// Thin wrapper for the Phase 1 hello slice: builds a [`WkWebViewEngine`],
/// creates one webview, and hands the thread to the run loop.
///
/// # Errors
///
/// Returns [`WvError`] if window or webview creation fails.
pub fn run_hello(spec: &WebviewSpec) -> Result<(), WvError> {
    let mut engine = WkWebViewEngine::new();
    engine.create(spec)?;
    engine.run_until_closed()
}

#[cfg(test)]
mod view_drop_order_tests {
    use super::View;
    use crate::view_drop_order::assert_wry_view_field_order;

    #[test]
    fn view_declares_webview_before_window() {
        assert_wry_view_field_order!(View);
    }
}
