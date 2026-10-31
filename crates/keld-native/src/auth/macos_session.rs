//! Native-owned main-thread AS presentation and primitive handoff.
//! No profile, PKCE, grant, registry, caller or capacity policy lives here.
#![allow(unsafe_code)] // KEL-89 typed AS operations; nearest AGENTS.md owns the exact allowance.
#![deny(unsafe_op_in_unsafe_fn)]

use super::OperationId;
use super::primitives::MAX_OAUTH_TEXT_BYTES;
use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::NSWindow;
use objc2_authentication_services::{
    ASWebAuthenticationPresentationContextProviding, ASWebAuthenticationSession,
    ASWebAuthenticationSessionCallback, ASWebAuthenticationSessionErrorDomain,
};
use objc2_foundation::{
    NSError, NSObject, NSObjectProtocol, NSString, NSURL, NSUTF8StringEncoding,
};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError, Weak};
use std::time::Instant;

/// Static host invariant; never contains SDK/provider/caller text or a wire code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsFault {
    /// The service was closed before a start could be submitted.
    Stopping,
    /// An admitted operation was submitted twice or otherwise lost identity.
    Identity,
    /// A private start value was malformed or exceeded the existing text bound.
    StartValue,
    /// No live host-owned presentation window could be resolved.
    Anchor,
    /// `NSObject` initialization failed before presentation.
    ProviderInit,
    /// Primitive handoff state was poisoned.
    StatePoisoned,
    /// SDK callback shape/count violated the one-result contract.
    CallbackProtocol,
    /// Accepted callback or UI-anchor authority did not drain by the supplied bound.
    CleanupUnproved,
}
/// Pre-start presentation unavailability or SDK API/start refusal, eligible for fresh T3 fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsUnavailable {
    /// The selected typed callback API is absent on this runtime.
    CallbackApi,
    /// The actual SDK start returned false.
    StartRefused,
    /// No live presentation context was available before the SDK start call.
    PresentationContext,
}
/// Owned callback text. Its bytes remain private to Native correlation.
pub struct AsCallbackUri(String);
impl AsCallbackUri {
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for AsCallbackUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AsCallbackUri([redacted])")
    }
}
/// Primitive result; no SDK object crosses the main/worker boundary.
pub enum AsOutcome {
    /// A bounded URL for the existing Native state/PKCE correlator.
    CallbackUri(AsCallbackUri),
    /// Exact AS error-domain/canceledLogin result, never automatic retry.
    CanceledLogin,
    /// AS completion reported a missing or invalid presentation context; never retry via T3.
    PresentationContextFailed,
    /// Pre-start context unavailability or API/start refusal; not an absent configuration binding.
    StartUnavailable(AsUnavailable),
    /// Host stop/cancellation, whose cause remains owned by the Native job.
    HostStopped,
    /// Resource-free host invariant; Core retains its existing fatal owner.
    Fault(AsFault),
}
impl fmt::Debug for AsOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CallbackUri(_) => f.write_str("CallbackUri([redacted])"),
            Self::CanceledLogin => f.write_str("CanceledLogin"),
            Self::PresentationContextFailed => f.write_str("PresentationContextFailed"),
            Self::StartUnavailable(reason) => {
                f.debug_tuple("StartUnavailable").field(reason).finish()
            }
            Self::HostStopped => f.write_str("HostStopped"),
            Self::Fault(reason) => f.debug_tuple("Fault").field(reason).finish(),
        }
    }
}
/// Value receipt emitted only after owned stop, callback execution and UI authority drain.
#[derive(Debug)]
pub struct AsReply {
    /// Existing broker identity, never serialized or minted here.
    pub operation: OperationId,
    /// The one primitive terminal result.
    pub outcome: AsOutcome,
}
/// Opaque start value, constructed only by Native after actual guard admission.
pub struct AsStartRequest {
    operation: OperationId,
    authorization_uri: String,
    callback_scheme: String,
    request_end: Instant,
    cleanup_end: Instant,
    cancelled: Arc<AtomicBool>,
}
impl AsStartRequest {
    pub(super) fn after_allow(
        operation: OperationId,
        authorization_uri: String,
        callback_scheme: String,
        request_end: Instant,
        cleanup_end: Instant,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            operation,
            authorization_uri,
            callback_scheme,
            request_end,
            cleanup_end,
            cancelled,
        }
    }
}
impl fmt::Debug for AsStartRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AsStartRequest([redacted])")
    }
}

