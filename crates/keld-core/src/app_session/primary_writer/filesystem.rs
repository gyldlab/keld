//! The existing FS single-flight owner retains its native lease through the
//! same physical writer completion. A completion observation is not a queue.

use super::super::{
    FsInFlight, FsTerminalWrite, PendingFsWriteFailure, PrimaryRouterHandle, append_app_cleanup,
    with_primary_caller_locked,
};
use super::{
    Arc, HostAppError, Instant, PreparedFrame, Reservation, WriterLink, app_detail, router,
};

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn has_outstanding_fs_call_owned(&self, attempt: u32) -> bool {
        let Some(session) = self.fs.as_ref().and_then(std::sync::Weak::upgrade) else {
            return self.fs.is_some();
        };
        let completion = {
            let _transition = self.shutdown.transition_guard();
            let Ok(state) = session.state.lock() else {
                return true;
            };
            state
                .terminal_write
                .as_ref()
                .filter(|terminal| terminal.pending.attempt == attempt)
                .map(|terminal| terminal.completion.clone())
        };
        if let Some(completion) = completion {
            // The old writer held transition/current during this same wait.
            // Preserve serial reader behavior, with every canonical lock free.
            if completion.wait_quiesced().is_err() {
                return true;
            }
            let _transition = self.shutdown.transition_guard();
            let Ok(mut state) = session.state.lock() else {
                return true;
            };
            if state.terminal_write.as_ref().is_some_and(|terminal| {
                terminal.pending.attempt == attempt && terminal.completion.same_record(&completion)
            }) {
                state.terminal_write = None;
                if state.pending_call.is_none() {
                    state.call_outstanding = false;
                }
            }
        }
        session.has_outstanding_call_for(attempt)
    }

    pub(in crate::app_session) fn submit_fs_terminal_owned(
        &self,
        admitted: FsInFlight,
        attempt: u32,
        prepare: impl FnOnce(Reservation, Arc<WriterLink>) -> Result<PreparedFrame, HostAppError>,
    ) -> Result<Option<PendingFsWriteFailure>, HostAppError> {
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let original = admitted
            .caller
            .as_ref()
            .filter(|caller| caller.attempt == attempt)
            .ok_or_else(|| {
                app_detail(
                    "filesystem terminal caller",
                    "original authenticated caller capture is unavailable",
                )
            })?;
        let Some(target) = self.primary_writer_target(original)? else {
            return Ok(None);
        };
        let caller = target.caller();
        let reservation =
            self.reserve_primary_value(caller, deadline, "filesystem terminal reservation")?;
        let mut frame = Some(prepare(reservation, Arc::clone(&target))?);
        let pending = admitted.pending_call.ok_or_else(|| {
            app_detail(
                "filesystem terminal admission",
                "native lease has no original call",
            )
        })?;
        let mut receipt = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current.as_ref().ok_or_else(|| {
                app_detail(
                    "filesystem terminal admission",
                    "validated role disappeared",
                )
            })?;
            let mut native = admitted.session.state.lock().map_err(|_| {
                app_detail("filesystem terminal admission", "native record poisoned")
            })?;
            if native.pending_call != Some(pending) || native.terminal_write.is_some() {
                return Ok(());
            }
            let prepared = frame.take().ok_or_else(|| {
                app_detail(
                    "filesystem terminal admission",
                    "prepared frame disappeared",
                )
            })?;
            let admitted_write = router::admit_locked(active, prepared)?;
            // The sole terminal commit follows successful final enqueue, while
            // the same native state and actual caller fence are still retained.
            native.pending_call = None;
            native.terminal_write = Some(FsTerminalWrite {
                pending,
                completion: admitted_write.observation(),
            });
            // Do not expose the single-flight credit until physical completion.
            receipt = Some(admitted_write);
            Ok(())
        })?;
        let Some(receipt) = receipt else {
            return Ok(None);
        };
        let written = receipt
            .wait(deadline)
            .map_err(|error| error.into_host_error("filesystem terminal write"))
            .and_then(|outcome| outcome.into_result("filesystem terminal write"));
        let failure = if written.is_err() {
            receipt
                .failure_evidence()
                .map_err(|error| error.into_host_error("filesystem terminal evidence"))?
        } else {
            None
        };
        drop(receipt);
        if written.is_ok() {
            let mut native = admitted.session.state.lock().map_err(|_| {
                app_detail("filesystem terminal completion", "native record poisoned")
            })?;
            if native
                .terminal_write
                .as_ref()
                .is_some_and(|terminal| terminal.pending == pending)
            {
                native.terminal_write = None;
                if native.pending_call.is_none() {
                    native.call_outstanding = false;
                }
            }
            drop(native);
            #[cfg(test)]
            admitted.session.hold_after_terminal_write();
            return Ok(None);
        }
        let primary = written
            .err()
            .ok_or_else(|| app_detail("filesystem terminal completion", "failure disappeared"))?;
        let retirement = failure.as_ref().map_or_else(
            || self.retire_generation_after_write_failure(caller),
            |failure| self.record_primary_write_failure(failure),
        );
        // Runtime successor preparation waits for this native lease. It must
        // drain before asking that owner to acknowledge the exact writer fact.
        // The old numeric FS failed-write marker is not a second permission.
        drop(admitted);
        let recovery = failure.map_or(Ok(()), |failure| self.report_primary_write_failure(failure));
        if let Err(cleanup) = super::super::collapse_app_results([retirement, recovery]) {
            return Err(append_app_cleanup(primary, [Err(cleanup)]));
        }
        Ok(None)
    }
}
