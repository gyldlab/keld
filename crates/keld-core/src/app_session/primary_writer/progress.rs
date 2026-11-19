//! One coalesced unit wake for actual writer and native CONTROL continuations.
//! A marker carries no payload, selector, capacity, role, or completion authority.

use super::{AppWindowCommand, PrimaryWriter, Shared, WriterRefusal};

impl PrimaryWriter {
    /// Register inside the same final Core fence after Full. The retained record
    /// remains the authority; this marker carries no key, bytes, or completion.
    pub(in crate::app_session) fn register_value_interest(&self) -> Result<(), WriterRefusal> {
        self.register_interest(1)
    }

    #[cfg(test)]
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
