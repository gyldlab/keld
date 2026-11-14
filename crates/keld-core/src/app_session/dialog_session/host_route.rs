#[cfg(any(not(windows), test))]
use super::super::{app_ipc, encode, write_frame};
use super::{
    ActivePrimaryGeneration, CallError, CorrelationId, DialogFault, DialogOwner, DialogReplyV1,
    DialogSession, DialogUiOutcome, DialogWorkOptions, HostAppError, PrimaryCallerContext,
    PrimaryRouterHandle, WebviewId, WindowError, WindowOwner,
};
#[cfg(any(not(windows), test))]
use super::{FrameKind, app_detail, with_primary_caller_locked};

pub(super) enum DialogAdmission {
    Native,
    Reply(DialogReplyV1),
    Refused(CallError),
}

impl PrimaryRouterHandle {
    pub(super) fn dialog_parent_view(
        &self,
        owner: WindowOwner,
        parent: keld_ipc::window::WindowRef,
    ) -> Result<WebviewId, DialogFault> {
        let windows = self.windows.as_ref().ok_or(DialogFault::Unsupported)?;
        let mut windows = windows.lock().map_err(|_| DialogFault::NativeFailure)?;
        windows
            .admitted_dialog_view(owner, parent)
            .map_err(|reason| match reason {
                WindowError::StaleWindow | WindowError::WrongOwner => DialogFault::ParentRetired,
                _ => DialogFault::InvalidParent,
            })
    }

    fn admit_dialog_effect(
        &self,
        active: &mut ActivePrimaryGeneration,
        dialog: &DialogSession,
        correlation: CorrelationId,
        nonce: u64,
        parent: Option<keld_ipc::window::WindowRef>,
        options: DialogWorkOptions,
    ) -> Result<(), DialogFault> {
        let parent = parent.ok_or(DialogFault::Unsupported)?;
        let view =
            self.dialog_parent_view(WindowOwner::new(active.identity, active.attempt), parent)?;
        if !dialog.has_capacity() {
            return Err(DialogFault::Busy);
        }
        let previous = active.dialog_nonce_high_water;
        if !active.accept_dialog_nonce(nonce) {
            return Err(DialogFault::InvalidRequest);
        }
        let owner = DialogOwner {
            identity: active.identity,
            attempt: active.attempt,
            correlation,
            nonce,
            parent: Some(parent),
        };
        #[cfg(windows)]
        let reservation = dialog.reserve_for_caller(
            owner,
            options,
            view,
            PrimaryCallerContext {
                attempt: active.attempt,
                identity: active.identity,
                reader_stop: std::sync::Arc::clone(&active.reader_stop),
            },
        );
        #[cfg(not(windows))]
        let reservation = match options {
            DialogWorkOptions::Message(message) => dialog.reserve(owner, message, view),
            file @ (DialogWorkOptions::Open(_) | DialogWorkOptions::Save(_)) => {
                dialog.reserve_file(owner, file, view)
            }
        };
        if reservation.is_err() {
            // The existing caller fence owns the same high-water rollback:
            // no native effect was queued and no second CALL can race it.
            active.dialog_nonce_high_water = previous;
        }
        reservation
    }