// Private handoff lock: every guard unlocks before the existing wake receipt.
// Arming then retrying once closes the failed-try/unlock lost-wake interval.
struct Handoff<T> {
    value: Mutex<T>,
    waiting: AtomicBool,
    wake: Arc<dyn Fn() + Send + Sync>,
}
struct UnlockWake<'a> {
    waiting: &'a AtomicBool,
    wake: &'a (dyn Fn() + Send + Sync),
}
impl Drop for UnlockWake<'_> {
    fn drop(&mut self) {
        if self.waiting.swap(false, Ordering::AcqRel) {
            (self.wake)();
        }
    }
}
struct HandoffGuard<'a, T> {
    guard: MutexGuard<'a, T>,
    _wake_after_unlock: UnlockWake<'a>,
    poisoned: bool,
}
impl<T> std::ops::Deref for HandoffGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}
impl<T> std::ops::DerefMut for HandoffGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}
impl<T> Handoff<T> {
    fn new(value: T, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            value: Mutex::new(value),
            waiting: AtomicBool::new(false),
            wake,
        }
    }
    fn try_lock(&self) -> Option<HandoffGuard<'_, T>> {
        for attempt in 0..2 {
            let (guard, poisoned) = match self.value.try_lock() {
                Ok(guard) => (guard, false),
                Err(TryLockError::Poisoned(error)) => (error.into_inner(), true),
                Err(TryLockError::WouldBlock) => {
                    if attempt == 0 {
                        self.waiting.store(true, Ordering::Release);
                        continue;
                    }
                    return None;
                }
            };
            return Some(HandoffGuard {
                guard,
                poisoned,
                _wake_after_unlock: UnlockWake {
                    waiting: &self.waiting,
                    wake: &*self.wake,
                },
            });
        }
        None
    }
}
struct State {
    request: Option<AsStartRequest>,
    claimed: bool,
    accepting_callbacks: bool,
    outcome: Option<AsOutcome>,
    fault: Option<AsFault>,
    receipt: Receiver<AsOutcome>,
}
impl State {
    fn receive(&mut self) {
        if let Ok(outcome) = self.receipt.try_recv() {
            if self.outcome.is_some() {
                self.fault = Some(AsFault::CallbackProtocol);
            } else {
                self.outcome = Some(outcome);
            }
        }
    }
}
#[derive(Clone, Copy)]
#[repr(u8)]
enum StopOrigin {
    AnchorClosing = 1,
    HostTail = 2,
}
struct Context {
    operation: OperationId,
    request_end: Instant,
    cleanup_end: Instant,
    cancelled: Arc<AtomicBool>,
    stop_requested: AtomicBool,
    stop_origin: AtomicU8,
    publication_open: AtomicBool,
    owned_stop_done: AtomicBool,
    provider_required: AtomicBool,
    provider_released: AtomicBool,
    executions: AtomicUsize,
    callback_seen: AtomicBool,
    callback_fault: AtomicBool,
    publish: SyncSender<AsOutcome>,
    state: Handoff<State>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl Context {
    fn try_lock(&self) -> Option<HandoffGuard<'_, State>> {
        let mut state = self.state.try_lock()?;
        if state.poisoned {
            state.fault = Some(AsFault::StatePoisoned);
            state.accepting_callbacks = false;
            self.stop_requested.store(true, Ordering::Release);
        }
        self.reconcile(&mut state);
        Some(state)
    }
    fn reconcile(&self, state: &mut State) {
        if self.callback_fault.load(Ordering::Acquire) {
            state.fault = Some(AsFault::CallbackProtocol);
            state.accepting_callbacks = false;
        }
        state.receive();
    }
    fn protocol_fault(&self) {
        self.callback_fault.store(true, Ordering::Release);
        self.stop_requested.store(true, Ordering::Release);
        (self.wake)();
    }
    fn publish_value(&self, outcome: AsOutcome) {
        // Atomic first-publication/stop edge: a claim before stop is fenced by
        // Execution; a claim after stop cannot enqueue a late accepted result.
        if !self.publication_open.swap(false, Ordering::SeqCst) {
            return;
        }
        match self.publish.try_send(outcome) {
            Ok(()) => {}
            // The actual SDK contract supplies one terminal result; preserve the
            // first owned value and separately record a duplicate/protocol fault.
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => self.protocol_fault(),
        }
        (self.wake)();
    }
    fn set_outcome(&self, outcome: AsOutcome) {
        self.publish_value(outcome);
        self.stop_requested.store(true, Ordering::Release);
    }
    fn request_main_stop(&self, stop: AsMainStop) {
        match stop {
            AsMainStop::Running => self.progress_stop(false),
            AsMainStop::AnchorClosing => self.register_stop(StopOrigin::AnchorClosing),
            AsMainStop::HostTail => self.request_stop(),
        }
    }
    fn request_stop(&self) {
        self.register_stop(StopOrigin::HostTail);
    }
    fn register_stop(&self, origin: StopOrigin) {
        // Record priority and revoke before any contended claim observation.
        let prior = self.stop_origin.fetch_max(origin as u8, Ordering::SeqCst);
        self.publication_open.store(false, Ordering::SeqCst);
        let first_stop = !self.stop_requested.swap(true, Ordering::SeqCst);
        self.progress_stop(first_stop || prior < origin as u8);
    }
    fn progress_stop(&self, wake_for_transition: bool) {
        if !self.stop_requested.load(Ordering::Acquire) {
            return;
        }
        let mut resolved_unclaimed = false;
        if let Some(mut state) = self.try_lock() {
            state.accepting_callbacks = false;
            if !state.claimed {
                // The same claim guard owns classification and resource-free drain.
                state.request.take();
                let anchor_only = self.stop_origin.load(Ordering::SeqCst)
                    == StopOrigin::AnchorClosing as u8
                    && !self.cancelled.load(Ordering::Acquire);
                let replace_local = !anchor_only
                    && matches!(
                        state.outcome.as_ref(),
                        Some(AsOutcome::StartUnavailable(
                            AsUnavailable::PresentationContext
                        ))
                    );
                if state.outcome.is_none() || replace_local {
                    state.outcome = Some(if anchor_only {
                        AsOutcome::StartUnavailable(AsUnavailable::PresentationContext)
                    } else {
                        AsOutcome::HostStopped
                    });
                }
                resolved_unclaimed = !self.owned_stop_done.swap(true, Ordering::AcqRel);
            }
        }
        if wake_for_transition || resolved_unclaimed {
            (self.wake)();
        }
    }
    fn drained(&self) -> bool {
        self.owned_stop_done.load(Ordering::Acquire)
            && self.executions.load(Ordering::SeqCst) == 0
            && (!self.provider_required.load(Ordering::Acquire)
                || self.provider_released.load(Ordering::Acquire))
    }
}
struct Execution(Arc<Context>);
impl Execution {
    fn enter(context: Arc<Context>) -> Option<Self> {
        // Increment before testing revocation. A late rejected shim may briefly
        // count but cannot inspect SDK values or publish after stop. An accepted
        // callback is already counted when main revokes and checks its fence.
        if context
            .executions
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                count.checked_add(1)
            })
            .is_err()
        {
            context.protocol_fault();
            return None;
        }
        let execution = Self(context);
        if execution.0.stop_requested.load(Ordering::SeqCst)
            || execution.0.cancelled.load(Ordering::Acquire)
        {
            return None;
        }
        if execution.0.callback_seen.swap(true, Ordering::AcqRel) {
            execution.0.protocol_fault();
            return None;
        }
        Some(execution)
    }
    fn publish(&self, outcome: AsOutcome) {
        // Values only: no callback pointer/object is queued. Main/worker receive
        // under their existing state owner and original deadlines.
        if !self.0.stop_requested.load(Ordering::Acquire)
            && !self.0.cancelled.load(Ordering::Acquire)
        {
            self.0.publish_value(outcome);
        }
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        if self
            .0
            .executions
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                count.checked_sub(1)
            })
            .is_err()
        {
            self.0.protocol_fault();
        }
        (self.0.wake)();
    }
}

