//! Bounded GH540 physical-delivery ledger for the existing Core primary router.
//!
//! Native `DocumentOwner` provides one checked source and actual physical completion;
//! Runtime provides authenticated `PrimaryCallerContext`. This owner only retains
//! separate DATA and CONTROL charges. It mints neither document nor role identity.
//! Uses the canonical DTO, immutable Guard snapshot, existing Registry mapping,
//! current Runtime caller fence, ordered writer and native endpoint. No new
//! reader, transport, principal, policy parser or native document owner is created.

use super::PrimaryCallerContext;
use super::window_registry::{RendererRoute, WindowOwner};
use super::{
    ActivePrimaryGeneration, HostAppError, PrimaryRouterHandle, app_detail, app_ipc, encode,
    with_primary_caller_locked, write_frame,
};
use keld_guard::verified_manifest::VerifiedManifest;
use keld_guard::{Principal, RendererChannelQuery};
use keld_ipc::compat_control::{
    self, CompatControlEvent, CompatControlRequest, CompatControlResponse, ConsumerDisposition,
    TerminalDisposition,
};
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::guard_dispatch::{dispatch_privileged_query, preflight_privileged_query};
use keld_ipc::{CallError, LifecycleResponse, SubscribeRefusal};
use keld_wv::WebviewId;
use keld_wv::wkwebview::{
    MacNativeDocumentLease, RendererBridgeOperation, RendererBridgeOutcome, RendererBridgeRequest,
    RendererRoleWitness,
};
use keld_wv::wv_link::{WvConsumerDisposition, WvTerminalDisposition};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, mpsc::Sender};

