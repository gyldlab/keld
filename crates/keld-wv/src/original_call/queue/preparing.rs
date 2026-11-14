use super::{
    ActiveOriginalCall, ActiveOriginalRecord, NativeDocumentOwner, OriginalBinding,
    OriginalCallAdmission, OriginalCallQueue, OriginalControlFault, OriginalInvocationKind,
    OriginalQueueProgress, PendingOriginalCall, PendingOriginalEvent, PendingOriginalSend,
    PreparationDisposition, PreparationPhase, PreparingOriginalIdentity,
    PreparingOriginalInvocation, PreparingRecord, QueuedOriginalCall, Rc, RendererBridgeRequest,
    RendererRoleWitness, SyncSender, WvError, bridge_error,
};

impl<P> OriginalCallQueue<P> {
    /// The renderer is bounded pure Rust: mutate only this same payload within
    /// its already reserved `payload_bytes`, and return its retained byte count.
    /// Native argument construction and native entry happen after this returns.
    pub(crate) fn take_next_preparation(
        &mut self,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        endpoint: &SyncSender<RendererBridgeRequest>,
        occupancy: (Option<&PendingOriginalSend>, Option<&PendingOriginalEvent>),
        fault: &OriginalControlFault,
        prepare: impl FnOnce(&mut P, OriginalInvocationKind, u32, &str, usize) -> Result<usize, WvError>,
    ) -> Result<OriginalQueueProgress<Option<PreparingOriginalInvocation<P>>>, WvError> {
        if fault.is_faulted() {
            return Ok(OriginalQueueProgress::quiet(None));
        }
        if self.backing.phase.get() == PreparationPhase::Unproved {
            return Err(bridge_error("original carrier has unproved native custody"));
        }
        self.reconcile(binding, owner)?;
        if self.numeric_occupied() {
            return Ok(OriginalQueueProgress::quiet(None));
        }
        if self.active_call.is_some() {
            return self.take_available_preparation(prepare);
        }
        if !self.backing_is_empty() {
            return Err(bridge_error(
                "original carrier backing lacks its empty sole-owner state",
            ));
        }
        if let Some(pending) = self.pending_call.as_ref()
            && pending.locals_before == 0
        {
            if pending.admission != OriginalCallAdmission::Accepted
                || occupancy.0.is_some()
                || occupancy.1.is_some()
            {
                return Ok(OriginalQueueProgress::quiet(None));
            }
            let pending = self
                .pending_call
                .take()
                .ok_or_else(|| bridge_error("admitted original call disappeared"))?;
            if !Self::queued_current(&pending.queued, binding, owner) {
                self.pending_call = Some(PendingOriginalCall {
                    admission: OriginalCallAdmission::Cancelled,
                    ..pending
                });
                return Ok(OriginalQueueProgress::wake(None));
            }
            return self.stage_preparation(pending.queued, PreparationDisposition::Effect, prepare);
        }
        if self
            .local_calls
            .front()
            .is_some_and(|call| call.channel.is_some() && !call.cancelled)
            && (self.pending_call.is_some() || occupancy.0.is_some())
        {
            return Ok(OriginalQueueProgress::quiet(None));
        }
        let Some(local) = self.local_calls.pop_front() else {
            return Ok(OriginalQueueProgress::quiet(None));
        };
        if !Self::queued_current(&local, binding, owner) {
            let disposition = if Self::original_current(&local, binding, owner) {
                PreparationDisposition::Refusal
            } else {
                PreparationDisposition::Discard
            };
            return self.stage_preparation(local, disposition, prepare);
        }
        if local.channel.is_some() {
            let progress = self.begin_preflight(local, 0, binding, owner, endpoint)?;
            if let Some(local) = progress.value {
                self.local_calls.push_front(local);
            }
            return Ok(OriginalQueueProgress {
                value: None,
                needs_service_wake: progress.needs_service_wake,
            });
        }
        if let Some(pending) = self.pending_call.as_mut() {
            let Some(remaining) = pending.locals_before.checked_sub(1) else {
                self.local_calls.push_front(local);
                return Err(bridge_error("original call order barrier underflow"));
            };
            pending.locals_before = remaining;
        }
        self.stage_preparation(local, PreparationDisposition::Local, prepare)
    }

