//! Shared native terminal mapping; Windows retains it until writer admission.
#[cfg(windows)]
use super::publication;
use super::{
    AppWindowCommand, DialogFault, DialogReplyV1, DialogSession, DialogTerminal, DialogUiOutcome,
    HostAppError, TerminalKind, app_detail,
};

#[derive(Clone, Copy)]
enum Finish {
    #[cfg(any(target_os = "macos", test))]
    Immediate,
    #[cfg(windows)]
    Writer,
}

impl DialogSession {
    /// Historical immediate publisher used by the unchanged macOS route.
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn complete(
        &self,
        attempt: u32,
        nonce: u64,
        outcome: DialogUiOutcome,
    ) -> Result<Option<DialogTerminal>, HostAppError> {
        self.complete_inner(attempt, nonce, outcome, Finish::Immediate)
    }

    #[cfg(windows)]
    pub(super) fn complete_for_writer(
        &self,
        attempt: u32,
        nonce: u64,
        outcome: DialogUiOutcome,
    ) -> Result<(), HostAppError> {
        self.complete_inner(attempt, nonce, outcome, Finish::Writer)
            .map(|_| ())
    }

    fn complete_inner(
        &self,
        attempt: u32,
        nonce: u64,
        outcome: DialogUiOutcome,
        finish: Finish,
    ) -> Result<Option<DialogTerminal>, HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("native dialog terminal", "custody lock poisoned"))?;
        let Some(owner) = state
            .custody
            .active_owner()
            .filter(|owner| owner.attempt == attempt && owner.nonce == nonce)
        else {
            return Ok(None);
        };
        #[cfg(windows)]
        if matches!(finish, Finish::Writer) && state.publication.is_some() {
            // The first actual callback already owns this bounded publication.
            return Ok(None);
        }
        let file_result = state.file_result.take();
        #[cfg(windows)]
        let native_fault = state
            .native_fault
            .take()
            .unwrap_or(DialogFault::NativeFailure);
        #[cfg(not(windows))]
        let native_fault = DialogFault::NativeFailure;
        // The exact existing custody chooses the winner. Merely observing the
        // terminal for a writer never takes its original correlation.
        let terminal = match finish {
            #[cfg(any(target_os = "macos", test))]
            Finish::Immediate => state.custody.take_original_terminal(owner),
            #[cfg(windows)]
            Finish::Writer => state.custody.original_terminal(owner),
        }
        .map(|(_, kind)| {
            let result = match (kind, outcome) {
                (
                    TerminalKind::NativeResult,
                    DialogUiOutcome::Selected {
                        response,
                        checkbox_checked,
                    },
                ) => Ok(DialogReplyV1::Message {
                    response,
                    checkbox_checked,
                }),
                (TerminalKind::NativeResult, DialogUiOutcome::FileResult) => match file_result {
                    Some((retained_owner, value)) if retained_owner == owner => Ok(value),
                    _ => Err(DialogFault::NativeFailure),
                },
                (TerminalKind::Cancelled, _) => Err(DialogFault::Cancelled),
                (TerminalKind::Expired, _) => Err(DialogFault::Expired),
                (TerminalKind::NativeFailure | TerminalKind::NativeResult, _) => Err(native_fault),
            };
            (owner, result)
        });
        state.custody.quiesced(owner).map_err(|_| {
            app_detail(
                "native dialog terminal",
                "physical quiescence has no active effect",
            )
        })?;
        #[cfg(windows)]
        {
            state.parent = None;
            if state.custody.active_owner().is_none() {
                state.caller = None;
            }
        }
        let result = match finish {
            #[cfg(any(target_os = "macos", test))]
            Finish::Immediate => terminal,
            #[cfg(windows)]
            Finish::Writer => {
                if let Some((owner, result)) = terminal {
                    let caller = state.caller.take().ok_or_else(|| {
                        app_detail(
                            "native dialog publication",
                            "original authenticated caller capture is unavailable",
                        )
                    })?;
                    state.publication =
                        Some(publication::DialogPublication::new(owner, caller, result));
                }
                None
            }
        };
        drop(state);
        // Actual callback quiescence, not a promise or a transport receipt.
        let _ = self.wake.send(AppWindowCommand::DialogWake);
        Ok(result)
    }
}