/// A native-checked source stamp. These numeric fields are not credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeRouteKey {
    webview: u32,
    navigation: u32,
    message: u32,
}
impl NativeRouteKey {
    fn valid(self) -> bool {
        self.webview != 0 && self.navigation != 0 && self.message != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    RendererToMain,
    MainToRenderer,
}
impl Direction {
    const fn index(self) -> usize {
        match self {
            Self::RendererToMain => 0,
            Self::MainToRenderer => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Admission {
    AtCapacity,
    TooLarge,
    InvalidNativeIdentity,
}
const MAX_DATA_RETAINED_BYTES: usize = 262_144;
const MAX_CONTROL_BYTES: usize = compat_control::COMPAT_CONTROL_FRAME_BYTES;

struct Charge {
    key: NativeRouteKey,
    owner: PrimaryCallerContext,
    retained: usize,
    consumer_done: bool,
    native_done: bool,
    retired: bool,
    // Opaque native authority is retained, never recreated from the numeric key.
    document: Option<MacNativeDocumentLease>,
    pending_reply: Option<CorrelationId>,
    terminal: Option<ConsumerDisposition>,
}
impl Charge {
    fn owned_by(&self, caller: &PrimaryCallerContext) -> bool {
        self.owner.attempt == caller.attempt
            && self.owner.identity == caller.identity
            && Arc::ptr_eq(&self.owner.reader_stop, &caller.reader_stop)
    }
}
struct ControlCharge {
    owner: PrimaryCallerContext,
    correlation: u32,
    retained: usize,
}
impl ControlCharge {
    fn owned_by(&self, caller: &PrimaryCallerContext) -> bool {
        self.owner.attempt == caller.attempt
            && self.owner.identity == caller.identity
            && Arc::ptr_eq(&self.owner.reader_stop, &caller.reader_stop)
    }
}

/// Lives across role changes for the entire Host application session.
/// Each direction owns one DATA lane and four CONTROL slots are independent.
struct CompatLedger {
    data: [Option<Charge>; 2],
    controls: [Option<ControlCharge>; 4],
    preflight: Option<PreflightAdmission>,
}
impl CompatLedger {
    const fn new() -> Self {
        Self {
            data: [None, None],
            controls: [None, None, None, None],
            preflight: None,
        }
    }

    /// Caller must first authenticate the role, native document, and exact
    /// per-window topic grant. No raw page input may invoke this method.
    fn reserve(
        &mut self,
        direction: Direction,
        key: NativeRouteKey,
        caller: &PrimaryCallerContext,
        retained: usize,
    ) -> Result<(), Admission> {
        if !key.valid() {
            return Err(Admission::InvalidNativeIdentity);
        }
        if retained > MAX_DATA_RETAINED_BYTES
            || self.retained_bytes().saturating_add(retained)
                > 2 * MAX_DATA_RETAINED_BYTES + 4 * MAX_CONTROL_BYTES
        {
            return Err(Admission::TooLarge);
        }
        let slot = &mut self.data[direction.index()];
        if slot.is_some() || caller.reader_stop.load(Ordering::Acquire) {
            return Err(Admission::AtCapacity);
        }
        *slot = Some(Charge {
            key,
            owner: caller.clone(),
            retained,
            consumer_done: false,
            native_done: false,
            retired: false,
            document: None,
            pending_reply: None,
            terminal: None,
        });
        Ok(())
    }

    fn occupied(&self, direction: Direction) -> bool {
        self.data[direction.index()].is_some()
    }

    /// The real `WorkerLink` must have advanced `R_BYTES` and `R_RECS` first.
    fn consumer_released(
        &mut self,
        direction: Direction,
        key: NativeRouteKey,
        caller: &PrimaryCallerContext,
    ) {
        if let Some(charge) = &mut self.data[direction.index()]
            && charge.key == key
            && charge.owned_by(caller)
            && !caller.reader_stop.load(Ordering::Acquire)
        {
            charge.consumer_done = true;
        }
        self.clear_if_complete(direction);
    }

    /// This notification is legal only from worker19's native completion
    /// owner, never from a peer's consumed/released payload or timeout.
    fn native_quiesced(&mut self, direction: Direction, key: NativeRouteKey) {
        if let Some(charge) = &mut self.data[direction.index()]
            && charge.key == key
        {
            charge.native_done = true;
        }
        self.clear_if_complete(direction);
    }

    fn document_retired(&mut self, webview: u32, navigation: u32) {
        if self.preflight.as_ref().is_some_and(|p| {
            p.document.stamp().view().0 == webview
                && p.document.stamp().observed_generation() == navigation
        }) {
            self.preflight = None; // observation only, no dispatched DATA
        }
        for charge in self.data.iter_mut().flatten() {
            if charge.key.webview == webview && charge.key.navigation == navigation {
                charge.retired = true;
            }
        }
    }

    fn role_retired(&mut self, caller: &PrimaryCallerContext) {
        if self
            .preflight
            .as_ref()
            .is_some_and(|p| same_caller(&p.caller, caller))
        {
            self.preflight = None; // never dispatched; no DATA charge is refunded
        }
        for charge in self.data.iter_mut().flatten() {
            if charge.owned_by(caller) {
                charge.retired = true;
            }
        }
    }

    /// Only canonical WorkerLink/Runtime AFTER proved ring/role drain.
    fn framework_quiesced_after_drain(
        &mut self,
        direction: Direction,
        key: NativeRouteKey,
        caller: &PrimaryCallerContext,
    ) {
        if direction != Direction::RendererToMain {
            return;
        }
        if let Some(charge) = &mut self.data[direction.index()]
            && charge.key == key
            && charge.owned_by(caller)
            && charge.retired
        {
            charge.consumer_done = true;
        }
        self.clear_if_complete(direction);
    }

    fn clear_if_complete(&mut self, direction: Direction) {
        if self.data[direction.index()]
            .as_ref()
            .is_some_and(|c| c.consumer_done && c.native_done)
        {
            self.data[direction.index()] = None;
        }
    }

    fn reserve_control(
        &mut self,
        caller: &PrimaryCallerContext,
        correlation: u32,
        retained: usize,
    ) -> Result<(), Admission> {
        if retained > MAX_CONTROL_BYTES {
            return Err(Admission::TooLarge);
        }
        if correlation == 0 {
            return Err(Admission::InvalidNativeIdentity);
        }
        if caller.reader_stop.load(Ordering::Acquire) {
            return Err(Admission::AtCapacity);
        }
        if self
            .controls
            .iter()
            .flatten()
            .any(|c| c.owned_by(caller) && c.correlation == correlation)
        {
            return Err(Admission::AtCapacity);
        }
        let Some(slot) = self.controls.iter_mut().find(|slot| slot.is_none()) else {
            return Err(Admission::AtCapacity);
        };
        *slot = Some(ControlCharge {
            owner: caller.clone(),
            correlation,
            retained,
        });
        Ok(())
    }

    fn release_control(&mut self, caller: &PrimaryCallerContext, correlation: u32) {
        if let Some(slot) = self.controls.iter_mut().find(|slot| {
            slot.as_ref()
                .is_some_and(|c| c.owned_by(caller) && c.correlation == correlation)
        }) {
            *slot = None;
        }
    }

    #[cfg(test)]
    fn controls_in_flight(&self) -> usize {
        self.controls.iter().flatten().count()
    }

    fn retained_bytes(&self) -> usize {
        self.data
            .iter()
            .flatten()
            .map(|c| c.retained)
            .sum::<usize>()
            + self
                .controls
                .iter()
                .flatten()
                .map(|c| c.retained)
                .sum::<usize>()
    }
}

fn same_caller(left: &PrimaryCallerContext, right: &PrimaryCallerContext) -> bool {
    left.attempt == right.attempt
        && left.identity == right.identity
        && Arc::ptr_eq(&left.reader_stop, &right.reader_stop)
}

// Call only inside the existing current-caller fence. The captured native role
// must still own this preflight; resolving the latest caller is not authority.
fn preflight_role_matches(
    caller: &PrimaryCallerContext,
    role: Option<&RendererRoleWitness>,
) -> bool {
    role.is_some_and(|witness| witness.matches(&caller.reader_stop))
}

struct PreflightAdmission {
    caller: PrimaryCallerContext,
    document: MacNativeDocumentLease,
    call: u32,
    channel: String,
}

/// One application-session owner; retained across zero-window boot and Bun recovery.
/// The sender is the existing UI endpoint, never a new delivery queue.
pub(super) struct CompatSession {
    verified: VerifiedManifest,
    outcomes: Sender<RendererBridgeOutcome>,
    subscribed: Option<PrimaryCallerContext>,
    ledger: CompatLedger,
}
impl CompatSession {
    pub(super) fn new(verified: VerifiedManifest, outcomes: Sender<RendererBridgeOutcome>) -> Self {
        Self {
            verified,
            outcomes,
            subscribed: None,
            ledger: CompatLedger::new(),
        }
    }
    fn send(&self, outcome: RendererBridgeOutcome) -> Result<(), HostAppError> {
        self.outcomes
            .send(outcome)
            .map_err(|_| app_detail("compat native outcome", "existing native endpoint ended"))
    }
    fn subscribed_to(&self, caller: &PrimaryCallerContext) -> bool {
        caller.identity.is_primary()
            && !caller.reader_stop.load(Ordering::Acquire)
            && self
                .subscribed
                .as_ref()
                .is_some_and(|owner| same_caller(owner, caller))
    }
}

fn key(request: &RendererBridgeRequest) -> Option<NativeRouteKey> {
    let navigation = u32::try_from(request.navigation()).ok()?;
    let key = NativeRouteKey {
        webview: request.webview().0,
        navigation,
        message: request.request(),
    };
    (key.valid()
        || (matches!(
            request.operation(),
            RendererBridgeOperation::Retired | RendererBridgeOperation::Registered
        ) && key.webview != 0
            && key.navigation != 0))
        .then_some(key)
}
fn exact_document(document: &MacNativeDocumentLease, key: NativeRouteKey) -> bool {
    document.stamp().view().0 == key.webview
        && document.stamp().observed_generation() == key.navigation
}
fn principal(route: &RendererRoute) -> Principal {
    Principal::Webview {
        id: route.document.stamp().view().0,
        generation: route.document.stamp().observed_generation(),
    }
}
fn query<'a>(
    session: &'a CompatSession,
    route: &'a RendererRoute,
    channel: &'a str,
) -> RendererChannelQuery<'a, 'a> {
    RendererChannelQuery {
        channel,
        current_window: session
            .verified
            .manifest()
            .window_channel_grants(route.window_key()),
    }
}
fn refused(detail: &'static str) -> CallError {
    CallError {
        code: String::from("KELD-WV-011"),
        message: detail.to_owned(),
    }
}
fn native_terminal(disposition: ConsumerDisposition) -> WvTerminalDisposition {
    match disposition {
        ConsumerDisposition::Consumed => WvTerminalDisposition::Consumed,
        ConsumerDisposition::NoListener => WvTerminalDisposition::NoListener,
        ConsumerDisposition::Refused { error } => WvTerminalDisposition::Refused {
            code: error.code,
            message: error.message,
        },
    }
}
fn consumer(disposition: &WvConsumerDisposition) -> ConsumerDisposition {
    match disposition {
        WvConsumerDisposition::Consumed => ConsumerDisposition::Consumed,
        WvConsumerDisposition::NoListener => ConsumerDisposition::NoListener,
        WvConsumerDisposition::Refused { code, message } => ConsumerDisposition::Refused {
            error: CallError {
                code: code.clone(),
                message: message.clone(),
            },
        },
    }
}
fn terminal(disposition: ConsumerDisposition) -> TerminalDisposition {
    match disposition {
        ConsumerDisposition::Consumed => TerminalDisposition::Consumed,
        ConsumerDisposition::NoListener => TerminalDisposition::NoListener,
        ConsumerDisposition::Refused { error } => TerminalDisposition::Refused { error },
    }
}
fn write_response(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    response: &CompatControlResponse,
) -> Result<(), HostAppError> {
    let payload = compat_control::encode_response(response)
        .map_err(|source| app_ipc("compat response", &source))?;
    write_frame(
        &mut active.writer,
        FrameKind::Reply,
        0,
        keld_ipc::channel_table::COMPAT_CONTROL.id(),
        correlation,
        &payload,
    )
    .map_err(|source| app_ipc("compat response writer", &source))
}
fn write_error(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    error: &CallError,
) -> Result<(), HostAppError> {
    keld_ipc::write_call_error(
        &mut active.writer,
        keld_ipc::channel_table::COMPAT_CONTROL.id(),
        correlation,
        error,
    )
    .map_err(|source| app_ipc("compat call refusal", &source))
}

/// The caller retains the current primary and a Registry-qualified route.
/// This observation grants no channel permission or renderer DATA credit.
fn write_document_bound(
    active: &mut ActivePrimaryGeneration,
    route: &RendererRoute,
) -> Result<(), HostAppError> {
    if !route.document.is_current() {
        return Ok(());
    }
    let stamp = route.document.stamp();
    let payload = compat_control::encode_event(&CompatControlEvent::DocumentBound {
        webview: stamp.view().0,
        navigation: stamp.observed_generation(),
        window_id: route.window.id.0,
        window_incarnation: route.window.incarnation.0,
    })
    .map_err(|source| app_ipc("compat document binding", &source))?;
    write_frame(
        &mut active.writer,
        FrameKind::Event,
        0,
        keld_ipc::channel_table::COMPAT_CONTROL.id(),
        CorrelationId(0),
        &payload,
    )
    .map_err(|source| app_ipc("compat document binding writer", &source))
}

impl PrimaryRouterHandle {
    /// Called only with transition/current held and a Registry-qualified native
    /// Registered route for that exact current `WindowOwner`. Windows may be held:
    /// this final availability producer acquires only compat, never current or
    /// windows. It carries no topic permit and cannot authorize any delivery.
    pub(super) fn compat_registered_route_ready_locked(
        &self,
        caller: &PrimaryCallerContext,
        active: &mut ActivePrimaryGeneration,
        route: RendererRoute,
    ) -> Result<(), HostAppError> {
        self.compat_registered_route_state_locked(caller, route, true, Some(active))
    }

    /// Caller owns the same current/Registry retirement cut and passes the
    /// exact affected registered route before clearing its Entry association.
    /// False is published only while that native lease is physically current;
    /// an already-revoked document is retired by its native owner, never by
    /// selecting a successor route from a late old navigation observation.
    pub(super) fn compat_registered_route_retired_locked(
        &self,
        caller: &PrimaryCallerContext,
        route: RendererRoute,
    ) -> Result<(), HostAppError> {
        self.compat_registered_route_state_locked(caller, route, false, None)
    }

    fn compat_registered_route_state_locked(
        &self,
        caller: &PrimaryCallerContext,
        route: RendererRoute,
        ready: bool,
        binding_writer: Option<&mut ActivePrimaryGeneration>,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(());
        };
        let state = compat
            .lock()
            .map_err(|_| app_detail("compat availability", "session lock poisoned"))?;
        if !route.document.is_current()
            || (ready && (!self.shutdown.is_running() || !state.subscribed_to(caller)))
        {
            return Ok(());
        }
        if let Some(active) = binding_writer {
            write_document_bound(active, &route)?;
        }
        let result = state.send(RendererBridgeOutcome::RouteState {
            webview: route.document.stamp().view(),
            navigation: u64::from(route.document.stamp().observed_generation()),
            ready,
            role: RendererRoleWitness::new(Arc::clone(&caller.reader_stop)),
        });
        // Preserve the owned route snapshot through the send, then release it
        // at the existing local-lock-before-parameter end-of-call cut.
        drop(state);
        drop(route);
        result
    }

    /// Replay facts before Subscribe settles, or after a current Registered.
    /// Reuse the same Registry Entries; role loss already clears registration.
    /// Window admission proves that this owner completed its window replay and
    /// could admit the matching load/commit. No successor document is inferred.
    fn compat_registered_document_bounds_locked(
        &self,
        caller: &PrimaryCallerContext,
        active: &mut ActivePrimaryGeneration,
    ) -> Result<(), HostAppError> {
        let Some(windows) = &self.windows else {
            return Ok(());
        };
        let windows = windows
            .lock()
            .map_err(|_| app_detail("compat document binding", "registry lock poisoned"))?;
        let owner = WindowOwner::new(caller.identity, caller.attempt);
        windows.for_each_registered_renderer_route(owner, |route| {
            if windows
                .admitted_renderer_window(owner, route.document.stamp().view())
                .ok()
                != Some(route.window)
            {
                return Ok(());
            }
            write_document_bound(active, &route)
        })
    }

    /// Caller owns transition/current. Enumerate existing registered Entries;
    /// there is no second document list, retained readiness map or new epoch.
    fn compat_registered_routes_state_locked(
        &self,
        caller: &PrimaryCallerContext,
        ready: bool,
    ) -> Result<(), HostAppError> {
        let Some(windows) = &self.windows else {
            return Ok(());
        };
        let windows = windows
            .lock()
            .map_err(|_| app_detail("compat availability", "registry lock poisoned"))?;
        windows.for_each_registered_renderer_route(
            WindowOwner::new(caller.identity, caller.attempt),
            |route| {
                if ready {
                    // The preceding replay already wrote binding facts. Native
                    // availability waits for the successful Subscribed reply.
                    self.compat_registered_route_state_locked(caller, route, true, None)
                } else {
                    self.compat_registered_route_retired_locked(caller, route)
                }
            },
        )
    }

    /// Native Registered is an authenticated identity observation, independent
    /// of whether a Bun link already exists. It consumes no DATA/preflight credit.
    fn note_renderer_registered(
        &self,
        request: &RendererBridgeRequest,
        key: NativeRouteKey,
    ) -> Result<(), HostAppError> {
        let Some(document) = request
            .document_lease()
            .filter(|document| document.is_current() && exact_document(document, key))
        else {
            return Ok(());
        };
        let Some(windows) = &self.windows else {
            return Ok(());
        };
        let registered = windows
            .lock()
            .map_err(|_| app_detail("compat native registration", "registry lock poisoned"))?
            .note_renderer_registered(request.webview(), document.clone());
        // Domain refusal is the default-deny result for stale/wrong-source or
        // unknown native views. It must never create availability or a retry.
        if registered.is_err() {
            return Ok(());
        }
        let caller = self
            .current
            .lock()
            .map_err(|_| app_detail("compat native registration", "generation lock poisoned"))?
            .as_ref()
            .map(|active| PrimaryCallerContext {
                attempt: active.attempt,
                identity: active.identity,
                reader_stop: Arc::clone(&active.reader_stop),
            });
        let Some(caller) = caller else {
            return Ok(());
        };
        with_primary_caller_locked(&self.current, &self.shutdown, &caller, |current| {
            let active = current.as_mut().ok_or_else(|| {
                app_detail("compat native registration", "validated role disappeared")
            })?;
            let Some(compat) = &self.compat else {
                return Ok(());
            };
            let subscribed = compat
                .lock()
                .map_err(|_| app_detail("compat native registration", "session lock poisoned"))?
                .subscribed_to(&caller);
            if subscribed {
                self.compat_registered_document_bounds_locked(&caller, active)?;
            }
            self.compat_registered_routes_state_locked(&caller, true)
        })
    }

    /// The existing actor transfers custody of its actual native input here.
    /// Only legacy Invoke is returned; named input is physically dropped before
    /// native-input quiescence can release its DATA charge.
    pub(super) fn dispatch_renderer_request(
        &self,
        request: RendererBridgeRequest,
    ) -> Result<Option<RendererBridgeRequest>, HostAppError> {
        if matches!(request.operation(), RendererBridgeOperation::Invoke) {
            return Ok(Some(request));
        }
        let physical_input = if matches!(request.operation(), RendererBridgeOperation::Send { .. })
        {
            key(&request).zip(request.document_lease().cloned())
        } else {
            None
        };
        let result = self.dispatch_named_renderer_request(&request);
        drop(request); // releases the actual native payload and charged frame facts
        if let (Some(compat), Some((key, document))) = (&self.compat, physical_input) {
            let mut state = compat.lock().map_err(|_| {
                app_detail("compat inbound physical release", "session lock poisoned")
            })?;
            let exact = state.ledger.data[Direction::RendererToMain.index()]
                .as_ref()
                .is_some_and(|charge| {
                    charge.key == key
                        && charge
                            .document
                            .as_ref()
                            .is_some_and(|held| held.stamp() == document.stamp())
                });
            if exact {
                state.ledger.native_quiesced(Direction::RendererToMain, key);
            }
        }
        result?;
        Ok(None)
    }

    /// Operates on borrowed input while this owner retains its physical charge.
    #[allow(clippy::too_many_lines)] // one native endpoint dispatch keeps final writer/Guard and both release witnesses visible
    fn dispatch_named_renderer_request(
        &self,
        request: &RendererBridgeRequest,
    ) -> Result<bool, HostAppError> {
        if matches!(request.operation(), RendererBridgeOperation::Invoke) {
            return Ok(false);
        }
        let Some(compat) = &self.compat else {
            return Ok(true);
        };
        let Some(key) = key(request) else {
            return Err(app_detail(
                "compat native request",
                "native request has an invalid selector",
            ));
        };
        if matches!(request.operation(), RendererBridgeOperation::Registered) {
            self.note_renderer_registered(request, key)?;
            return Ok(true);
        }
        let caller = {
            let state = compat
                .lock()
                .map_err(|_| app_detail("compat ledger", "session lock poisoned"))?;
            match request.operation() {
                RendererBridgeOperation::Send { .. }
                | RendererBridgeOperation::PreflightAborted => {
                    state.ledger.preflight.as_ref().map(|p| p.caller.clone())
                }
                RendererBridgeOperation::Consumed { .. }
                | RendererBridgeOperation::EventQuiesced { .. } => state.ledger.data
                    [Direction::MainToRenderer.index()]
                .as_ref()
                .map(|c| c.owner.clone()),
                _ => None,
            }
        };
        let caller = if let Some(caller) = caller {
            Some(caller)
        } else if matches!(
            request.operation(),
            RendererBridgeOperation::Preflight { .. } | RendererBridgeOperation::Retired
        ) {
            self.current
                .lock()
                .map_err(|_| app_detail("compat role", "generation lock poisoned"))?
                .as_ref()
                .map(|active| PrimaryCallerContext {
                    attempt: active.attempt,
                    identity: active.identity,
                    reader_stop: Arc::clone(&active.reader_stop),
                })
        } else {
            None
        };
        let Some(caller) = caller else {
            let state = compat
                .lock()
                .map_err(|_| app_detail("compat ledger", "session lock poisoned"))?;
            match request.operation() {
                RendererBridgeOperation::Preflight { .. } => {
                    state.send(RendererBridgeOutcome::Preflight {
                        webview: request.webview(),
                        navigation: request.navigation(),
                        request: request.request(),
                        accepted: false,
                    })?;
                }
                RendererBridgeOperation::Send { .. } => {
                    state.send(RendererBridgeOutcome::SendResult {
                        webview: request.webview(),
                        navigation: request.navigation(),
                        request: request.request(),
                        disposition: WvTerminalDisposition::RouteRetired {
                            certainty: keld_wv::wv_link::WvDispatchCertainty::NotDispatched,
                        },
                    })?;
                }
                _ => {}
            }
            return Ok(true);
        };
        // Completion may outlive current role/document. It updates only the original
        // retained charge; final response still goes through that caller's fence.
        if matches!(
            request.operation(),
            RendererBridgeOperation::Consumed { .. }
                | RendererBridgeOperation::EventQuiesced { .. }
        ) {
            self.complete_native_event(compat, &caller, key, request)?;
            return Ok(true);
        }
        let mut dispatched_to_owner = false;
        with_primary_caller_locked(&self.current, &self.shutdown, &caller, |current| {
            dispatched_to_owner = true;
            let active = current
                .as_mut()
                .ok_or_else(|| app_detail("compat role", "validated role disappeared"))?;
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat ledger", "session lock poisoned"))?;
            // Transition/current are held until admission finishes. Refuse a
            // delayed old-role call before any Registry lookup or reservation.
            if matches!(
                request.operation(),
                RendererBridgeOperation::Preflight { .. }
            ) && !preflight_role_matches(&caller, request.role_witness())
            {
                return state.send(RendererBridgeOutcome::Preflight {
                    webview: request.webview(),
                    navigation: request.navigation(),
                    request: request.request(),
                    accepted: false,
                });
            }
            if matches!(request.operation(), RendererBridgeOperation::Retired) {
                state.ledger.document_retired(key.webview, key.navigation);
                if state.subscribed_to(&caller) {
                    let payload =
                        compat_control::encode_event(&CompatControlEvent::DocumentRetired {
                            webview: key.webview,
                            navigation: key.navigation,
                        })
                        .map_err(|source| app_ipc("compat retirement event", &source))?;
                    write_frame(
                        &mut active.writer,
                        FrameKind::Event,
                        0,
                        keld_ipc::channel_table::COMPAT_CONTROL.id(),
                        CorrelationId(0),
                        &payload,
                    )
                    .map_err(|source| app_ipc("compat retirement writer", &source))?;
                }
                return Ok(());
            }
            if matches!(
                request.operation(),
                RendererBridgeOperation::PreflightAborted
            ) {
                if state.ledger.preflight.as_ref().is_some_and(|p| {
                    p.call == key.message
                        && same_caller(&p.caller, &caller)
                        && request
                            .document_lease()
                            .is_some_and(|lease| lease.stamp() == p.document.stamp())
                }) {
                    state.ledger.preflight = None;
                }
                return Ok(());
            }
            drop(state); // order: transition -> current -> Registry -> compat
            let route = if let Some(windows) = &self.windows {
                let mut windows = windows
                    .lock()
                    .map_err(|_| app_detail("compat window", "registry lock poisoned"))?;
                request
                    .document_lease()
                    .filter(|lease| lease.is_current() && exact_document(lease, key))
                    .and_then(|lease| {
                        windows
                            .admit_renderer_route(
                                WindowOwner::new(caller.identity, caller.attempt),
                                request.webview(),
                                lease.clone(),
                            )
                            .ok()
                    })
            } else {
                None
            };
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat ledger", "session lock poisoned"))?;
            let accepted = request.url().is_some()
                && request.origin().is_some()
                && state.subscribed_to(&caller)
                && route
                    .as_ref()
                    .is_some_and(|route| route.document.is_current());
            match request.operation() {
                RendererBridgeOperation::Preflight { channel } => {
                    let accepted = accepted
                        && !state.ledger.occupied(Direction::RendererToMain)
                        && state.ledger.preflight.is_none()
                        && route.as_ref().is_some_and(|route| {
                            let (Some(url), Some(origin)) = (request.url(), request.origin())
                            else {
                                return false;
                            };
                            preflight_privileged_query(
                                state.verified.manifest(),
                                principal(route),
                                query(&state, route, channel),
                            )
                            .is_ok()
                                && compat_control::encode_event(
                                    &CompatControlEvent::RendererMessage {
                                        webview: key.webview,
                                        navigation: key.navigation,
                                        message: key.message,
                                        window_id: route.window.id.0,
                                        window_incarnation: route.window.incarnation.0,
                                        url: url.to_owned(),
                                        origin: origin.to_owned(),
                                        topic: channel.clone(),
                                        payload: Vec::new(),
                                    },
                                )
                                .is_ok()
                        });
                    if accepted {
                        let route = route.as_ref().ok_or_else(|| {
                            app_detail("compat preflight", "admitted native route disappeared")
                        })?;
                        state.ledger.preflight = Some(PreflightAdmission {
                            caller: caller.clone(),
                            document: route.document.clone(),
                            call: key.message,
                            channel: channel.clone(),
                        });
                    }
                    state.send(RendererBridgeOutcome::Preflight {
                        webview: request.webview(),
                        navigation: request.navigation(),
                        request: key.message,
                        accepted,
                    })
                }
                RendererBridgeOperation::Send { channel } => {
                    let admitted = accepted
                        && state.ledger.preflight.as_ref().is_some_and(|p| {
                            same_caller(&p.caller, &caller)
                                && p.channel == *channel
                                && route.as_ref().is_some_and(|route| {
                                    p.document.stamp() == route.document.stamp()
                                })
                        });
                    if !admitted {
                        return state.send(RendererBridgeOutcome::SendResult {
                            webview: request.webview(),
                            navigation: request.navigation(),
                            request: key.message,
                            disposition: WvTerminalDisposition::Refused {
                                code: String::from("KELD-WV-011"),
                                message: String::from(
                                    "original send no longer has its admitted role and document",
                                ),
                            },
                        });
                    }
                    let route = route.as_ref().ok_or_else(|| {
                        app_detail("compat send", "admitted native route disappeared")
                    })?;
                    let event = CompatControlEvent::RendererMessage {
                        webview: key.webview,
                        navigation: key.navigation,
                        message: key.message,
                        window_id: route.window.id.0,
                        window_incarnation: route.window.incarnation.0,
                        url: request
                            .url()
                            .ok_or_else(|| {
                                app_detail(
                                    "compat native provenance",
                                    "actual native frame URL is unavailable",
                                )
                            })?
                            .to_owned(),
                        origin: request
                            .origin()
                            .ok_or_else(|| {
                                app_detail(
                                    "compat native provenance",
                                    "actual native frame origin is unavailable",
                                )
                            })?
                            .to_owned(),
                        topic: channel.clone(),
                        payload: request.payload().to_vec(),
                    };
                    let payload = compat_control::encode_event(&event)
                        .map_err(|source| app_ipc("compat renderer event", &source))?;
                    // Only this callback can reserve/write. Repeat native currentness
                    // after encoding, then final exact Guard at the actual writer cut.
                    if !route.document.is_current() {
                        return Ok(());
                    }
                    let CompatSession {
                        verified,
                        ledger,
                        outcomes,
                        ..
                    } = &mut *state;
                    let dispatched = dispatch_privileged_query(
                        verified.manifest(),
                        principal(route),
                        RendererChannelQuery {
                            channel,
                            current_window: verified
                                .manifest()
                                .window_channel_grants(route.window_key()),
                        },
                        |_| {
                            if !route.document.is_current() {
                                return Ok(());
                            }
                            ledger
                                .reserve(Direction::RendererToMain, key, &caller, payload.len())
                                .map_err(|_| {
                                    app_detail(
                                        "compat renderer admission",
                                        "bounded renderer DATA lane is occupied",
                                    )
                                })?;
                            if let Some(charge) =
                                &mut ledger.data[Direction::RendererToMain.index()]
                            {
                                charge.document = Some(route.document.clone());
                            }
                            ledger.preflight = None;
                            write_frame(
                                &mut active.writer,
                                FrameKind::Event,
                                0,
                                keld_ipc::channel_table::COMPAT_CONTROL.id(),
                                CorrelationId(0),
                                &payload,
                            )
                            .map_err(|source| app_ipc("compat renderer event writer", &source))?;
                            // The outer custody owner sets native-input quiescence
                            // only after dropping this actual owned native request.
                            Ok(())
                        },
                    );
                    match dispatched {
                        Ok(result) => result,
                        Err(reason) => {
                            ledger.preflight = None;
                            let error = CallError::from(&reason);
                            outcomes
                                .send(RendererBridgeOutcome::SendResult {
                                    webview: request.webview(),
                                    navigation: request.navigation(),
                                    request: key.message,
                                    disposition: WvTerminalDisposition::Refused {
                                        code: error.code,
                                        message: error.message,
                                    },
                                })
                                .map_err(|_| {
                                    app_detail(
                                        "compat native refusal",
                                        "existing native endpoint ended",
                                    )
                                })
                        }
                    }
                }
                _ => Ok(()),
            }
        })?;
        if !dispatched_to_owner {
            let state = compat
                .lock()
                .map_err(|_| app_detail("compat retired admission", "session lock poisoned"))?;
            match request.operation() {
                RendererBridgeOperation::Preflight { .. } => {
                    state.send(RendererBridgeOutcome::Preflight {
                        webview: request.webview(),
                        navigation: request.navigation(),
                        request: request.request(),
                        accepted: false,
                    })?;
                }
                RendererBridgeOperation::Send { .. } => {
                    state.send(RendererBridgeOutcome::SendResult {
                        webview: request.webview(),
                        navigation: request.navigation(),
                        request: request.request(),
                        disposition: WvTerminalDisposition::RouteRetired {
                            certainty: keld_wv::wv_link::WvDispatchCertainty::NotDispatched,
                        },
                    })?;
                }
                _ => {}
            }
        }
        Ok(true)
    }

    /// Called by the sole canonical primary reader after shared header admission.
    #[allow(clippy::too_many_lines)] // sole canonical CALL dispatch keeps role fence, final Guard, and deferred terminal ownership together
    pub(super) fn dispatch_compat(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: Vec<u8>,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Err(app_detail("compat call", "compat service unavailable"));
        };
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current
                .as_mut()
                .ok_or_else(|| app_detail("compat call", "validated role disappeared"))?;
            let request = compat_control::decode_request(&payload)
                .map_err(|source| app_ipc("compat request", &source))?;
            drop(payload); // original canonical frame bytes are physically released before effect admission
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat ledger", "session lock poisoned"))?;
            if !state.subscribed_to(caller) {
                return write_error(
                    active,
                    correlation,
                    &refused("current authenticated primary role has no compat subscription"),
                );
            }
            if state
                .ledger
                .reserve_control(
                    caller,
                    correlation.0,
                    compat_control::COMPAT_CONTROL_FRAME_BYTES,
                )
                .is_err()
            {
                return write_error(active, correlation, &CallError::admission_capacity());
            }
            if let CompatControlRequest::ReleaseConsumed {
                webview,
                navigation,
                message,
                disposition,
            } = request
            {
                let key = NativeRouteKey {
                    webview,
                    navigation,
                    message,
                };
                let matched = state.ledger.data[Direction::RendererToMain.index()]
                    .as_ref()
                    .is_some_and(|charge| charge.key == key && charge.owned_by(caller));
                // This request is the actual WorkerLink post-R_BYTES/R_RECS
                // receipt. Unknown/stale/duplicate receipts free nothing.
                if matched {
                    state
                        .ledger
                        .consumer_released(Direction::RendererToMain, key, caller);
                    state.send(RendererBridgeOutcome::SendResult {
                        webview: WebviewId(webview),
                        navigation: u64::from(navigation),
                        request: message,
                        disposition: native_terminal(disposition),
                    })?;
                }
                let result = write_response(active, correlation, &CompatControlResponse::Released);
                state.ledger.release_control(caller, correlation.0);
                return result;
            }
            drop(state);
            let (webview, navigation, topic, value) = match request {
                CompatControlRequest::DeliverToCurrentMain {
                    webview,
                    topic,
                    payload,
                } => (webview, None, topic, payload),
                CompatControlRequest::DeliverToOriginalDocument {
                    webview,
                    navigation,
                    topic,
                    payload,
                } => (webview, Some(navigation), topic, payload),
                CompatControlRequest::ReleaseConsumed { .. } => {
                    return Err(app_detail(
                        "compat call",
                        "release dispatch ownership changed",
                    ));
                }
            };
            let route = if let Some(windows) = &self.windows {
                windows
                    .lock()
                    .map_err(|_| app_detail("compat window", "registry lock poisoned"))?
                    .renderer_route(
                        WindowOwner::new(caller.identity, caller.attempt),
                        WebviewId(webview),
                        navigation,
                    )
                    .ok()
            } else {
                None
            };
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat ledger", "session lock poisoned"))?;
            let result = (|| {
                let Some(route) = route.filter(|route| route.document.is_current()) else {
                    return write_response(
                        active,
                        correlation,
                        &CompatControlResponse::Deliver {
                            disposition: TerminalDisposition::Stale,
                        },
                    );
                };
                let key = NativeRouteKey {
                    webview,
                    navigation: route.document.stamp().observed_generation(),
                    message: correlation.0,
                };
                if state.ledger.occupied(Direction::MainToRenderer) {
                    return write_error(active, correlation, &CallError::admission_capacity());
                }
                // Own fields are split so the only effect callback can mutate the
                // one ledger while borrowing the same immutable Guard snapshot.
                let CompatSession {
                    verified,
                    ledger,
                    outcomes,
                    ..
                } = &mut *state;
                let guarded = dispatch_privileged_query(
                    verified.manifest(),
                    principal(&route),
                    RendererChannelQuery {
                        channel: &topic,
                        current_window: verified
                            .manifest()
                            .window_channel_grants(route.window_key()),
                    },
                    |_| {
                        if !route.document.is_current() {
                            return write_response(
                                active,
                                correlation,
                                &CompatControlResponse::Deliver {
                                    disposition: TerminalDisposition::Stale,
                                },
                            );
                        }
                        ledger
                            .reserve(Direction::MainToRenderer, key, caller, value.len())
                            .map_err(|_| {
                                app_detail(
                                    "compat native admission",
                                    "bounded native DATA lane is occupied",
                                )
                            })?;
                        if let Some(charge) = &mut ledger.data[Direction::MainToRenderer.index()] {
                            charge.document = Some(route.document.clone());
                            charge.pending_reply = Some(correlation);
                        }
                        outcomes
                            .send(RendererBridgeOutcome::Event {
                                webview: WebviewId(webview),
                                navigation: u64::from(key.navigation),
                                request: key.message,
                                channel: topic.clone(),
                                payload: value,
                            })
                            .map_err(|_| {
                                app_detail(
                                    "compat native event",
                                    "existing native endpoint ended after DATA admission",
                                )
                            })
                    },
                );
                match guarded {
                    Ok(result) => result,
                    Err(reason) => write_error(active, correlation, &CallError::from(&reason)),
                }
            })();
            state.ledger.release_control(caller, correlation.0);
            result
        })
    }

    #[allow(clippy::too_many_lines)] // exact consumer and physical completion jointly own one deferred terminal write
    fn complete_native_event(
        &self,
        compat: &Mutex<CompatSession>,
        caller: &PrimaryCallerContext,
        key: NativeRouteKey,
        request: &RendererBridgeRequest,
    ) -> Result<(), HostAppError> {
        {
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat native completion", "session lock poisoned"))?;
            let Some(charge) = &mut state.ledger.data[Direction::MainToRenderer.index()] else {
                return Ok(());
            };
            if charge.key != key
                || !charge.owned_by(caller)
                || !request.document_lease().is_some_and(|lease| {
                    charge
                        .document
                        .as_ref()
                        .is_some_and(|held| held.stamp() == lease.stamp())
                })
            {
                return Ok(());
            }
            match request.operation() {
                RendererBridgeOperation::Consumed { disposition } => {
                    if charge.consumer_done {
                        return Ok(());
                    }
                    charge.consumer_done = true;
                    charge.terminal = Some(consumer(disposition));
                }
                RendererBridgeOperation::EventQuiesced { completed } => {
                    if charge.native_done {
                        return Ok(());
                    }
                    charge.native_done = true; // actual WK evaluation completion only
                    if !completed {
                        // Actual evaluation failure is not a consumer receipt or
                        // proof of no effect. Retain the charge and let the existing
                        // Fatal/session-tail path perform physical teardown.
                        return Err(app_detail(
                            "compat native evaluation",
                            "native event evaluation failed after a possible original effect; end this session without replaying the delivery",
                        ));
                    }
                }
                _ => return Ok(()),
            }
            if !charge.consumer_done || !charge.native_done {
                return Ok(());
            }
            if charge.pending_reply.is_none() {
                // Retirement has already written its real terminal. Both physical
                // witnesses now allow this exact native pending-event to release.
                state.send(RendererBridgeOutcome::EventReleased {
                    webview: WebviewId(key.webview),
                    navigation: u64::from(key.navigation),
                    request: key.message,
                })?;
                state.ledger.clear_if_complete(Direction::MainToRenderer);
                return Ok(());
            }
        }
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current.as_mut().ok_or_else(|| {
                app_detail("compat native completion", "validated role disappeared")
            })?;
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat native completion", "session lock poisoned"))?;
            let Some(charge) = &state.ledger.data[Direction::MainToRenderer.index()] else {
                return Ok(());
            };
            if charge.key != key
                || !charge.owned_by(caller)
                || !charge.consumer_done
                || !charge.native_done
            {
                return Ok(());
            }
            let Some(correlation) = charge.pending_reply else {
                return Ok(());
            };
            let Some(disposition) = charge.terminal.clone() else {
                return Ok(());
            };
            write_response(
                active,
                correlation,
                &CompatControlResponse::Deliver {
                    disposition: terminal(disposition),
                },
            )?;
            state.send(RendererBridgeOutcome::EventReleased {
                webview: WebviewId(key.webview),
                navigation: u64::from(key.navigation),
                request: key.message,
            })?;
            state.ledger.clear_if_complete(Direction::MainToRenderer);
            Ok(())
        })
    }

    /// Table-derived supported guarded-event subscription, before window fallback.
    pub(super) fn subscribe_compat(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
    ) -> Result<bool, HostAppError> {
        let entry = keld_ipc::channel_table::COMPAT_CONTROL;
        if channel != entry.wire_id() {
            return Ok(false);
        }
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current
                .as_mut()
                .ok_or_else(|| app_detail("compat Subscribe", "validated role disappeared"))?;
            let reason = if !entry.class().carries_host_events()
                || !matches!(
                    entry.authority(),
                    keld_ipc::channel_table::Authority::RendererChannelGuardQuery
                )
                || self.compat.is_none()
            {
                Some(SubscribeRefusal::NotSubscribable)
            } else if !caller.identity.is_primary() {
                Some(SubscribeRefusal::RoleNotAdmitted)
            } else {
                None
            };
            let response = reason.map_or(LifecycleResponse::Subscribed, |reason| {
                LifecycleResponse::SubscribeRefused { reason }
            });
            let accepted = matches!(response, LifecycleResponse::Subscribed);
            let already_subscribed = if accepted && let Some(compat) = &self.compat {
                // An in-progress or failed reply is not a live subscription.
                let mut state = compat
                    .lock()
                    .map_err(|_| app_detail("compat Subscribe", "session lock poisoned"))?;
                let previous = state.subscribed_to(caller);
                state.subscribed = None;
                previous
            } else {
                false
            };
            if accepted {
                // Ordered on the same writer after this owner's window facts.
                // The API applies this replay before settling its Subscribe;
                // no native availability or DATA grant exists at this cut.
                self.compat_registered_document_bounds_locked(caller, active)?;
            }
            let payload =
                encode(&response).map_err(|source| app_ipc("compat Subscribe", &source))?;
            write_frame(
                &mut active.writer,
                FrameKind::Reply,
                0,
                keld_ipc::LIFECYCLE_CHANNEL,
                correlation,
                &payload,
            )
            .map_err(|source| app_ipc("compat Subscribe writer", &source))?;
            if accepted {
                if let Some(compat) = &self.compat {
                    compat
                        .lock()
                        .map_err(|_| app_detail("compat Subscribe", "session lock poisoned"))?
                        .subscribed = Some(caller.clone());
                }
                // Registered may have preceded Subscribe. Only this successful
                // writer cut can now publish availability for those live Entries.
                if !already_subscribed {
                    self.compat_registered_routes_state_locked(caller, true)?;
                }
            }
            Ok(())
        })?;
        Ok(true)
    }

    /// Parent already holds transition/current and calls BEFORE reader-stop/close.
    /// A terminal answer never recycles ambiguous native or ring work.
    pub(super) fn retire_compat_locked(
        &self,
        active: &mut ActivePrimaryGeneration,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(());
        };
        let caller = PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        // Publish false before the owning Window retirement clears Entry
        // registration. Keep cleanup running even if the native endpoint failed.
        let availability = self.compat_registered_routes_state_locked(&caller, false);
        let settlement = (|| {
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat retirement", "session lock poisoned"))?;
            if state
                .subscribed
                .as_ref()
                .is_some_and(|owner| same_caller(owner, &caller))
            {
                state.subscribed = None;
            }
            state.ledger.role_retired(&caller);
            for direction in [Direction::RendererToMain, Direction::MainToRenderer] {
                let Some(charge) = &mut state.ledger.data[direction.index()] else {
                    continue;
                };
                if !charge.owned_by(&caller) {
                    continue;
                }
                let key = charge.key;
                if let Some(correlation) = charge.pending_reply {
                    write_response(
                        active,
                        correlation,
                        &CompatControlResponse::Deliver {
                            disposition: TerminalDisposition::RouteRetired {
                                certainty: compat_control::DispatchCertainty::OutcomeUnknown,
                            },
                        },
                    )?;
                    charge.pending_reply = None;
                }
                if direction == Direction::RendererToMain {
                    state.send(RendererBridgeOutcome::SendResult {
                        webview: WebviewId(key.webview),
                        navigation: u64::from(key.navigation),
                        request: key.message,
                        disposition: WvTerminalDisposition::RouteRetired {
                            certainty: keld_wv::wv_link::WvDispatchCertainty::OutcomeUnknown,
                        },
                    })?;
                }
            }
            Ok(())
        })();
        super::collapse_app_results([availability, settlement])
    }

    /// Consume only the authenticated Runtime owner's actual physical-drain
    /// event. The existing charge retains Core's exact reader-stop Arc; an
    /// attempt number or a new Arc cannot manufacture that original caller.
    pub(super) fn compat_runtime_drained(
        &self,
        principal: keld_runtime::primary::RolePrincipal,
        attempt: u32,
    ) -> Result<(), HostAppError> {
        let caller = if let Some(compat) = &self.compat {
            let state = compat
                .lock()
                .map_err(|_| app_detail("compat Runtime drain", "session lock poisoned"))?;
            state.ledger.data[Direction::RendererToMain.index()]
                .as_ref()
                .filter(|charge| {
                    charge.retired
                        && charge.owner.attempt == attempt
                        && charge.owner.identity == super::PrimaryLinkIdentity::Role(principal)
                })
                .map(|charge| charge.owner.clone())
        } else {
            None
        };
        // Never hold the ledger across the shared drain transition.
        caller
            .as_ref()
            .map_or(Ok(()), |caller| self.compat_framework_drained(caller))
    }

    /// Only Runtime/WorkerLink owner after actual old-process/ring physical drain.
    /// Reader retirement, an EOF, a join alone or a peer field is insufficient.
    pub(super) fn compat_framework_drained(
        &self,
        caller: &PrimaryCallerContext,
    ) -> Result<(), HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(());
        };
        let mut state = compat
            .lock()
            .map_err(|_| app_detail("compat physical drain", "session lock poisoned"))?;
        let direction = Direction::RendererToMain;
        let key = state.ledger.data[direction.index()]
            .as_ref()
            .filter(|charge| charge.owned_by(caller))
            .map(|charge| charge.key);
        if let Some(key) = key {
            state
                .ledger
                .framework_quiesced_after_drain(direction, key, caller);
        }
        Ok(())
    }
}