// Field drop order releases this borrowed host-window authority before receipt.
struct AnchorLease {
    window: Retained<NSWindow>,
    _receipt: AnchorReceipt,
}
struct AnchorReceipt(Arc<Context>);
impl Drop for AnchorReceipt {
    fn drop(&mut self) {
        self.0.provider_released.store(true, Ordering::Release);
        (self.0.wake)();
    }
}
struct ProviderIvars {
    anchor: AnchorLease,
}
impl fmt::Debug for ProviderIvars {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AsProviderIvars([redacted])")
    }
}
define_class!(
    // SAFETY: exact final NSObject subclass; initialized Rust ivars.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KeldAuthPresentationProvider"]
    #[ivars = ProviderIvars]
    struct PresentationProvider;
    // SAFETY: initialized NSObject; no additional protocol invariants.
    unsafe impl NSObjectProtocol for PresentationProvider {}
    // SAFETY: SDK protocol is main-thread-only. The Core window lease
    // survives until AnchorReceipt, so even a cached provider cannot access a
    // retired host window. Passive session/block retention is irrelevant here.
    unsafe impl ASWebAuthenticationPresentationContextProviding for PresentationProvider {
        // SAFETY: exact selector and nonnull retained NSWindow return.
        #[unsafe(method_id(presentationAnchorForWebAuthenticationSession:))]
        fn anchor(&self, _session: &ASWebAuthenticationSession) -> Retained<NSWindow> {
            Retained::clone(&self.ivars().anchor.window)
        }
    }
);
impl PresentationProvider {
    fn new(
        mtm: MainThreadMarker,
        window: Retained<NSWindow>,
        context: Arc<Context>,
    ) -> Option<Retained<Self>> {
        let allocated = Self::alloc(mtm).set_ivars(ProviderIvars {
            anchor: AnchorLease {
                window,
                _receipt: AnchorReceipt(context),
            },
        });
        // SAFETY: newly allocated final NSObject with initialized ivars;
        // one consuming super-init; nil is represented by the typed Option.
        unsafe { msg_send![super(allocated), init] }
    }
}
struct Roots {
    session: Retained<ASWebAuthenticationSession>,
    _provider: Retained<PresentationProvider>,
    _completion: RcBlock<dyn Fn(*mut NSURL, *mut NSError)>,
}
struct MainEntry {
    context: Arc<Context>,
    roots: Option<Roots>,
}
impl MainEntry {
    fn stop(&mut self) -> bool {
        // Only the actual stop transition schedules work. Pending callback or
        // provider fences have their own progress wake; polling cannot wake itself.
        if self.context.owned_stop_done.load(Ordering::Acquire) {
            return true;
        }
        self.context.publication_open.store(false, Ordering::SeqCst);
        self.context.stop_requested.store(true, Ordering::SeqCst);
        if let Some(mut state) = self.context.try_lock() {
            state.accepting_callbacks = false;
        } // Atomic revocation already applies; no lock is held over SDK callbacks.
        autoreleasepool(|_| {
            if let Some(roots) = self.roots.take() {
                // SAFETY: actual main-owned session; nullable weak
                // provider setter revokes lookup, cancel is documented idempotent.
                unsafe {
                    roots.session.setPresentationContextProvider(None);
                    roots.session.cancel();
                }
                drop(roots);
            }
        });
        self.context.owned_stop_done.store(true, Ordering::Release);
        (self.context.wake)();
        true
    }
}

