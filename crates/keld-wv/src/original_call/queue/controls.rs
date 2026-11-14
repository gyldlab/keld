use super::{
    ActiveOriginalRecord, NativeDocumentOwner, OriginalBinding, OriginalCallAdmission,
    OriginalCallQueue, OriginalQueueProgress, PendingOriginalEvent, PreparationDisposition,
    RendererBridgeOperation, RendererBridgeRequest, SyncSender, WvError, bridge_error,
    request_for_lease,
};

impl<P> OriginalCallQueue<P> {
    /// Preserve the existing control handoff order on the same records. Full
    /// leaves each fact and charged payload intact; only endpoint success marks
    /// a handoff. Physical native/Function/consumer receipts remain independent.
    #[expect(
        clippy::too_many_lines,
        reason = "keep the existing ordered control handoffs and their independent receipt commits in one owner turn"
    )]
    pub(crate) fn flush_controls(
        &mut self,
        binding: &mut OriginalBinding,
        owner: &NativeDocumentOwner,
        pending_event: &mut Option<PendingOriginalEvent>,
        endpoint: &SyncSender<RendererBridgeRequest>,
    ) -> Result<OriginalQueueProgress<()>, WvError> {
        self.reconcile(binding, owner)?;
        let mut needs_service_wake = false;
        if let Some(event) = pending_event.as_ref()
            && let Some(disposition) = event.pending_consumed().cloned()
        {
            let request = request_for_lease(
                event.lease().clone(),
                event.id(),
                RendererBridgeOperation::Consumed { disposition },
                Vec::new(),
            );
            #[cfg(windows)]
            let request = {
                let mut request = request;
                request.role_witness = event.original_role().cloned();
                request
            };
            if Self::enqueue_control(endpoint, request)?
                && let Some(event) = pending_event.as_mut()
            {
                event.consumed_handed_off().map_err(bridge_error)?;
            }
        }
        if let Some(event) = pending_event.as_ref()
            && let Some(completed) = event.pending_quiescence()
        {
            let request = request_for_lease(
                event.lease().clone(),
                event.id(),
                RendererBridgeOperation::EventQuiesced { completed },
                Vec::new(),
            );
            #[cfg(windows)]
            let request = {
                let mut request = request;
                request.role_witness = event.original_role().cloned();
                request
            };
            if Self::enqueue_control(endpoint, request)?
                && let Some(event) = pending_event.as_mut()
            {
                event.quiescence_handed_off().map_err(bridge_error)?;
            }
        }
        if let Some(call) = self.active()
            && self.entered_ready_to_release()
        {
            if call.needs_preflight_abort() {
                let request = request_for_lease(
                    call.lease().clone(),
                    call.id(),
                    RendererBridgeOperation::PreflightAborted,
                    Vec::new(),
                );
                if Self::enqueue_control(endpoint, request)? {
                    self.clear_entered()?;
                    needs_service_wake = true;
                }
            } else {
                self.clear_entered()?;
                needs_service_wake = true;
            }
        }
        needs_service_wake |= self.flush_preparing_abort(binding, owner, endpoint)?;
        if let Some(pending) = self.pending_call.as_ref()
            && pending.admission == OriginalCallAdmission::Cancelled
        {
            let request = request_for_lease(
                pending.queued.lease.clone(),
                pending.queued.id,
                RendererBridgeOperation::PreflightAborted,
                Vec::new(),
            );
            if Self::enqueue_control(endpoint, request)?
                && let Some(pending) = self.pending_call.take()
            {
                if Self::original_current(&pending.queued, binding, owner) {
                    needs_service_wake |= self
                        .retain_cancelled(pending.queued, pending.locals_before)?
                        .needs_service_wake;
                } else {
                    self.drop_queued(pending.queued)?;
                }
            }
        }
        if let Some(lease) = binding.registration_notice().cloned() {
            if lease.is_current() {
                let request = request_for_lease(
                    lease.clone(),
                    0,
                    RendererBridgeOperation::Registered,
                    Vec::new(),
                );
                if Self::enqueue_control(endpoint, request)? {
                    binding
                        .registration_handed_off(&lease)
                        .map_err(bridge_error)?;
                }
            } else {
                binding.discard_revoked_registration();
            }
        }
        Ok(OriginalQueueProgress {
            value: (),
            needs_service_wake,
        })
    }

    fn flush_preparing_abort(
        &mut self,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        endpoint: &SyncSender<RendererBridgeRequest>,
    ) -> Result<bool, WvError> {
        let mut needs_service_wake = false;
        if let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_ref()
            && record.disposition == PreparationDisposition::AbortBeforeRefusal
        {
            let request = request_for_lease(
                record.queued.lease.clone(),
                record.queued.id,
                RendererBridgeOperation::PreflightAborted,
                Vec::new(),
            );
            if Self::enqueue_control(endpoint, request)?
                && let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_mut()
            {
                needs_service_wake = true;
                record.queued.cancelled = true;
                record.disposition = if Self::original_current(&record.queued, binding, owner) {
                    PreparationDisposition::Refusal
                } else {
                    PreparationDisposition::Discard
                };
            }
        }
        let discard = matches!(&self.active_call,
            Some(ActiveOriginalRecord::Preparing(record))
                if record.disposition == PreparationDisposition::Discard
                    && self.backing_available());
        if discard {
            self.clear_discarded_preparation()?;
            needs_service_wake = true;
        }
        Ok(needs_service_wake)
    }
}
