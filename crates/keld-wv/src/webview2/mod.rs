//! Windows backend: `WebView2` via direct windows-rs COM bindings (KEL-65).
//!
//! This is the `docs/architecture/05-webview-and-native.md` §1 destination for
//! the create path — environment, controller, permission handler, and
//! navigation are Keld-owned COM calls through `webview2-com` — with tao still
//! providing the window and event loop until keld-core owns the command queue.
//! wry remains the interim scaffolding on macOS only; Windows no longer links
//! it. Layout still mirrors wry's `competitors/wry/src/webview2/` per-platform
//! module pattern.
//!
//! Why direct COM instead of wry's `InnerWebView` (measured under KEL-65):
//! wry 0.56.1 unconditionally injects its `window.ipc` bridge script through
//! a **blocking** cross-process `AddScriptToExecuteOnDocumentCreated`
//! round-trip against a renderer that is still booting — 96–109 ms for a bridge
//! Keld never uses (kipc has its own transport design). Owning the environment
//! also drops wry's default `AdditionalBrowserArguments`, which disabled
//! `SmartScreen` (KEL-66), and makes the KEL-62 initial-bounds fix structural:
//! the controller gets its size before the first navigation, so Chromium never
//! composites a zero-sized surface.
//!
//! Ordering contract inside `create`: **guard permission handler → devtools
//! policy → bounds + visibility → navigate**. Content must never run before
//! the `keld-guard` handler is registered (crate `AGENTS.md`), which the
//! compiler enforces: `navigate_initial` demands the `GuardInstalled` proof
//! only `install_guarded_media_permissions` can mint.
//!
//! Unlike the macOS backend this module owns one extra Windows-only concern:
//! the Evergreen runtime is a separate redistributable that may be absent.
//! [`runtime_version`] probes for it up front so the failure is a typed
//! `KELD-WV-008` with install guidance, not an opaque COM `HRESULT`.
//!
// SAFETY: this module speaks to WebView2 over COM. The WebView2 threading
// contract is a single-threaded apartment: the environment, controller, and
// webview are created on the process main thread (tao's event-loop thread),
// and every later use — resize, navigate, eval, devtools, drop — happens on
// that same thread inside engine methods or tao's event loop, satisfying both
// the COM contract and the crate `AGENTS.md` "UI-thread-only mutations"
// invariant. Async completions are delivered by `webview2_com::wait_with_pump`,
// which pumps this thread's message queue — the documented WebView2 callback
// mechanism (learn.microsoft.com, "Threading model for WebView2 apps").
// `GetAvailableCoreWebView2BrowserVersionString` is a pure query whose
// COM-allocated out-string is released exactly once with `CoTaskMemFree`.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use tao::dpi::PhysicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::WindowExtWindows;
use tao::window::{Window, WindowBuilder};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC, COREWEBVIEW2_PERMISSION_KIND,
    COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
    ICoreWebView2Environment, ICoreWebView2EnvironmentOptions,
};
use webview2_com::{
    CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, ExecuteScriptCompletedHandler,
    NavigationCompletedEventHandler, PermissionRequestedEventHandler, wait_with_pump,
};
use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED, HWND, RECT};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::core::{BOOL, HSTRING};

use keld_guard::{PermissionsManifest, Principal};

use crate::WebviewId;
use crate::engine::{
    AppWindowCommand, AppWindowEvent, DevtoolsAction, NavTarget, Rect, WebEngine,
    WebView2EngineExt, WebviewSpec,
};
use crate::error::WvError;
use crate::media::{media_permission_allowed, webview_media_principal, webview2_media_kind};