#[cfg(test)]
mod availability_tests;

#[cfg(test)]
mod tests {
    use super::super::PrimaryLinkIdentity;
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn role() -> PrimaryCallerContext {
        PrimaryCallerContext {
            attempt: 1,
            identity: PrimaryLinkIdentity::Fixture,
            reader_stop: Arc::new(AtomicBool::new(false)),
        }
    }
    fn stamp(sequence: u32) -> NativeRouteKey {
        NativeRouteKey {
            webview: 9,
            navigation: 3,
            message: sequence,
        }
    }

    #[test]
    fn preflight_role_witness_accepts_exact_live_caller() {
        let owner = role();
        let witness = RendererRoleWitness::new(Arc::clone(&owner.reader_stop));
        assert!(preflight_role_matches(&owner, Some(&witness)));
        assert!(preflight_role_matches(
            &owner.clone(),
            Some(&witness.clone())
        ));
    }

    #[test]
    fn preflight_role_witness_rejects_foreign_live_reader() {
        let owner = role();
        let foreign = role();
        assert_eq!(owner.attempt, foreign.attempt);
        assert!(owner.identity == foreign.identity);
        let witness = RendererRoleWitness::new(Arc::clone(&foreign.reader_stop));
        assert!(witness.is_live());
        assert!(!preflight_role_matches(&owner, Some(&witness)));
    }

