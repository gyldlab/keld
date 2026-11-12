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
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
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

pub use crate::engine::AppReadyTrigger;
pub use crate::engine::MacDialogUiOutcome;
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
use crate::{ApplicationNameForUserAgent, WebviewId};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod application_name_fixture;
mod macos_bridge;
mod macos_document;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub use macos_document::MacNativeDocumentFixture;
mod macos_navigation_owner;
mod macos_profile;
mod macos_resource_intake;
mod macos_resource_native;
mod macos_resource_source;
mod macos_termination;
mod managed_windows;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub use application_name_fixture::run_application_name_fixture_child;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod preload_native_fixture;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod s1_resource_fixture;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod s1_resource_lifecycle;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod s1_resource_lifecycle_observer;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub use s1_resource_fixture::{admit_s1_fixture_source, run_s1_resource_fixture_child};

pub use macos_bridge::{
    AppPreloadBudget, AppPreloadImage, AppPreloadReservation, MAX_APP_PRELOAD_BYTES,
    RendererBridgeEndpoint, RendererBridgeOperation, RendererBridgeOutcome, RendererBridgeRequest,
    RendererRoleWitness,
};
use macos_bridge::{BridgeNavigationHandle, MacRendererBridge};
use macos_document::{NativeAttempt, NativeCommittedStamp};
use macos_navigation_owner::KeldNavigationOwner;
use macos_resource_native::{KeldAppSchemeHandler, ReaderTerminated, ResourceCompleted};
pub use macos_resource_source::{
    AdmittedWebAppSource, MacNavigationUrl, VerifiedResourceResponsePolicy, VerifiedWebAsset,
};

/// Immutable **observation** of the one real GH542 `WKNavigation` commit. The
/// inner native witness/identity is opaque to Core and the page. Storing or
/// comparing this snapshot does NOT prove that it remains current. Each
/// privilege-bearing native operation must recheck it against the living
/// `NativeDocumentOwner` on the UI owner in the same admission turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacNativeDocumentStamp(NativeCommittedStamp);

impl MacNativeDocumentStamp {
    /// Actual native view that produced this immutable observation.
    #[must_use]
    pub fn view(self) -> WebviewId {
        self.0.view
    }

    /// Observed native committed generation; not future-operation authority.
    #[must_use]
    pub fn observed_generation(self) -> u32 {
        self.0.generation.get()
    }

    /// Actual native attempt retained by this committed observation.
    /// This accessor cannot mint or authorize a document.
    #[must_use]
    pub const fn attempt(self) -> MacNativeAttempt {
        MacNativeAttempt(self.0.attempt)
    }
}

/// A host-started native navigation attempt, without a committed document.
/// No IPC or page authority can derive from this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacNativeAttempt(NativeAttempt);

impl MacNativeAttempt {
    /// Actual native view to which this opaque attempt belongs.
    #[must_use]
    pub fn view(self) -> WebviewId {
        self.0.view()
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use macos_profile::MacMediaPromptProbe;
use macos_profile::MacProfileOwner;

/// Revocable current-document witness for the final Host IPC admission cut.
/// Only the native document owner can mint/revoke it. A snapshot or true result
/// does not replace rechecking the UI owner for a later native side effect.
#[derive(Clone, Debug)]
pub struct MacNativeDocumentLease(macos_document::NativeDocumentLease);
impl PartialEq for MacNativeDocumentLease {
    fn eq(&self, other: &Self) -> bool {
        self.same_binding(other)
    }
}
impl Eq for MacNativeDocumentLease {}
impl MacNativeDocumentLease {
    /// Acquire the native owner's synchronous revocation state now.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.0.is_current()
    }
    /// Read the exact immutable native commit this witness belongs to.
    #[must_use]
    pub fn stamp(&self) -> MacNativeDocumentStamp {
        MacNativeDocumentStamp(self.0.stamp())
    }
    /// Match the same actual admitted source allocation, including policy aliases.
    #[must_use]
    pub fn belongs_to_source(&self, source: &Arc<AdmittedWebAppSource>) -> bool {
        self.0.belongs_to_source(source)
    }
    /// Match the exact original irreversible native lease, not numeric coincidence.
    #[must_use]
    pub fn same_binding(&self, other: &Self) -> bool {
        self.0.same_binding(&other.0)
    }
}

/// Terminal choice at the native conditional document-destroy admission cut.
/// Acceptance means irreversible retirement, never physical window release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OriginalDocumentDestroyDecision {
    /// Neither native admission nor Core retirement has won yet.
    Pending = 0,
    /// Native accepted the exact current document and retired it on its UI turn.
    Accepted = 1,
    /// Native refused before changing document, bridge, resources or window.
    Refused = 2,
    /// Core retirement won before native admission; no conditional effect may run.
    Cancelled = 3,
}