/// Returns the installed `WebView2` Evergreen runtime version.
///
/// This is the detection path Microsoft documents. We deliberately do not read
/// the `EdgeUpdate` registry keys ourselves: which key is authoritative differs
/// between the Evergreen, fixed-version, and per-user channels, and the loader
/// already encodes those rules.
///
/// # Errors
///
/// Returns [`WvError::WebView2RuntimeMissing`] when the runtime is absent, too
/// old, or the loader reports any other failure.
pub fn runtime_version() -> Result<String, WvError> {
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::core::{PCWSTR, PWSTR};

    let mut version = PWSTR::null();
    // SAFETY: `PCWSTR::null()` selects the installed Evergreen runtime (the
    // documented "use the default install" argument). `&mut version` is a valid
    // out-pointer for the duration of the call. See the module SAFETY note.
    let probe =
        unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &raw mut version) };

    if let Err(err) = probe {
        return Err(WvError::WebView2RuntimeMissing {
            detail: err.to_string(),
        });
    }
    if version.is_null() {
        return Err(WvError::WebView2RuntimeMissing {
            detail: String::from("loader reported success but returned no version"),
        });
    }

    // SAFETY: the loader returned success and a non-null pointer, so `version`
    // points at a NUL-terminated UTF-16 string it allocated with the COM task
    // allocator. We read it before freeing, and free exactly once.
    let text = unsafe { version.to_string() };
    // SAFETY: `version` came from the COM task allocator and has not been freed.
    unsafe { CoTaskMemFree(Some(version.as_ptr().cast())) };

    text.map_err(|err| WvError::WebView2RuntimeMissing {
        detail: format!("runtime version string was not valid UTF-16: {err}"),
    })
}

/// Identifier the `WebView2` profile directory is named after.
///
/// v0 constant. It becomes the app's `identifier` from `keld.config.ts` when
/// that plumbing exists — deliberately not built yet (YAGNI), because the hello
/// slice has no config to read and a fake indirection would not make the path
/// any more correct.
const PROFILE_IDENTIFIER: &str = "dev.keld";
const INITIAL_NAVIGATION_DEADLINE: Duration = Duration::from_secs(5);

/// Directory `WebView2` keeps its profile in.
///
/// Without an explicit user-data folder the loader defaults to
/// `<exe-name>.WebView2\` **beside the executable** (KEL-63). A shipped app
/// lives in `C:\Program Files\<App>\`, which a standard user cannot write, so
/// first launch would fail there while working fine from `target\release\`. It
/// is also per-install-path rather than per-user, so two OS users would share
/// one cookie/localStorage store.
///
/// `%LOCALAPPDATA%` is per-user and writable by design. If it is somehow
/// missing we fall back to the OS temp dir: a profile that does not survive
/// reboot is bad, but a webview that cannot start is worse.
fn user_data_dir() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, std::path::PathBuf::from)
        .join(PROFILE_IDENTIFIER)
}

/// Creates the shared `WebView2` environment for this engine.
///
/// Options are Keld-owned and deliberately the `webview2-com` defaults, which
/// means **empty** `AdditionalBrowserArguments`: wry's default disabled
/// `SmartScreen` via `--disable-features=…,msSmartScreenProtection` (KEL-66),
/// and Keld does not inherit that. Environment creation is cheap (3–6 ms
/// measured) — it only resolves the runtime; the browser process launches at
/// first controller creation (`learn.microsoft.com`, `WebView2` process
/// model).
fn create_environment() -> Result<ICoreWebView2Environment, WvError> {
    let options = CoreWebView2EnvironmentOptions::default();
    let user_data = HSTRING::from(user_data_dir().as_os_str());
    let (tx, rx) = mpsc::channel();

    // SAFETY: called on the engine thread with a live STA (module note). The
    // handler is a one-shot completion callback delivered via this thread's
    // message pump; `tx` outlives the wait below because `rx` blocks in
    // `wait_with_pump` until the send happens or the loader fails the call.
    let launched = unsafe {
        CreateCoreWebView2EnvironmentWithOptions(
            windows::core::PCWSTR::null(),
            &user_data,
            &ICoreWebView2EnvironmentOptions::from(options),
            &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                move |result, environment| {
                    let environment = result
                        .and(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)));
                    tx.send(environment)
                        .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
                },
            )),
        )
    };
    if let Err(err) = launched {
        return Err(WvError::WebView2RuntimeMissing {
            detail: err.to_string(),
        });
    }

    let environment = wait_with_pump(rx).map_err(|err| WvError::WebView2RuntimeMissing {
        detail: err.to_string(),
    })?;
    environment.map_err(|err| WvError::WebView2RuntimeMissing {
        detail: err.to_string(),
    })
}

