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

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::NSWindow;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::Window;
use wry::WebViewExtMacOS;

use crate::WebviewId;
pub use crate::engine::AppReadyTrigger;
pub use crate::engine::{AppWindowCommand, AppWindowEvent};
use crate::engine::{
    DevtoolsAction, NavTarget, Rect, WebEngine, WebviewSpec, WkWebViewEngineExt,
    first_host_init_turn,
};
use crate::error::WvError;
use crate::profile::{
    EphemeralProfile, ProfileError, ProfileErrorKind, ProfileIdentity, WebProfileSelection,
};
use crate::startup::{PageLoad, StartupPhase, StartupTrace, trace_enabled};

mod macos_bridge;
mod macos_profile;
mod managed_windows;

use macos_bridge::{BridgeNavigationHandle, MacRendererBridge};
pub use macos_bridge::{RendererBridgeEndpoint, RendererBridgeOutcome, RendererBridgeRequest};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use macos_profile::MacMediaPromptProbe;
use macos_profile::MacProfileOwner;

/// One live webview and the host window it fills (v0: one per window).
#[repr(C)]
struct View {
    webview: wry::WebView,
    window: Window,
    // Build provenance only: legacy initial renderers remain outside the Core
    // registry, and must retain their existing LastWindowClosed close path.
    registry_managed: bool,
}

enum WkUserEvent {
    App(AppWindowCommand),
    Renderer(RendererBridgeOutcome),
}

/// The macOS [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WkWebViewEngine::run_until_closed`]
/// consumes it. Uses tao `run_return` so the host can reap supervised children
/// after the last window closes (KEL-30 concurrent hello app-link).
pub struct WkWebViewEngine {
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<WkUserEvent>>,
    views: BTreeMap<u32, View>,
    renderer_bridges: BTreeMap<u32, MacRendererBridge>,
    renderer_outcomes: Option<Receiver<RendererBridgeOutcome>>,
    next_id: u32,
    profile: MacProfileOwner,
    profile_test_media_seed_allow: bool,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    media_prompt_probe: Option<MacMediaPromptProbe>,
    /// KEL-62: navigation completion (`PageLoadEvent::Finished`), not window create.
    startup: Arc<Mutex<StartupTrace>>,
}

