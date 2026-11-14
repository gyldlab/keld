//! The single settled native dialog result uses the session's sole writer.
//! Full retains its source and original correlation; enqueue commits custody.
use super::super::primary_writer::{RetainedWrite, WriterRefusal};
use super::{
    AppWindowCommand, DialogFault, DialogOwner, DialogReplyV1, DialogSession, DialogState,
    FrameKind, HostAppError, Instant, PrimaryCallerContext, PrimaryRouterHandle, WindowOwner,
    app_detail, with_primary_caller_locked,
};
use std::sync::Arc;
use std::sync::mpsc::TrySendError;

mod source;
use source::DialogSource;

pub(super) struct DialogPublication {
    pub(super) owner: DialogOwner,
    pub(super) caller: PrimaryCallerContext,
    pub(super) result: Option<Result<DialogReplyV1, DialogFault>>,
    pub(super) preparing: bool,
    pub(super) receipt: Option<RetainedWrite>,
    pub(super) deadline: Instant,
}

impl DialogPublication {
    pub(super) fn new(
        owner: DialogOwner,
        caller: PrimaryCallerContext,
        result: Result<DialogReplyV1, DialogFault>,
    ) -> Self {
        Self {
            owner,
            caller,
            result: Some(result),
            preparing: false,
            receipt: None,
            deadline: Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE,
        }
    }
}

