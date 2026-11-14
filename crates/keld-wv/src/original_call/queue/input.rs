//! Pre-copy admission in the SAME `LocalCallBudget`. This is a transient owner
//! of one existing record, not another queue, quota, or document authority.
use super::{
    NativeDocumentOwner, OriginalBinding, OriginalCallInput, OriginalCallQueue,
    OriginalControlFault, OriginalQueueProgress, PendingOriginalSend, PreparedBacking,
    QueuedOriginalCall, Rc, RendererBridgeRequest, SyncSender, WvError, bridge_error,
};

/// Account identity aliases the queue's one cold backing; it allocates nothing.
/// No Drop refunds credit: the caller must dispose partial/raw/decoded storage
/// first, then release, or transfer this exact record into the existing queue.
pub(crate) struct OriginalInputCharge<P> {
    backing: Rc<PreparedBacking<P>>,
    bytes: usize,
    metadata_floor: usize,
}
impl<P> OriginalInputCharge<P> {
    pub(crate) const fn retained_bytes(&self) -> usize {
        self.bytes
    }
}

impl<P> OriginalCallQueue<P> {
    pub(crate) fn begin_input(
        &mut self,
        metadata_bytes: usize,
    ) -> Result<OriginalInputCharge<P>, WvError> {
        let bytes = metadata_bytes
            .checked_add(std::mem::size_of::<OriginalInputCharge<P>>())
            .ok_or_else(|| bridge_error("input metadata count overflow"))?;
        self.local_budget.reserve(bytes)?;
        Ok(OriginalInputCharge {
            backing: Rc::clone(&self.backing),
            bytes,
            metadata_floor: bytes,
        })
    }

    pub(crate) fn remaining_input_storage(&self) -> usize {
        self.local_budget.remaining_storage()
    }

    /// Caller supplies only storage whose real native/raw/parser owner has
    /// already dropped. This never changes the existing record count or its
    /// immutable metadata floor, and foreign/overflow refusals preserve credit.
    pub(crate) fn release_disposed_input_storage(
        &mut self,
        charge: &mut OriginalInputCharge<P>,
        bytes: usize,
    ) -> Result<(), WvError> {
        if !Rc::ptr_eq(&self.backing, &charge.backing) {
            return Err(bridge_error(
                "disposed input storage belongs to another queue",
            ));
        }
        let retained = charge
            .bytes
            .checked_sub(bytes)
            .filter(|retained| *retained >= charge.metadata_floor)
            .ok_or_else(|| bridge_error("disposed input storage crosses live metadata"))?;
        self.local_budget.release_record_storage(bytes)?;
        charge.bytes = retained;
        Ok(())
    }

    /// Called before allocation by the canonical decoder's pure budget hook.
    /// Overflow/foreign ownership leaves both this account and token unchanged.
    pub(crate) fn grow_input(
        &mut self,
        charge: &mut OriginalInputCharge<P>,
        additional: usize,
    ) -> Result<(), WvError> {
        if !Rc::ptr_eq(&self.backing, &charge.backing) {
            return Err(bridge_error(
                "input storage belongs to another original queue",
            ));
        }
        let bytes = charge
            .bytes
            .checked_add(additional)
            .ok_or_else(|| bridge_error("input retained-byte count overflow"))?;
        self.local_budget.grow_record_storage(additional)?;
        charge.bytes = bytes;
        Ok(())
    }

    /// Only after every owned input/partial decoder allocation was disposed.
    /// A refused release leaves the exact token in the caller's retained slot.
    pub(crate) fn release_input(
        &mut self,
        charge: &mut Option<OriginalInputCharge<P>>,
    ) -> Result<(), WvError> {
        let retained = charge
            .as_ref()
            .ok_or_else(|| bridge_error("input charge was already transferred or released"))?;
        if !Rc::ptr_eq(&self.backing, &retained.backing) {
            return Err(bridge_error(
                "input release belongs to another original queue",
            ));
        }
        self.local_budget.release(retained.bytes)?;
        // Pure metadata removal only; the queue still owns the same backing.
        drop(charge.take());
        Ok(())
    }

    /// The same canonical record/layout observation used by both admissions.
    /// It reserves no credit and supplies no document or caller authority.
    pub(crate) fn prepaid_input_bytes(input: &OriginalCallInput<P>) -> Result<usize, WvError> {
        Self::input_bytes(input)
    }

    /// The already-prepaid owned body and record move together. Borrowed
    /// refusal leaves both caller slots unchanged. No second reservation,
    /// payload clone, allocation, or native entry occurs at this transfer.
    #[expect(
        clippy::too_many_arguments,
        reason = "transfer the same body and charge with each existing source owner; no grouped replacement owner"
    )]
    pub(crate) fn enqueue_prepaid(
        &mut self,
        input: &mut Option<OriginalCallInput<P>>,
        charge: &mut Option<OriginalInputCharge<P>>,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        endpoint: &SyncSender<RendererBridgeRequest>,
        pending_send: Option<&PendingOriginalSend>,
        fault: &mut OriginalControlFault,
    ) -> Result<OriginalQueueProgress<()>, WvError> {
        let candidate = input
            .as_ref()
            .ok_or_else(|| bridge_error("prepaid original input is absent"))?;
        let credit = charge
            .as_ref()
            .ok_or_else(|| bridge_error("prepaid original charge is absent"))?;
        self.validate_input(candidate, fault)?;
        if !Rc::ptr_eq(&self.backing, &credit.backing)
            || credit.bytes < Self::input_bytes(candidate)?
        {
            return Err(bridge_error(
                "prepaid original input lacks its exact complete storage charge",
            ));
        }
        let Some(input_value) = input.take() else {
            return Err(bridge_error("prepaid original disappeared before transfer"));
        };
        let Some(credit) = charge.take() else {
            *input = Some(input_value);
            return Err(bridge_error(
                "prepaid original charge disappeared before transfer",
            ));
        };
        let queued = QueuedOriginalCall {
            id: input_value.id,
            lease: input_value.lease,
            payload: Some(input_value.payload),
            payload_bytes: input_value.payload_bytes,
            credential: input_value.credential,
            bytes: credit.bytes,
            channel: input_value.channel,
            role_witness: input_value.role_witness,
            facts: input_value.facts,
            cancelled: false,
        };
        drop(credit); // Same record is now owned by queued; no refund.
        self.admit_queued(queued, binding, owner, endpoint, pending_send, fault)
    }
}