    #[test]
    fn preflight_role_witness_rejects_retired_owner_and_successor() {
        let owner = role();
        let witness = RendererRoleWitness::new(Arc::clone(&owner.reader_stop));
        owner.reader_stop.store(true, Ordering::Release);
        assert!(!preflight_role_matches(&owner, Some(&witness)));

        let successor = role();
        assert!(!preflight_role_matches(&successor, Some(&witness)));
        let current = RendererRoleWitness::new(Arc::clone(&successor.reader_stop));
        assert!(preflight_role_matches(&successor, Some(&current)));
    }

    #[test]
    fn preflight_role_witness_rejects_missing_owner() {
        assert!(!preflight_role_matches(&role(), None));
    }

    #[test]
    fn one_data_per_direction_and_duplicate_or_early_receipts() {
        let owner = role();
        let mut ledger = CompatLedger::new();
        ledger
            .reserve(Direction::RendererToMain, stamp(4), &owner, 2048)
            .expect("admitted");
        assert_eq!(
            ledger.reserve(Direction::RendererToMain, stamp(5), &owner, 0),
            Err(Admission::AtCapacity)
        );
        ledger.consumer_released(Direction::RendererToMain, stamp(4), &owner);
        assert!(ledger.occupied(Direction::RendererToMain));
        ledger.consumer_released(Direction::RendererToMain, stamp(4), &owner);
        assert!(ledger.occupied(Direction::RendererToMain));
        ledger.native_quiesced(Direction::RendererToMain, stamp(4));
        assert!(!ledger.occupied(Direction::RendererToMain));
        ledger.consumer_released(Direction::RendererToMain, stamp(4), &owner);
        assert_eq!(
            ledger.reserve(Direction::RendererToMain, stamp(5), &owner, 0),
            Ok(())
        );
    }

