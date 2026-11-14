//! Synchronous dialog replies use the existing writer without holding Core locks.
use super::host_route::DialogAdmission;
use super::{
    CorrelationId, DialogUiOutcome, FrameKind, HostAppError, Instant, PrimaryCallerContext,
    PrimaryRouterHandle, app_detail, with_primary_caller_locked,
};
use std::sync::Arc;

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
        let mut result = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current.as_mut().ok_or_else(|| {
                app_detail("dialog admission", "authenticated generation disappeared")
            })?;
            result = Some(self.admit_dialog_call(active, dialog, correlation, payload));
            Ok(())
        })?;
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
