use super::{
    AppWindowEvent, DialogOwner, DialogSession, DialogWorkOptions, HostAppError,
    MacDialogUiOutcome, WebviewId, app_detail,
};
use keld_native::dialog::DialogReplyV1;
use keld_native::dialog::macos_file::MacFilePanel;
use keld_native::dialog::macos_message::{MacMessageOutcome, MacMessagePanel, MacMessageParent};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

enum NativePanel {
    Message(MacMessagePanel),
    File(MacFilePanel),
}

impl NativePanel {
    fn prepare(main: MainThreadMarker, options: DialogWorkOptions) -> Result<Self, HostAppError> {
        match options {
            DialogWorkOptions::Message(value) => MacMessagePanel::prepare(main, &value)
                .map(Self::Message)
                .map_err(|source| app_detail("native Message preparation", source.to_string())),
            DialogWorkOptions::Open(value) => MacFilePanel::prepare_open(main, &value)
                .map(Self::File)
                .map_err(|source| app_detail("native Open preparation", source.to_string())),
            DialogWorkOptions::Save(value) => MacFilePanel::prepare_save(main, &value)
                .map(Self::File)
                .map_err(|source| app_detail("native Save preparation", source.to_string())),
        }
    }

    fn begin_sheet(&mut self, parent: &MacMessageParent) -> Result<(), HostAppError> {
        let result = match self {
            Self::Message(panel) => panel.begin_sheet(parent),
            Self::File(panel) => panel.begin_sheet(parent),
        };
        result.map_err(|source| app_detail("native dialog sheet", source.to_string()))
    }

    fn request_cancel(&mut self) -> Result<bool, HostAppError> {
        let result = match self {
            Self::Message(panel) => panel.request_cancel(),
            Self::File(panel) => panel.request_cancel(),
        };
        result.map_err(|source| app_detail("native dialog cancellation", source.to_string()))
    }

    fn poll_outcome(
        &mut self,
    ) -> Result<Option<(MacDialogUiOutcome, Option<DialogReplyV1>)>, HostAppError> {
        match self {
            Self::Message(panel) => panel
                .poll_outcome()
                .map(|outcome| {
                    outcome.map(|value| {
                        let outcome = match value {
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
                        (outcome, None)
                    })
                })
                .map_err(|source| app_detail("native Message callback", source.to_string())),
            Self::File(panel) => match panel.poll_outcome() {
                Ok(result) => Ok(result.map(|value| (MacDialogUiOutcome::FileResult, Some(value)))),
                Err(_source) if panel.quiescent() => {
                    // A malformed OS path is never a user Cancel. Real native
                    // callback and invisibility have already drained the UI.
                    Ok(Some((MacDialogUiOutcome::Failed, None)))
                }
                Err(source) => Err(app_detail("native file callback", source.to_string())),
            },
        }
    }
}

/// The retained macOS UI owner. All `NSAlert`, callbacks and `NSWindow`
/// references stay on the actual Tao thread and never enter the worker queue.
pub(crate) struct DialogMainOwner {
    session: Arc<DialogSession>,
    events: Sender<AppWindowEvent>,
    active: Option<(DialogOwner, WebviewId, NativePanel, Instant)>,
}

impl DialogMainOwner {
    pub(crate) fn new(session: Arc<DialogSession>, events: Sender<AppWindowEvent>) -> Self {
        Self {
            session,
            events,
            active: None,
        }
    }

    fn notify(
        &self,
        owner: DialogOwner,
        outcome: MacDialogUiOutcome,
        file_result: Option<DialogReplyV1>,
    ) -> Result<(), HostAppError> {
        self.session
            .record_native_outcome(owner, outcome, file_result)?;
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
    pub(crate) fn drive(
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
                self.notify(work.owner, MacDialogUiOutcome::Dismissed, None)?;
            } else if stopping || !self.session.may_present(work.owner)? {
                self.notify(work.owner, MacDialogUiOutcome::Dismissed, None)?;
            } else if let Some(parent) = resolve(work.parent) {
                match NativePanel::prepare(main, work.options) {
                    Ok(mut panel) => {
                        if let Err(error) = panel.begin_sheet(&parent) {
                            // A failed begin is still in Prepared state, so
                            // no physical sheet needs draining. Preserve the
                            // original correlation instead of killing Host.
                            eprintln!("{error}");
                            self.notify(work.owner, MacDialogUiOutcome::Failed, None)?;
                        } else {
                            self.active = Some((work.owner, work.parent, panel, work.deadline));
                        }
                    }
                    Err(error) => {
                        // take_work already owns this reservation. A failed
                        // AppKit preparation must complete it through normal
                        // custody, never leave it retained on a fatal exit.
                        eprintln!("{error}");
                        self.notify(work.owner, MacDialogUiOutcome::Failed, None)?;
                    }
                }
            } else {
                self.notify(work.owner, MacDialogUiOutcome::Failed, None)?;
            }
        }
        if let Some((owner, parent_view, panel, deadline)) = self.active.as_mut() {
            // A registry window may be destroyed while its sheet was visible.
            // Never retain the native panel solely because the former NSWindow
            // pointer still has an AppKit retain count: the UI owner checks
            // the live `WebviewId` map on every active service turn.
            if stopping || !self.session.may_present(*owner)? || resolve(*parent_view).is_none() {
                panel.request_cancel()?;
            }
            if let Some((outcome, file_result)) = panel.poll_outcome()? {
                let owner = *owner;
                self.notify(owner, outcome, file_result)?;
                self.active = None;
            } else if Instant::now() >= *deadline {
                self.session.expire(*owner)?;
                panel.request_cancel()?;
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