    /// One shared permission/nonce/cancellation rule. Platform publication only
    /// decides when these existing source results enter the physical writer.
    pub(super) fn admit_dialog_call(
        &self,
        active: &mut ActivePrimaryGeneration,
        dialog: &DialogSession,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> DialogAdmission {
        if !active.identity.is_primary() {
            return DialogAdmission::Refused(DialogFault::InvalidRequest.call_error());
        }
        if let Err(denied) = dialog.authorize() {
            return DialogAdmission::Refused(denied);
        }
        let Ok(request) = keld_native::dialog::decode_dialog_request(payload) else {
            return DialogAdmission::Refused(DialogFault::InvalidRequest.call_error());
        };
        let (nonce, parent, options) = match request {
            keld_native::dialog::DialogRequestV1::Cancel { nonce } => {
                return match dialog.cancel(active.identity, active.attempt, nonce, correlation) {
                    Ok(disposition) => {
                        DialogAdmission::Reply(DialogReplyV1::Cancel { disposition })
                    }
                    Err(reason) => DialogAdmission::Refused(reason.call_error()),
                };
            }
            keld_native::dialog::DialogRequestV1::Message {
                nonce,
                parent,
                options,
            } => (nonce, parent, DialogWorkOptions::Message(options)),
            keld_native::dialog::DialogRequestV1::Open {
                nonce,
                parent,
                options,
            } => (nonce, parent, DialogWorkOptions::Open(options)),
            keld_native::dialog::DialogRequestV1::Save {
                nonce,
                parent,
                options,
            } => (nonce, parent, DialogWorkOptions::Save(options)),
        };
        match self.admit_dialog_effect(active, dialog, correlation, nonce, parent, options) {
            Ok(()) => DialogAdmission::Native,
            Err(fault) => DialogAdmission::Refused(fault.call_error()),
        }
    }

    pub(in super::super) fn dispatch_dialog(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        #[cfg(windows)]
        {
            self.dispatch_dialog_writer(caller, correlation, payload)
        }
        #[cfg(not(windows))]
        {
            self.dispatch_dialog_legacy(caller, correlation, payload)
        }
    }

    #[cfg(any(not(windows), test))]
    pub(super) fn dispatch_dialog_legacy(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        let Some(dialog) = &self.dialog else {
            return Ok(());
        };
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current.as_mut().ok_or_else(|| {
                app_detail("dialog admission", "authenticated generation disappeared")
            })?;
            match self.admit_dialog_call(active, dialog, correlation, payload) {
                DialogAdmission::Native => Ok(()),
                DialogAdmission::Reply(reply) => write_reply(active, correlation, &reply),
                DialogAdmission::Refused(error) => write_error(active, correlation, &error),
            }
        })
    }

    pub(in super::super) fn dialog_native_finished(
        &self,
        attempt: u32,
        nonce: u64,
        outcome: DialogUiOutcome,
    ) -> Result<(), HostAppError> {
        #[cfg(windows)]
        {
            self.dialog_native_finished_writer(attempt, nonce, outcome)
        }
        #[cfg(not(windows))]
        {
            self.dialog_native_finished_legacy(attempt, nonce, outcome)
        }
    }

    /// Called by the Core UI event coordinator after the genuine `AppKit` sheet
    /// completed, not by the untrusted caller or the background reader.
    #[cfg(any(not(windows), test))]
    pub(super) fn dialog_native_finished_legacy(
        &self,
        attempt: u32,
        nonce: u64,
        outcome: DialogUiOutcome,
    ) -> Result<(), HostAppError> {
        let Some(dialog) = &self.dialog else {
            return Ok(());
        };
        let terminal = dialog.complete(attempt, nonce, outcome)?;
        let Some((owner, result)) = terminal else {
            return Ok(());
        };
        let transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("dialog native terminal", "generation writer lock poisoned"))?;
        if !self.shutdown.is_running() {
            return Ok(());
        }
        let Some(active) = current.as_mut().filter(|active| {
            active.attempt == owner.attempt
                && active.identity == owner.identity
                && !active
                    .reader_stop
                    .load(std::sync::atomic::Ordering::Acquire)
        }) else {
            return Ok(()); // Retired generations cannot be answered by a late UI callback.
        };
        // The real AppKit sheet's callback can arrive after the Host accepted
        // Window Destroy. The same registry which admitted the parent must
        // still validate that exact incarnation when publishing the original
        // terminal; a retained old NSWindow pointer never authorizes success.
        let result = if owner.parent.is_some_and(|parent| {
            self.dialog_parent_view(WindowOwner::new(owner.identity, owner.attempt), parent)
                .is_err()
        }) {
            Err(DialogFault::ParentRetired)
        } else {
            result
        };
        let written = match result {
            Ok(value) => write_reply(active, owner.correlation, &value),
            Err(error) => write_error(active, owner.correlation, &error.call_error()),
        };
        let Err(primary) = written else {
            return Ok(());
        };
        let cleanup = self.retire_current_generation_locked(
            &mut current,
            owner.attempt,
            "dialog terminal write lost",
            "dialog terminal write link close",
        );
        // The existing Guardian owns successor recovery. Release both locks
        // before waiting for its acknowledgment, just as auth publication does.
        drop(current);
        drop(transition);
        let recovery = self.link_failed(owner.attempt);
        match (cleanup, recovery) {
            (Ok(()), Ok(())) => {
                eprintln!("{primary}");
                Ok(())
            }
            (cleanup, recovery) => Err(super::super::append_app_cleanup(
                primary,
                [cleanup, recovery],
            )),
        }
    }
}

#[cfg(any(not(windows), test))]
fn write_error(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    error: &CallError,
) -> Result<(), HostAppError> {
    keld_ipc::write_call_error(
        &mut active.writer,
        keld_ipc::channel_table::DIALOG.id(),
        correlation,
        error,
    )
    .map_err(|source| app_ipc("dialog correlated refusal", &source))
}

#[cfg(any(not(windows), test))]
fn write_reply(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    reply: &DialogReplyV1,
) -> Result<(), HostAppError> {
    let bytes = encode(reply).map_err(|source| app_ipc("dialog reply encode", &source))?;
    write_frame(
        &mut active.writer,
        FrameKind::Reply,
        0,
        keld_ipc::channel_table::DIALOG.id(),
        correlation,
        &bytes,
    )
    .map_err(|source| app_ipc("dialog correlated reply", &source))
}