struct Shared {
    accepting: AtomicBool,
    slots: Handoff<Vec<Arc<Context>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl Shared {
    fn apply_main_stop(&self, stop: AsMainStop) {
        if stop == AsMainStop::HostTail {
            self.accepting.store(false, Ordering::Release);
        }
    }
}
/// Primitive-only Native worker port. It contains no SDK handle or capacity policy.
#[derive(Clone)]
pub struct AsWorkerPort(Arc<Shared>);
impl fmt::Debug for AsWorkerPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsWorkerPort").finish_non_exhaustive()
    }
}
pub(super) enum AsSubmit {
    Submitted,
    Pending(AsStartRequest),
    Rejected(AsFault),
}
/// Existing worker keeps the admission lease until the value's actual drain receipt.
pub enum AsWorkerProgress {
    /// Keep the original native job and lease.
    Pending,
    /// Actual stop is unproved; keep job/lease through existing fatal cleanup.
    Unproved(AsFault),
    /// Consume the one primitive outcome after actual ownership fences.
    Drained(AsReply),
}
impl AsWorkerPort {
    pub(super) fn submit(&self, request: AsStartRequest) -> AsSubmit {
        if !self.0.accepting.load(Ordering::Acquire) {
            return AsSubmit::Rejected(AsFault::Stopping);
        }
        let Some(mut slots) = self.0.slots.try_lock() else {
            return AsSubmit::Pending(request);
        };
        if slots.poisoned {
            return AsSubmit::Rejected(AsFault::StatePoisoned);
        }
        if !self.0.accepting.load(Ordering::Acquire) {
            return AsSubmit::Rejected(AsFault::Stopping);
        }
        if slots.iter().any(|slot| slot.operation == request.operation) {
            return AsSubmit::Rejected(AsFault::Identity);
        }
        let (publish, receipt) = mpsc::sync_channel(1);
        let context = Arc::new(Context {
            operation: request.operation.clone(),
            request_end: request.request_end,
            cleanup_end: request.cleanup_end,
            cancelled: Arc::clone(&request.cancelled),
            stop_requested: AtomicBool::new(false),
            stop_origin: AtomicU8::new(0),
            publication_open: AtomicBool::new(true),
            owned_stop_done: AtomicBool::new(false),
            provider_required: AtomicBool::new(false),
            provider_released: AtomicBool::new(false),
            executions: AtomicUsize::new(0),
            callback_seen: AtomicBool::new(false),
            callback_fault: AtomicBool::new(false),
            publish,
            state: Handoff::new(
                State {
                    request: Some(request),
                    claimed: false,
                    accepting_callbacks: true,
                    outcome: None,
                    fault: None,
                    receipt,
                },
                Arc::clone(&self.0.wake),
            ),
            wake: Arc::clone(&self.0.wake),
        });
        slots.push(context);
        drop(slots);
        (self.0.wake)();
        AsSubmit::Submitted
    }
    pub(super) fn cancel(&self, operation: &OperationId) {
        if let Some(slots) = self.0.slots.try_lock() {
            let found = slots
                .iter()
                .find(|slot| &slot.operation == operation)
                .cloned();
            drop(slots);
            if let Some(context) = found {
                context.request_stop();
            }
        }
    }
    pub(super) fn poll(&self, operation: &OperationId) -> AsWorkerProgress {
        let Some(mut slots) = self.0.slots.try_lock() else {
            return AsWorkerProgress::Pending;
        };
        if slots.poisoned {
            return AsWorkerProgress::Unproved(AsFault::StatePoisoned);
        }
        let Some(index) = slots.iter().position(|slot| &slot.operation == operation) else {
            return AsWorkerProgress::Unproved(AsFault::Identity);
        };
        let context = Arc::clone(&slots[index]);
        if !self.0.accepting.load(Ordering::Acquire)
            || context.cancelled.load(Ordering::Acquire)
            || Instant::now() >= context.request_end
        {
            context.request_stop();
        }
        let Some(mut state) = context.try_lock() else {
            return AsWorkerProgress::Pending;
        };
        if context.drained() {
            // The acquired count-zero execution fence precedes the final mailbox
            // receive: an accepted publication that raced the earlier receive
            // must be consumed before synthesizing HostStopped/removing its slot.
            context.reconcile(&mut state);
            let outcome = match state.fault {
                Some(fault) => AsOutcome::Fault(fault),
                None => state.outcome.take().unwrap_or(AsOutcome::HostStopped),
            };
            drop(state);
            slots.remove(index);
            return AsWorkerProgress::Drained(AsReply {
                operation: operation.clone(),
                outcome,
            });
        }
        if Instant::now() >= context.cleanup_end {
            return AsWorkerProgress::Unproved(AsFault::CleanupUnproved);
        }
        AsWorkerProgress::Pending
    }
}
/// Host-owned stop domain for the current authentication presentation anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsMainStop {
    /// Continue serving starts against the current resolver.
    Running,
    /// Retire only the current anchor and its presentation authority.
    AnchorClosing,
    /// Accepted global host cutoff; permanently close this service's admission.
    HostTail,
}
/// Main-loop progress; `Unproved` never authorizes view/lease destruction.
#[derive(Debug)]
pub enum AsMainProgress {
    /// Wait only until a future original bound, or for the existing payload-free wake.
    Pending {
        /// Original future bound, or None when waiting solely for the existing wake.
        next_deadline: Option<Instant>,
    },
    /// Retain this service and host window while existing fatal cleanup services the tail.
    Unproved {
        /// Existing Native identity whose authority remains retained.
        operation: OperationId,
        /// Static reason that does not claim physical completion.
        fault: AsFault,
    },
    /// No SDK/UI authority remains on this main owner; Native jobs drain separately.
    Drained,
}
/// Main-thread-only SDK owner; not a new worker, registry or queue-capacity owner.
pub struct AsMainOwner {
    shared: Arc<Shared>,
    entries: Vec<MainEntry>,
    _mtm: MainThreadMarker,
}
impl fmt::Debug for AsMainOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsMainOwner").finish_non_exhaustive()
    }
}
/// Prepares a resource-free pair before the existing worker/engine startup sequence.
/// SDK creation occurs only when driving a Native guard-admitted start value.
pub fn prepare_as_presentation(
    mtm: MainThreadMarker,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> (AsWorkerPort, AsMainOwner) {
    let shared = Arc::new(Shared {
        accepting: AtomicBool::new(true),
        slots: Handoff::new(Vec::new(), Arc::clone(&wake)),
        wake,
    });
    (
        AsWorkerPort(Arc::clone(&shared)),
        AsMainOwner {
            shared,
            entries: Vec::new(),
            _mtm: mtm,
        },
    )
}
impl AsMainOwner {
    fn apply_stop_before_claim(&self, stop: AsMainStop) -> bool {
        self.shared.apply_main_stop(stop);
        // No main SDK owner exists at this startup cut. Worker cancellation
        // still resolves unclaimed slots under their existing claim guard.
        stop == AsMainStop::HostTail && self.entries.is_empty()
    }

    /// Services only admitted starts/stops; the resolver supplies the current live
    /// Core-owned initial window, not a child-selected window or public selector.
    pub fn drive(
        &mut self,
        mtm: MainThreadMarker,
        anchor: &mut dyn FnMut(&OperationId) -> Option<Retained<NSWindow>>,
        stop: AsMainStop,
    ) -> AsMainProgress {
        if self.apply_stop_before_claim(stop) {
            return AsMainProgress::Drained;
        }
        let slots = match self.shared.slots.try_lock() {
            Some(slots) if !slots.poisoned => slots.clone(),
            Some(_) => {
                return match self.entries.first() {
                    Some(entry) => AsMainProgress::Unproved {
                        operation: entry.context.operation.clone(),
                        fault: AsFault::StatePoisoned,
                    },
                    None => AsMainProgress::Pending {
                        next_deadline: None,
                    },
                };
            }
            // Unlock receipt on the actual holder sends the existing main wake.
            None => Vec::new(),
        };
        let now = Instant::now();
        for context in &slots {
            context.request_main_stop(stop);
            if !self.shared.accepting.load(Ordering::Acquire)
                || context.cancelled.load(Ordering::Acquire)
                || now >= context.request_end
            {
                context.request_stop();
            }
            let request = {
                let Some(mut state) = context.try_lock() else {
                    continue;
                };
                if !state.claimed && !context.stop_requested.load(Ordering::Acquire) {
                    state.claimed = true;
                    state.request.take()
                } else {
                    None
                }
            };
            if let Some(request) = request {
                let roots = Self::start_roots(mtm, anchor, context, &request);
                self.entries.push(MainEntry {
                    context: Arc::clone(context),
                    roots,
                });
                if let Some(entry) = self.entries.last_mut()
                    && let Some(roots) = &entry.roots
                {
                    if context.stop_requested.load(Ordering::Acquire)
                        || context.cancelled.load(Ordering::Acquire)
                        || Instant::now() >= context.request_end
                    {
                        context.request_stop();
                    // SAFETY: roots are already installed on the
                    // actual main owner, one call on the selected live session.
                    } else if !unsafe { roots.session.start() } {
                        context
                            .set_outcome(AsOutcome::StartUnavailable(AsUnavailable::StartRefused));
                    }
                }
            }
        }
        let mut next_deadline = None;
        let mut unproved = None;
        self.entries.retain_mut(|entry| {
            let stop = match entry.context.try_lock() {
                Some(state) => {
                    state.outcome.is_some()
                        || state.fault.is_some()
                        || entry.context.stop_requested.load(Ordering::Acquire)
                }
                None => false,
            };
            if stop {
                entry.stop();
            }
            if entry.context.drained() {
                return false;
            }
            let bound = if entry.context.stop_requested.load(Ordering::Acquire) {
                entry.context.cleanup_end
            } else {
                entry.context.request_end
            };
            if bound > now {
                next_deadline = Some(next_deadline.map_or(bound, |old: Instant| old.min(bound)));
            } else if now >= entry.context.cleanup_end {
                unproved = Some((entry.context.operation.clone(), AsFault::CleanupUnproved));
            }
            true
        });
        if let Some((operation, fault)) = unproved {
            return AsMainProgress::Unproved { operation, fault };
        }
        if self.entries.is_empty()
            && (stop != AsMainStop::Running || !self.shared.accepting.load(Ordering::Acquire))
        {
            AsMainProgress::Drained
        } else {
            AsMainProgress::Pending { next_deadline }
        }
    }
    fn start_roots(
        mtm: MainThreadMarker,
        anchor: &mut dyn FnMut(&OperationId) -> Option<Retained<NSWindow>>,
        context: &Arc<Context>,
        request: &AsStartRequest,
    ) -> Option<Roots> {
        let fail = |outcome| {
            context.set_outcome(outcome);
        };
        if request.authorization_uri.len() > MAX_OAUTH_TEXT_BYTES
            || request.callback_scheme.is_empty()
        {
            fail(AsOutcome::Fault(AsFault::StartValue));
            return None;
        }
        if context.stop_requested.load(Ordering::Acquire)
            || request.cancelled.load(Ordering::Acquire)
        {
            fail(AsOutcome::HostStopped);
            return None;
        }
        if AnyClass::get(c"ASWebAuthenticationSessionCallback").is_none() {
            fail(AsOutcome::StartUnavailable(AsUnavailable::CallbackApi));
            return None;
        }
        let Some(window) = anchor(&request.operation) else {
            fail(AsOutcome::StartUnavailable(
                AsUnavailable::PresentationContext,
            ));
            return None;
        };
        autoreleasepool(|_| {
            let Some(url) = NSURL::URLWithString(&NSString::from_str(&request.authorization_uri))
            else {
                fail(AsOutcome::Fault(AsFault::StartValue));
                return None;
            };
            let Some(provider) = PresentationProvider::new(mtm, window, Arc::clone(context)) else {
                fail(AsOutcome::Fault(AsFault::ProviderInit));
                return None;
            };
            context.provider_required.store(true, Ordering::Release);
            let weak: Weak<Context> = Arc::downgrade(context);
            let completion: RcBlock<dyn Fn(*mut NSURL, *mut NSError)> =
                RcBlock::new(move |url: *mut NSURL, error: *mut NSError| {
                    let Some(context) = weak.upgrade() else {
                        return;
                    };
                    let Some(execution) = Execution::enter(context) else {
                        return;
                    };
                    autoreleasepool(|_| {
                        // SAFETY: nullable SDK callback objects are borrowed
                        // for this invocation only. No pointer/SDK object escapes.
                        let outcome =
                            callback_value(unsafe { url.as_ref() }, unsafe { error.as_ref() });
                        execution.publish(outcome);
                    });
                });
            // SAFETY: actual main-thread-only use; selected class was
            // checked available, owned immutable scheme and copied block stay live.
            let callback = unsafe {
                ASWebAuthenticationSessionCallback::callbackWithCustomScheme(&NSString::from_str(
                    &request.callback_scheme,
                ))
            };
            // SAFETY: exact typed constructor; RcBlock::as_ptr supplies
            // the valid block pointer without cast; SDK copies it for its lifetime.
            let session = unsafe {
                ASWebAuthenticationSession::initWithURL_callback_completionHandler(
                    ASWebAuthenticationSession::alloc(),
                    &url,
                    &callback,
                    RcBlock::as_ptr(&completion),
                )
            };
            // SAFETY: typed, initialized main-only provider; host strong
            // ownership is kept separately because the SDK property is weak.
            unsafe {
                session.setPresentationContextProvider(Some(ProtocolObject::from_ref(&*provider)));
            }
            let roots = Roots {
                session,
                _provider: provider,
                _completion: completion,
            };
            Some(roots)
        })
    }
}
impl Drop for AsMainOwner {
    fn drop(&mut self) {
        self.shared.accepting.store(false, Ordering::Release);
        for entry in &mut self.entries {
            entry.context.stop_requested.store(true, Ordering::Release);
            entry.stop();
        }
        // MainThreadMarker makes this owner non-Send. Drop is fallback only.
    }
}
fn callback_value(url: Option<&NSURL>, error: Option<&NSError>) -> AsOutcome {
    match (url, error) {
        (Some(url), None) => {
            let Some(text) = url.absoluteString() else {
                return AsOutcome::Fault(AsFault::CallbackProtocol);
            };
            if text.lengthOfBytesUsingEncoding(NSUTF8StringEncoding) > MAX_OAUTH_TEXT_BYTES {
                return AsOutcome::Fault(AsFault::CallbackProtocol);
            }
            AsOutcome::CallbackUri(AsCallbackUri(text.to_string()))
        }
        (None, Some(error)) => {
            // SAFETY: immutable SDK error-domain symbol, linked by the
            // selected framework; borrowed error/domain values never escape.
            let expected_domain = unsafe { ASWebAuthenticationSessionErrorDomain };
            if !error.domain().isEqualToString(expected_domain) {
                return AsOutcome::Fault(AsFault::CallbackProtocol);
            }
            callback_error_outcome(error.code())
        }
        _ => AsOutcome::Fault(AsFault::CallbackProtocol),
    }
}

fn callback_error_outcome(code: isize) -> AsOutcome {
    match code {
        1 => AsOutcome::CanceledLogin,
        2 | 3 => AsOutcome::PresentationContextFailed,
        _ => AsOutcome::Fault(AsFault::CallbackProtocol),
    }
}

#[cfg(test)]
pub(super) fn assert_stop_wake_contract(operation: OperationId) {
    // Primitive component regression only: no actual AS session/window/provider.
    // The real Execution/AnchorReceipt/stop owners supply the local wake oracles.
    let wakes = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&wakes);
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    let (publish, receipt) = mpsc::sync_channel(1);
    let request_end = Instant::now()
        .checked_add(std::time::Duration::from_secs(1))
        .expect("original request bound");
    let cleanup_end = request_end
        .checked_add(std::time::Duration::from_secs(1))
        .expect("original cleanup bound");
    let context = Arc::new(Context {
        operation,
        request_end,
        cleanup_end,
        cancelled: Arc::new(AtomicBool::new(false)),
        stop_requested: AtomicBool::new(false),
        stop_origin: AtomicU8::new(0),
        publication_open: AtomicBool::new(true),
        owned_stop_done: AtomicBool::new(false),
        provider_required: AtomicBool::new(true),
        provider_released: AtomicBool::new(false),
        executions: AtomicUsize::new(0),
        callback_seen: AtomicBool::new(false),
        callback_fault: AtomicBool::new(false),
        publish,
        state: Handoff::new(
            State {
                request: None,
                claimed: true,
                accepting_callbacks: true,
                outcome: None,
                fault: None,
                receipt,
            },
            Arc::clone(&wake),
        ),
        wake,
    });
    let execution =
        Execution::enter(Arc::clone(&context)).expect("accepted primitive execution guard");
    let mut entry = MainEntry {
        context: Arc::clone(&context),
        roots: None,
    };
    context.request_stop();
    assert!(entry.stop());
    assert_eq!(wakes.load(Ordering::SeqCst), 2);
    assert!(
        !context.drained(),
        "held execution/provider fences remain required"
    );
    for _ in 0..100 {
        context.request_stop();
        assert!(entry.stop());
    }
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        2,
        "no repeated polling self-wake"
    );
    assert_eq!(context.request_end, request_end);
    assert_eq!(context.cleanup_end, cleanup_end);
    let guard = context.try_lock().expect("actual state guard");
    context.request_stop(); // contention still arms the existing after-unlock wake
    assert_eq!(wakes.load(Ordering::SeqCst), 2);
    drop(guard);
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        3,
        "no deferred stop lost wake"
    );
    drop(AnchorReceipt(Arc::clone(&context)));
    assert_eq!(wakes.load(Ordering::SeqCst), 4);
    assert!(
        !context.drained(),
        "accepted callback execution still retained"
    );
    drop(execution);
    assert_eq!(wakes.load(Ordering::SeqCst), 5);
    assert!(context.drained(), "same actual primitive fences now passed");
}

