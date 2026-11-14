//! One authenticated primary generation's guarded native Message custody.
//! The app-link reader owns admission; Tao owns the actual !Send native dialog.
//! Both use the existing Runtime generation and `WindowRegistry` identities.

use std::sync::Mutex;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use keld_guard::Principal;
use keld_guard::capability;
use keld_guard::verified_manifest::VerifiedManifest;
use keld_ipc::CallError;
use keld_ipc::guard_dispatch::dispatch_privileged;
use keld_native::dialog::{
    CancelDisposition, DialogReplyV1, MessageOptions, OpenDialogOptions, SaveDialogOptions,
};
use keld_wv::WebviewId;
use keld_wv::{AppWindowCommand, AppWindowEvent, DialogUiOutcome};

use super::dialog_custody::{CancelResult, DialogCustody, DialogOwner, ReserveError, TerminalKind};
use super::window_registry::{WindowError, WindowOwner};
use super::{
    ActivePrimaryGeneration, CorrelationId, FrameKind, HostAppError, PrimaryCallerContext,
    PrimaryLinkIdentity, PrimaryRouterHandle, app_detail, with_primary_caller_locked,
};

/// Native UI custody deadline, independent of transport's much shorter CALL
/// first-byte deadline. No timer resets when the link sends another request.
pub(super) const DIALOG_NATIVE_DEADLINE: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DialogFault {
    InvalidRequest,
    Unsupported,
    InvalidParent,
    ParentRetired,
    Busy,
    Expired,
    Cancelled,
    NativeFailure,
}

impl DialogFault {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "KELD-NATIVE-009",
            Self::Unsupported => "KELD-NATIVE-010",
            Self::InvalidParent => "KELD-NATIVE-011",
            Self::ParentRetired => "KELD-NATIVE-012",
            Self::Busy => "KELD-NATIVE-013",
            Self::Expired => "KELD-NATIVE-014",
            Self::Cancelled => "KELD-NATIVE-015",
            Self::NativeFailure => "KELD-NATIVE-016",
        }
    }

    fn fix(self) -> &'static str {
        match self {
            Self::InvalidRequest => {
                "Use a positive, increasing dialog nonce and bounded, source-exact options."
            }
            Self::Unsupported => {
                "Use a qualified visible window-backed native dialog; parentless presentation is not yet admitted."
            }
            Self::InvalidParent => {
                "Pass one live, authentic BrowserWindow from the same role; never pass a numeric window id."
            }
            Self::ParentRetired => {
                "Do not reuse a destroyed, closing or previous-generation parent window."
            }
            Self::Busy => {
                "Wait until the previous native dialog has physically drained before opening the next."
            }
            Self::Expired => {
                "The native owner exceeded its dialog deadline; inspect the OS session and do not replay without user consent."
            }
            Self::Cancelled => {
                "The original dialog was canceled; do not interpret dismissal as a button click."
            }
            Self::NativeFailure => {
                "The native presentation or callback failed; inspect the Host-owned native error and retry only as a new user action."
            }
        }
    }

    pub(super) fn call_error(self) -> CallError {
        let code = self.code();
        CallError {
            code: code.to_owned(),
            message: format!("{code}: native dialog did not complete. {}", self.fix()),
        }
    }
}

pub(super) struct DialogWork {
    pub(super) owner: DialogOwner,
    pub(super) options: DialogWorkOptions,
    pub(super) parent: WebviewId,
    pub(super) deadline: Instant,
}

/// One source-exact native operation; variants share the same authenticated
/// role, generation nonce, window parent and original/cancel custody.
pub(super) enum DialogWorkOptions {
    Message(MessageOptions),
    Open(OpenDialogOptions),
    Save(SaveDialogOptions),
}

struct DialogState {
    custody: DialogCustody,
    pending: Option<DialogWork>,
    /// File paths are captured only after the actual native callback and
    /// physical sheet drain. No duplicate authority or new socket exists.
    file_result: Option<(DialogOwner, DialogReplyV1)>,
    #[cfg(windows)]
    parent: Option<WebviewId>,
    #[cfg(windows)]
    native_fault: Option<DialogFault>,
    #[cfg(windows)]
    caller: Option<PrimaryCallerContext>,
    #[cfg(windows)]
    publication: Option<publication::DialogPublication>,
}

/// The verified manifest is immutable; native effects and current-generation
/// high-water live in their existing separate owners. No second role identity.
pub(super) struct DialogSession {
    verified: VerifiedManifest,
    state: Mutex<DialogState>,
    wake: Sender<AppWindowCommand>,
}

type DialogTerminal = (DialogOwner, Result<DialogReplyV1, DialogFault>);

impl DialogSession {
    pub(super) fn new(verified: VerifiedManifest, wake: Sender<AppWindowCommand>) -> Self {
        Self {
            verified,
            state: Mutex::new(DialogState {
                custody: DialogCustody::default(),
                pending: None,
                file_result: None,
                #[cfg(windows)]
                parent: None,
                #[cfg(windows)]
                native_fault: None,
                #[cfg(windows)]
                caller: None,
                #[cfg(windows)]
                publication: None,
            }),
            wake,
        }
    }