    fn stage_preparation(
        &mut self,
        queued: QueuedOriginalCall<P>,
        disposition: PreparationDisposition,
        prepare: impl FnOnce(&mut P, OriginalInvocationKind, u32, &str, usize) -> Result<usize, WvError>,
    ) -> Result<OriginalQueueProgress<Option<PreparingOriginalInvocation<P>>>, WvError> {
        // Attach the existing charged record before a fallible check. An error
        // never drops or returns its payload through an unowned callback path.
        self.active_call = Some(ActiveOriginalRecord::Preparing(PreparingRecord {
            queued,
            disposition,
        }));
        let backing = Rc::get_mut(&mut self.backing)
            .ok_or_else(|| bridge_error("original backing still has another immutable handle"))?;
        if backing.phase.get() != PreparationPhase::Empty || backing.payload.is_some() {
            return Err(bridge_error("original backing still retains a predecessor"));
        }
        let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_mut() else {
            return Err(bridge_error("staged original preparation lost its owner"));
        };
        let payload =
            record.queued.payload.take().ok_or_else(|| {
                bridge_error("staged original preparation has no retained payload")
            })?;
        backing.payload = Some(payload);
        backing.phase.set(PreparationPhase::Available);
        self.take_available_preparation(prepare)
    }

    fn take_available_preparation(
        &mut self,
        prepare: impl FnOnce(&mut P, OriginalInvocationKind, u32, &str, usize) -> Result<usize, WvError>,
    ) -> Result<OriginalQueueProgress<Option<PreparingOriginalInvocation<P>>>, WvError> {
        let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_ref() else {
            return Ok(OriginalQueueProgress::quiet(None));
        };
        if !self.backing_available()
            || record.disposition == PreparationDisposition::AbortBeforeRefusal
        {
            return Ok(OriginalQueueProgress::quiet(None));
        }
        if record.disposition == PreparationDisposition::Discard {
            self.clear_discarded_preparation()?;
            return Ok(OriginalQueueProgress::wake(None));
        }
        let kind = match record.disposition {
            PreparationDisposition::Local | PreparationDisposition::Effect => {
                OriginalInvocationKind::Original
            }
            PreparationDisposition::Refusal => OriginalInvocationKind::Refusal,
            PreparationDisposition::AbortBeforeRefusal | PreparationDisposition::Discard => {
                return Ok(OriginalQueueProgress::quiet(None));
            }
        };
        let backing = Rc::get_mut(&mut self.backing)
            .ok_or_else(|| bridge_error("original payload cannot be prepared while borrowed"))?;
        // A renderer error or unwind preserves the same P and charge as
        // unproved. It cannot silently leave a partially rendered invocation.
        backing.phase.set(PreparationPhase::Unproved);
        let payload = backing
            .payload
            .as_mut()
            .ok_or_else(|| bridge_error("original preparation lost its retained payload"))?;
        let retained = prepare(
            payload,
            kind,
            record.queued.id,
            &record.queued.credential,
            record.queued.payload_bytes,
        )?;
        if retained > record.queued.payload_bytes {
            return Err(bridge_error(
                "original carrier renderer exceeded its admitted payload charge",
            ));
        }
        backing.phase.set(PreparationPhase::Borrowed);
        Ok(OriginalQueueProgress::quiet(Some(
            PreparingOriginalInvocation {
                identity: PreparingOriginalIdentity {
                    id: record.queued.id,
                    lease: record.queued.lease.clone(),
                },
                kind,
                backing: Rc::clone(&self.backing),
            },
        )))
    }

    /// Observe the actual Borrowed -> Available transition on this same record.
    /// Only a now-unblocked refusal/discard requests another service turn. An
    /// occupied event/send or a Full abort creates no unconditional re-wake.
    pub(crate) fn defer_preparation(
        &mut self,
        invocation: PreparingOriginalInvocation<P>,
    ) -> Result<OriginalQueueProgress<()>, WvError> {
        let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_ref() else {
            return Err(bridge_error(
                "deferred original lacks its preparation owner",
            ));
        };
        if !self.invocation_matches(record, &invocation) {
            return Err(bridge_error(
                "deferred original differs from its retained backing",
            ));
        }
        let needs_service_wake = matches!(
            record.disposition,
            PreparationDisposition::Refusal | PreparationDisposition::Discard
        );
        invocation.defer()?;
        Ok(OriginalQueueProgress {
            value: (),
            needs_service_wake,
        })
    }

