//! Windows native calls consume the same guarded reservation and terminal owner.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Instant;

use keld_native::dialog::windows::{WindowsDialogError, WindowsDialogOptions};
use keld_wv::webview2::WindowsDialogParentResolver;

use super::{
    AppWindowEvent, DialogFault, DialogOwner, DialogReplyV1, DialogSession, DialogUiOutcome,
    DialogWorkOptions, HostAppError, WebviewId, app_detail,
};

pub(crate) struct DialogMainOwner {
    session: Arc<DialogSession>,
    events: Sender<AppWindowEvent>,
}

impl DialogMainOwner {
    pub(crate) fn new(session: Arc<DialogSession>, events: Sender<AppWindowEvent>) -> Self {
        Self { session, events }
    }

    fn finish(
        &self,
        owner: DialogOwner,
        outcome: DialogUiOutcome,
        file_result: Option<DialogReplyV1>,
    ) -> Result<(), HostAppError> {
        self.session
            .record_native_outcome(owner, outcome, file_result)?;
        self.events
            .send(AppWindowEvent::DialogFinished {
                attempt: owner.attempt,
                nonce: owner.nonce,
                outcome,
            })
            .map_err(|_| app_detail("Windows dialog terminal", "Core event owner is unavailable"))
    }

    /// Called outside `WebView2` callbacks. Parent resolution ends its W2 map
    /// borrow before native entry; the returned lease lives through modal return.
    pub(crate) fn drive(
        &mut self,
        resolve: &mut WindowsDialogParentResolver<'_>,
        terminal: Arc<AtomicBool>,
    ) -> Result<(), HostAppError> {
        let Some(work) = self.session.take_work()? else {
            return Ok(());
        };
        if Instant::now() >= work.deadline {
            self.session.expire(work.owner)?;
            return self.finish(work.owner, DialogUiOutcome::Dismissed, None);
        }
        if !self.session.may_present(work.owner)? {
            return self.finish(work.owner, DialogUiOutcome::Dismissed, None);
        }
        let Some(parent) = resolve(work.parent) else {
            self.session.parent_retiring(work.parent)?;
            return self.finish(work.owner, DialogUiOutcome::Dismissed, None);
        };
        let session = Arc::downgrade(&self.session);
        let owner = work.owner;
        let deadline = work.deadline;
        let cancelled: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
            session.upgrade().is_none_or(|session| {
                session.modal_must_cancel(owner, deadline, terminal.load(Ordering::Acquire))
            })
        });
        let options = match &work.options {
            DialogWorkOptions::Message(options) => WindowsDialogOptions::Message(options),
            DialogWorkOptions::Open(options) => WindowsDialogOptions::Open(options),
            DialogWorkOptions::Save(options) => WindowsDialogOptions::Save(options),
        };
        let result = if cancelled() {
            Ok(None)
        } else {
            keld_native::dialog::windows::show(parent.hwnd(), options, cancelled)
        };
        let (outcome, file_result) = match result {
            Ok(Some(DialogReplyV1::Message {
                response,
                checkbox_checked,
            })) => (
                DialogUiOutcome::Selected {
                    response,
                    checkbox_checked,
                },
                None,
            ),
            Ok(Some(reply @ (DialogReplyV1::Open { .. } | DialogReplyV1::Save { .. }))) => {
                (DialogUiOutcome::FileResult, Some(reply))
            }
            Ok(None) => (DialogUiOutcome::Dismissed, None),
            Err(WindowsDialogError::UnsupportedParent) => {
                self.session
                    .record_native_fault(owner, DialogFault::Unsupported)?;
                (DialogUiOutcome::Failed, None)
            }
            Err(WindowsDialogError::Failed(error)) => {
                eprintln!("KELD-NATIVE-016: {error}");
                (DialogUiOutcome::Failed, None)
            }
            Ok(Some(DialogReplyV1::Cancel { .. })) => (DialogUiOutcome::Failed, None),
        };
        self.finish(owner, outcome, file_result)
    }
}

impl DialogSession {
    fn modal_must_cancel(&self, owner: DialogOwner, deadline: Instant, terminal: bool) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return true;
        };
        if state.custody.active_owner() != Some(owner) {
            return true;
        }
        if terminal {
            // UI terminal intent can be a profile handover with a live link.
            // Only generation retirement suppresses that link's original reply.
            state
                .custody
                .cancel(owner.identity, owner.attempt, owner.nonce);
        } else if Instant::now() >= deadline {
            state.custody.expire(owner);
        }
        !state.custody.may_present(owner)
    }

    pub(in super::super) fn parent_retiring(&self, parent: WebviewId) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("Windows dialog parent", "custody lock poisoned"))?;
        if state.parent == Some(parent)
            && let Some(owner) = state.custody.active_owner()
        {
            state
                .custody
                .cancel(owner.identity, owner.attempt, owner.nonce);
        }
        Ok(())
    }

    fn record_native_fault(
        &self,
        owner: DialogOwner,
        fault: DialogFault,
    ) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("Windows dialog fault", "custody lock poisoned"))?;
        if state.custody.active_owner() == Some(owner) {
            state.native_fault = Some(fault);
        }
        Ok(())
    }
}
