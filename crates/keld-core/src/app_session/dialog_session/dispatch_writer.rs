//! Synchronous dialog replies use the existing writer without holding Core locks.
use super::super::append_app_cleanup;
use super::super::primary_writer::{RetainedWrite, WriteCompletion, WriterLink};
use super::host_route::{DialogAdmission, reused_correlation};
use super::{
    CorrelationId, DialogSession, DialogUiOutcome, FrameKind, HostAppError, Instant,
    PrimaryCallerContext, PrimaryRouterHandle, app_detail, with_primary_caller_locked,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;

impl PrimaryRouterHandle {
    pub(super) fn dispatch_dialog_writer(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        #[cfg(test)]
        if !self.uses_primary_writer() {
            return self.dispatch_dialog_legacy(caller, correlation, payload);
        }
        let Some(dialog) = &self.dialog else {
            return Ok(());
        };
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        self.finish_dialog_primary_receipt(dialog)?;
        let mut result = None;
        let mut collision = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current.as_mut().ok_or_else(|| {
                app_detail("dialog admission", "authenticated generation disappeared")
            })?;
            let target = active
                .writer_link
                .as_ref()
                .filter(|link| link.matches(caller))
                .ok_or_else(|| {
                    app_detail("dialog admission", "exact writer target is unavailable")
                })?;
            if dialog.correlation_in_flight(target, correlation)? {
                // No new publication may pass this same caller fence. The exact
                // writer retirement below drains any already admitted frame.
                caller.reader_stop.store(true, Ordering::Release);
                collision = Some(Arc::clone(target));
                return Ok(());
            }
            result = Some(self.admit_dialog_call(active, dialog, correlation, payload));
            Ok(())
        })?;
        if let Some(target) = collision {
            let retirement = self.retire_exact_link_after_protocol_refusal(&target);
            return Err(append_app_cleanup(reused_correlation(), [retirement]));
        }
        let Some(result) = result else {
            return Ok(());
        };
        if matches!(result, DialogAdmission::Native) {
            return Ok(()); // No ordinary writer slot while the user chooses.
        }
        let Some(target) = self.primary_writer_target(caller)? else {
            return Ok(());
        };
        let reservation =
            self.reserve_primary_value(caller, deadline, "dialog reply reservation")?;
        let frame = match result {
            DialogAdmission::Reply(reply) => reservation.encode_value(
                Arc::clone(&target),
                FrameKind::Reply,
                keld_ipc::channel_table::DIALOG.id(),
                correlation,
                &reply,
                keld_native::dialog::MAX_DIALOG_RESPONSE_BYTES,
            )?,
            DialogAdmission::Refused(error) => reservation.encode_error_value(
                Arc::clone(&target),
                keld_ipc::channel_table::DIALOG.id(),
                correlation,
                &error,
            )?,
            DialogAdmission::Native => return Ok(()),
        };
        if let Some(receipt) = self.admit_primary_frame(caller, frame)? {
            self.await_primary_write(&target, receipt, deadline, "dialog correlated terminal")?;
            if dialog.finish_cancel_ack(&target, correlation)? {
                self.primary_writer_owner()?
                    .register_value_interest()
                    .map_err(|error| error.into_host_error("dialog Cancel completion interest"))?;
            }
        }
        Ok(())
    }

    pub(super) fn dialog_native_finished_writer(
        &self,
        attempt: u32,
        nonce: u64,
        outcome: DialogUiOutcome,
    ) -> Result<(), HostAppError> {
        #[cfg(test)]
        if !self.uses_primary_writer() {
            return self.dialog_native_finished_legacy(attempt, nonce, outcome);
        }
        let Some(dialog) = &self.dialog else {
            return Ok(());
        };
        dialog.complete_for_writer(attempt, nonce, outcome)?;
        self.service_dialog_writer_progress()
    }
}

impl DialogSession {
    fn correlation_in_flight(
        &self,
        target: &WriterLink,
        correlation: CorrelationId,
    ) -> Result<bool, HostAppError> {
        let state = self
            .state
            .lock()
            .map_err(|_| app_detail("dialog admission", "custody lock poisoned"))?;
        let caller = target.caller();
        if !state
            .original_caller()
            .is_some_and(|original| target.matches(original))
        {
            return Ok(false);
        }
        if state
            .custody
            .original_pending_for(caller.identity, caller.attempt)
            == Some(correlation)
        {
            return Ok(true);
        }
        let receipt = state
            .publication
            .as_ref()
            .filter(|record| record.owner.correlation == correlation)
            .and_then(|record| record.receipt.as_ref());
        if let Some(RetainedWrite::Pending(receipt)) = receipt {
            let completed = receipt
                .poll()
                .map_err(|error| error.into_host_error("dialog original write custody"))?;
            return Ok(completed != Some(WriteCompletion::Written));
        }
        Ok(matches!(receipt, Some(RetainedWrite::Completed(Some(_)))))
    }

    fn finish_cancel_ack(
        &self,
        target: &WriterLink,
        correlation: CorrelationId,
    ) -> Result<bool, HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("dialog Cancel acknowledgement", "custody lock poisoned"))?;
        if state.cancel_ack != Some(correlation)
            || !state
                .original_caller()
                .is_some_and(|caller| target.matches(caller))
        {
            return Ok(false);
        }
        state.cancel_ack = None;
        Ok(state.publication.is_some())
    }
}
