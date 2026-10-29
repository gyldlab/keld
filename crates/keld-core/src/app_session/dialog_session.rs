//! One authenticated primary generation's guarded native Message custody.
//! The app-link reader owns admission; Tao owns the actual !Send `AppKit` sheet.
//! Both use the existing Runtime generation and `WindowRegistry` identities.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keld_guard::Principal;
use keld_guard::capability;
use keld_guard::verified_manifest::VerifiedManifest;
use keld_ipc::CallError;
use keld_ipc::guard_dispatch::dispatch_privileged;
use keld_native::dialog::macos_message::{MacMessageOutcome, MacMessagePanel, MacMessageParent};
use keld_native::dialog::{CancelDisposition, DialogReplyV1, MessageOptions};
use keld_wv::WebviewId;
use keld_wv::wkwebview::{AppWindowCommand, AppWindowEvent, MacDialogUiOutcome};
use objc2::MainThreadMarker;
use objc2::rc::Retained;

use super::dialog_custody::{CancelResult, DialogCustody, DialogOwner, ReserveError, TerminalKind};
use super::window_registry::{WindowError, WindowOwner};
use super::{
    ActivePrimaryGeneration, CorrelationId, FrameKind, HostAppError, PrimaryCallerContext,
    PrimaryLinkIdentity, PrimaryRouterHandle, app_detail, app_ipc, encode,
    with_primary_caller_locked, write_frame,
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
                "Use a qualified macOS window-backed native dialog; parentless presentation is not yet admitted."
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
                "The macOS native presentation or callback failed; inspect the Host-owned native error and retry only as a new user action."
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
    pub(super) options: MessageOptions,
    pub(super) parent: WebviewId,
    pub(super) deadline: Instant,
}

struct DialogState {
    custody: DialogCustody,
    pending: Option<DialogWork>,
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
    pub(super) fn reserve(
        &self,
        owner: DialogOwner,
        options: MessageOptions,
        parent: WebviewId,
    ) -> Result<(), DialogFault> {
        {
            let mut state = self.state.lock().map_err(|_| DialogFault::NativeFailure)?;
            state.custody.reserve(owner).map_err(|error| match error {
                ReserveError::Busy => DialogFault::Busy,
                ReserveError::Stale | ReserveError::InvalidTransition => DialogFault::NativeFailure,
            })?;
            state.pending = Some(DialogWork {
                owner,
                options,
                parent,
                deadline: Instant::now() + DIALOG_NATIVE_DEADLINE,
            });
        }
        if self.wake.send(AppWindowCommand::MacDialogWake).is_err() {
            // The native owner never received the queue wake. Retire and drain
            // the reserved but never-presented effect without fabricating UI.
            if let Ok(mut state) = self.state.lock() {
                state.pending = None;
                state.custody.retire(owner.identity, owner.attempt);
                let _ = state.custody.quiesced(owner);
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
                    .send(AppWindowCommand::MacDialogWake)
                    .map_err(|_| DialogFault::NativeFailure)?;
                Ok(CancelDisposition::Requested)
            }
            CancelResult::AlreadyCompleted => Ok(CancelDisposition::AlreadyCompleted),
            CancelResult::NotFound => Err(DialogFault::InvalidRequest),
        }
    }

    /// Role retirement tombstones the effect but retains the physical panel
    /// until a genuine `AppKit` callback and invisibility release it.
    pub(super) fn retire(&self, identity: PrimaryLinkIdentity, attempt: u32) {
        // A guarded FS-only role and a dialog-capable role with no admitted
        // panel must retire without inserting a gratuitous UI command into
        // the shared Host queue. This also keeps existing FS/Ready teardown
        // ordering unchanged when the policy happens to include `dialog`.
        let wake_needed = if let Ok(mut state) = self.state.lock() {
            state.custody.retire(identity, attempt)
        } else {
            false
        };
        if wake_needed {
            let _ = self.wake.send(AppWindowCommand::MacDialogWake);
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
        self.state
            .lock()
            .is_ok_and(|state| state.custody.active_owner().is_none())
    }

    pub(super) fn fully_drained(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.pending.is_none() && state.custody.active_owner().is_none())
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

    /// The Tao owner invokes this only after observing the actual `AppKit`
    /// callback and physical sheet invisibility. Commit the terminal *before*
    /// posting its event to the Core coordinator. Otherwise a later Cancel
    /// could overtake a click already observed on the main thread while the
    /// event receiver was temporarily descheduled. This creates no new nonce
    /// or generation owner; the one existing custody lock decides the winner.
    fn record_native_outcome(
        &self,
        owner: DialogOwner,
        outcome: MacDialogUiOutcome,
    ) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("native dialog callback", "custody lock poisoned"))?;
        if state.custody.active_owner() != Some(owner) {
            return Ok(()); // Already retired or from an older physical owner.
        }
        match outcome {
            MacDialogUiOutcome::Selected { .. } => {
                state.custody.commit_native_result(owner);
            }
            MacDialogUiOutcome::Dismissed | MacDialogUiOutcome::Failed => {
                state.custody.commit_native_failure(owner);
            }
        }
        Ok(())
    }

    /// A native result is delivered to Core only after physical quiescence.
    /// Nothing here is a trusted caller: outcome may no longer have a live role.
    pub(super) fn complete(
        &self,
        attempt: u32,
        nonce: u64,
        outcome: MacDialogUiOutcome,
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
            // Stale/duplicate callbacks cannot mint a reply or release custody.
            return Ok(None);
        };
        // Do not invent a winning terminal here: the native owner committed
        // the outcome on the UI thread before sending this event. An observed
        // Cancel/expiry/retirement already owns the cause if it happened first.
        let terminal = state
            .custody
            .take_original_terminal(owner)
            .map(|(_, kind)| {
                let result = match (kind, outcome) {
                    (
                        TerminalKind::NativeResult,
                        MacDialogUiOutcome::Selected {
                            response,
                            checkbox_checked,
                        },
                    ) => Ok(DialogReplyV1::Message {
                        response,
                        checkbox_checked,
                    }),
                    (TerminalKind::Cancelled, _) => Err(DialogFault::Cancelled),
                    (TerminalKind::Expired, _) => Err(DialogFault::Expired),
                    (TerminalKind::NativeFailure | TerminalKind::NativeResult, _) => {
                        Err(DialogFault::NativeFailure)
                    }
                };
                (owner, result)
            });
        state.custody.quiesced(owner).map_err(|_| {
            app_detail(
                "native dialog terminal",
                "physical quiescence has no active effect",
            )
        })?;
        drop(state);
        // A terminal native callback can be delivered after Tao's last tick;
        // this wake makes final close wait until the custody record is gone.
        let _ = self.wake.send(AppWindowCommand::MacDialogWake);
        Ok(terminal)
    }
}

