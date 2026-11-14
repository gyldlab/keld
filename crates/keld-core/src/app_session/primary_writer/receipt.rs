//! Unique physical-write receipts and payload-free completion observations.

use super::{
    Arc, Instant, POSITIONS, Position, PrimaryWriter, Shared, WriteCompletion, WriterLink,
    WriterRefusal,
};

/// One receipt owner shares the fixed slot itself; no per-frame channel or Arc.
pub(in crate::app_session) struct WriteReceipt {
    pub(super) shared: Arc<Shared>,
    pub(super) position: Position,
    pub(super) deadline: Instant,
}

/// Non-owning completion observation retained by an existing single-flight
/// source record. Only the unique receipt controls release; no payload/channel.
#[derive(Clone)]
pub(in crate::app_session) struct WriteObservation {
    pub(super) shared: Arc<Shared>,
    pub(super) position: Position,
}
impl WriteObservation {
    pub(in crate::app_session) fn same_record(&self, other: &Self) -> bool {
        self.position == other.position && Arc::ptr_eq(&self.shared, &other.shared)
    }
    pub(in crate::app_session) fn is_quiesced(&self) -> Result<bool, WriterRefusal> {
        Ok(!self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?
            .pending(self.position))
    }

    pub(in crate::app_session) fn register_quiescence_interest(&self) -> Result<(), WriterRefusal> {
        let wake = {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| WriterRefusal::Poisoned)?;
            if state.pending(self.position) {
                state.register_interest(1 << (2 + self.position.index))
            } else {
                false
            }
        };
        self.shared.wake(wake);
        Ok(())
    }

    /// Quiescence is not successful delivery. The original native I/O deadline
    /// remains with the packet. Expiry of an observer's clock, or a poisoned
    /// mutex, cannot prove that the physical call and its backing have returned.
    /// The worker's outer ended guard also proves never-written cleanup on unwind.
    pub(in crate::app_session) fn wait_quiesced(&self) -> Result<(), WriterRefusal> {
        self.wait_quiesced_result().map(|_| ())
    }

    /// Preserve an actual completion still retained by its unique receipt owner.
    /// None means that owner already released the old serial, never Written or
    /// consumer success. This observation cannot release/reuse another receipt.
    pub(in crate::app_session) fn wait_quiesced_result(
        &self,
    ) -> Result<Option<WriteCompletion>, WriterRefusal> {
        let (mut state, mut poisoned) = match self.shared.state.lock() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        while state.pending(self.position) {
            #[cfg(test)]
            if let Some(observed) = state.quiescence_wait_observer.take() {
                let _ = observed.try_send(());
            }
            state = match self.shared.changed.wait(state) {
                Ok(state) => state,
                Err(error) => {
                    poisoned = true;
                    error.into_inner()
                }
            };
        }
        if poisoned {
            Err(WriterRefusal::Poisoned)
        } else {
            Ok(state.observed_completion(self.position))
        }
    }
}

impl WriteReceipt {
    /// Actual physical write/flush completion time retained in this same slot.
    /// Producer scheduling cannot move the lifecycle handover deadline forward.
    pub(in crate::app_session) fn written_at(&self) -> Result<Option<Instant>, WriterRefusal> {
        self.shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?
            .written_at(self.position)
    }

    pub(in crate::app_session) fn observation(&self) -> WriteObservation {
        WriteObservation {
            shared: Arc::clone(&self.shared),
            position: self.position,
        }
    }

    pub(in crate::app_session) fn register_completion_interest(&self) -> Result<(), WriterRefusal> {
        let wake = {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| WriterRefusal::Poisoned)?;
            state.completion(self.position)?;
            state.register_interest(1 << (2 + self.position.index))
        };
        self.shared.wake(wake);
        Ok(())
    }

    pub(in crate::app_session) fn poll(&self) -> Result<Option<WriteCompletion>, WriterRefusal> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?;
        state.completion(self.position)
    }

    /// The caller releases every Core/native fence before waiting. Timeout is
    /// ambiguous; dropping this receipt leaves the worker's charged position live.
    pub(in crate::app_session) fn wait(
        &self,
        deadline: Instant,
    ) -> Result<WriteCompletion, WriterRefusal> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?;
        loop {
            if let Some(completion) = state.completion(self.position)? {
                return Ok(completion);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(WriterRefusal::Deadline);
            };
            let (next, _) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| WriterRefusal::Poisoned)?;
            state = next;
        }
    }
}
impl Drop for WriteReceipt {
    fn drop(&mut self) {
        self.shared.release(self.position, false);
    }
}

impl PrimaryWriter {
    /// Snapshot only already admitted queued/writing positions for this exact
    /// retained A link at the caller's final canonical revocation cut. Fixed
    /// metadata only; waiting never drops another producer's unique receipt.
    pub(in crate::app_session) fn snapshot_target_writes(
        &self,
        target: &Arc<WriterLink>,
    ) -> Result<[Option<WriteObservation>; POSITIONS], WriterRefusal> {
        let observed = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?
            .target_observations(target);
        Ok(observed.map(|record| {
            record.map(|position| WriteObservation {
                shared: Arc::clone(&self.shared),
                position,
            })
        }))
    }
}