#[cfg(test)]
pub(super) fn test_as_cancellation_port() -> AsWorkerPort {
    // Primitive consumer fixture only; no main marker, SDK roots or window.
    AsWorkerPort(Arc::new(Shared {
        accepting: AtomicBool::new(true),
        slots: Handoff::new(Vec::new(), Arc::new(|| {})),
        wake: Arc::new(|| {}),
    }))
}

#[cfg(test)]
pub(super) fn test_deliver_canceled_login(port: &AsWorkerPort, operation: &OperationId) {
    let context = port
        .0
        .slots
        .try_lock()
        .expect("primitive submitted queue")
        .iter()
        .find(|slot| &slot.operation == operation)
        .cloned()
        .expect("actual admitted operation context");
    let mut state = context.try_lock().expect("actual request claim guard");
    assert!(!state.claimed);
    assert!(state.request.take().is_some());
    state.claimed = true;
    drop(state);
    let execution = Execution::enter(Arc::clone(&context))
        .expect("actual accepted primitive callback execution");
    execution.publish(AsOutcome::CanceledLogin);
    drop(execution);
    // roots=None qualifies the consumer's primitive receipt, not SDK cancel.
    let mut entry = MainEntry {
        context,
        roots: None,
    };
    assert!(entry.stop());
}