    #[test]
    fn native_completion_without_after_ring_consumer_receipt_retains_data() {
        let owner = role();
        let mut ledger = CompatLedger::new();
        ledger
            .reserve(Direction::MainToRenderer, stamp(1), &owner, 0)
            .expect("admitted");
        ledger.native_quiesced(Direction::MainToRenderer, stamp(1));
        assert!(ledger.occupied(Direction::MainToRenderer));
        ledger.consumer_released(Direction::MainToRenderer, stamp(1), &owner);
        assert!(!ledger.occupied(Direction::MainToRenderer));
    }

    #[test]
    fn wrong_role_same_numeric_attempt_and_wrong_document_cannot_free_charge() {
        let owner = role();
        let foreign = role();
        let mut ledger = CompatLedger::new();
        ledger
            .reserve(Direction::RendererToMain, stamp(4), &owner, 2)
            .expect("admitted");
        ledger.consumer_released(Direction::RendererToMain, stamp(4), &foreign);
        ledger.consumer_released(Direction::RendererToMain, stamp(99), &owner);
        ledger.native_quiesced(Direction::RendererToMain, stamp(99));
        assert!(ledger.occupied(Direction::RendererToMain));
        ledger.native_quiesced(Direction::RendererToMain, stamp(4));
        assert!(ledger.occupied(Direction::RendererToMain));
        ledger.consumer_released(Direction::RendererToMain, stamp(4), &owner);
        assert!(!ledger.occupied(Direction::RendererToMain));
    }

