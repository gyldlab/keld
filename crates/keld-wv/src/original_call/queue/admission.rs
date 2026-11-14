use super::{
    ActiveOriginalRecord, NativeDocumentOwner, OriginalBinding, OriginalCallAdmission,
    OriginalCallInput, OriginalCallQueue, OriginalControlFault, OriginalQueueProgress,
    PendingOriginalCall, PendingOriginalSend, PreparationPhase, PreparingOriginalInvocation,
    QueuedOriginalCall, RendererBridgeOperation, RendererBridgeRequest, SyncSender, TrySendError,
    WvError, bridge_error, request_for_lease,
};

impl<P> OriginalCallQueue<P> {
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn enqueue(
        &mut self,
        input: OriginalCallInput<P>,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        endpoint: &SyncSender<RendererBridgeRequest>,
        pending_send: Option<&PendingOriginalSend>,
        fault: &mut OriginalControlFault,
    ) -> Result<OriginalQueueProgress<()>, WvError> {
        self.validate_input(&input, fault)?;
        let bytes = Self::input_bytes(&input)?;
        self.local_budget.reserve(bytes)?;
        let queued = QueuedOriginalCall {
            id: input.id,
            lease: input.lease,
            payload: Some(input.payload),
            payload_bytes: input.payload_bytes,
            credential: input.credential,
            bytes,
            channel: input.channel,
            role_witness: input.role_witness,
            facts: input.facts,
            cancelled: false,
        };
        self.admit_queued(queued, binding, owner, endpoint, pending_send, fault)
    }

    pub(super) fn validate_input(
        &self,
        input: &OriginalCallInput<P>,
        fault: &mut OriginalControlFault,
    ) -> Result<(), WvError> {
        if self.backing.phase.get() == PreparationPhase::Unproved {
            fault.latch();
        }
        if fault.is_faulted() {
            return Err(bridge_error("original call admission is faulted"));
        }
        if input.channel.is_some() != input.role_witness.is_some() {
            return Err(bridge_error(
                "original effect lacks its ingress role witness",
            ));
        }
        Ok(())
    }

    pub(super) fn input_bytes(input: &OriginalCallInput<P>) -> Result<usize, WvError> {
        input.payload_bytes
            .checked_add(input.credential.capacity())
            .and_then(|value| value.checked_add(input.channel.as_ref().map_or(0, String::capacity)))
            .and_then(|value| value.checked_add(std::mem::size_of::<ActiveOriginalRecord<P>>()
                .max(std::mem::size_of::<QueuedOriginalCall<P>>())
                .max(std::mem::size_of::<PendingOriginalCall<P>>())))
            // The existing record and immutable invocation handle coexist.
            // The reusable Rc allocation is charged once at queue construction.
            .and_then(|value| value.checked_add(std::mem::size_of::<OriginalQueueProgress<Option<PreparingOriginalInvocation<P>>>>()))
            .ok_or_else(|| bridge_error("original call retained-byte count overflow"))
    }

    pub(super) fn admit_queued(
        &mut self,
        queued: QueuedOriginalCall<P>,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        endpoint: &SyncSender<RendererBridgeRequest>,
        pending_send: Option<&PendingOriginalSend>,
        fault: &mut OriginalControlFault,
    ) -> Result<OriginalQueueProgress<()>, WvError> {
        let mut needs_service_wake = false;
        if queued.channel.is_some()
            && self.pending_call.is_none()
            && !self.numeric_occupied()
            && pending_send.is_none()
            && self.active_channel().is_none()
            && !self.local_calls.iter().any(|call| call.channel.is_some())
        {
            let locals_before = self.local_calls.len();
            match self.begin_preflight(queued, locals_before, binding, owner, endpoint) {
                Ok(progress) => {
                    needs_service_wake = progress.needs_service_wake;
                    if let Some(queued) = progress.value {
                        self.local_calls.push_back(queued);
                    }
                }
                Err(error) => {
                    fault.latch();
                    return Err(error);
                }
            }
        } else {
            self.local_calls.push_back(queued);
        }
        Ok(OriginalQueueProgress {
            value: (),
            needs_service_wake,
        })
    }