#[cfg(test)]
pub(super) fn test_deliver_callback_error(
    port: &AsWorkerPort,
    operation: &OperationId,
    code: isize,
) {
    let context = port
        .0
        .slots
        .try_lock()
        .expect("primitive submitted queue")
        .iter()
        .find(|slot| &slot.operation == operation)
        .cloned()
        .expect("actual admitted operation context");
    let mut state = context.try_lock().expect("actual request claim guard");
    assert!(!state.claimed);
    assert!(state.request.take().is_some());
    state.claimed = true;
    drop(state);
    let execution = Execution::enter(Arc::clone(&context))
        .expect("accepted session completion callback execution");
    execution.publish(callback_error_outcome(code));
    drop(execution);
    let mut entry = MainEntry {
        context,
        roots: None,
    };
    assert!(entry.stop());
}

#[cfg(test)]
pub(super) fn test_apply_main_stop(port: &AsWorkerPort, stop: AsMainStop) -> bool {
    port.0.apply_main_stop(stop);
    let contexts = port
        .0
        .slots
        .try_lock()
        .expect("primitive stop queue")
        .clone();
    for context in contexts {
        context.request_main_stop(stop);
    }
    port.0.accepting.load(Ordering::Acquire)
}

#[cfg(test)]
pub(super) fn test_active_anchor_retirement(port: &AsWorkerPort, operation: &OperationId) {
    // Actual primitive owners only; roots=None cannot qualify SDK cancellation.
    let context = port
        .0
        .slots
        .try_lock()
        .expect("submitted primitive queue")
        .iter()
        .find(|slot| &slot.operation == operation)
        .cloned()
        .expect("actual admitted context");
    let request_end = context.request_end;
    let cleanup_end = context.cleanup_end;
    let mut state = context.try_lock().expect("actual claim guard");
    assert!(!state.claimed);
    assert!(state.request.take().is_some());
    state.claimed = true;
    drop(state);
    context.provider_required.store(true, Ordering::Release);
    let receipt = AnchorReceipt(Arc::clone(&context));
    let execution = Execution::enter(Arc::clone(&context)).expect("accepted execution");
    assert!(test_apply_main_stop(port, AsMainStop::AnchorClosing));
    let mut entry = MainEntry {
        context: Arc::clone(&context),
        roots: None,
    };
    assert!(entry.stop());
    assert!(matches!(port.poll(operation), AsWorkerProgress::Pending));
    execution.publish(AsOutcome::CanceledLogin); // Publication is revoked locally.
    drop(execution);
    assert!(matches!(port.poll(operation), AsWorkerProgress::Pending));
    drop(receipt);
    assert!(context.drained());
    assert_eq!(context.request_end, request_end);
    assert_eq!(context.cleanup_end, cleanup_end);
}

