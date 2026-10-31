use super::{
    ActivePrimaryGeneration, CallError, CorrelationId, DialogFault, DialogOwner, DialogReplyV1,
    FrameKind, HostAppError, MacDialogUiOutcome, PrimaryCallerContext, PrimaryRouterHandle,
    WebviewId, WindowError, WindowOwner, app_detail, app_ipc, encode, with_primary_caller_locked,
    write_frame,
};

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

impl PrimaryRouterHandle {
    fn dialog_parent_view(
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
    /// This is the only shipping entry from the primary app-link's validated
    /// DIALOG CALL. It owns Guard evaluation and existing Runtime caller/nonce
    /// fences, then reserves one native effect before queuing a Tao wake.
    pub(in super::super) fn dispatch_dialog(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        let Some(dialog) = &self.dialog else {
            return Ok(()); // The channel validator cannot admit an unconfigured owner.
        };
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current.as_mut().ok_or_else(|| {
                app_detail("dialog admission", "authenticated generation disappeared")
            })?;
            if !active.identity.is_primary() {
                return write_error(
                    active,
                    correlation,
                    &DialogFault::InvalidRequest.call_error(),
                );
            }
            if let Err(denied) = dialog.authorize() {
                return write_error(active, correlation, &denied);
            }
            let Ok(request) = keld_native::dialog::decode_dialog_request(payload) else {
                return write_error(
                    active,
                    correlation,
                    &DialogFault::InvalidRequest.call_error(),
                );
            };
            match request {
                keld_native::dialog::DialogRequestV1::Cancel { nonce } => {
                    match dialog.cancel(active.identity, active.attempt, nonce, correlation) {
                        Ok(disposition) => {
                            write_reply(active, correlation, &DialogReplyV1::Cancel { disposition })
                        }
                        Err(reason) => write_error(active, correlation, &reason.call_error()),
                    }
                }
                keld_native::dialog::DialogRequestV1::Message {
                    nonce,
                    parent,
                    options,
                } => {
                    let Some(parent) = parent else {
                        // Parentless modal requires its separately approved
                        // deferred CFRunLoop service, never an in-Tao runModal.
                        return write_error(
                            active,
                            correlation,
                            &DialogFault::Unsupported.call_error(),
                        );
                    };
                    let owner = WindowOwner::new(active.identity, active.attempt);
                    let view = match self.dialog_parent_view(owner, parent) {
                        Ok(view) => view,
                        Err(reason) => {
                            return write_error(active, correlation, &reason.call_error());
                        }
                    };
                    if !dialog.has_capacity() {
                        return write_error(active, correlation, &DialogFault::Busy.call_error());
                    }
                    let previous = active.dialog_nonce_high_water;
                    if !active.accept_dialog_nonce(nonce) {
                        return write_error(
                            active,
                            correlation,
                            &DialogFault::InvalidRequest.call_error(),
                        );
                    }
                    let effect_owner = DialogOwner {
                        identity: active.identity,
                        attempt: active.attempt,
                        correlation,
                        nonce,
                        parent: Some(parent),
                    };
                    if let Err(reason) = dialog.reserve(effect_owner, options, view) {
                        // No effect was admitted on failure. Another original
                        // cannot race while the existing current writer is held.
                        active.dialog_nonce_high_water = previous;
                        return write_error(active, correlation, &reason.call_error());
                    }
                    Ok(())
                }
            }
        })
    }

    /// Called by the Core UI event coordinator after the genuine `AppKit` sheet
    /// completed, not by the untrusted caller or the background reader.
    pub(in super::super) fn dialog_native_finished(
        &self,
        attempt: u32,
        nonce: u64,
        outcome: MacDialogUiOutcome,
    ) -> Result<(), HostAppError> {
        let Some(dialog) = &self.dialog else {
            return Ok(());
        };
        let terminal = dialog.complete(attempt, nonce, outcome)?;
        let Some((owner, result)) = terminal else {
            return Ok(());
        };
        let _transition = self.shutdown.transition_guard();
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
        match result {
            Ok(value) => write_reply(active, owner.correlation, &value),
            Err(error) => write_error(active, owner.correlation, &error.call_error()),
        }
    }
}
