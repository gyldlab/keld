//! The existing original-call queue, preflight and charged active slot.
//! Native adapters own carrier preparation and physical completion receipts.

use super::active::{ActiveOriginalCall, PendingOriginalSend};
use super::{LocalCallBudget, bridge_error, request_for_lease};
use crate::WvError;
use crate::native_document::NativeDocumentOwner;
use crate::original_binding::OriginalBinding;
use crate::original_event::PendingOriginalEvent;
use crate::renderer_bridge::{
    NativeMessageFacts, RendererBridgeOperation, RendererBridgeRequest, RendererRoleWitness,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc::{SyncSender, TrySendError};

#[cfg(windows)]
mod input;
#[cfg(windows)]
pub(crate) use input::OriginalInputCharge;
mod admission;
mod backing;
mod controls;
mod fault;
#[cfg(windows)]
pub(crate) mod numeric;
mod preparing;
use backing::{PreparationPhase, PreparedBacking};
pub(crate) use backing::{PreparingOriginalIdentity, PreparingOriginalInvocation};
pub(crate) use fault::OriginalControlFault;

/// Ephemeral notification of an actual queue transition. It is not stored
/// authority or another queue; adapters post only the existing service wake
/// after every AppState/document borrow has ended.
pub(crate) struct OriginalQueueProgress<T> {
    pub(crate) value: T,
    pub(crate) needs_service_wake: bool,
}
impl<T> OriginalQueueProgress<T> {
    fn quiet(value: T) -> Self {
        Self {
            value,
            needs_service_wake: false,
        }
    }
    fn wake(value: T) -> Self {
        Self {
            value,
            needs_service_wake: true,
        }
    }
}

pub(crate) struct OriginalCallInput<P> {
    pub(crate) id: u32,
    pub(crate) lease: CheckedNativeDocumentLease,
    pub(crate) payload: P,
    pub(crate) payload_bytes: usize,
    pub(crate) credential: String,
    pub(crate) channel: Option<String>,
    pub(crate) role_witness: Option<RendererRoleWitness>,
    pub(crate) facts: Option<NativeMessageFacts>,
}

struct QueuedOriginalCall<P> {
    id: u32,
    lease: CheckedNativeDocumentLease,
    payload: Option<P>,
    payload_bytes: usize,
    credential: String,
    bytes: usize,
    channel: Option<String>,
    role_witness: Option<RendererRoleWitness>,
    facts: Option<NativeMessageFacts>,
    cancelled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OriginalCallAdmission {
    Waiting,
    Accepted,
    Cancelled,
}
struct PendingOriginalCall<P> {
    queued: QueuedOriginalCall<P>,
    locals_before: usize,
    admission: OriginalCallAdmission,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OriginalInvocationKind {
    Original,
    Refusal,
}

// These are retained scheduling stages, never current authority observations.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PreparationDisposition {
    Local,
    Effect,
    AbortBeforeRefusal,
    Refusal,
    Discard,
}
struct PreparingRecord<P> {
    queued: QueuedOriginalCall<P>,
    disposition: PreparationDisposition,
}
enum ActiveOriginalRecord<P> {
    Preparing(PreparingRecord<P>),
    Entered(ActiveOriginalCall),
}

/// One active/preparing owner. Its backing is allocated once at construction,
/// then reused only after the same call's real API return and completion facts.
/// It adds neither a production slot nor a second queue or byte account.
pub(crate) struct OriginalCallQueue<P> {
    local_calls: VecDeque<QueuedOriginalCall<P>>,
    pending_call: Option<PendingOriginalCall<P>>,
    active_call: Option<ActiveOriginalRecord<P>>,
    backing: Rc<PreparedBacking<P>>,
    local_budget: LocalCallBudget,
    #[cfg(windows)]
    numeric: Option<numeric::NumericResultOwner>,
}
impl<P> OriginalCallQueue<P> {
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn new() -> Result<Self, WvError> {
        let local_budget = LocalCallBudget::with_fixed_storage(backing::allocation_bytes::<P>()?)?;
        Self::with_budget(local_budget)
    }
    fn with_budget(mut local_budget: LocalCallBudget) -> Result<Self, WvError> {
        let slots = super::MAX_LOCAL_CALL_RECORDS;
        let queue_bytes = slots
            .checked_mul(std::mem::size_of::<QueuedOriginalCall<P>>())
            .ok_or_else(|| bridge_error("original queue allocation layout overflow"))?;
        // Reserve requested backing before the one cold allocation. All moved,
        // pending and active record metadata remains separately charged.
        local_budget.reserve_fixed_storage(queue_bytes)?;
        let mut local_calls = VecDeque::new();
        local_calls
            .try_reserve_exact(slots)
            .map_err(|_| bridge_error("original queue cold allocation failed"))?;
        // Do not retain an allocator-advertised capacity outside the amount
        // just prepaid. No fallback growth or second allocation is attempted.
        if local_calls.capacity() != slots {
            drop(local_calls);
            return Err(bridge_error(
                "original queue capacity differs from prepaid storage",
            ));
        }
        Ok(Self {
            local_calls,
            pending_call: None,
            active_call: None,
            backing: Rc::new(PreparedBacking::empty()),
            local_budget,
            #[cfg(windows)]
            numeric: None,
        })
    }
    fn numeric_occupied(&self) -> bool {
        #[cfg(windows)]
        {
            self.numeric
                .as_ref()
                .is_some_and(|owner| !owner.is_drained())
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    fn fixed_storage_bytes(&self) -> Result<usize, WvError> {
        let queue_bytes = self
            .local_calls
            .capacity()
            .checked_mul(std::mem::size_of::<QueuedOriginalCall<P>>())
            .ok_or_else(|| bridge_error("original queue retained layout overflow"))?;
        let bytes = backing::allocation_bytes::<P>()?
            .checked_add(queue_bytes)
            .ok_or_else(|| bridge_error("original fixed storage overflow"))?;
        #[cfg(windows)]
        if self.numeric.is_some() {
            return bytes
                .checked_add(numeric::fixed_bytes()?)
                .ok_or_else(|| bridge_error("original fixed storage overflow"));
        }
        Ok(bytes)
    }
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn has_pending(&self) -> bool {
        self.pending_call.is_some()
    }
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn has_active(&self) -> bool {
        self.active_call.is_some()
    }
    #[cfg(any(
        test,
        all(target_os = "macos", feature = "profile-test-hooks", debug_assertions)
    ))]
    pub(crate) fn local_len(&self) -> usize {
        self.local_calls.len()
    }
    #[cfg(any(
        test,
        all(target_os = "macos", feature = "profile-test-hooks", debug_assertions)
    ))]
    pub(crate) fn retained_records(&self) -> usize {
        self.local_budget.retained_records()
    }
    #[cfg(any(
        test,
        all(target_os = "macos", feature = "profile-test-hooks", debug_assertions)
    ))]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.local_budget.retained_bytes()
    }
    pub(crate) fn is_drained(&self) -> bool {
        self.local_calls.is_empty()
            && self.pending_call.is_none()
            && self.active_call.is_none()
            && self.backing_is_empty()
            && !self.numeric_occupied()
            && self.local_budget.retained_records() == 0
            && self
                .fixed_storage_bytes()
                .is_ok_and(|fixed| self.local_budget.retained_bytes() == fixed)
    }
    pub(crate) fn active(&self) -> Option<&ActiveOriginalCall> {
        match self.active_call.as_ref()? {
            ActiveOriginalRecord::Entered(call) => Some(call),
            ActiveOriginalRecord::Preparing(_) => None,
        }
    }
    pub(crate) fn active_mut(&mut self) -> Option<&mut ActiveOriginalCall> {
        match self.active_call.as_mut()? {
            ActiveOriginalRecord::Entered(call) => Some(call),
            ActiveOriginalRecord::Preparing(_) => None,
        }
    }
    #[cfg(any(
        test,
        all(target_os = "macos", feature = "profile-test-hooks", debug_assertions)
    ))]
    pub(crate) fn pending_observation(
        &self,
    ) -> Option<(
        u32,
        &CheckedNativeDocumentLease,
        usize,
        OriginalCallAdmission,
    )> {
        self.pending_call.as_ref().map(|pending| {
            (
                pending.queued.id,
                &pending.queued.lease,
                pending.locals_before,
                pending.admission,
            )
        })
    }
    pub(crate) fn cancel_pending(&mut self) {
        if let Some(pending) = self.pending_call.as_mut() {
            pending.admission = OriginalCallAdmission::Cancelled;
        }
    }
    fn active_channel(&self) -> Option<&str> {
        match self.active_call.as_ref()? {
            ActiveOriginalRecord::Entered(call) => call.channel(),
            ActiveOriginalRecord::Preparing(record) => matches!(
                record.disposition,
                PreparationDisposition::Effect | PreparationDisposition::AbortBeforeRefusal
            )
            .then_some(record.queued.channel.as_deref())
            .flatten(),
        }
    }
    fn original_current(
        call: &QueuedOriginalCall<P>,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
    ) -> bool {
        binding.original_current(owner, &call.lease, &call.credential)
    }
    fn queued_current(
        call: &QueuedOriginalCall<P>,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
    ) -> bool {
        !call.cancelled
            && Self::original_current(call, binding, owner)
            && (call.channel.is_none()
                || call
                    .role_witness
                    .as_ref()
                    .is_some_and(|role| binding.role_current(role)))
    }
    fn drop_queued(&mut self, call: QueuedOriginalCall<P>) -> Result<(), WvError> {
        let bytes = call.bytes;
        drop(call);
        self.local_budget.release(bytes)
    }
    fn sole_backing(&self) -> bool {
        Rc::strong_count(&self.backing) == 1 && Rc::weak_count(&self.backing) == 0
    }
    fn backing_is_empty(&self) -> bool {
        self.sole_backing()
            && self.backing.phase.get() == PreparationPhase::Empty
            && self.backing.payload.is_none()
    }
    fn backing_available(&self) -> bool {
        self.sole_backing()
            && self.backing.phase.get() == PreparationPhase::Available
            && self.backing.payload.is_some()
    }
    fn entered_ready_to_release(&self) -> bool {
        self.sole_backing()
            && self.backing.phase.get() == PreparationPhase::Returned
            && self.backing.payload.is_some()
            && self
                .active()
                .is_some_and(ActiveOriginalCall::ready_to_release)
    }
    fn drop_backing_payload(&mut self) -> Result<(), WvError> {
        let backing = Rc::get_mut(&mut self.backing).ok_or_else(|| {
            bridge_error("original carrier still has an outstanding native handle")
        })?;
        let payload = backing
            .payload
            .take()
            .ok_or_else(|| bridge_error("original carrier payload disappeared"))?;
        // Drop the actual retained body before resetting the backing or charge.
        drop(payload);
        backing.phase.set(PreparationPhase::Empty);
        Ok(())
    }
    fn clear_entered(&mut self) -> Result<(), WvError> {
        if !self.entered_ready_to_release() {
            return Err(bridge_error(
                "active original release lacks actual API return or completion",
            ));
        }
        self.drop_backing_payload()?;
        let Some(ActiveOriginalRecord::Entered(call)) = self.active_call.take() else {
            return Err(bridge_error(
                "active original release has no entered record",
            ));
        };
        let bytes = call.local_bytes();
        drop(call);
        self.local_budget.release(bytes)
    }
    fn clear_discarded_preparation(&mut self) -> Result<(), WvError> {
        if !self.backing_available() {
            return Err(bridge_error(
                "unentered original carrier still has borrowed or unproved custody",
            ));
        }
        self.drop_backing_payload()?;
        let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.take() else {
            return Err(bridge_error(
                "discarded preparation lost its retained owner",
            ));
        };
        self.drop_queued(record.queued)
    }
    fn enqueue_control(
        endpoint: &SyncSender<RendererBridgeRequest>,
        request: RendererBridgeRequest,
    ) -> Result<bool, WvError> {
        match endpoint.try_send(request) {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => Err(bridge_error(
                "existing bounded renderer endpoint disconnected",
            )),
        }
    }
}

#[cfg(all(test, windows))]
#[path = "queue_tests.rs"]
mod tests;