/// Creates the controller that hosts one webview inside `hwnd`.
///
/// This is where the runtime processes actually launch — 313–506 ms measured,
/// and per Microsoft "the bulk of starting a `WebView2` control"
/// (`WebView2Feedback` #1536). Everything Keld controls happens around it, not
/// inside it.
fn create_controller(
    environment: &ICoreWebView2Environment,
    hwnd: HWND,
) -> Result<ICoreWebView2Controller, WvError> {
    let (tx, rx) = mpsc::channel();

    // SAFETY: `environment` was created on this thread (STA) and `hwnd` is a
    // live tao window owned by the caller for the duration of the call. The
    // completion handler is one-shot and delivered via this thread's pump.
    let launched = unsafe {
        environment.CreateCoreWebView2Controller(
            hwnd,
            &CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                move |result, controller| {
                    let controller =
                        result.and(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)));
                    tx.send(controller)
                        .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
                },
            )),
        )
    };
    if let Err(err) = launched {
        return Err(WvError::WebView2RuntimeMissing {
            detail: err.to_string(),
        });
    }

    let controller = wait_with_pump(rx).map_err(|err| WvError::WebView2RuntimeMissing {
        detail: err.to_string(),
    })?;
    controller.map_err(|err| WvError::WebView2RuntimeMissing {
        detail: err.to_string(),
    })
}

/// Proof that the `keld-guard` permission handler is registered on a webview.
///
/// [`navigate_initial`] demands it, so "content ran before the guard existed"
/// is a compile error rather than a review catch. Only
/// [`install_guarded_media_permissions`] mints one.
struct GuardInstalled(());

/// Registers the default-deny media-capture handler backed by `keld-guard`
/// (KEL-59 parity with the macOS backend).
///
/// Without a handler `WebView2` falls back to its own **user prompt** —
/// default-ask, not default-deny. A prompt lets a renderer obtain capture the
/// manifest never granted, and the manifest is the authority
/// (`docs/architecture/03-security.md` §1). Kinds without a Keld capability
/// (geolocation, notifications, …) are denied outright: there is no v0
/// capability for them, and fail-closed is the crate rule.
fn install_guarded_media_permissions(
    webview: &ICoreWebView2,
    manifest: PermissionsManifest,
    principal: Principal,
) -> Result<GuardInstalled, WvError> {
    // Built outside the registration's `unsafe` block so the COM calls inside
    // the callback carry their own SAFETY proofs instead of inheriting one
    // lexically.
    let handler = PermissionRequestedEventHandler::create(Box::new(move |_, args| {
        // Fail closed: without args no state can be set, and `Ok(())` would
        // silently hand the decision back to WebView2's own prompt
        // (default-ask). An error at least refuses to report success.
        let Some(args) = args else {
            return Err(windows::core::Error::from(E_POINTER));
        };

        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
        // SAFETY: `args` is live for the duration of the callback; the
        // out-pointer is valid.
        unsafe { args.PermissionKind(&raw mut kind) }?;

        let state =
            if media_permission_allowed(&manifest, Some(principal), webview2_media_kind(kind)) {
                COREWEBVIEW2_PERMISSION_STATE_ALLOW
            } else {
                COREWEBVIEW2_PERMISSION_STATE_DENY
            };
        // SAFETY: same liveness as above; `SetState` takes the enum by value.
        unsafe { args.SetState(state) }
    }));

    let mut token = 0_i64;
    // SAFETY: `webview` lives on this thread (module note). The handler runs
    // on this same thread for the webview's whole life — WebView2 raises
    // events on the creating thread.
    let registered = unsafe { webview.add_PermissionRequested(&handler, &raw mut token) };
    registered.map_err(|err| WvError::Webview(format!("permission handler: {err}")))?;
    Ok(GuardInstalled(()))
}

/// Performs the first navigation of a freshly created webview.
///
/// Takes [`GuardInstalled`] so the type system enforces the crate rule that no
/// content runs before the permission guard is registered.
fn navigate_initial(
    webview: &ICoreWebView2,
    _guard: &GuardInstalled,
    target: &NavTarget,
) -> Result<(), WvError> {
    // SAFETY: `webview` lives on this thread; both calls take an HSTRING by
    // reference that outlives the call.
    let loaded = match target {
        NavTarget::Html(html) => unsafe { webview.NavigateToString(&HSTRING::from(html.as_str())) },
        NavTarget::Url(url) => unsafe { webview.Navigate(&HSTRING::from(url.as_str())) },
    };
    loaded.map_err(|err| WvError::Navigate(err.to_string()))
}