    pub(super) fn begin_preflight(
        &mut self,
        mut queued: QueuedOriginalCall<P>,
        locals_before: usize,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        endpoint: &SyncSender<RendererBridgeRequest>,
    ) -> Result<OriginalQueueProgress<Option<QueuedOriginalCall<P>>>, WvError> {
        if !Self::queued_current(&queued, binding, owner) {
            if Self::original_current(&queued, binding, owner) {
                queued.cancelled = true;
                return Ok(OriginalQueueProgress::wake(Some(queued)));
            }
            self.drop_queued(queued)?;
            return Ok(OriginalQueueProgress::quiet(None));
        }
        if self.numeric_occupied() {
            return Ok(OriginalQueueProgress::quiet(Some(queued)));
        }
        let Some(channel) = queued.channel.as_ref() else {
            self.drop_queued(queued)?;
            return Err(bridge_error(
                "local registration cannot request effect preflight",
            ));
        };
        let mut request = request_for_lease(
            queued.lease.clone(),
            queued.id,
            RendererBridgeOperation::Preflight {
                channel: channel.clone(),
            },
            Vec::new(),
        );
        request.native_facts = queued.facts.take();
        request.role_witness.clone_from(&queued.role_witness);
        match endpoint.try_send(request) {
            Ok(()) => {
                self.pending_call = Some(PendingOriginalCall {
                    queued,
                    locals_before,
                    admission: OriginalCallAdmission::Waiting,
                });
                Ok(OriginalQueueProgress::quiet(None))
            }
            Err(TrySendError::Full(request)) => {
                queued.facts = request.native_facts;
                Ok(OriginalQueueProgress::quiet(Some(queued)))
            }
            Err(TrySendError::Disconnected(_)) => {
                self.drop_queued(queued)?;
                Err(bridge_error(
                    "existing bounded renderer endpoint disconnected",
                ))
            }
        }
    }

    pub(crate) fn deliver_preflight(
        &mut self,
        navigation: u64,
        request: u32,
        accepted: bool,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
    ) -> Result<OriginalQueueProgress<()>, WvError> {
        if self.pending_call.as_ref().is_none_or(|pending| {
            pending.queued.id != request
                || u64::from(pending.queued.lease.stamp().observed_generation()) != navigation
                || pending.admission != OriginalCallAdmission::Waiting
        }) {
            return Ok(OriginalQueueProgress::quiet(()));
        }
        if accepted {
            if let Some(pending) = self.pending_call.as_mut() {
                pending.admission = OriginalCallAdmission::Accepted;
            }
            return Ok(OriginalQueueProgress::quiet(()));
        }
        let Some(pending) = self.pending_call.take() else {
            return Ok(OriginalQueueProgress::quiet(()));
        };
        if Self::original_current(&pending.queued, binding, owner) {
            self.retain_cancelled(pending.queued, pending.locals_before)
        } else {
            self.drop_queued(pending.queued)?;
            Ok(OriginalQueueProgress::quiet(()))
        }
    }

    pub(super) fn retain_cancelled(
        &mut self,
        mut queued: QueuedOriginalCall<P>,
        older_locals: usize,
    ) -> Result<OriginalQueueProgress<()>, WvError> {
        queued.cancelled = true;
        if older_locals > self.local_calls.len() {
            self.local_calls.push_back(queued);
            return Err(bridge_error(
                "cancelled original call predecessor census is invalid",
            ));
        }
        self.local_calls.insert(older_locals, queued);
        Ok(OriginalQueueProgress::wake(()))
    }

    pub(crate) fn cancel_queued(&mut self) -> Result<(), WvError> {
        while let Some(call) = self.local_calls.pop_front() {
            self.drop_queued(call)?;
        }
        self.cancel_pending();
        #[cfg(windows)]
        self.cancel_numeric_result();
        if let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_mut() {
            // The same backing may have a live immutable native handle.
            // Preserve its charge; no native/function fact is set.
            record.queued.cancelled = true;
        }
        Ok(())
    }

    pub(crate) fn reconcile(
        &mut self,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
    ) -> Result<(), WvError> {
        let count = self.local_calls.len();
        for _ in 0..count {
            let Some(mut call) = self.local_calls.pop_front() else {
                break;
            };
            if Self::original_current(&call, binding, owner) {
                if !Self::queued_current(&call, binding, owner) {
                    call.cancelled = true;
                }
                self.local_calls.push_back(call);
            } else {
                self.drop_queued(call)?;
            }
        }
        if self
            .pending_call
            .as_ref()
            .is_some_and(|pending| !Self::queued_current(&pending.queued, binding, owner))
        {
            self.cancel_pending();
        }
        self.refresh_preparing(binding, owner);
        #[cfg(windows)]
        self.reconcile_numeric_owner(owner);
        Ok(())
    }
}