    /// Retain only the captured ingress witness for the platform's existing
    /// final fence. This observation cannot acquire a successor role.
    pub(crate) fn preparation_role(
        &self,
        invocation: &PreparingOriginalInvocation<P>,
    ) -> Option<RendererRoleWitness> {
        let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_ref() else {
            return None;
        };
        (self.invocation_matches(record, invocation)
            && record.disposition == PreparationDisposition::Effect)
            .then(|| record.queued.role_witness.clone())
            .flatten()
    }

    /// Run at the platform's final same-turn admission cut after native argument
    /// preparation. Windows calls this inside the actual Core role fence.
    /// This moves metadata only; native entry follows after all guards end.
    pub(crate) fn try_enter_prepared(
        &mut self,
        invocation: &PreparingOriginalInvocation<P>,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
        occupancy: (Option<&PendingOriginalSend>, Option<&PendingOriginalEvent>),
        fault: &OriginalControlFault,
    ) -> bool {
        if fault.is_faulted()
            || self.numeric_occupied()
            || !Rc::ptr_eq(&self.backing, &invocation.backing)
            || self.backing.phase.get() != PreparationPhase::Borrowed
        {
            return false;
        }
        self.refresh_preparing(binding, owner);
        let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_ref() else {
            return false;
        };
        if !self.invocation_matches(record, invocation) {
            return false;
        }
        let eligible = match (record.disposition, invocation.kind) {
            (PreparationDisposition::Local, OriginalInvocationKind::Original) => {
                Self::queued_current(&record.queued, binding, owner)
            }
            (PreparationDisposition::Effect, OriginalInvocationKind::Original) => {
                Self::queued_current(&record.queued, binding, owner)
                    && occupancy.0.is_none()
                    && occupancy.1.is_none()
            }
            (PreparationDisposition::Refusal, OriginalInvocationKind::Refusal) => {
                Self::original_current(&record.queued, binding, owner)
            }
            _ => false,
        };
        if !eligible {
            return false;
        }
        let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.take() else {
            return false;
        };
        let queued = record.queued;
        let active = if invocation.kind == OriginalInvocationKind::Refusal {
            ActiveOriginalCall::refused_before_entry(
                queued.id,
                queued.lease,
                queued.credential,
                queued.bytes,
            )
        } else {
            ActiveOriginalCall::entered(
                queued.id,
                queued.lease,
                queued.credential,
                queued.channel,
                queued.bytes,
                queued.role_witness,
            )
        };
        self.active_call = Some(ActiveOriginalRecord::Entered(active));
        self.backing.phase.set(PreparationPhase::Entered);
        true
    }

    fn invocation_matches(
        &self,
        record: &PreparingRecord<P>,
        invocation: &PreparingOriginalInvocation<P>,
    ) -> bool {
        Rc::ptr_eq(&self.backing, &invocation.backing)
            && self.backing.phase.get() == PreparationPhase::Borrowed
            && record.queued.payload.is_none()
            && self.backing.payload.is_some()
            && record.queued.id == invocation.identity.id
            && record.queued.lease.same_binding(&invocation.identity.lease)
    }

    pub(super) fn refresh_preparing(
        &mut self,
        binding: &OriginalBinding,
        owner: &NativeDocumentOwner,
    ) {
        let Some(ActiveOriginalRecord::Preparing(record)) = self.active_call.as_mut() else {
            return;
        };
        let original_current = Self::original_current(&record.queued, binding, owner);
        record.disposition = match record.disposition {
            PreparationDisposition::Effect
                if !Self::queued_current(&record.queued, binding, owner) =>
            {
                PreparationDisposition::AbortBeforeRefusal
            }
            PreparationDisposition::Local
                if !Self::queued_current(&record.queued, binding, owner) =>
            {
                if original_current {
                    PreparationDisposition::Refusal
                } else {
                    PreparationDisposition::Discard
                }
            }
            PreparationDisposition::Refusal if !original_current => PreparationDisposition::Discard,
            other => other,
        };
    }
}