/// One bounded in-flight conditional native effect shared with the Core Entry.
/// Arc allocation identity associates the command and receipt; this is neither
/// a document identity nor a principal. Only the living native owner may accept.
pub struct OriginalDocumentDestroyAdmission {
    decision: AtomicU8,
}

impl OriginalDocumentDestroyAdmission {
    /// Create one pending admission before dispatching the immutable command.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            decision: AtomicU8::new(OriginalDocumentDestroyDecision::Pending as u8),
        }
    }

    /// Observe the same cancellation/admission winner before successor adoption.
    #[must_use]
    pub fn decision(&self) -> OriginalDocumentDestroyDecision {
        // Only the constructors and terminal CAS below write these four values.
        // Any unrecognized representation refuses effect/adoption as cancelled.
        match self.decision.load(Ordering::Acquire) {
            0 => OriginalDocumentDestroyDecision::Pending,
            1 => OriginalDocumentDestroyDecision::Accepted,
            2 => OriginalDocumentDestroyDecision::Refused,
            _ => OriginalDocumentDestroyDecision::Cancelled,
        }
    }

    /// Cancel only before the native effect is admitted. False requires reading
    /// `decision()`: Accepted must synchronously become nonadoptable in Core.
    #[must_use]
    pub fn cancel_before_admission(&self) -> bool {
        self.finish_pending(OriginalDocumentDestroyDecision::Cancelled)
    }

    fn try_accept_on_ui(&self) -> bool {
        self.finish_pending(OriginalDocumentDestroyDecision::Accepted)
    }

    fn refuse_on_ui(&self) -> bool {
        self.finish_pending(OriginalDocumentDestroyDecision::Refused)
    }

    fn finish_pending(&self, terminal: OriginalDocumentDestroyDecision) -> bool {
        self.decision
            .compare_exchange(
                OriginalDocumentDestroyDecision::Pending as u8,
                terminal as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Construct Accepted for Core state-only reconciliation tests. This mints
    /// no native lease, view, document or authority and is absent in production.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    #[must_use]
    pub const fn fixture_accepted() -> Self {
        Self {
            decision: AtomicU8::new(OriginalDocumentDestroyDecision::Accepted as u8),
        }
    }

    /// Compete with Core cancellation in state-only retirement-order tests.
    /// This supplies no native lease or effect and is absent in production.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    #[must_use]
    pub fn fixture_try_accept(&self) -> bool {
        self.try_accept_on_ui()
    }

    /// Construct Refused for Core state-only reconciliation tests, never native
    /// evidence. This fixture is absent from production builds.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    #[must_use]
    pub const fn fixture_refused() -> Self {
        Self {
            decision: AtomicU8::new(OriginalDocumentDestroyDecision::Refused as u8),
        }
    }
}

impl Default for OriginalDocumentDestroyAdmission {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Debug for OriginalDocumentDestroyAdmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OriginalDocumentDestroyAdmission")
            .field("decision", &self.decision())
            .finish()
    }
}
impl PartialEq for OriginalDocumentDestroyAdmission {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
impl Eq for OriginalDocumentDestroyAdmission {}

/// One live webview and the host window it fills (v0: one per window).
#[repr(C)]
struct View {
    webview: wry::WebView,
    window: Window,
    // Build provenance only: legacy initial renderers remain outside the Core
    // registry, and must retain their existing LastWindowClosed close path.
    registry_managed: bool,
    _preload_image: Option<Arc<AppPreloadImage>>,
    /// One view/document/source owner. Existing renderer-declared KEL142
    /// windows retain their narrower independent pre-GH542 behavior.
    resource: Option<MacResourceView>,
}

struct MacResourceView {
    handler: Retained<KeldAppSchemeHandler>,
    navigation: Retained<KeldNavigationOwner>,
}

enum WkUserEvent {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    ApplicationNameFixtureWake,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    ResourceLifecycle(s1_resource_lifecycle::LifecycleEvent),
    App(AppWindowCommand),
    Renderer(RendererBridgeOutcome),
    // An AppKit Quit intent is never an accepted terminal command.
    NativeQuitRequested,
    RendererCustody,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    PreloadFixtureSnapshot {
        view: WebviewId,
        document: MacNativeDocumentLease,
        raw: String,
    },
    Resource(ResourceCompleted),
    ReaderTerminated(ReaderTerminated),
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    S1FixtureSnapshot {
        view: WebviewId,
        stamp: NativeCommittedStamp,
        raw: String,
    },
}

/// The macOS [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WkWebViewEngine::run_until_closed`]
/// consumes it. Uses tao `run_return` so the host can reap supervised children
/// after the last window closes (KEL-30 concurrent hello app-link).
pub struct WkWebViewEngine {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    resource_lifecycle_probe:
        Option<std::rc::Rc<std::cell::RefCell<s1_resource_lifecycle::LifecycleProbe>>>,
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<WkUserEvent>>,
    views: BTreeMap<u32, View>,
    renderer_bridges: BTreeMap<u32, MacRendererBridge>,
    renderer_outcomes: Option<Receiver<RendererBridgeOutcome>>,
    renderer_requests: Option<(std::sync::mpsc::SyncSender<RendererBridgeRequest>, u16)>,
    /// Set exactly once by Core after GH499 graph and GH552 policy admission.
    /// A JS call cannot choose this source or supply a second webapp root.
    original_resource: Option<Arc<AdmittedWebAppSource>>,
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
        Self::new_with_application_name_for_user_agent(selection, None)
    }

