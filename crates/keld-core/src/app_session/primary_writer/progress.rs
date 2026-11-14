//! One coalesced unit wake for actual writer and native CONTROL continuations.
//! A marker carries no payload, selector, capacity, role, or completion authority.

use super::{
    AppWindowCommand, Instant, PrimaryCallerContext, PrimaryWriter, Shared, WriterRefusal,
};

impl PrimaryWriter {
    /// Register inside the same final Core fence after Full. The retained record
    /// remains the authority; this marker carries no key, bytes, or completion.
    pub(in crate::app_session) fn register_value_interest(&self) -> Result<(), WriterRefusal> {
        self.register_interest(1)
    }

    pub(in crate::app_session) fn register_control_interest(&self) -> Result<(), WriterRefusal> {
        self.register_interest(2)
    }

    pub(super) fn register_interest(&self, mask: u8) -> Result<(), WriterRefusal> {
        let wake = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?
            .register_interest(mask);
        self.shared.wake(wake);
        Ok(())
    }

    /// Acknowledge the one unit UI marker before draining actual retained records.
    pub(in crate::app_session) fn acknowledge_progress(&self) -> Result<(), WriterRefusal> {
        let wake = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?
            .acknowledge_progress();
        self.shared.wake(wake);
        Ok(())
    }

    /// Pure receipt settlement leaves a completed record for ordinary source
    /// service. This is the existing unit hint, never native/UI dispatch.
    pub(in crate::app_session) fn completed_source_progress(&self) -> Result<(), WriterRefusal> {
        let wake = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?
            .completed_source_progress();
        self.shared.wake(wake);
        Ok(())
    }

    /// Canonical retirement wakes capacity waiters; it does not refund positions.
    pub(in crate::app_session) fn role_retired(&self) {
        self.shared.changed.notify_all();
        let wake = self
            .shared
            .state
            .lock()
            .is_ok_and(|mut state| state.retired());
        self.shared.wake(wake);
    }

    /// Register only after the actual `CompatLedger` reported its existing four
    /// CONTROL slots unavailable. That owner rechecks release flags afterward.
    pub(in crate::app_session) fn register_native_control_interest(
        &self,
    ) -> Result<(), WriterRefusal> {
        self.register_interest(128)
    }

    /// The exact original native reservation released storage, or `CompatLedger`
    /// observed capacity after registration. This wakes only retained interest;
    /// availability is re-read from that same ledger by the eventual service.
    pub(in crate::app_session) fn native_control_progress(&self) {
        let wake = self
            .shared
            .state
            .lock()
            .is_ok_and(|mut state| state.native_control_progress());
        // The existing condition variable also serves the one synchronous
        // Window/Compat publisher. A queued UI marker does not consume this
        // actual release observation or require the UI to dispatch anything.
        self.shared.changed.notify_all();
        self.shared.wake(wake);
    }

    /// Storage progress only. The caller registered before its final SAME-ledger
    /// reservation attempt, then released every canonical/source guard. Neither
    /// this bit nor a wake grants a slot or permission; retry that exact owner.
    pub(in crate::app_session) fn wait_native_control_progress(
        &self,
        caller: &PrimaryCallerContext,
        deadline: Instant,
    ) -> Result<(), WriterRefusal> {
        self.wait_native_storage_progress(deadline, || {
            caller
                .reader_stop
                .load(std::sync::atomic::Ordering::Acquire)
        })
    }

    /// Same storage/CV only. Its sole caller holds an exact old-route retirement
    /// permit after canonical revocation; expected reader stop cannot cancel it.
    pub(in crate::app_session) fn wait_retired_native_control_progress(
        &self,
        deadline: Instant,
    ) -> Result<(), WriterRefusal> {
        self.wait_native_storage_progress(deadline, || false)
    }

    fn wait_native_storage_progress(
        &self,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), WriterRefusal> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?;
        loop {
            if Instant::now() >= deadline {
                return Err(WriterRefusal::Deadline);
            }
            if cancelled() {
                return Err(WriterRefusal::Retired);
            }
            if state.closed {
                return Err(WriterRefusal::Disconnected);
            }
            if !state.native_control_interest_pending() {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            #[cfg(test)]
            if let Some(observed) = state.capacity_wait_observer.take() {
                let _ = observed.try_send(());
            }
            let (next, _) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| WriterRefusal::Poisoned)?;
            state = next;
        }
    }
}

impl Shared {
    pub(super) fn wake(&self, needed: bool) {
        if needed
            && self
                .progress
                .send(AppWindowCommand::HostWriterProgress)
                .is_err()
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            self.changed.notify_all();
        }
    }
}