#[cfg(test)]
pub(super) fn test_contended_anchor_stop(port: &AsWorkerPort, operation: &OperationId) {
    let context = port
        .0
        .slots
        .try_lock()
        .expect("submitted primitive queue")
        .iter()
        .find(|slot| &slot.operation == operation)
        .cloned()
        .expect("actual admitted context");
    let request_end = context.request_end;
    let cleanup_end = context.cleanup_end;
    let guard = context.try_lock().expect("held real claim guard");
    assert!(!guard.claimed);
    assert!(test_apply_main_stop(port, AsMainStop::AnchorClosing));
    assert!(
        context.stop_requested.load(Ordering::SeqCst),
        "stop must register before lock succeeds"
    );
    assert!(
        !context.publication_open.load(Ordering::SeqCst),
        "publication revoked before lock succeeds"
    );
    assert!(!guard.claimed);
    assert!(guard.request.is_some());
    drop(guard);
    assert!(test_apply_main_stop(port, AsMainStop::Running));
    assert!(context.owned_stop_done.load(Ordering::Acquire));
    let state = context.try_lock().expect("resolved original claim guard");
    assert!(
        !state.claimed,
        "a closing anchor cannot claim/start a request"
    );
    assert!(state.request.is_none());
    assert!(matches!(
        state.outcome.as_ref(),
        Some(AsOutcome::StartUnavailable(
            AsUnavailable::PresentationContext
        ))
    ));
    assert_eq!(context.request_end, request_end);
    assert_eq!(context.cleanup_end, cleanup_end);
}

#[cfg(test)]
pub(super) fn test_assert_unclaimed_host_priority(port: &AsWorkerPort, operation: &OperationId) {
    let context = port
        .0
        .slots
        .try_lock()
        .expect("submitted primitive queue")
        .iter()
        .find(|slot| &slot.operation == operation)
        .cloned()
        .expect("actual admitted context");
    let state = context.try_lock().expect("actual resolved host stop");
    assert!(!state.claimed);
    assert!(matches!(
        state.outcome.as_ref(),
        Some(AsOutcome::HostStopped)
    ));
}

#[cfg(test)]
mod callback_error_tests {
    use super::{AsOutcome, callback_error_outcome};

    #[test]
    fn accepted_session_presentation_errors_are_not_prestart_fallbacks() {
        assert!(matches!(
            callback_error_outcome(1),
            AsOutcome::CanceledLogin
        ));
        for code in [2_isize, 3] {
            assert!(
                matches!(
                    callback_error_outcome(code),
                    AsOutcome::PresentationContextFailed
                ),
                "AS completion error {code} must not replay a fresh loopback login"
            );
        }
    }
}