mod host_route;

/// The retained macOS UI owner. All `NSAlert`, callbacks and `NSWindow`
/// references stay on the actual Tao thread and never enter the worker queue.
pub(super) struct DialogMainOwner {
    session: Arc<DialogSession>,
    events: Sender<AppWindowEvent>,
    active: Option<(DialogOwner, WebviewId, MacMessagePanel, Instant)>,
}

impl DialogMainOwner {
    pub(super) fn new(session: Arc<DialogSession>, events: Sender<AppWindowEvent>) -> Self {
        Self {
            session,
            events,
            active: None,
        }
    }

    fn notify(&self, owner: DialogOwner, outcome: MacDialogUiOutcome) -> Result<(), HostAppError> {
        self.session.record_native_outcome(owner, outcome)?;
        self.events
            .send(AppWindowEvent::MacDialogFinished {
                attempt: owner.attempt,
                nonce: owner.nonce,
                outcome,
            })
            .map_err(|_| {
                app_detail(
                    "native dialog terminal",
                    "Core event coordinator unavailable",
                )
            })
    }

    /// Runs one bounded main-loop turn. No `AppKit` call is made while holding
    /// a shared mutex; the caller's Tao event loop owns autorelease pools.
    ///
    /// The sole parent resolver returns a live Wry-owned native `NSWindow` for
    /// a previously registry-validated `WebviewId`, never a Bun pointer.
    pub(super) fn drive(
        &mut self,
        main: MainThreadMarker,
        resolve: &mut dyn FnMut(WebviewId) -> Option<Retained<MacMessageParent>>,
        stopping: bool,
    ) -> Result<(bool, Option<Instant>), HostAppError> {
        if self.active.is_none()
            && let Some(work) = self.session.take_work()?
        {
            if Instant::now() >= work.deadline {
                self.session.expire(work.owner)?;
                self.notify(work.owner, MacDialogUiOutcome::Dismissed)?;
            } else if stopping || !self.session.may_present(work.owner)? {
                self.notify(work.owner, MacDialogUiOutcome::Dismissed)?;
            } else if let Some(parent) = resolve(work.parent) {
                let mut panel = MacMessagePanel::prepare(main, &work.options)
                    .map_err(|source| app_detail("native dialog prepare", source.to_string()))?;
                panel
                    .begin_sheet(&parent)
                    .map_err(|source| app_detail("native dialog sheet", source.to_string()))?;
                self.active = Some((work.owner, work.parent, panel, work.deadline));
            } else {
                self.notify(work.owner, MacDialogUiOutcome::Failed)?;
            }
        }
        if let Some((owner, parent_view, panel, deadline)) = self.active.as_mut() {
            // A registry window may be destroyed while its sheet was visible.
            // Never retain the native panel solely because the former NSWindow
            // pointer still has an AppKit retain count: the UI owner checks
            // the live `WebviewId` map on every active service turn.
            if stopping || !self.session.may_present(*owner)? || resolve(*parent_view).is_none() {
                panel
                    .request_cancel()
                    .map_err(|source| app_detail("native dialog cancel", source.to_string()))?;
            }
            if let Some(outcome) = panel
                .poll_outcome()
                .map_err(|source| app_detail("native dialog callback", source.to_string()))?
            {
                let owner = *owner;
                let outcome = match outcome {
                    MacMessageOutcome::Selected(DialogReplyV1::Message {
                        response,
                        checkbox_checked,
                    }) => MacDialogUiOutcome::Selected {
                        response,
                        checkbox_checked,
                    },
                    MacMessageOutcome::Selected(_) | MacMessageOutcome::Dismissed => {
                        MacDialogUiOutcome::Dismissed
                    }
                };
                self.notify(owner, outcome)?;
                self.active = None;
            } else if Instant::now() >= *deadline {
                self.session.expire(*owner)?;
                panel
                    .request_cancel()
                    .map_err(|source| app_detail("native dialog expired", source.to_string()))?;
            }
        }
        let next = self.active.as_ref().map(|(_, _, _, deadline)| {
            // After a native cancellation the callback can precede physical
            // invisibility by one main turn; wake promptly to check quiescence.
            (*deadline).min(Instant::now() + Duration::from_millis(20))
        });
        Ok((self.active.is_none() && self.session.fully_drained(), next))
    }
}

#[cfg(test)]
mod tests;