impl PrimaryRouterHandle {
    /// The unit writer wake is only a hint. This same retained dialog record
    /// supplies every caller, correlation, result and actual completion.
    #[expect(
        clippy::too_many_lines,
        reason = "One settled dialog source retains its original result through reservation, final admission, restoration, and actual completion."
    )]
    pub(in crate::app_session) fn service_dialog_writer_progress(
        &self,
    ) -> Result<(), HostAppError> {
        let Some(dialog) = &self.dialog else {
            return Ok(());
        };
        self.finish_dialog_primary_receipt(dialog)?;
        let Some(mut source) = DialogSource::take(dialog)? else {
            return Ok(());
        };
        let Some(target) = self.primary_writer_target(&source.caller)? else {
            return Ok(()); // No successor can authorize the old native result.
        };
        if source.owner.parent.is_some_and(|parent| {
            self.dialog_parent_view(
                WindowOwner::new(source.owner.identity, source.owner.attempt),
                parent,
            )
            .is_err()
        }) {
            source.result = Some(Err(DialogFault::ParentRetired));
        }
        let writer = self.primary_writer_owner()?;
        let reservation = match writer.try_reserve_value(source.deadline) {
            Ok(reservation) => reservation,
            Err(WriterRefusal::Full) => {
                self.restore_dialog_interest(&mut source, target.caller())?;
                return Ok(());
            }
            Err(error) => return Err(error.into_host_error("dialog result reservation")),
        };
        let frame = match source
            .result
            .as_ref()
            .ok_or_else(|| app_detail("dialog result encoding", "settled source disappeared"))?
        {
            Ok(value) => reservation.encode_value(
                Arc::clone(&target),
                FrameKind::Reply,
                keld_ipc::channel_table::DIALOG.id(),
                source.owner.correlation,
                value,
                keld_native::dialog::MAX_DIALOG_RESPONSE_BYTES,
            )?,
            Err(fault) => reservation.encode_error_value(
                Arc::clone(&target),
                keld_ipc::channel_table::DIALOG.id(),
                source.owner.correlation,
                &fault.call_error(),
            )?,
        };
        let mut prepared = Some(frame);
        let mut disconnected = false;
        with_primary_caller_locked(&self.current, &self.shutdown, target.caller(), |current| {
            let active = current.as_ref().ok_or_else(|| {
                app_detail("dialog result admission", "validated role disappeared")
            })?;
            // Keep the established Registry-before-dialog lock order. This is
            // the final original-parent check, not a retained HWND assertion.
            let parent_retired = source.owner.parent.is_some_and(|parent| {
                self.dialog_parent_view(
                    WindowOwner::new(source.owner.identity, source.owner.attempt),
                    parent,
                )
                .is_err()
            });
            let mut state = dialog
                .state
                .lock()
                .map_err(|_| app_detail("dialog result admission", "custody lock poisoned"))?;
            let DialogState {
                custody,
                publication,
                ..
            } = &mut *state;
            let Some(record) = publication.as_mut() else {
                return Ok(());
            };
            if !source.matches(record) || custody.original_terminal(source.owner).is_none() {
                return Ok(());
            }
            let Some(frame) = prepared.take() else {
                return Ok(());
            };
            if !active
                .writer_link
                .as_ref()
                .is_some_and(|link| frame.targets(link))
            {
                return Err(app_detail(
                    "dialog result admission",
                    "prepared target is not the original link",
                ));
            }
            if Instant::now() >= source.deadline {
                return Err(WriterRefusal::Deadline.into_host_error("dialog result admission"));
            }
            if parent_retired && !matches!(source.result, Some(Err(DialogFault::ParentRetired))) {
                // The source changed at the final fence. Discard only this
                // never-admitted encoding and prepare the fixed refusal later.
                drop(frame);
                source.result = Some(Err(DialogFault::ParentRetired));
                source.restore(&mut state);
                writer
                    .register_value_interest()
                    .map_err(|error| error.into_host_error("dialog parent-loss interest"))?;
                return Ok(());
            }
            match frame.try_send() {
                Ok(receipt) => {
                    // No fallible work between actual enqueue and the original
                    // terminal commit. The same custody was checked above.
                    let _ = custody.take_original_terminal(source.owner);
                    record.preparing = false;
                    record.receipt = Some(RetainedWrite::new(receipt));
                    source.result = None;
                    if let Some(receipt) = record.receipt.as_ref() {
                        receipt
                            .register_completion_interest()
                            .map_err(|error| error.into_host_error("dialog completion interest"))?;
                    }
                }
                Err(TrySendError::Full(frame)) => {
                    drop(frame);
                    source.restore(&mut state);
                    writer.register_value_interest().map_err(|error| {
                        error.into_host_error("dialog result capacity interest")
                    })?;
                }
                Err(TrySendError::Disconnected(frame)) => {
                    drop(frame); // Only this returned frame was never written.
                    source.restore(&mut state);
                    disconnected = true;
                }
            }
            Ok(())
        })?;
        drop(source);
        if disconnected {
            self.retire_generation_after_write_failure(target.caller())?;
            return Err(WriterRefusal::Disconnected.into_host_error("dialog result writer owner"));
        }
        self.finish_dialog_primary_receipt(dialog)
    }

    fn restore_dialog_interest(
        &self,
        source: &mut DialogSource<'_>,
        caller: &PrimaryCallerContext,
    ) -> Result<(), HostAppError> {
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let session = source.session;
            let mut state = session
                .state
                .lock()
                .map_err(|_| app_detail("dialog result interest", "custody lock poisoned"))?;
            if state
                .publication
                .as_ref()
                .is_some_and(|record| source.matches(record))
                && state.custody.original_terminal(source.owner).is_some()
            {
                source.restore(&mut state);
                self.primary_writer_owner()?
                    .register_value_interest()
                    .map_err(|error| error.into_host_error("dialog result capacity interest"))?;
            }
            Ok(())
        })
    }

    /// Pure completion service used by a synchronous capacity wait. This takes
    /// no caller/Registry guard and sends no native outcome or recovery request.
    pub(in crate::app_session) fn settle_dialog_primary_receipt(
        &self,
    ) -> Result<bool, WriterRefusal> {
        let Some(dialog) = &self.dialog else {
            return Ok(false);
        };
        let mut state = dialog.state.lock().map_err(|_| WriterRefusal::Poisoned)?;
        let Some(record) = state.publication.as_mut() else {
            return Ok(false);
        };
        record
            .receipt
            .as_mut()
            .map_or(Ok(false), RetainedWrite::settle)
    }

    fn finish_dialog_primary_receipt(&self, dialog: &DialogSession) -> Result<(), HostAppError> {
        self.settle_dialog_primary_receipt()
            .map_err(|error| error.into_host_error("dialog result completion"))?;
        let failed = {
            let mut state = dialog
                .state
                .lock()
                .map_err(|_| app_detail("dialog result completion", "custody lock poisoned"))?;
            let Some(record) = state.publication.as_mut() else {
                return Ok(());
            };
            let Some(failed) = RetainedWrite::take_completed(&mut record.receipt) else {
                return Ok(());
            };
            drop(state.publication.take());
            failed
        };
        let _ = dialog.wake.send(AppWindowCommand::DialogWake);
        if let Some(failure) = failed {
            self.report_primary_write_failure(failure)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