fn install_app_navigation_handler(
    webview: &ICoreWebView2,
    events: Sender<AppWindowEvent>,
    ready: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
) -> Result<(), WvError> {
    let handler = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            failed.store(true, Ordering::Release);
            return Ok(());
        };
        // SAFETY: WebView2 invokes this handler on the creating STA and keeps
        // the event args alive for this call. `IsSuccess` writes no caller
        // memory and returns its BOOL by value.
        let mut succeeded = BOOL::default();
        // SAFETY: `succeeded` is a live BOOL out-parameter for this call.
        unsafe { args.IsSuccess(&raw mut succeeded)? };
        if succeeded.as_bool() {
            if !ready.swap(true, Ordering::AcqRel) {
                let _ = events.send(AppWindowEvent::NavigationReady);
            }
        } else {
            failed.store(true, Ordering::Release);
        }
        Ok(())
    }));
    let mut token = 0_i64;
    // SAFETY: `webview` and the callback are created and registered on the
    // engine STA. WebView2 retains the handler for the webview lifetime.
    unsafe { webview.add_NavigationCompleted(&handler, &raw mut token) }
        .map_err(|error| WvError::Webview(format!("navigation handler: {error}")))
}

/// One live webview and the host window it fills (v0: one per window).
struct View {
    window: Window,
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
}

impl View {
    /// Sizes the controller to fill the window's client area.
    ///
    /// Best-effort by design: a transient failure during a live resize must
    /// not tear down the event loop, and the next resize event re-applies the
    /// correct bounds anyway.
    fn fit_controller(&self, size: PhysicalSize<u32>) {
        let rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(size.width).unwrap_or(i32::MAX),
            bottom: i32::try_from(size.height).unwrap_or(i32::MAX),
        };
        // SAFETY: controller was created on this thread and `RECT` is plain
        // data passed by value.
        let _ = unsafe { self.controller.SetBounds(rect) };
    }
}

impl Drop for View {
    fn drop(&mut self) {
        // SAFETY: dropped on the engine thread (module note). `Close` releases
        // the browser-side resources deterministically instead of waiting for
        // COM refcounts.
        let _ = unsafe { self.controller.Close() };
    }
}

/// The Windows [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WebView2Engine::run_until_closed`] consumes
/// it. Uses tao `run_return` so the host can reap supervised children after the last
/// window closes (KEL-30 concurrent hello app-link).
pub struct WebView2Engine {
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<AppWindowCommand>>,
    /// One environment per engine: every webview shares its profile directory
    /// and browser process (`learn.microsoft.com`, `WebView2` process model).
    environment: ICoreWebView2Environment,
    views: BTreeMap<u32, View>,
    next_id: u32,
    pending_app_events: Option<Sender<AppWindowEvent>>,
    navigation_ready: Arc<AtomicBool>,
    navigation_failed: Arc<AtomicBool>,
}