impl fmt::Debug for WkWebViewEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WkWebViewEngine")
            .field("views", &self.views.len())
            .field("running", &self.event_loop.is_none())
            .field("profile_mode", &self.profile.mode_name())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl WkWebViewEngine {
    /// Creates the engine with one host-selected persistent or ephemeral store.
    ///
    /// Must be called on the process main thread — `AppKit`'s platform
    /// contract, enforced by tao (which aborts otherwise).
    /// # Errors
    ///
    /// Returns [`WvError::ProfileSelection`] when identity, UUID binding,
    /// lifecycle, ownership, or the macOS persistent-store version gate fails.
    pub fn new(selection: WebProfileSelection) -> Result<Self, WvError> {
        Self::new_with_profile_test_media(selection, false)
    }

    /// Creates the private real-OS profile fixture with one explicit test-only
    /// media seed override. This symbol is absent from normal builds.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::ProfileSelection`] when the selected profile cannot
    /// be acquired or verified.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub fn new_profile_test_fixture(
        selection: WebProfileSelection,
        seed_media_allow: bool,
    ) -> Result<Self, WvError> {
        let mut engine = Self::new_with_profile_test_media(selection, seed_media_allow)?;
        engine.media_prompt_probe = Some(MacMediaPromptProbe::new()?);
        Ok(engine)
    }

    fn new_with_profile_test_media(
        selection: WebProfileSelection,
        profile_test_media_seed_allow: bool,
    ) -> Result<Self, WvError> {
        let startup = Arc::new(Mutex::new(StartupTrace::new()));
        let mut event_loop = EventLoopBuilder::with_user_event().build();
        let profile = MacProfileOwner::new(selection, &mut event_loop)?;
        mark_startup(&startup, StartupPhase::EventLoop);
        Ok(Self {
            event_loop: Some(event_loop),
            views: BTreeMap::new(),
            renderer_bridges: BTreeMap::new(),
            renderer_outcomes: None,
            next_id: 1,
            profile,
            profile_test_media_seed_allow,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            media_prompt_probe: None,
            startup,
        })
    }

    /// Creates an explicitly ephemeral engine for a standalone development window.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::ProfileSelection`] if OS randomness or the
    /// nonpersistent `WebKit` store is unavailable.
    pub fn new_dev_ephemeral() -> Result<Self, WvError> {
        let mut launch_nonce = [0_u8; 32];
        getrandom::fill(&mut launch_nonce).map_err(|_| {
            WvError::from(ProfileError::platform_failure(
                ProfileErrorKind::InvalidEphemeralNonce,
            ))
        })?;
        let ephemeral = EphemeralProfile::from_host_random(launch_nonce)?;
        Self::new(WebProfileSelection::ephemeral_dev(ephemeral))
    }

    /// Removes only the persistent store for one Core-verified app identity.
    ///
    /// The caller must run this in a clean signed host process after every
    /// process that used the profile has exited. `WebKit`'s completion callback
    /// and a follow-up identifier enumeration are required before Keld clears
    /// its exact-identity binding records.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::ProfileSelection`] when the identity binding is
    /// invalid, the store is still in use, `WebKit` removal fails, or recovery
    /// metadata cannot prove the exact purge target.
    pub fn purge_persistent_profile(identity: ProfileIdentity) -> Result<(), WvError> {
        let mut event_loop = EventLoopBuilder::with_user_event().build();
        MacProfileOwner::purge_persistent_profile(identity, &mut event_loop)
    }

    /// Checks a persistent store identifier in the current app's `WebKit` registry.
    ///
    /// This read-only fixture hook initializes `WebKit` with a memory-only store and
    /// enumerates identifiers. It never opens or creates the requested persistent
    /// store.
    ///
    /// # Errors
    ///
    /// Returns an error when `WebKit` cannot enumerate the current app's store
    /// identifiers or the macOS lifecycle cannot be proven.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub fn persistent_store_identifier_present_for_test(
        identifier: [u8; 16],
    ) -> Result<bool, WvError> {
        let mut event_loop = EventLoopBuilder::with_user_event().build();
        MacProfileOwner::persistent_store_identifier_present_for_test(identifier, &mut event_loop)
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
        let profile_result = self.profile.clean_shutdown();
        match self.startup.lock() {
            Ok(guard) => guard.emit_if_nav_never_finished(),
            Err(poisoned) => poisoned.into_inner().emit_if_nav_never_finished(),
        }
        let loop_result = if code == 0 {
            Ok(())
        } else {
            Err(WvError::EventLoop(format!(
                "event loop exited with status {code}"
            )))
        };
        match (loop_result, profile_result) {
            (Ok(()), result) => result,
            (Err(primary), Ok(())) => Err(primary),
            (Err(primary), Err(cleanup)) => Err(WvError::EventLoop(format!(
                "{primary}; profile cleanup failed: {cleanup}"
            ))),
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
        self.create_internal(spec, Some(events), None)
    }

    /// Creates the initial app window with the approved KEL-142 renderer bridge.
    ///
    /// # Errors
    ///
    /// Returns a typed webview/bridge error when the isolated-world bridge
    /// cannot be installed before first navigation.
    pub fn create_app_with_renderer_bridge(
        &mut self,
        spec: &WebviewSpec,
        events: Sender<AppWindowEvent>,
        endpoint: RendererBridgeEndpoint,
    ) -> Result<WebviewId, WvError> {
        if self.renderer_outcomes.is_some() || !self.renderer_bridges.is_empty() {
            return Err(WvError::RendererBridge {
                detail: String::from("this minimum slice supports one bridged WebView"),
            });
        }
        self.create_internal(spec, Some(events), Some(endpoint))
    }

    /// Runs the live macOS event loop until a Quit or fatal app-session command.
    ///
    /// `commands` is bridged through tao's [`tao::event_loop::EventLoopProxy`],
    /// so I/O threads never touch a window handle and the UI thread does not
    /// poll. Closing the last window emits [`AppWindowEvent::LastWindowClosed`]
    /// but keeps the event loop alive until the app sends Quit. The engine is
    /// borrowed so its profile owner and exclusive locks outlive Core's ordered
    /// app-link and child cleanup after this method returns.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::EventLoop`] if the loop already ran, the bridge/UI
    /// reports a fatal session command, or tao exits non-zero.
    pub fn run_app_until_quit(
        &mut self,
        commands: Receiver<AppWindowCommand>,
        events: Sender<AppWindowEvent>,
    ) -> Result<(), WvError> {
        let mut unproved = false;
        self.run_app_until_quit_with_main_owner(
            commands,
            events,
            None,
            AppReadyTrigger::NavigationReady,
            &mut |_, _, _| Ok((true, None)),
            (&mut unproved, || {}),
        )
    }

    /// Runs the same application loop with a main-owned native presentation service.
    ///
    /// `anchor` is the host-created view, never a caller-supplied selector.
    /// The resolver returns its actual `AppKit` window through wry's safe accessor.
    /// `service` receives the main-thread marker, resolver, and stop intent; it
    /// returns (actual owner drained, next original absolute deadline), or an
    /// owned-custody failure. `retains_main_custody` is set on that failed cut,
    /// requiring the caller to retain this engine and the concrete service.
    /// A pending
    /// owner keeps its view and the main loop alive through terminal commands.
    ///
    /// # Errors
    ///
    /// Returns the existing typed loop/navigation/profile errors. Native owner
    /// faults remain with Core's existing fatal owner; they do not authorize
    /// view destruction or create a successful drain receipt.
    #[allow(
        clippy::too_many_lines,
        reason = "one UI-loop owner keeps terminal, window, and renderer wake ordering contiguous"
    )]
    pub fn run_app_until_quit_with_main_owner<F>(
        &mut self,
        commands: Receiver<AppWindowCommand>,
        events: Sender<AppWindowEvent>,
        anchor: Option<WebviewId>,
        ready_trigger: AppReadyTrigger,
        service: &mut F,
        // Keep the main-custody result paired with the loop-entry clock handoff.
        // Neither moves its underlying AppKit/profile owner to another thread.
        startup: (&mut bool, impl FnOnce()),
    ) -> Result<(), WvError>
    where
        F: FnMut(
            MainThreadMarker,
            &mut dyn FnMut() -> Option<Retained<NSWindow>>,
            bool,
        ) -> Result<(bool, Option<Instant>), WvError>,
    {
        let (retains_main_custody, on_loop_entry) = startup;
        *retains_main_custody = false;
        let Some(main_thread) = MainThreadMarker::new() else {
            return Err(WvError::EventLoop(String::from(
                "presentation requires the main thread",
            )));
        };
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
        let fatal_outcome = Arc::new(AtomicBool::new(false));
        let fatal_for_bridge = Arc::clone(&fatal_outcome);
        let bridge = spawn_app_wake_bridge(
            commands,
            proxy.clone(),
            stop_for_bridge,
            terminal_intent_for_bridge,
            fatal_for_bridge,
        )?;
        let renderer_bridge = self
            .renderer_outcomes
            .take()
            .map(|outcomes| {
                spawn_renderer_outcome_bridge(outcomes, proxy, Arc::clone(&stop_bridge))
            })
            .transpose()?;
        let navigation_timed_out = Arc::new(AtomicBool::new(false));
        let navigation_timed_out_in_loop = Arc::clone(&navigation_timed_out);
        let navigation_deadline = Instant::now() + crate::INITIAL_NAVIGATION_DEADLINE;
        let startup_in_loop = Arc::clone(&self.startup);
        let terminal_intent_in_loop = Arc::clone(&terminal_intent);
        let fatal_in_loop = Arc::clone(&fatal_outcome);
        let mut views = std::mem::take(&mut self.views);
        let mut renderer_bridges = std::mem::take(&mut self.renderer_bridges);
        let mut terminal_requested = false;
        let mut closing = Vec::new();
        let mut main_owner_failure = None;
        let mut host_init_emitted = false;
        let profile = &self.profile;
        let next_id = &mut self.next_id;
        let media_seed_allow = self.profile_test_media_seed_allow;
        // Arm the host's external Init watchdog only after loop setup has succeeded.
        // The watchdog is independent of Tao's pre-Init proxy/waker availability.
        on_loop_entry();
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if ready_trigger == AppReadyTrigger::HostInitialized
            && std::env::var_os("KELD_T1B_STALL_BEFORE_INIT").is_some()
        {
            // Private child-only failure control. The coordinator must terminate
            // this process: the main thread cannot service a proxy or unwind.
            std::thread::park();
        }
        let code = event_loop.run_return(|event, target, control_flow| {
            // The existing bridge accepts terminal intent before its proxy event.
            // One terminal state closes renderer dispatch and Native main admission.
            if terminal_intent_in_loop.load(Ordering::Acquire) {
                terminal_requested = true;
                destroy_renderer_bridges(&mut renderer_bridges);
            }
            if ready_trigger == AppReadyTrigger::HostInitialized
                && !terminal_requested
                && first_host_init_turn(&event, &mut host_init_emitted)
            {
                // Capture on the native UI owner before enqueueing: the Core
                // receiver may be descheduled past the absolute first-Init
                // deadline even when this genuine event happened on time.
                let observed_at = Instant::now();
                if events
                    .send(AppWindowEvent::HostInitialized { observed_at })
                    .is_err()
                {
                    fatal_in_loop.store(true, Ordering::Release);
                    terminal_requested = true;
                    destroy_renderer_bridges(&mut renderer_bridges);
                }
            }
            let terminal = is_terminal_app_event(&event) || terminal_requested;
            let live_navigation = views.iter().any(|(id, view)| {
                crate::engine::initial_renderer_pending(view.registry_managed, closing.contains(id))
            });
            if crate::navigation_deadline_expired(
                navigation_finished(&startup_in_loop),
                live_navigation,
                terminal,
                Instant::now(),
                navigation_deadline,
            ) {
                navigation_timed_out_in_loop.store(true, Ordering::Release);
                terminal_requested = true;
                destroy_renderer_bridges(&mut renderer_bridges);
            }
            if !terminal_requested || is_terminal_app_event(&event) {
                match event {
                    Event::LoopDestroyed => {
                        if !terminal_requested {
                            fatal_in_loop.store(true, Ordering::Release);
                        }
                        terminal_requested = true;
                        destroy_renderer_bridges(&mut renderer_bridges);
                    }
                    Event::UserEvent(WkUserEvent::App(AppWindowCommand::Quit)) => {
                        terminal_requested = true;
                        destroy_renderer_bridges(&mut renderer_bridges);
                    }
                    Event::UserEvent(WkUserEvent::App(AppWindowCommand::Fatal)) => {
                        fatal_in_loop.store(true, Ordering::Release);
                        terminal_requested = true;
                        destroy_renderer_bridges(&mut renderer_bridges);
                    }
                    Event::UserEvent(WkUserEvent::App(command)) => {
                        // Debug+feature-gated real-main-thread hostile fixture:
                        // a test-controlled maximization request instead calls
                        // AppKit's genuine `NSWindow.close()` (NOT Tao's
                        // cancelable `performClose:` / AX red-button path).
                        // Default/shipping builds compile out this injection,
                        // and only the dedicated test host opts into it.
                        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                        let test_external_close = if let AppWindowCommand::SetMaximized {
                            view,
                            ..
                        } = command
                        {
                            if std::env::var_os("KELD_T1B_UNEXPECTED_NATIVE_CLOSE").is_some() {
                                if let Some(window) = views
                                    .get(&view.0)
                                    .filter(|view| view.registry_managed)
                                    .and_then(|view| view.webview.webview().window())
                                {
                                    window.close();
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
                        let test_external_close = false;
                        let native = managed_windows::NativeBuild {
                            target,
                            profile,
                            startup: &startup_in_loop,
                            media_seed_allow,
                        };
                        if !test_external_close
                            && let Err(error) = managed_windows::command(
                                command,
                                &native,
                                next_id,
                                &mut views,
                                &mut renderer_bridges,
                                &events,
                            )
                        {
                            main_owner_failure.get_or_insert(error);
                            fatal_in_loop.store(true, Ordering::Release);
                            terminal_requested = true;
                        }
                    }
                    Event::UserEvent(WkUserEvent::Renderer(outcome)) => {
                        let id = outcome.webview().0;
                        let delivered =
                            renderer_bridges
                                .get(&id)
                                .zip(views.get(&id))
                                .map(|(bridge, view)| {
                                    let native = view.webview.webview();
                                    bridge.deliver(&native, outcome)
                                });
                        if delivered.is_some_and(|result| result.is_err()) {
                            fatal_in_loop.store(true, Ordering::Release);
                            terminal_requested = true;
                            destroy_renderer_bridges(&mut renderer_bridges);
                        }
                    }
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::CloseRequested,
                        ..
                    } => {
                        let requested: Vec<u32> = views
                            .iter()
                            .filter_map(|(id, view)| (view.window.id() == window_id).then_some(*id))
                            .collect();
                        let mut legacy_closed = false;
                        for id in requested {
                            if views.get(&id).is_some_and(|view| view.registry_managed) {
                                // Core owns the sole T1 teardown decision. A
                                // late build may already have a DestroyWindow
                                // queued after role retirement; dropping here
                                // as well would race that legitimate command.
                                let close = managed_windows::send(
                                    &events,
                                    AppWindowEvent::WindowCloseRequested {
                                        view: WebviewId(id),
                                    },
                                );
                                if let Err(error) = close {
                                    main_owner_failure.get_or_insert(error);
                                    fatal_in_loop.store(true, Ordering::Release);
                                    terminal_requested = true;
                                }
                                continue;
                            }
                            legacy_closed = true;
                            if let Some(bridge) = renderer_bridges.remove(&id) {
                                bridge.destroy();
                            }
                            if anchor.is_some_and(|view| view.0 == id) {
                                if !closing.contains(&id) {
                                    closing.push(id);
                                }
                            } else {
                                views.remove(&id);
                            }
                        }
                        if legacy_closed && views.is_empty() {
                            let _ = events.send(AppWindowEvent::LastWindowClosed);
                        }
                    }
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::Destroyed,
                        ..
                    } => {
                        // Tao 0.35.3 can report `windowWillClose:` as Destroyed
                        // after an out-of-band NSWindow.close(), *without* a
                        // preceding cancelable CloseRequested. Ordinary Keld
                        // DestroyWindow removes the owned view from `views`
                        // BEFORE its NSWindow drops, so that path is not here.
                        // An observed Destroyed while this registry-managed
                        // view is still present means physical release has
                        // bypassed Core's close veto and T11 tombstone order.
                        // No caller or renderer may fabricate a successful
                        // Released reply or keep a ghost BrowserWindow alive.
                        if let Some((id, _)) = views.iter().find(|(_, view)| {
                            view.registry_managed && view.window.id() == window_id
                        }) {
                            main_owner_failure.get_or_insert_with(|| {
                                WvError::EventLoop(format!(
                                    "unexpected out-of-band native NSWindow close for registry view {id}; fail-closed Host shutdown instead of a ghost window or forged close veto"
                                ))
                            });
                            fatal_in_loop.store(true, Ordering::Release);
                            terminal_requested = true;
                            destroy_renderer_bridges(&mut renderer_bridges);
                        }
                    }
                    Event::WindowEvent {
                        window_id,
                        event:
                            WindowEvent::Moved(_)
                            | WindowEvent::Resized(_)
                            | WindowEvent::Focused(_)
                            | WindowEvent::ScaleFactorChanged { .. },
                        ..
                    } => {
                        for (id, view) in &views {
                            if view.registry_managed
                                && view.window.id() == window_id
                                && let Err(error) =
                                    managed_windows::emit_state(&events, WebviewId(*id), view)
                            {
                                main_owner_failure.get_or_insert(error);
                                fatal_in_loop.store(true, Ordering::Release);
                                terminal_requested = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            let stopping = terminal_requested || anchor.is_some_and(|id| closing.contains(&id.0));
            let (drained, owner_deadline) = {
                let mut resolve = || {
                    anchor
                        .and_then(|id| views.get(&id.0))
                        .and_then(|view| view.webview.webview().window())
                };
                match service(main_thread, &mut resolve, stopping) {
                    Ok(progress) => progress,
                    Err(error) => {
                        main_owner_failure.get_or_insert(error);
                        fatal_in_loop.store(true, Ordering::Release);
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
            };
            if drained && !closing.is_empty() {
                for id in closing.drain(..) {
                    views.remove(&id);
                }
                if views.is_empty() {
                    let _ = events.send(AppWindowEvent::LastWindowClosed);
                }
            }
            if terminal_requested && drained {
                destroy_renderer_bridges(&mut renderer_bridges);
                if let Err(error) =
                    managed_windows::release_all(&mut views, &mut renderer_bridges, &events)
                {
                    main_owner_failure.get_or_insert(error);
                    fatal_in_loop.store(true, Ordering::Release);
                }
                views.clear();
                *control_flow = ControlFlow::Exit;
                return;
            }
            let navigation_wake = (!terminal
                && !navigation_finished(&startup_in_loop)
                && views.iter().any(|(id, view)| {
                    crate::engine::initial_renderer_pending(
                        view.registry_managed,
                        closing.contains(id),
                    )
                }))
            .then_some(navigation_deadline);
            let wake = match (navigation_wake, owner_deadline) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            *control_flow = wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil);
        });
        // Tao drops its stored callback on every return, including external
        // NSApp.run termination. The callback only borrowed these real owners.
        let mut tail_failure = main_owner_failure;
        let mut drained;
        loop {
            let progress = {
                let mut resolve = || {
                    anchor
                        .and_then(|id| views.get(&id.0))
                        .and_then(|view| view.webview.webview().window())
                };
                service(main_thread, &mut resolve, true)
            };
            let progress = match progress {
                Ok(progress) => progress,
                Err(error) => {
                    tail_failure.get_or_insert(error);
                    drained = false;
                    break;
                }
            };
            drained = progress.0;
            if drained {
                break;
            }
            let Some(original_deadline) = progress.1 else {
                tail_failure = Some(WvError::EventLoop(String::from(
                    "native main tail custody remains unproved",
                )));
                break;
            };
            if let Err(error) =
                macos_bridge::service_main_tail_before(main_thread, original_deadline)
            {
                tail_failure = Some(error);
                break;
            }
        }
        if drained {
            destroy_renderer_bridges(&mut renderer_bridges);
            if let Err(error) =
                managed_windows::release_all(&mut views, &mut renderer_bridges, &events)
            {
                tail_failure.get_or_insert(error);
            }
            views.clear();
        } else {
            *retains_main_custody = true;
            self.views = views;
            self.renderer_bridges = renderer_bridges;
        }
        stop_bridge.store(true, Ordering::Release);
        let _ = bridge.join();
        if let Some(renderer_bridge) = renderer_bridge {
            let _ = renderer_bridge.join();
        }
        if !drained {
            // Returning this failure is not a drain receipt. Core still owns
            // the borrowed service and this engine's retained actual views.
            return Err(tail_failure.unwrap_or_else(|| {
                WvError::EventLoop(String::from(
                    "UI loop returned while the native presentation owner remains active",
                ))
            }));
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(probe) = self.media_prompt_probe.take() {
            probe.finish();
        }
        let profile_result = self.profile.clean_shutdown();
        let loop_result = tail_failure.map_or_else(
            || {
                app_loop_result(
                    navigation_timed_out.load(Ordering::Acquire),
                    fatal_outcome.load(Ordering::Acquire),
                    code,
                )
            },
            Err,
        );
        let result = match (loop_result, profile_result) {
            (Ok(()), result) => result,
            (Err(primary), Ok(())) => Err(primary),
            (Err(primary), Err(cleanup)) => Err(WvError::EventLoop(format!(
                "{primary}; profile cleanup failed: {cleanup}"
            ))),
        };
        // Consume the owned event sender at the original method-return boundary.
        drop(events);
        result
    }

    fn create_internal(
        &mut self,
        spec: &WebviewSpec,
        app_events: Option<Sender<AppWindowEvent>>,
        renderer_endpoint: Option<RendererBridgeEndpoint>,
    ) -> Result<WebviewId, WvError> {
        let result = self.create_internal_inner(spec, app_events, renderer_endpoint);
        if let Err(primary) = result {
            if self.views.is_empty()
                && let Err(cleanup) = self.profile.clean_shutdown()
            {
                return Err(WvError::EventLoop(format!(
                    "{primary}; profile rollback failed: {cleanup}"
                )));
            }
            return Err(primary);
        }
        result
    }

    fn create_internal_inner(
        &mut self,
        spec: &WebviewSpec,
        app_events: Option<Sender<AppWindowEvent>>,
        renderer_endpoint: Option<RendererBridgeEndpoint>,
    ) -> Result<WebviewId, WvError> {
        let Some(event_loop) = self.event_loop.as_ref() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; create webviews before running it",
            )));
        };
        let webview_id = managed_windows::next_view_id(&mut self.next_id)?;
        let (view, installed) = managed_windows::NativeBuild {
            target: event_loop,
            profile: &self.profile,
            startup: &self.startup,
            media_seed_allow: self.profile_test_media_seed_allow,
        }
        .build(webview_id, spec, app_events, renderer_endpoint)
        .map_err(|failure| failure.source)?;
        if let Some(installed) = installed {
            self.renderer_outcomes = Some(installed.outcomes);
            self.renderer_bridges.insert(webview_id.0, installed.bridge);
        }
        self.views.insert(webview_id.0, view);
        Ok(webview_id)
    }

    fn view(&self, id: WebviewId) -> Result<&View, WvError> {
        self.views
            .get(&id.0)
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

impl WebEngine for WkWebViewEngine {
    fn create(&mut self, spec: &WebviewSpec) -> Result<WebviewId, WvError> {
        self.create_internal(spec, None, None)
    }

    fn navigate(&mut self, id: WebviewId, target: NavTarget) -> Result<(), WvError> {
        if let Some(bridge) = self.renderer_bridges.get(&id.0) {
            bridge.navigation_handle().navigation_started();
        }
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
        if let Some(bridge) = self.renderer_bridges.remove(&id.0) {
            bridge.destroy();
        }
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
    proxy: EventLoopProxy<WkUserEvent>,
    stop: Arc<AtomicBool>,
    terminal_intent: Arc<AtomicBool>,
    fatal_outcome: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, WvError> {
    thread::Builder::new()
        .name("keld-wv-macos-app-wake".to_owned())
        .spawn(move || {
            run_app_wake_bridge(
                &commands,
                &stop,
                &terminal_intent,
                &fatal_outcome,
                |command| {
                    let _ = proxy.send_event(WkUserEvent::App(command));
                },
            );
        })
        .map_err(|error| WvError::EventLoop(format!("failed to start app wake bridge: {error}")))
}

fn run_app_wake_bridge(
    commands: &Receiver<AppWindowCommand>,
    stop: &AtomicBool,
    terminal_intent: &AtomicBool,
    fatal_outcome: &AtomicBool,
    mut forward: impl FnMut(AppWindowCommand),
) {
    let mut terminal_forwarded = false;
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        match commands.recv_timeout(Duration::from_millis(100)) {
            Ok(command) => {
                terminal_forwarded |= mark_terminal_intent(terminal_intent, command);
                if command == AppWindowCommand::Fatal {
                    fatal_outcome.store(true, Ordering::Release);
                }
                forward(command);
                // Terminal intent closes normal admission; the same bridge
                // continues delivering main-owner stop/callback wakes until
                // the UI loop observes its physical owner drained.
            }
            Err(RecvTimeoutError::Timeout) if stop.load(Ordering::Acquire) => return,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) if terminal_forwarded => return,
            Err(RecvTimeoutError::Disconnected) => {
                mark_terminal_intent(terminal_intent, AppWindowCommand::Fatal);
                fatal_outcome.store(true, Ordering::Release);
                forward(AppWindowCommand::Fatal);
                return;
            }
        }
    }
}

fn spawn_renderer_outcome_bridge(
    outcomes: Receiver<RendererBridgeOutcome>,
    proxy: EventLoopProxy<WkUserEvent>,
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, WvError> {
    thread::Builder::new()
        .name("keld-wv-macos-renderer-wake".to_owned())
        .spawn(move || {
            loop {
                if stop.load(Ordering::Acquire) {
                    return;
                }
                match outcomes.recv_timeout(Duration::from_millis(100)) {
                    Ok(outcome) => {
                        let _ = proxy.send_event(WkUserEvent::Renderer(outcome));
                    }
                    Err(RecvTimeoutError::Timeout) if stop.load(Ordering::Acquire) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .map_err(|error| {
            WvError::EventLoop(format!("failed to start renderer outcome bridge: {error}"))
        })
}

fn destroy_renderer_bridges(bridges: &mut BTreeMap<u32, MacRendererBridge>) {
    for bridge in bridges.values() {
        bridge.destroy();
    }
    bridges.clear();
}

fn app_loop_result(navigation_timed_out: bool, fatal: bool, code: i32) -> Result<(), WvError> {
    if fatal {
        Err(WvError::EventLoop(String::from(
            "fatal app session command",
        )))
    } else if navigation_timed_out {
        Err(WvError::Navigate(String::from(
            "initial renderer navigation did not finish before the startup deadline",
        )))
    } else if code == 0 {
        Ok(())
    } else {
        Err(WvError::EventLoop(format!(
            "event loop exited with status {code}"
        )))
    }
}

fn is_terminal_app_event(event: &Event<'_, WkUserEvent>) -> bool {
    matches!(
        event,
        Event::LoopDestroyed
            | Event::UserEvent(WkUserEvent::App(
                AppWindowCommand::Quit | AppWindowCommand::Fatal
            ))
    )
}

fn mark_terminal_intent(intent: &AtomicBool, command: AppWindowCommand) -> bool {
    let terminal = matches!(command, AppWindowCommand::Quit | AppWindowCommand::Fatal);
    if terminal {
        intent.store(true, Ordering::Release);
    }
    terminal
}

/// wry `PageLoadEvent::Finished` → navigation completion (`nav_finished`).
fn page_load_trace_handler(
    startup: Arc<Mutex<StartupTrace>>,
    app_events: Option<Sender<AppWindowEvent>>,
    renderer_navigation: Option<BridgeNavigationHandle>,
) -> impl Fn(wry::PageLoadEvent, String) + 'static {
    move |event, url| {
        if matches!(event, wry::PageLoadEvent::Started)
            && let Some(renderer_navigation) = renderer_navigation.as_ref()
        {
            renderer_navigation.navigation_started();
        }
        #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
        let _ = url;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if std::env::var_os("KELD_PROFILE_ACCEPTANCE_REPORT").is_some() {
            let phase = match event {
                wry::PageLoadEvent::Started => "started",
                wry::PageLoadEvent::Finished => "finished",
            };
            eprintln!("KELD_KEL135_PAGE_LOAD event={phase} url={url}");
        }
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

    use super::{
        WkUserEvent, first_host_init_turn, page_load_trace_handler, run_app_wake_bridge,
        wry_page_load,
    };
    use crate::AppWindowCommand;
    use crate::startup::{PageLoad, StartupPhase, StartupTrace, phase_for_page_load};
    use tao::event::{Event, StartCause};

    #[test]
    fn facade_ready_is_only_the_first_real_tao_init_turn() {
        let mut emitted = false;
        let opened: Event<'_, WkUserEvent> = Event::Opened { urls: vec![] };
        assert!(!first_host_init_turn(&opened, &mut emitted));
        assert!(!emitted, "Opened can precede applicationDidFinishLaunching");

        // StartCause::WaitCancelled is non-exhaustive outside Tao; an
        // independently constructible non-Init turn checks the same boundary.
        let later = Event::MainEventsCleared;
        assert!(!first_host_init_turn(&later, &mut emitted));
        let init = Event::NewEvents(StartCause::Init);
        assert!(first_host_init_turn(&init, &mut emitted));
        assert!(!first_host_init_turn(&init, &mut emitted));
        assert!(!first_host_init_turn(&later, &mut emitted));
    }

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
        let handler = page_load_trace_handler(Arc::clone(&startup), None, None);
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
        let handler = page_load_trace_handler(Arc::clone(&startup), None, None);
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
        let handler = page_load_trace_handler(Arc::clone(&startup), Some(events_tx), None);
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
        assert!(crate::navigation_deadline_expired(
            super::navigation_finished(&startup),
            true,
            false,
            Instant::now(),
            expired,
        ));
        let terminal_intent = std::sync::atomic::AtomicBool::new(false);
        assert!(super::mark_terminal_intent(
            &terminal_intent,
            super::AppWindowCommand::Quit
        ));
        assert!(
            !crate::navigation_deadline_expired(
                super::navigation_finished(&startup),
                true,
                terminal_intent.load(std::sync::atomic::Ordering::Acquire),
                Instant::now(),
                expired,
            ),
            "a queued terminal UI command must win at the navigation deadline"
        );
        startup
            .lock()
            .expect("startup lock")
            .on_page_load(PageLoad::Finished);
        assert!(!crate::navigation_deadline_expired(
            super::navigation_finished(&startup),
            true,
            false,
            Instant::now(),
            expired,
        ));
    }

    #[test]
    fn fatal_app_command_is_an_error_even_when_tao_exits_zero() {
        assert!(matches!(
            super::app_loop_result(false, true, 0),
            Err(super::WvError::EventLoop(_))
        ));
        assert!(super::app_loop_result(false, false, 0).is_ok());
        assert!(matches!(
            super::app_loop_result(true, false, 0),
            Err(super::WvError::Navigate(_))
        ));
    }

    #[test]
    fn app_wake_bridge_disconnect_after_quit_does_not_emit_fatal() {
        let (commands, receiver) = std::sync::mpsc::channel();
        commands
            .send(AppWindowCommand::Quit)
            .expect("receiver is still live");
        drop(commands);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let terminal_intent = std::sync::atomic::AtomicBool::new(false);
        let fatal_outcome = std::sync::atomic::AtomicBool::new(false);
        let mut forwarded = Vec::new();
        run_app_wake_bridge(
            &receiver,
            &stop,
            &terminal_intent,
            &fatal_outcome,
            |command| {
                forwarded.push(command);
            },
        );
        assert_eq!(forwarded, [AppWindowCommand::Quit]);
        assert!(terminal_intent.load(std::sync::atomic::Ordering::Acquire));
        assert!(!fatal_outcome.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn app_wake_bridge_disconnect_before_terminal_remains_fatal() {
        let (commands, receiver) = std::sync::mpsc::channel();
        drop(commands);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let terminal_intent = std::sync::atomic::AtomicBool::new(false);
        let fatal_outcome = std::sync::atomic::AtomicBool::new(false);
        let mut forwarded = Vec::new();
        run_app_wake_bridge(
            &receiver,
            &stop,
            &terminal_intent,
            &fatal_outcome,
            |command| {
                forwarded.push(command);
            },
        );
        assert_eq!(forwarded, [AppWindowCommand::Fatal]);
        assert!(terminal_intent.load(std::sync::atomic::Ordering::Acquire));
        assert!(fatal_outcome.load(std::sync::atomic::Ordering::Acquire));
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
    let mut engine = WkWebViewEngine::new_dev_ephemeral()?;
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