    /// The *only* permission evaluator for both Message and Cancel calls.
    /// The caller's authenticated primary role is separately fenced by Core.
    pub(super) fn authorize(&self) -> Result<(), CallError> {
        dispatch_privileged(
            self.verified.manifest(),
            Principal::AppProcess,
            capability::DIALOG,
            "dialog",
            |_| (),
        )
        .map_err(|denial| CallError::from(&denial))
    }

    /// Reserve exactly one original after caller, Guard, parent and high-water
    /// checks. A queued message is never visible to UI before this reservation.
    #[cfg(any(not(windows), test))]
    pub(super) fn reserve(
        &self,
        owner: DialogOwner,
        options: MessageOptions,
        parent: WebviewId,
    ) -> Result<(), DialogFault> {
        self.reserve_operation(
            owner,
            DialogWorkOptions::Message(options),
            parent,
            #[cfg(windows)]
            None,
        )
    }

    /// The real Open/Save operation uses exactly the Message queue/custody.
    #[cfg(any(not(windows), test))]
    #[cfg_attr(
        all(windows, test),
        expect(
            dead_code,
            reason = "Keep the legacy Open/Save adapter type-checked in Windows tests; the live Windows publisher reserves the same operation with its captured caller."
        )
    )]
    pub(super) fn reserve_file(
        &self,
        owner: DialogOwner,
        options: DialogWorkOptions,
        parent: WebviewId,
    ) -> Result<(), DialogFault> {
        if matches!(options, DialogWorkOptions::Message(_)) {
            return Err(DialogFault::InvalidRequest);
        }
        self.reserve_operation(
            owner,
            options,
            parent,
            #[cfg(windows)]
            None,
        )
    }

    /// Capture the existing authenticated caller before native work is woken.
    #[cfg(windows)]
    fn reserve_for_caller(
        &self,
        owner: DialogOwner,
        options: DialogWorkOptions,
        parent: WebviewId,
        caller: PrimaryCallerContext,
    ) -> Result<(), DialogFault> {
        if caller.identity != owner.identity
            || caller.attempt != owner.attempt
            || caller
                .reader_stop
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(DialogFault::InvalidRequest);
        }
        self.reserve_operation(owner, options, parent, Some(caller))
    }

    fn reserve_operation(
        &self,
        owner: DialogOwner,
        options: DialogWorkOptions,
        parent: WebviewId,
        #[cfg(windows)] caller: Option<PrimaryCallerContext>,
    ) -> Result<(), DialogFault> {
        {
            let mut state = self.state.lock().map_err(|_| DialogFault::NativeFailure)?;
            #[cfg(windows)]
            if state.publication.is_some() {
                return Err(DialogFault::Busy);
            }
            state.custody.reserve(owner).map_err(|error| match error {
                ReserveError::Busy => DialogFault::Busy,
                ReserveError::Stale | ReserveError::InvalidTransition => DialogFault::NativeFailure,
            })?;
            #[cfg(windows)]
            {
                state.parent = Some(parent);
                state.native_fault = None;
                state.caller = caller;
            }
            state.pending = Some(DialogWork {
                owner,
                options,
                parent,
                deadline: Instant::now() + DIALOG_NATIVE_DEADLINE,
            });
        }
        if self.wake.send(AppWindowCommand::DialogWake).is_err() {
            // The native owner never received the queue wake. Retire and drain
            // the reserved but never-presented effect without fabricating UI.
            if let Ok(mut state) = self.state.lock() {
                state.pending = None;
                state.custody.retire(owner.identity, owner.attempt);
                let _ = state.custody.quiesced(owner);
                #[cfg(windows)]
                {
                    state.caller = None;
                }
            }
            return Err(DialogFault::NativeFailure);
        }
        Ok(())
    }

    /// Cancel is allowed while the one Message reservation occupies capacity.
    /// Caller, Guard and Cancel's *own* correlation are checked by Core first.
    pub(super) fn cancel(
        &self,
        identity: PrimaryLinkIdentity,
        attempt: u32,
        nonce: u64,
        cancel_correlation: CorrelationId,
    ) -> Result<CancelDisposition, DialogFault> {
        let mut state = self.state.lock().map_err(|_| DialogFault::NativeFailure)?;
        if state.custody.active_owner().is_some_and(|owner| {
            owner.identity == identity
                && owner.attempt == attempt
                && owner.nonce == nonce
                && owner.correlation == cancel_correlation
        }) {
            // The original CALL and Cancel must occupy different correlations;
            // an ACK for the same ID would leave the original abandoned forever.
            return Err(DialogFault::InvalidRequest);
        }
        let outcome = state.custody.cancel(identity, attempt, nonce);
        drop(state);
        match outcome {
            CancelResult::Requested => {
                self.wake
                    .send(AppWindowCommand::DialogWake)
                    .map_err(|_| DialogFault::NativeFailure)?;
                Ok(CancelDisposition::Requested)
            }
            CancelResult::AlreadyCompleted => Ok(CancelDisposition::AlreadyCompleted),
            CancelResult::NotFound => Err(DialogFault::InvalidRequest),
        }
    }

    /// Role retirement tombstones the effect but retains the physical panel
    /// until the native UI owner observes physical completion and releases it.
    pub(super) fn retire(&self, identity: PrimaryLinkIdentity, attempt: u32) {
        // A guarded FS-only role and a dialog-capable role with no admitted
        // panel must retire without inserting a gratuitous UI command into
        // the shared Host queue. This also keeps existing FS/Ready teardown
        // ordering unchanged when the policy happens to include `dialog`.
        let wake_needed = if let Ok(mut state) = self.state.lock() {
            let wake = state.custody.retire(identity, attempt);
            #[cfg(windows)]
            {
                if state.publication.as_ref().is_some_and(|record| {
                    record.owner.identity == identity
                        && record.owner.attempt == attempt
                        && !record.preparing
                        && record.receipt.is_none()
                }) {
                    state.publication = None;
                }
                if state.custody.active_owner().is_none() {
                    state.caller = None;
                }
            }
            wake
        } else {
            false
        };
        if wake_needed {
            let _ = self.wake.send(AppWindowCommand::DialogWake);
        }
    }

    /// Pending original correlation for the *current authenticated* generation.
    /// Existing Core retirement writes IPC023 before closing the sole writer;
    /// a completed original never gets a second terminal on that path.
    pub(super) fn pending_original_for(
        &self,
        identity: PrimaryLinkIdentity,
        attempt: u32,
    ) -> Option<super::CorrelationId> {
        self.state
            .lock()
            .ok()?
            .custody
            .original_pending_for(identity, attempt)
    }

    pub(super) fn has_capacity(&self) -> bool {
        self.state.lock().is_ok_and(|state| {
            #[cfg(windows)]
            if state.publication.is_some() {
                return false;
            }
            state.custody.active_owner().is_none()
        })
    }

    #[cfg(any(target_os = "macos", test))]
    pub(super) fn fully_drained(&self) -> bool {
        self.state.lock().is_ok_and(|state| {
            #[cfg(windows)]
            if state.publication.is_some() {
                return false;
            }
            state.pending.is_none() && state.custody.active_owner().is_none()
        })
    }

    fn take_work(&self) -> Result<Option<DialogWork>, HostAppError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| app_detail("native dialog queue", "custody lock poisoned"))?
            .pending
            .take())
    }

    fn may_present(&self, owner: DialogOwner) -> Result<bool, HostAppError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| app_detail("native dialog presentation", "custody lock poisoned"))?
            .custody
            .may_present(owner))
    }

    fn expire(&self, owner: DialogOwner) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("native dialog deadline", "custody lock poisoned"))?;
        state.custody.expire(owner);
        Ok(())
    }

    /// The Tao owner invokes this only after observing the actual native
    /// result and physical dialog completion. Commit the terminal *before*
    /// posting its event to the Core coordinator. Otherwise a later Cancel
    /// could overtake a click already observed on the main thread while the
    /// event receiver was temporarily descheduled. This creates no new nonce
    /// or generation owner; the one existing custody lock decides the winner.
    fn record_native_outcome(
        &self,
        owner: DialogOwner,
        outcome: DialogUiOutcome,
        file_result: Option<DialogReplyV1>,
    ) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("native dialog callback", "custody lock poisoned"))?;
        if state.custody.active_owner() != Some(owner) {
            return Ok(()); // Already retired or from an older physical owner.
        }
        match (outcome, file_result) {
            (DialogUiOutcome::Selected { .. }, None) => {
                state.custody.commit_native_result(owner);
            }
            (
                DialogUiOutcome::FileResult,
                Some(value @ (DialogReplyV1::Open { .. } | DialogReplyV1::Save { .. })),
            ) => {
                if state.custody.commit_native_result(owner) {
                    state.file_result = Some((owner, value));
                }
            }
            (DialogUiOutcome::Dismissed | DialogUiOutcome::Failed, None) => {
                state.custody.commit_native_failure(owner);
            }
            _ => {
                return Err(app_detail(
                    "native dialog result",
                    "wrong native operation/result correlation",
                ));
            }
        }
        Ok(())
    }
}

mod completion;
#[cfg(windows)]
mod dispatch_writer;
mod host_route;
#[cfg(windows)]
mod publication;

#[cfg(target_os = "macos")]
mod native_ui;
#[cfg(target_os = "macos")]
pub(super) use native_ui::DialogMainOwner;
#[cfg(windows)]
mod windows_ui;
#[cfg(windows)]
pub(super) use windows_ui::DialogMainOwner;

#[cfg(test)]
mod tests;