impl fmt::Debug for WebView2Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebView2Engine")
            .field("views", &self.views.len())
            .field("running", &self.event_loop.is_none())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl WebView2Engine {
    /// Creates the engine, its event loop, and the shared environment.
    ///
    /// Must be called on the process main thread: tao pumps the Win32 message
    /// loop, and `WebView2` objects are affine to the thread that created
    /// them.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::WebView2RuntimeMissing`] when the Evergreen runtime
    /// is not installed, so callers fail with install guidance before a window
    /// ever appears.
    pub fn new() -> Result<Self, WvError> {
        // Probe first: a missing runtime is a user-fixable setup problem, and
        // reporting it before any window exists avoids a flash of empty chrome.
        runtime_version()?;
        // The event loop comes before any COM object: tao declares the
        // process DPI awareness in `EventLoop::new`, and that must precede
        // window (and WebView2 helper-window) creation.
        let event_loop = EventLoopBuilder::with_user_event().build();
        // SAFETY: first COM init on this thread, or an S_FALSE no-op if tao's
        // OLE init already made it an STA — both fine, so the result is
        // deliberately ignored (the pattern wry uses for the same call).
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let environment = create_environment()?;
        Ok(Self {
            event_loop: Some(event_loop),
            environment,
            views: BTreeMap::new(),
            next_id: 1,
            pending_app_events: None,
            navigation_ready: Arc::new(AtomicBool::new(false)),
            navigation_failed: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Runs the event loop until the user closes the last window, then
    /// returns so the caller can tear down host-owned app-link state.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::EventLoop`] if the run loop was already started, or if
    /// tao reports a non-zero `run_return` status.
    pub fn run_until_closed(mut self) -> Result<(), WvError> {
        let Some(mut event_loop) = self.event_loop.take() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; call run_until_closed once",
            )));
        };
        let mut views = std::mem::take(&mut self.views);
        let code = event_loop.run_return(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                // Keld drives the controller size itself — there is no wry
                // WM_SIZE subclass anymore, and tao already delivers resizes
                // (DPI changes arrive as a scale-factor event followed by this
                // same `Resized`).
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    for view in views.values() {
                        if view.window.id() == window_id {
                            view.fit_controller(size);
                        }
                    }
                }
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    // v0 hello is one window. Drop by id so a second window
                    // would not tear down every view; exit when the map is
                    // empty. On Windows, closing the last window quits — the
                    // platform convention, and what KEL-57 V1-10 will assert.
                    views.retain(|_, view| view.window.id() != window_id);
                    if views.is_empty() {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                _ => {}
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
    /// Returns [`WvError`] when the window, `WebView2` controller, guarded
    /// permission hook, navigation callback, or initial navigation fails.
    pub fn create_app(
        &mut self,
        spec: &WebviewSpec,
        events: Sender<AppWindowEvent>,
    ) -> Result<WebviewId, WvError> {
        if self.pending_app_events.is_some() {
            return Err(WvError::EventLoop(String::from(
                "an app window creation is already pending",
            )));
        }
        self.navigation_ready.store(false, Ordering::Release);
        self.navigation_failed.store(false, Ordering::Release);
        self.pending_app_events = Some(events);
        let result = self.create(spec);
        self.pending_app_events = None;
        result
    }

    /// Runs the Windows UI loop until Quit or a fatal app-session command.
    ///
    /// Commands cross tao's [`EventLoopProxy`], so I/O threads never mutate
    /// the HWND or `WebView2` controller directly. Closing the last window emits
    /// [`AppWindowEvent::LastWindowClosed`] and waits for the app's Quit.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::Navigate`] when initial navigation fails or exceeds
    /// its deadline, and [`WvError::EventLoop`] for duplicate run, fatal
    /// session command, or a non-zero tao exit.
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
        let terminal_intent = Arc::new(AtomicBool::new(false));
        let bridge = spawn_app_wake_bridge(
            commands,
            proxy,
            Arc::clone(&stop_bridge),
            Arc::clone(&terminal_intent),
        )?;
        let fatal = Arc::new(AtomicBool::new(false));
        let fatal_in_loop = Arc::clone(&fatal);
        let navigation_timed_out = Arc::new(AtomicBool::new(false));
        let navigation_timed_out_in_loop = Arc::clone(&navigation_timed_out);
        let navigation_ready = Arc::clone(&self.navigation_ready);
        let navigation_failed = Arc::clone(&self.navigation_failed);
        let navigation_deadline = Instant::now() + INITIAL_NAVIGATION_DEADLINE;
        let terminal_intent_in_loop = Arc::clone(&terminal_intent);
        let mut views = std::mem::take(&mut self.views);
        let code = event_loop.run_return(move |event, _, control_flow| {
            let terminal = matches!(
                event,
                Event::UserEvent(AppWindowCommand::Quit | AppWindowCommand::Fatal)
            ) || terminal_intent_in_loop.load(Ordering::Acquire);
            if navigation_failed.load(Ordering::Acquire) {
                views.clear();
                *control_flow = ControlFlow::Exit;
                return;
            }
            if navigation_ready.load(Ordering::Acquire) {
                *control_flow = ControlFlow::Wait;
            } else if !terminal && Instant::now() >= navigation_deadline {
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
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    for view in views.values() {
                        if view.window.id() == window_id {
                            view.fit_controller(size);
                        }
                    }
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
        finish_app_run(
            self.navigation_failed.load(Ordering::Acquire),
            navigation_timed_out.load(Ordering::Acquire),
            fatal.load(Ordering::Acquire),
            code,
        )
    }

    fn view(&self, id: WebviewId) -> Result<&View, WvError> {
        self.views
            .get(&id.0)
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

fn finish_app_run(
    navigation_failed: bool,
    navigation_timed_out: bool,
    fatal: bool,
    code: i32,
) -> Result<(), WvError> {
    if navigation_failed {
        return Err(WvError::Navigate(String::from(
            "initial renderer navigation failed",
        )));
    }
    if navigation_timed_out {
        return Err(WvError::Navigate(String::from(
            "initial renderer navigation did not finish before the startup deadline",
        )));
    }
    if fatal {
        return Err(WvError::EventLoop(String::from(
            "primary app session failed while the Windows event loop was live",
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

fn spawn_app_wake_bridge(
    commands: Receiver<AppWindowCommand>,
    proxy: EventLoopProxy<AppWindowCommand>,
    stop: Arc<AtomicBool>,
    terminal_intent: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, WvError> {
    thread::Builder::new()
        .name("keld-wv-windows-app-wake".to_owned())
        .spawn(move || {
            loop {
                match commands.recv_timeout(Duration::from_millis(100)) {
                    Ok(command) => {
                        let terminal =
                            matches!(command, AppWindowCommand::Quit | AppWindowCommand::Fatal);
                        if terminal {
                            terminal_intent.store(true, Ordering::Release);
                        }
                        let _ = proxy.send_event(command);
                        if terminal {
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

impl WebEngine for WebView2Engine {
    fn create(&mut self, spec: &WebviewSpec) -> Result<WebviewId, WvError> {
        let Some(event_loop) = self.event_loop.as_ref() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; create webviews before run_until_closed",
            )));
        };
        let window = WindowBuilder::new()
            .with_title(&spec.title)
            // Logical (DPI-independent) points. tao declares per-monitor-v2 DPI
            // awareness for the process, so this is scaled by the monitor's
            // factor rather than bitmap-stretched by the OS.
            .with_inner_size(tao::dpi::LogicalSize::new(
                spec.size.width,
                spec.size.height,
            ))
            // KEL-25/KEL-27: standard Windows caption — minimize, maximize, close.
            .with_resizable(true)
            .with_minimizable(true)
            .with_closable(true)
            .build(event_loop)
            .map_err(|e| WvError::Window(e.to_string()))?;

        let hwnd = HWND(window.hwnd() as _);
        let controller = create_controller(&self.environment, hwnd)?;
        // SAFETY: controller was created on this thread a moment ago.
        let webview = unsafe { controller.CoreWebView2() }
            .map_err(|err| WvError::Webview(format!("controller returned no webview: {err}")))?;
        // Into a `View` before any further fallible step: its `Drop` closes the
        // controller, so an early `?` return below cannot leak the browser-side
        // resources behind a half-built webview.
        let view = View {
            window,
            controller,
            webview,
        };

        // Guard before content: nothing may load until default-deny is wired.
        // Empty manifest → deny everything (KEL-59). KEL-73: mint the webview
        // id first so capture cannot inherit AppProcess grants.
        let id = self.next_id;
        let guard = install_guarded_media_permissions(
            &view.webview,
            PermissionsManifest::default(),
            webview_media_principal(WebviewId(id)),
        )?;

        // Devtools follow the build profile, like the macOS backend: wired in
        // debug, off in release until keld-guard owns `web.devtools`. WebView2
        // defaults them to on, so release must opt out explicitly.
        // SAFETY: settings object is used and dropped on this thread.
        let devtools = unsafe {
            view.webview
                .Settings()
                .and_then(|settings| settings.SetAreDevToolsEnabled(cfg!(debug_assertions)))
        };
        devtools.map_err(|err| WvError::Webview(format!("devtools setting: {err}")))?;

        // KEL-62: bounds before navigation. WebView2 starts with a zero-sized
        // controller, and Chromium does not composite a zero-sized surface —
        // measured as ~640 ms of dead time when the size arrived only from a
        // later window event. This is create-time wiring, so a failure here is
        // a real error, unlike the best-effort live resizes.
        let size = view.window.inner_size();
        let rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(size.width).unwrap_or(i32::MAX),
            bottom: i32::try_from(size.height).unwrap_or(i32::MAX),
        };
        // SAFETY: controller lives on this thread; `RECT` is plain data, and
        // `SetIsVisible` takes a BOOL by value.
        let shown = unsafe {
            view.controller
                .SetBounds(rect)
                .and_then(|()| view.controller.SetIsVisible(true))
        };
        shown.map_err(|err| WvError::Webview(format!("initial bounds: {err}")))?;

        if let Some(events) = self.pending_app_events.as_ref() {
            install_app_navigation_handler(
                &view.webview,
                events.clone(),
                Arc::clone(&self.navigation_ready),
                Arc::clone(&self.navigation_failed),
            )?;
        }

        navigate_initial(&view.webview, &guard, &spec.initial)?;

        // Keyboard focus lands in the page, matching what wry's build did and
        // what a single-webview window should do.
        // SAFETY: controller lives on this thread; best-effort like a resize.
        let _ = unsafe {
            view.controller
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
        };

        self.next_id += 1;
        self.views.insert(id, view);
        Ok(WebviewId(id))
    }

    fn navigate(&mut self, id: WebviewId, target: NavTarget) -> Result<(), WvError> {
        let view = self.view(id)?;
        // The guard handler registered at create time lives for the webview's
        // whole life, so later navigations need no new proof.
        // SAFETY: webview lives on this thread; HSTRINGs outlive the calls.
        let loaded = match target {
            NavTarget::Html(html) => unsafe {
                view.webview.NavigateToString(&HSTRING::from(html.as_str()))
            },
            NavTarget::Url(url) => unsafe { view.webview.Navigate(&HSTRING::from(url.as_str())) },
        };
        loaded.map_err(|err| WvError::Navigate(err.to_string()))
    }

    fn eval(&mut self, id: WebviewId, script: &str) -> Result<(), WvError> {
        let view = self.view(id)?;
        // Fire-and-forget per the trait contract: the completion handler only
        // satisfies the COM signature. Result plumbing belongs to the
        // command-queue design (engine.rs module docs).
        // SAFETY: webview lives on this thread; the handler is dropped after
        // the script completes on this same thread.
        let queued = unsafe {
            view.webview.ExecuteScript(
                &HSTRING::from(script),
                &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))),
            )
        };
        queued.map_err(|err| WvError::Script(err.to_string()))
    }

    fn set_bounds(&mut self, id: WebviewId, rect: Rect) -> Result<(), WvError> {
        let view = self.view(id)?;
        // v0 webviews fill their host window, so bounds apply to the window…
        view.window
            .set_outer_position(tao::dpi::LogicalPosition::new(rect.x, rect.y));
        view.window
            .set_inner_size(tao::dpi::LogicalSize::new(rect.width, rect.height));
        // …and the controller follows immediately. Before the run loop starts
        // no resize event fires, so waiting for one would leave a stale
        // controller size; `inner_size` reads back what the move actually
        // produced (the OS may clamp).
        view.fit_controller(view.window.inner_size());
        Ok(())
    }

    fn devtools(&mut self, id: WebviewId, action: DevtoolsAction) -> Result<(), WvError> {
        let view = self.view(id)?;
        match action {
            // SAFETY: webview lives on this thread.
            DevtoolsAction::Open => unsafe { view.webview.OpenDevToolsWindow() }
                .map_err(|err| WvError::Webview(format!("devtools open: {err}"))),
            // WebView2 has no close-devtools API (wry no-ops here too); the
            // window is the user's to close. Not an error: the contract is
            // "the inspector is not open afterwards because of us".
            DevtoolsAction::Close => Ok(()),
        }
    }

    fn destroy(&mut self, id: WebviewId) -> Result<(), WvError> {
        // Dropping the `View` closes the controller, then the window.
        self.views
            .remove(&id.0)
            .map(|_| ())
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

impl WebView2EngineExt for WebView2Engine {}

/// Opens a window from `spec` and runs until the user closes it.
///
/// Thin wrapper for the Phase 1 hello slice: probes the runtime, builds a
/// [`WebView2Engine`], creates one webview, and hands the thread to the run
/// loop.
///
/// # Errors
///
/// Returns [`WvError::WebView2RuntimeMissing`] if the Evergreen runtime is
/// absent, or another [`WvError`] if window or webview creation fails.
pub fn run_hello(spec: &WebviewSpec) -> Result<(), WvError> {
    let mut engine = WebView2Engine::new()?;
    engine.create(spec)?;
    engine.run_until_closed()
}

#[cfg(test)]
mod tests {
    use super::{WebView2Engine, runtime_version};
    use crate::error::WvError;

    /// The CI runners and this developer machine both ship the Evergreen
    /// runtime, so the probe must succeed and return a dotted version. If it
    /// ever fails the message has to name the code and the fix, because that is
    /// the only thing a user sees when their runtime is missing.
    #[test]
    fn runtime_probe_reports_version_or_actionable_error() {
        match runtime_version() {
            Ok(version) => {
                assert!(
                    version.split('.').count() >= 2,
                    "expected a dotted WebView2 version, got: {version}"
                );
            }
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    matches!(err, WvError::WebView2RuntimeMissing { .. }),
                    "probe must fail as WebView2RuntimeMissing, got: {msg}"
                );
                assert!(msg.contains("KELD-WV-008"), "missing code in: {msg}");
                assert!(msg.contains("Evergreen Runtime"), "missing fix in: {msg}");
            }
        }
    }

    /// Keeps the engine type named from the test module so a rename fails the
    /// build here rather than silently orphaning these tests. Constructing one
    /// needs a tao `EventLoop`, which must be built on the process main thread —
    /// the harness gives each test its own thread, so the engine itself is
    /// exercised by the GUI pass in KEL-27, not here.
    fn _assert_engine_type(_: Option<&WebView2Engine>) {}

    /// A stale id must stay a typed error on this backend too (no panics in
    /// libs, per root `AGENTS.md`).
    #[test]
    fn unknown_webview_id_is_typed_not_panic() {
        let err = WvError::UnknownWebview { id: 3 };
        assert!(err.to_string().contains("KELD-WV-007"));
    }

    /// KEL-63: the profile must be per-user, not beside the executable.
    ///
    /// The loader default is `<exe>.WebView2\` next to the binary, which a
    /// standard user cannot create under `C:\Program Files\`. This asserts we
    /// are not on that default and that the directory is user-scoped.
    #[test]
    fn profile_dir_is_user_scoped_not_next_to_the_exe() {
        let dir = super::user_data_dir();
        assert!(
            dir.ends_with(super::PROFILE_IDENTIFIER),
            "profile must be namespaced by identifier, got: {}",
            dir.display()
        );

        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        if let Some(exe_dir) = exe_dir {
            assert!(
                !dir.starts_with(&exe_dir),
                "KEL-63: profile lands beside the executable ({}); \
                 that path is read-only under Program Files",
                dir.display()
            );
        }

        // Whatever %LOCALAPPDATA% is, the path has to be absolute or WebView2
        // resolves it against the process CWD, which `keld dev` changes.
        assert!(
            dir.is_absolute(),
            "profile dir must be absolute, got: {}",
            dir.display()
        );
    }

    /// KEL-66: the environment options this backend ships must not carry
    /// wry's SmartScreen-disabling browser arguments. `webview2-com` defaults
    /// to an empty argument string; if that default ever changes, or someone
    /// adds `--disable-features` here, this fails.
    #[test]
    fn environment_options_do_not_disable_smartscreen() {
        let options = webview2_com::CoreWebView2EnvironmentOptions::default();
        // SAFETY: reading the freshly constructed options object on this
        // thread; no COM aggregate exists yet.
        let args = unsafe { options.additional_browser_arguments() };
        assert!(
            args.is_empty(),
            "expected no default browser arguments, got: {args}"
        );
        // The options setter is the only way to hand the browser extra flags;
        // the module docs quote wry's old flag string, so scan for the API
        // call rather than the flag text.
        let src = include_str!("mod.rs");
        assert_eq!(
            src.matches("set_additional_browser_arguments").count(),
            1,
            "KEL-66: this backend must not pass browser arguments \
             (the one hit is this test's own scan string)"
        );
    }
}
