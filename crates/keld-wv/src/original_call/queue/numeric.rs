//! Numeric Echo reuses this original queue's `LocalCallBudget` and one pending
//! numeric record. A cold release witness crosses Core; it is no new ledger.

use super::{LocalCallBudget, OriginalCallQueue, WvError, bridge_error};
use super::{PendingOriginalEvent, PendingOriginalSend};
use crate::WebviewId;
use crate::native_document::NativeDocumentOwner;
use crate::renderer_bridge::{
    RendererBridgeOperation, RendererBridgeOutcome, RendererBridgeRequest,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::webview2::WindowsLoopEvent;
use std::sync::{
    Arc,
    mpsc::{SyncSender, TrySendError},
};
use tao::event_loop::EventLoopProxy;

mod storage;
pub use storage::NativeNumericResultReservation;
use storage::{RESULT_STORAGE_BYTES, StorageSignal};

/// The request owns the SAME storage token until actual Core handoff. No
/// cancellation may erase this record while Core or native output owns it.
struct PendingNumericCall {
    outgoing: Option<RendererBridgeRequest>,
    document: CheckedNativeDocumentLease,
    request: u32,
    bytes: usize,
    cancelled: bool,
    result_observed: bool,
    original_role: Option<crate::renderer_bridge::RendererRoleWitness>,
}
pub(super) struct NumericResultOwner {
    signal: Arc<StorageSignal>,
    pending: Option<PendingNumericCall>,
}
impl NumericResultOwner {
    pub(super) fn is_drained(&self) -> bool {
        self.pending.is_none() && StorageSignal::reusable(&self.signal)
    }
}
pub(super) fn fixed_bytes() -> Result<usize, WvError> {
    storage::allocation_bytes()?
        .checked_add(std::mem::size_of::<Option<NumericResultOwner>>())
        .ok_or_else(|| bridge_error("numeric fixed storage overflow"))
}

fn record_bytes(request_payload_capacity: usize) -> Result<usize, WvError> {
    RESULT_STORAGE_BYTES
        .checked_add(std::mem::size_of::<PendingNumericCall>())
        .and_then(|bytes| bytes.checked_add(request_payload_capacity))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<RendererBridgeRequest>()))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<NativeNumericResultReservation>()))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<RendererBridgeOutcome>()))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Vec<u16>>()))
        .ok_or_else(|| bridge_error("numeric result reservation overflow"))
}

impl<P> OriginalCallQueue<P> {
    /// Production constructs this once for its real view before callback install.
    /// The reusable witness allocation is paid by the SAME byte counter first.
    pub(crate) fn new_with_numeric_result_slot(
        wake: EventLoopProxy<WindowsLoopEvent>,
        view: WebviewId,
    ) -> Result<Self, WvError> {
        Self::new_with_numeric_slot(Some(wake), view)
    }

    fn new_with_numeric_slot(
        wake: Option<EventLoopProxy<WindowsLoopEvent>>,
        view: WebviewId,
    ) -> Result<Self, WvError> {
        let fixed = super::backing::allocation_bytes::<P>()?
            .checked_add(fixed_bytes()?)
            .ok_or_else(|| bridge_error("original fixed storage overflow"))?;
        let budget = LocalCallBudget::with_fixed_storage(fixed)?;
        let mut queue = Self::with_budget(budget)?;
        queue.numeric = Some(NumericResultOwner {
            signal: Arc::new(StorageSignal::new(wake, view)),
            pending: None,
        });
        Ok(queue)
    }

    #[cfg(test)]
    pub(crate) fn has_numeric_result(&self) -> bool {
        self.numeric_occupied()
    }

    pub(super) fn reconcile_numeric_owner(&mut self, owner: &NativeDocumentOwner) {
        if let Some(pending) = self
            .numeric
            .as_mut()
            .and_then(|owner| owner.pending.as_mut())
            && !pending.document.belongs_to_owner(owner)
        {
            pending.cancelled = true;
        }
    }