    #[test]
    fn retirement_never_recycles_ambiguous_work_for_the_successor() {
        let old = role();
        let successor = role();
        let mut ledger = CompatLedger::new();
        ledger
            .reserve(Direction::RendererToMain, stamp(1), &old, 1)
            .expect("admitted");
        ledger.role_retired(&old);
        ledger.document_retired(9, 3);
        assert_eq!(
            ledger.reserve(Direction::RendererToMain, stamp(2), &successor, 1),
            Err(Admission::AtCapacity)
        );
        ledger.consumer_released(Direction::RendererToMain, stamp(1), &successor);
        ledger.native_quiesced(Direction::RendererToMain, stamp(1));
        assert!(ledger.occupied(Direction::RendererToMain));
        ledger.framework_quiesced_after_drain(Direction::RendererToMain, stamp(1), &old);
        assert!(!ledger.occupied(Direction::RendererToMain));
        assert_eq!(
            ledger.reserve(Direction::RendererToMain, stamp(2), &successor, 1),
            Ok(())
        );
    }

    #[test]
    fn four_control_receipts_have_exact_correlation_and_byte_ownership() {
        let mut ledger = CompatLedger::new();
        let owner = role();
        let foreign = role();
        ledger
            .reserve(Direction::RendererToMain, stamp(1), &owner, 4096)
            .expect("data");
        for corr in 1..=4 {
            ledger.reserve_control(&owner, corr, 4096).expect("control");
        }
        assert_eq!(
            ledger.reserve_control(&owner, 5, 1),
            Err(Admission::AtCapacity)
        );
        assert_eq!(
            ledger.reserve_control(&owner, 6, 4097),
            Err(Admission::TooLarge)
        );
        ledger.release_control(&foreign, 1);
        ledger.release_control(&owner, 99);
        assert_eq!(ledger.controls_in_flight(), 4);
        ledger.release_control(&owner, 1);
        ledger.release_control(&owner, 1);
        assert_eq!(ledger.controls_in_flight(), 3);
        assert_eq!(ledger.reserve_control(&owner, 5, 1), Ok(()));
        assert_eq!(ledger.controls_in_flight(), 4);
    }