    /// Creates the engine with immutable, validated user-agent application metadata.
    ///
    /// This macOS-only cold constructor preserves the native engine user agent
    /// and existing application-name prefix. Metadata conveys no authority.
    /// Call on the process main thread before any view is created.
    ///
    /// # Errors
    /// Returns the same profile errors as [`Self::new`]. Native metadata
    /// assignment errors surface when a view configuration is created.
    pub fn new_with_application_name_for_user_agent(
        selection: WebProfileSelection,
        application_name_for_user_agent: Option<ApplicationNameForUserAgent>,
    ) -> Result<Self, WvError> {
        Self::new_with_profile_test_media(selection, false, application_name_for_user_agent)
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
        let mut engine = Self::new_with_profile_test_media(selection, seed_media_allow, None)?;
        engine.media_prompt_probe = Some(MacMediaPromptProbe::new()?);
        Ok(engine)
    }

    fn new_with_profile_test_media(
        selection: WebProfileSelection,
        profile_test_media_seed_allow: bool,
        application_name_for_user_agent: Option<ApplicationNameForUserAgent>,
    ) -> Result<Self, WvError> {
        let startup = Arc::new(Mutex::new(StartupTrace::new()));
        let mut event_loop = EventLoopBuilder::with_user_event().build();
        let profile =
            MacProfileOwner::new(selection, &mut event_loop, application_name_for_user_agent)?;
        mark_startup(&startup, StartupPhase::EventLoop);
        Ok(Self {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            resource_lifecycle_probe: None,
            event_loop: Some(event_loop),
            views: BTreeMap::new(),
            renderer_bridges: BTreeMap::new(),
            renderer_outcomes: None,
            renderer_requests: None,
            original_resource: None,
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

    /// Core calls on the native UI owner after exact retained graph and
    /// profile/policy preflight. The first facade window consumes this source.
    /// No content is loaded or authority minted merely by storing the source.
    ///
    /// # Errors
    /// Rejects a second selection or a selection after native creation.
    pub fn admit_initial_legacy_webapp(
        &mut self,
        source: Arc<AdmittedWebAppSource>,
    ) -> Result<(), WvError> {
        if self.original_resource.is_some() || self.event_loop.is_none() || !self.views.is_empty() {
            return Err(WvError::Webview(String::from(
                "GH473 original source must be selected once before facade WK construction",
            )));
        }
        self.original_resource = Some(source);
        Ok(())
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

    /// Retain the existing single Core renderer endpoint for later registry windows.
    /// No channel, dispatcher or document authority is created by this binding.
    ///
    /// # Errors
    /// Refuses duplicate setup or setup after UI loop entry.
    pub fn install_renderer_endpoint(
        &mut self,
        mut endpoint: RendererBridgeEndpoint,
    ) -> Result<(), WvError> {
        if self.event_loop.is_none()
            || self.renderer_outcomes.is_some()
            || self.renderer_requests.is_some()
        {
            return Err(WvError::RendererBridge {
                detail: String::from("renderer endpoint already selected or UI loop running"),
            });
        }
        self.renderer_requests = Some((endpoint.requests.clone(), endpoint.admitted_channel));
        self.renderer_outcomes = endpoint.outcomes.take();
        Ok(())
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
            &mut |_, _, _, _, _| Ok((true, None)),
            (&mut unproved, || {}),
        )
    }

    /// Runs the same application loop with a main-owned native presentation service.
    ///
    /// `anchor` is the host-created view, never a caller-supplied selector.
    /// The resolver returns its actual `AppKit` window through wry's safe accessor.
    /// `service` receives the main-thread marker, the legacy Auth anchor,
    /// an explicit `WebviewId` resolver for Host-validated dialog parents,
    /// a close-request callback for the exact focused native window, and stop
    /// intent. The callback emits the existing registry close request and never
    /// physically closes a window or decides its veto. Both resolvers return
    /// only the current live Wry-owned
    /// `NSWindow`; neither accepts a native pointer or creates a window. It
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
            &mut dyn FnMut(WebviewId) -> Option<Retained<NSWindow>>,
            &mut dyn FnMut(&NSWindow) -> Result<(), WvError>,
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
        let native_quit = macos_termination::NativeQuitBridge::bind(main_thread, proxy.clone())?;
        let resource_wake = proxy.clone();
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
                spawn_renderer_outcome_bridge(outcomes, proxy.clone(), Arc::clone(&stop_bridge))
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
        let mut resource_unproved = false;
        let mut host_init_emitted = false;
        let profile = &self.profile;
        let selected_resource = self.original_resource.as_ref();
        let next_id = &mut self.next_id;
        let media_seed_allow = self.profile_test_media_seed_allow;
        let renderer_endpoint = self.renderer_requests.clone();
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
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let lifecycle_probe = self.resource_lifecycle_probe.clone();
        let code = event_loop.run_return(|event, target, control_flow| {
            if let Some(error) = native_quit.failure() {
                main_owner_failure.get_or_insert(error);
                fatal_in_loop.store(true, Ordering::Release);
                terminal_requested = true;
                destroy_renderer_bridges(&mut renderer_bridges);
            }
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if let Some(probe)=lifecycle_probe.as_ref()
                && let Err(error)=probe.borrow_mut().before_event(&event, &views) {
                    main_owner_failure.get_or_insert(error);
                    resource_unproved = true;
                    *control_flow = ControlFlow::Exit;
                    return;
            }
            // Completion must remain consumable after Quit closes app dispatch.
            // The view routes the nudge; only its exact custody cell can accept it.
            let reader_completion_turn = if let Event::UserEvent(
                WkUserEvent::ReaderTerminated(completion),
            ) = &event {
                if let Some(resource) = views.get(&completion.view.0)
                    .and_then(|view| view.resource.as_ref()) {
                    resource.handler.observe_reader_termination(completion);
                }
                true
            } else {
                false
            };
            // Native WebKit evaluation completions may retire the independent
            // renderer owner AFTER the resource reader has joined. Revisit the
            // existing pending releases on their actual completion nudge.
            let retirement_progress_turn = reader_completion_turn
                || matches!(
                    &event,
                    Event::UserEvent(WkUserEvent::RendererCustody | WkUserEvent::Renderer(_))
                );
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
                // The first-Init native timestamp must cover the full S1
                // desktop observation. Taking it before capture would let a
                // slow native sample falsely pass the existing 15s watchdog.
                // Core can still be descheduled after the UI completed the
                // capture, so record completion on the native owner before
                // enqueuing DesktopObserved then HostInitialized.
                // GH481 S1: first native screen and appearance sample happens
                // at the genuine Init, before HostInitialized may publish raw
                // Ready. No hidden window is allocated to obtain these facts.
                let facts = match managed_windows::capture_desktop() {
                    Ok(facts) => Some(facts),
                    Err(error) => {
                        eprintln!("KELD-CORE-049: first native desktop capture failed: {error}");
                        None
                    }
                };
                let observed_at = Instant::now();
                if events.send(AppWindowEvent::DesktopObserved { facts }).is_err()
                    || events.send(AppWindowEvent::HostInitialized { observed_at }).is_err()
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
                            renderer_endpoint: renderer_endpoint.as_ref(),
                            resource_source: selected_resource,
                            resource_wake: resource_wake.clone(),
                            resource_events: Some(events.clone()),
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
                    Event::UserEvent(WkUserEvent::RendererCustody | WkUserEvent::Renderer(RendererBridgeOutcome::ControlCapacity)) => {
                        let mut failure = None;
                        for (id, bridge) in &renderer_bridges {
                            if let Some(view) = views.get(id) {
                                if let Err(error) = bridge.service_original_calls(&view.webview.webview()) {
                                    failure = Some(error);
                                    break;
                                }
                            } else {
                                bridge.flush_controls();
                            }
                        }
                        if let Some(error) = failure {
                            main_owner_failure.get_or_insert(error);
                            fatal_in_loop.store(true, Ordering::Release);
                            terminal_requested = true;
                            destroy_renderer_bridges(&mut renderer_bridges);
                        }
                    }
                    Event::UserEvent(WkUserEvent::Renderer(outcome)) => {
                        // Untargeted capacity notices have their own service
                        // branch above. Preserve end-of-turn retirement checks
                        // even if a future outcome has no specific view.
                        let delivered = outcome.webview().and_then(|id| {
                            renderer_bridges
                                .get(&id.0)
                                .zip(views.get(&id.0))
                                .map(|(bridge, view)| {
                                    let native = view.webview.webview();
                                    bridge.deliver(&native, outcome)
                                })
                        });
                        if delivered.is_some_and(|result| result.is_err()) {
                            fatal_in_loop.store(true, Ordering::Release);
                            terminal_requested = true;
                            destroy_renderer_bridges(&mut renderer_bridges);
                        }
                    }
                    Event::UserEvent(WkUserEvent::Resource(completion)) => {
                        if let Some(resource) = views
                            .get(&completion.view.0)
                            .and_then(|view| view.resource.as_ref())
                        {
                            resource.handler.deliver(completion);
                        }
                        // A missing/destroyed view drops the owned charged
                        // result; its RAII token releases only after payload
                        // and worker completion actually leave the queue.
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
                    Event::UserEvent(WkUserEvent::NativeQuitRequested) => {
                        match native_quit.request_renderer_close(
                            ready_trigger,
                            navigation_finished(&startup_in_loop),
                            &views,
                            &closing,
                        ) {
                            Ok(true) => {}
                            Ok(false) => eprintln!(
                                "KELD-WV-004: native Quit retained; this window composition has no admitted renderer close route"
                            ),
                            Err(error) => {
                                main_owner_failure.get_or_insert(error);
                                fatal_in_loop.store(true, Ordering::Release);
                                terminal_requested = true;
                            }
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
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if let Some(probe)=lifecycle_probe.as_ref()
                && let Err(error)=probe.borrow_mut().after_turn(&views) {
                    main_owner_failure.get_or_insert(error);
                    resource_unproved = true;
                    *control_flow = ControlFlow::Exit;
                    return;
            }
            // Real completion drives release; the original absolute deadline
            // supplies only a failure turn when the nudge/outcome is missing.
            let reader_deadline_reached = managed_windows::resource_shutdown_deadline(&views)
                .is_some_and(|deadline| Instant::now() >= deadline);
            let renderer_deadline_reached =
                pending_renderer_retirement_deadline(&views, &renderer_bridges)
                    .is_some_and(|deadline| Instant::now() >= deadline);
            if !terminal_requested
                && (retirement_progress_turn || reader_deadline_reached || renderer_deadline_reached)
                && let Err(error) = managed_windows::finish_pending_releases(
                    &mut views,
                    &mut renderer_bridges,
                    &events,
                )
            {
                main_owner_failure.get_or_insert(error);
                resource_unproved = true;
                fatal_in_loop.store(true, Ordering::Release);
                *control_flow = ControlFlow::Exit;
                return;
            }
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if let Some(probe)=lifecycle_probe.as_ref()
                && let Err(error)=probe.borrow_mut().after_turn(&views) {
                    main_owner_failure.get_or_insert(error);
                    resource_unproved = true;
                    *control_flow = ControlFlow::Exit;
                    return;
            }
            let stopping = terminal_requested || anchor.is_some_and(|id| closing.contains(&id.0));
            let (drained, owner_deadline) = {
                let mut resolve = || {
                    anchor
                        .and_then(|id| views.get(&id.0))
                        .and_then(|view| view.webview.webview().window())
                };
                let mut resolve_view = |id: WebviewId| {
                    views
                        .get(&id.0)
                        .and_then(|view| view.registry_managed.then_some(view))
                        .and_then(|view| view.webview.webview().window())
                };
                let mut request_close = |window: &NSWindow| {
                    managed_windows::request_native_close(&views, &events, window, stopping)
                };
                match service(main_thread, &mut resolve, &mut resolve_view, &mut request_close, stopping) {
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
                match managed_windows::release_all(&mut views, &mut renderer_bridges, &events) {
                    Ok(true) => {
                        views.clear();
                        *control_flow = ControlFlow::Exit;
                    }
                    Ok(false) => {
                        let next_deadline = [
                            managed_windows::resource_shutdown_deadline(&views),
                            pending_renderer_retirement_deadline(&views, &renderer_bridges),
                        ]
                        .into_iter()
                        .flatten()
                        .min();
                        *control_flow = next_deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil);
                    }
                    Err(error) => {
                        main_owner_failure.get_or_insert(error);
                        resource_unproved = true;
                        fatal_in_loop.store(true, Ordering::Release);
                        *control_flow = ControlFlow::Exit;
                    }
                }
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                if let Some(probe)=lifecycle_probe.as_ref()
                    && let Err(error)=probe.borrow_mut().after_turn(&views) {
                        main_owner_failure.get_or_insert(error);
                        resource_unproved = true;
                        *control_flow = ControlFlow::Exit;
                }
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
            let wake=[navigation_wake,owner_deadline,managed_windows::resource_shutdown_deadline(&views),pending_renderer_retirement_deadline(&views, &renderer_bridges)].into_iter().flatten().min();
            *control_flow = wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil);
        });
        // Tao drops its stored callback on every return, including external
        // NSApp.run termination. The callback only borrowed these real owners.
        let mut tail_failure = main_owner_failure.or_else(|| native_quit.failure());
        // Retire the callback before native tail servicing can pump AppKit again.
        // Late requests still cancel but cannot reach the ended UI loop.
        drop(native_quit);
        let mut drained = !resource_unproved;
        while drained {
            let progress = {
                let mut resolve = || {
                    anchor
                        .and_then(|id| views.get(&id.0))
                        .and_then(|view| view.webview.webview().window())
                };
                let mut resolve_view = |id: WebviewId| {
                    views
                        .get(&id.0)
                        .and_then(|view| view.registry_managed.then_some(view))
                        .and_then(|view| view.webview.webview().window())
                };
                let mut request_close = |window: &NSWindow| {
                    managed_windows::request_native_close(&views, &events, window, true)
                };
                service(
                    main_thread,
                    &mut resolve,
                    &mut resolve_view,
                    &mut request_close,
                    true,
                )
            };
            let progress = match progress {
                Ok(progress) => progress,
                Err(error) => {
                    tail_failure.get_or_insert(error);
                    drained = false;
                    break;
                }
            };
            if progress.0 {
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
            match managed_windows::release_all(&mut views, &mut renderer_bridges, &events) {
                Ok(true) => views.clear(),
                Ok(false) => {
                    // Tao has exited unexpectedly before the resource worker
                    // had finished. No physical release or stage cleanup is
                    // permitted without a native turn to confirm quiescence.
                    drained = false;
                    tail_failure.get_or_insert_with(|| {
                        WvError::EventLoop(String::from(
                            "native resource shutdown pending after UI exit; retained custody",
                        ))
                    });
                }
                Err(error) => {
                    drained = false;
                    tail_failure.get_or_insert(error);
                }
            }
        }
        if !drained {
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
            renderer_endpoint: self.renderer_requests.as_ref(),
            resource_source: None,
            resource_wake: event_loop.create_proxy(),
            resource_events: None,
        }
        .build(webview_id, spec, app_events, renderer_endpoint, false)
        .map_err(|failure| failure.source)?;
        if let Some(installed) = installed {
            if let Some(outcomes) = installed.outcomes {
                self.renderer_outcomes = Some(outcomes);
            }
            self.renderer_requests = Some(installed.bridge.request_sender());
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
        // The first disposable Legacy app must use the correlated native
        // loadRequest returning its actual WKNavigation object; this legacy
        // convenience path cannot mint a GH542 attempt/document stamp.
        if self.view(id)?.resource.is_some() {
            return Err(WvError::Navigate(String::from(
                "GH473 original resource view requires the authenticated GH459 native load attempt",
            )));
        }
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
                terminal_forwarded |= mark_terminal_intent(terminal_intent, &command);
                if matches!(&command, AppWindowCommand::Fatal) {
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
                mark_terminal_intent(terminal_intent, &AppWindowCommand::Fatal);
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
    // Revoke all renderer authority, but retain native evaluation custody.
    // managed_windows::release removes each bridge after real quiescence.
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

fn pending_renderer_retirement_deadline(
    views: &BTreeMap<u32, View>,
    bridges: &BTreeMap<u32, MacRendererBridge>,
) -> Option<Instant> {
    bridges
        .iter()
        .filter(|(id, _)| views.contains_key(*id))
        .filter_map(|(_, bridge)| bridge.retirement_deadline())
        .min()
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

fn mark_terminal_intent(intent: &AtomicBool, command: &AppWindowCommand) -> bool {
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
        let later: Event<'_, WkUserEvent> = Event::MainEventsCleared;
        assert!(!first_host_init_turn(&later, &mut emitted));
        let init: Event<'_, WkUserEvent> = Event::NewEvents(StartCause::Init);
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
            &super::AppWindowCommand::Quit
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
        assert!(matches!(forwarded.as_slice(), [AppWindowCommand::Quit]));
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
        assert!(matches!(forwarded.as_slice(), [AppWindowCommand::Fatal]));
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

#[cfg(test)]
mod original_document_destroy_admission_tests {
    use super::{OriginalDocumentDestroyAdmission, OriginalDocumentDestroyDecision};
    use std::sync::{Arc, Barrier};

    // Private claim exercises only the shared state machine. No native lease,
    // view or source is forged, and these tests do not prove UI admission.
    #[test]
    fn original_document_destroy_pending_cancel_winner_cannot_be_admitted() {
        let admission = OriginalDocumentDestroyAdmission::new();
        assert_eq!(
            admission.decision(),
            OriginalDocumentDestroyDecision::Pending
        );
        assert!(admission.cancel_before_admission());
        assert_eq!(
            admission.decision(),
            OriginalDocumentDestroyDecision::Cancelled
        );
        assert!(!admission.try_accept_on_ui());
        assert!(!admission.refuse_on_ui());
        assert!(!admission.cancel_before_admission());
        assert_eq!(
            admission.decision(),
            OriginalDocumentDestroyDecision::Cancelled
        );
    }

    #[test]
    fn original_document_destroy_accepted_winner_cannot_be_cancelled_or_readmitted() {
        let admission = OriginalDocumentDestroyAdmission::new();
        assert!(admission.try_accept_on_ui());
        assert_eq!(
            admission.decision(),
            OriginalDocumentDestroyDecision::Accepted
        );
        assert!(!admission.cancel_before_admission());
        assert!(!admission.refuse_on_ui());
        assert!(!admission.try_accept_on_ui());
        assert_eq!(
            admission.decision(),
            OriginalDocumentDestroyDecision::Accepted
        );
    }

    #[test]
    fn original_document_destroy_refusal_is_terminal() {
        let admission = OriginalDocumentDestroyAdmission::new();
        assert!(admission.refuse_on_ui());
        assert_eq!(
            admission.decision(),
            OriginalDocumentDestroyDecision::Refused
        );
        assert!(!admission.try_accept_on_ui());
        assert!(!admission.cancel_before_admission());
        assert!(!admission.refuse_on_ui());
    }

    #[test]
    fn original_document_destroy_concurrent_claim_and_cancel_have_exactly_one_winner() {
        let admission = Arc::new(OriginalDocumentDestroyAdmission::new());
        let start = Arc::new(Barrier::new(3));
        let (accepted, cancelled) = std::thread::scope(|scope| {
            let accept_ticket = Arc::clone(&admission);
            let accept_start = Arc::clone(&start);
            let accepted = scope.spawn(move || {
                accept_start.wait();
                accept_ticket.try_accept_on_ui()
            });
            let cancel_ticket = Arc::clone(&admission);
            let cancel_start = Arc::clone(&start);
            let cancelled = scope.spawn(move || {
                cancel_start.wait();
                cancel_ticket.cancel_before_admission()
            });
            start.wait();
            (
                accepted.join().expect("state-only claimant"),
                cancelled.join().expect("state-only canceller"),
            )
        });
        assert_ne!(accepted, cancelled, "one shared atomic transition must win");
        assert_eq!(
            admission.decision(),
            if accepted {
                OriginalDocumentDestroyDecision::Accepted
            } else {
                OriginalDocumentDestroyDecision::Cancelled
            }
        );
        assert!(!admission.try_accept_on_ui());
        assert!(!admission.cancel_before_admission());
    }
}

#[cfg(test)]
mod app_command_layout_tests {
    #[test]
    fn gui_events_do_not_inline_the_cold_query_payload() {
        assert!(std::mem::size_of::<super::WkUserEvent>() <= 256);
    }
}