    /// Reserve result storage BEFORE taking the actual request/Core admission.
    /// Refusal leaves the same input and any existing token wholly with caller.
    pub(crate) fn begin_numeric_result(
        &mut self,
        input: &mut Option<RendererBridgeRequest>,
        owner: &NativeDocumentOwner,
        pending_send: Option<&PendingOriginalSend>,
        pending_event: Option<&PendingOriginalEvent>,
    ) -> Result<(), WvError> {
        let request = input
            .as_ref()
            .ok_or_else(|| bridge_error("numeric request is absent"))?;
        let document = request
            .document_lease
            .as_ref()
            .filter(|document| document.belongs_to_owner(owner))
            .ok_or_else(|| bridge_error("numeric request lacks its exact current native lease"))?;
        let numeric = self
            .numeric
            .as_ref()
            .ok_or_else(|| bridge_error("numeric result storage is not installed"))?;
        if !matches!(request.operation, RendererBridgeOperation::Invoke)
            || request.request == 0
            || request.payload.len() > crate::wv_link::MAX_RENDERER_PAYLOAD_LEN
            || request.numeric_result.is_some()
            || request.webview != numeric.signal.view()
            || request.navigation != u64::from(document.stamp().observed_generation())
            || request.webview != document.stamp().view()
            || numeric.pending.is_some()
            || !StorageSignal::reusable(&numeric.signal)
            || self.pending_call.is_some()
            || self.active_call.is_some()
            || pending_send.is_some()
            || pending_event.is_some()
        {
            return Err(bridge_error(
                "numeric result cannot enter the existing DATA floor",
            ));
        }
        let document = document.clone();
        let id = request.request;
        let original_role = request.role_witness.clone();
        let bytes = record_bytes(request.payload.capacity())?;
        self.local_budget.reserve(bytes)?;
        let Some(mut request) = input.take() else {
            self.local_budget.release(bytes)?;
            return Err(bridge_error("numeric input disappeared before attachment"));
        };
        // No native/user/fallible call intervenes between take and attachment.
        let Some(numeric) = self.numeric.as_mut() else {
            *input = Some(request);
            self.local_budget.release(bytes)?;
            return Err(bridge_error("numeric owner disappeared before attachment"));
        };
        request.numeric_result = Some(NativeNumericResultReservation::mint(
            &numeric.signal,
            document.clone(),
            id,
        ));
        numeric.pending = Some(PendingNumericCall {
            outgoing: Some(request),
            document,
            request: id,
            bytes,
            cancelled: false,
            result_observed: false,
            original_role,
        });
        Ok(())
    }

    /// Full returns the exact same request/token to this SAME pending record.
    /// No caller-selected timeout or logical cancellation frees physical storage.
    pub(crate) fn flush_numeric_request(
        &mut self,
        owner: &NativeDocumentOwner,
        endpoint: &SyncSender<RendererBridgeRequest>,
    ) -> Result<bool, WvError> {
        let Some(pending) = self
            .numeric
            .as_mut()
            .and_then(|owner| owner.pending.as_mut())
        else {
            return Ok(false);
        };
        if !pending.document.belongs_to_owner(owner) {
            pending.cancelled = true;
        }
        if pending.cancelled {
            return Ok(false);
        }
        let Some(request) = pending.outgoing.take() else {
            return Ok(false);
        };
        match endpoint.try_send(request) {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(request)) => {
                pending.outgoing = Some(request);
                Ok(false)
            }
            Err(TrySendError::Disconnected(request)) => {
                pending.outgoing = Some(request);
                Err(bridge_error(
                    "numeric endpoint disconnected with original request retained",
                ))
            }
        }
    }

    /// Borrow original correlation/role only; never obtain a successor's role.
    pub(crate) fn numeric_original(
        &self,
        view: WebviewId,
        navigation: u64,
        request: u32,
    ) -> Option<(
        &CheckedNativeDocumentLease,
        Option<&crate::renderer_bridge::RendererRoleWitness>,
    )> {
        let pending = self.numeric.as_ref()?.pending.as_ref()?;
        (pending.request == request
            && pending.document.stamp().view() == view
            && u64::from(pending.document.stamp().observed_generation()) == navigation)
            .then_some((&pending.document, pending.original_role.as_ref()))
    }

    /// Called only after that exact outcome is attached to its native operation.
    /// Observing a result is separate from its storage disposal or native entry.
    pub(crate) fn observe_numeric_result(
        &mut self,
        view: WebviewId,
        navigation: u64,
        request: u32,
        storage: &NativeNumericResultReservation,
    ) -> bool {
        let Some(numeric) = self.numeric.as_mut() else {
            return false;
        };
        let Some(pending) = numeric.pending.as_mut() else {
            return false;
        };
        if pending.outgoing.is_some()
            || pending.result_observed
            || pending.request != request
            || !storage.belongs_to(&numeric.signal)
            || !storage.matches_request(view, navigation, request, &pending.document)
        {
            return false;
        }
        pending.result_observed = true;
        true
    }

    pub(crate) fn cancel_numeric_result(&mut self) {
        if let Some(pending) = self
            .numeric
            .as_mut()
            .and_then(|owner| owner.pending.as_mut())
        {
            pending.cancelled = true;
        }
    }

    /// Return proved never-handed-off input for disposal OUTSIDE the bridge's
    /// interior borrow: its token Drop posts the existing service notification.
    pub(crate) fn take_cancelled_numeric_request(&mut self) -> Option<RendererBridgeRequest> {
        let pending = self.numeric.as_mut()?.pending.as_mut()?;
        if !pending.cancelled {
            return None;
        }
        pending.outgoing.take()
    }

    /// UI alone reconciles the original plain-usize counter, once logical
    /// terminal/cancel AND actual storage disposal are independently present.
    pub(crate) fn reconcile_numeric_result(&mut self) -> Result<bool, WvError> {
        let Some(numeric) = self.numeric.as_mut() else {
            return Ok(false);
        };
        let ready = numeric.pending.as_ref().is_some_and(|pending| {
            pending.outgoing.is_none()
                && (pending.cancelled || pending.result_observed)
                && StorageSignal::reusable(&numeric.signal)
        });
        if !ready {
            return Ok(false);
        }
        let Some(pending) = numeric.pending.take() else {
            return Err(bridge_error(
                "numeric release lost its exact original record",
            ));
        };
        let bytes = pending.bytes;
        drop(pending);
        self.local_budget.release(bytes)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