    #[test]
    fn ring_drain_never_substitutes_for_native_physical_quiescence() {
        let owner = role();
        let mut ledger = CompatLedger::new();
        ledger
            .reserve(Direction::RendererToMain, stamp(8), &owner, 64)
            .expect("renderer DATA");
        ledger.role_retired(&owner);
        ledger.framework_quiesced_after_drain(Direction::RendererToMain, stamp(8), &owner);
        assert!(ledger.occupied(Direction::RendererToMain));
        assert_eq!(ledger.retained_bytes(), 64);
        ledger.native_quiesced(Direction::RendererToMain, stamp(8));
        assert!(!ledger.occupied(Direction::RendererToMain));
    }

    #[test]
    fn framework_ring_drain_cannot_synthesize_native_consumer_completion() {
        let owner = role();
        let mut ledger = CompatLedger::new();
        ledger
            .reserve(Direction::MainToRenderer, stamp(7), &owner, 1024)
            .expect("native DATA");
        ledger.role_retired(&owner);
        ledger.native_quiesced(Direction::MainToRenderer, stamp(7));
        ledger.framework_quiesced_after_drain(Direction::MainToRenderer, stamp(7), &owner);
        assert!(ledger.occupied(Direction::MainToRenderer));
        assert_eq!(ledger.retained_bytes(), 1024);
        ledger.consumer_released(Direction::MainToRenderer, stamp(7), &owner);
        assert!(!ledger.occupied(Direction::MainToRenderer));
    }

    #[test]
    fn invalid_native_ids_and_bytes_refuse_before_retaining() {
        let mut ledger = CompatLedger::new();
        let owner = role();
        for bad in [
            NativeRouteKey {
                webview: 0,
                navigation: 1,
                message: 1,
            },
            NativeRouteKey {
                webview: 1,
                navigation: 0,
                message: 1,
            },
            NativeRouteKey {
                webview: 1,
                navigation: 1,
                message: 0,
            },
        ] {
            assert_eq!(
                ledger.reserve(Direction::RendererToMain, bad, &owner, 0),
                Err(Admission::InvalidNativeIdentity)
            );
        }
        assert_eq!(
            ledger.reserve(Direction::RendererToMain, stamp(1), &owner, 262_145),
            Err(Admission::TooLarge)
        );
        assert!(!ledger.occupied(Direction::RendererToMain));
    }
}
