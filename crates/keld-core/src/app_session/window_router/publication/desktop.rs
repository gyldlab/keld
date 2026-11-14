//! Approved Windows S1: exact-reader resampling on the existing Window source.
//! No native call, writer wait or failure-owner RPC crosses a canonical guard.
use super::super::publication_turn::{PendingWindowReplay, ReplayStage};
use super::{
    Arc, CorrelationId, HostAppError, Instant, NativePublicationDisposition, PrimaryRouterHandle,
    PublicationResult, WindowEmitter, WindowEvent, WriterLink, app_detail,
    with_primary_caller_locked,
};

mod source;
mod wire;

use source::{DesktopBefore, DesktopSource, prepare_snapshot};
pub(super) use wire::retains_snapshot;

/// The one existing early Subscribe keeps its actual link and original clock.
/// It has no encoded body, native operation or independent completion queue.
pub(in crate::app_session::window_router) struct PendingDesktopSubscribe {
    pub(super) target: Arc<WriterLink>,
    pub(super) correlation: CorrelationId,
    pub(super) deadline: Instant,
}

enum DesktopPublication {
    None,
    Replay {
        correlation: CorrelationId,
        deadline: Instant,
    },
    Update,
}

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn desktop_snapshot_selected(&self) -> Result<bool, HostAppError> {
        self.windows.as_ref().map_or(Ok(false), |windows| {
            windows
                .lock()
                .map(|session| session.desktop_required)
                .map_err(|_| app_detail("desktop selection", "Registry lock poisoned"))
        })
    }

    /// Installation already owns transition/current. Enqueue only the exact
    /// captured reader; the existing UI source performs acquisition later.
    pub(in crate::app_session) fn bind_desktop_producer(
        &self,
        capture: keld_wv::AppDesktopCapture,
    ) -> Result<(), HostAppError> {
        if self.desktop_snapshot_selected()? {
            self.window_commands
                .send(keld_wv::AppWindowCommand::WindowsDesktopProducer { capture })
                .map_err(|_| app_detail("desktop producer bind", "native receiver closed"))?;
        }
        Ok(())
    }

    pub(super) fn publish_native_desktop(
        &self,
        capture: Option<keld_wv::AppDesktopCapture>,
        facts: Option<keld_wv::AppDesktopFacts>,
        deadline: Instant,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        // Capture before any ordering wait. A wake cannot recapture successor B.
        let Some(source) = DesktopSource::capture(self, capture, deadline)? else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let mut published = None;
        let mut turn = None;
        let result: PublicationResult<()> = (|| {
            turn = self.claim_window_publication(source.deadline, None)?;
            let Some(turn) = turn.as_ref() else {
                return Err(app_detail("desktop observation", "Registry disappeared").into());
            };
            let Some(before) = source.before(self, turn)? else {
                return Ok(());
            };
            let prepared = prepare_snapshot(facts, &before)?;
            let Some(snapshot) = prepared else {
                return Ok(());
            };
            let action = self.commit_desktop_observation(turn, &source, &before, &snapshot)?;
            // Only Arc aliases survive; neither the old fact Vec nor a second
            // encoded replay body is retained through physical writer waiting.
            drop(before);
            published = Some(Arc::clone(&snapshot));
            match action {
                DesktopPublication::None => Ok(()),
                DesktopPublication::Replay {
                    correlation,
                    deadline,
                } => WindowEmitter {
                    router: self,
                    turn,
                    target: &source.target,
                    deadline,
                }
                .replay(correlation),
                DesktopPublication::Update => {
                    let emitter = WindowEmitter {
                        router: self,
                        turn,
                        target: &source.target,
                        deadline: source.deadline,
                    };
                    emitter
                        .desktop_snapshot(&snapshot, |session| {
                            session.subscribed == Some(emitter.owner())
                        })
                        .map(drop)
                }
            }
        })();
        // Original timeout/poison cause survives unchanged; no owner RPC under turn.
        drop(turn);
        let disposition = self.finish_native_publication(
            source.target.caller(),
            |session| {
                session.desktop_required
                    && published.as_ref().is_none_or(|snapshot| {
                        session
                            .desktop_latest
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, snapshot))
                    })
            },
            result,
        )?;
        if matches!(disposition, NativePublicationDisposition::Handled) {
            self.finish_window_publication_service(Ok(()))?;
        }
        Ok(disposition)
    }

    fn commit_desktop_observation(
        &self,
        turn: &super::WindowPublicationTurn<'_>,
        source: &DesktopSource,
        before: &DesktopBefore,
        snapshot: &Arc<WindowEvent>,
    ) -> Result<DesktopPublication, HostAppError> {
        let mut action = DesktopPublication::None;
        with_primary_caller_locked(
            &self.current,
            &self.shutdown,
            source.target.caller(),
            |_| {
                let windows = self.windows.as_ref().ok_or_else(|| {
                    app_detail("desktop observation commit", "Registry disappeared")
                })?;
                let mut session = windows.lock().map_err(|_| {
                    app_detail("desktop observation commit", "Registry lock poisoned")
                })?;
                if !turn.owns(windows, &session) {
                    return Err(app_detail(
                        "desktop observation commit",
                        "foreign source turn",
                    ));
                }
                if !source.matches(&session) {
                    return Ok(());
                }
                if !before.matches(&session) {
                    return Err(app_detail(
                        "desktop observation commit",
                        "same source changed",
                    ));
                }
                let pending = session.pending_desktop.as_ref().map(|pending| {
                    (
                        Arc::clone(&pending.target),
                        pending.correlation,
                        pending.deadline,
                    )
                });
                if pending
                    .as_ref()
                    .is_some_and(|(target, _, _)| !Arc::ptr_eq(target, &source.target))
                {
                    return Err(app_detail(
                        "desktop observation commit",
                        "foreign pending reader",
                    ));
                }
                let WindowEvent::DesktopSnapshot { epoch, .. } = snapshot.as_ref() else {
                    return Err(app_detail(
                        "desktop observation commit",
                        "non-desktop source",
                    ));
                };
                session.desktop_seen = true;
                session.desktop_epoch.0 = *epoch;
                session.desktop_latest = Some(Arc::clone(snapshot));
                if source.initial() {
                    session.initial_desktop_closed = true;
                }
                if let Some((target, correlation, deadline)) = pending {
                    // Infallible transfer of the same source/correlation/clock
                    // into the existing turn-owned cursor; no second pending call.
                    session.pending_desktop = None;
                    session.publication.replay = Some(PendingWindowReplay {
                        target,
                        correlation,
                        deadline,
                        stage: ReplayStage::Desktop,
                        after: 0,
                    });
                    action = DesktopPublication::Replay {
                        correlation,
                        deadline,
                    };
                } else if session.subscribed
                    == Some(super::WindowOwner::new(
                        source.target.caller().identity,
                        source.target.caller().attempt,
                    ))
                {
                    action = DesktopPublication::Update;
                }
                Ok(())
            },
        )?;
        Ok(action)
    }
}

#[cfg(test)]
mod tests;
