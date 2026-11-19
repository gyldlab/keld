//! Ordinary native facts retain the original Registry entry and writer target.

use super::super::super::primary_writer::WriterLink;
use super::super::native_outcome::{
    NativeOutcome, build_refusal, report_ignored_build, report_ignored_build_failure,
    report_native_close, report_native_fact, report_release,
};
use super::super::publication_turn::WindowPublicationTurn;
use super::super::{
    AppWindowEvent, BuildOutcome, HostAppError, Ordering, PrimaryCallerContext,
    PrimaryRouterHandle, WindowOwner, WindowReply, WindowResponse, app_detail, owner,
};
use super::{NativePublicationDisposition, PublicationError, PublicationResult, WindowEmitter};
use keld_ipc::window::WindowRef;
use std::sync::Arc;
use std::time::Instant;

struct CapturedNativeSource {
    window: Option<WindowRef>,
    target: Option<Arc<WriterLink>>,
}

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn window_native_event_ordered(
        &self,
        event: AppWindowEvent,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        self.primary_writer_owner()?;
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let captured = self.capture_window_native_source(&event)?;
        let turn = match self.claim_window_publication(deadline, None) {
            Ok(Some(turn)) => turn,
            Ok(None) => return Err(app_detail("Window native source", "Registry disappeared")),
            Err(error) => {
                // The native fact still owns its physical transition. No frame
                // can use a turn owned by another source.
                let cleanup = self
                    .prepare_window_native_source(event, None)
                    .and_then(|outcome| self.cleanup_unpublished_window_native(outcome));
                if captured.target.is_none() {
                    return cleanup.map(|()| NativePublicationDisposition::Handled);
                }
                return self.finish_window_native_source(
                    &captured,
                    Err(PublicationError::from(error).with_cleanup(cleanup)),
                );
            }
        };
        let outcome = self.prepare_window_native_source(event, Some(&turn));
        if let (Some(window), Some(target)) = (captured.window, captured.target.as_ref()) {
            let emitter = WindowEmitter {
                router: self,
                turn: &turn,
                target,
                deadline,
            };
            let result = outcome
                .map_err(PublicationError::from)
                .and_then(|outcome| emitter.native(window, outcome));
            drop(turn);
            self.finish_window_native_source(&captured, result)
        } else {
            outcome.and_then(|outcome| self.cleanup_unpublished_window_native(outcome))?;
            Ok(NativePublicationDisposition::Handled)
        }
    }

    fn capture_window_native_source(
        &self,
        event: &AppWindowEvent,
    ) -> Result<CapturedNativeSource, HostAppError> {
        let phase = "Window native source";
        let _transition = self.shutdown.transition_guard();
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail(phase, "current lock poisoned"))?;
        let windows = self
            .windows
            .as_ref()
            .ok_or_else(|| app_detail(phase, "Registry disappeared"))?;
        let session = windows
            .lock()
            .map_err(|_| app_detail(phase, "Registry lock poisoned"))?;
        let window = session.registry.native_publication_window(event);
        let active = current.as_ref().filter(|active| {
            self.shutdown.is_running()
                && active.identity.is_primary()
                && !active.reader_stop.load(Ordering::Acquire)
                && session.subscribed == Some(owner(active))
                && window.is_some_and(|window| {
                    session
                        .registry
                        .retains_publication_window(owner(active), window)
                })
        });
        let target = active
            .map(|active| {
                let caller = PrimaryCallerContext {
                    identity: active.identity,
                    attempt: active.attempt,
                    reader_stop: Arc::clone(&active.reader_stop),
                };
                active
                    .writer_link
                    .as_ref()
                    .filter(|link| link.matches(&caller))
                    .map(Arc::clone)
                    .ok_or_else(|| app_detail(phase, "exact original writer link is unavailable"))
            })
            .transpose()?;
        Ok(CapturedNativeSource { window, target })
    }

    fn prepare_window_native_source(
        &self,
        event: AppWindowEvent,
        turn: Option<&WindowPublicationTurn<'_>>,
    ) -> Result<NativeOutcome, HostAppError> {
        let phase = "Window native transition";
        let _transition = self.shutdown.transition_guard();
        let _current = self
            .current
            .lock()
            .map_err(|_| app_detail(phase, "current lock poisoned"))?;
        let windows = self
            .windows
            .as_ref()
            .ok_or_else(|| app_detail(phase, "Registry disappeared"))?;
        let mut session = windows
            .lock()
            .map_err(|_| app_detail(phase, "Registry lock poisoned"))?;
        if turn.is_some_and(|turn| !turn.owns(windows, &session)) {
            return Err(app_detail(phase, "foreign Window publication turn"));
        }
        Self::prepare_window_native_locked(&mut session, event)
    }

    fn finish_window_native_source(
        &self,
        captured: &CapturedNativeSource,
        result: PublicationResult<()>,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let (Some(window), Some(target)) = (captured.window, captured.target.as_ref()) else {
            return self
                .finish_call_publication(result)
                .map(|()| NativePublicationDisposition::Handled);
        };
        let caller = target.caller();
        let owner = WindowOwner::new(caller.identity, caller.attempt);
        self.finish_native_publication(
            caller,
            |session| {
                session.subscribed == Some(owner)
                    && session.registry.retains_publication_window(owner, window)
            },
            result,
        )
    }

    fn cleanup_unpublished_window_native(
        &self,
        outcome: NativeOutcome,
    ) -> Result<(), HostAppError> {
        match outcome {
            NativeOutcome::Build(BuildOutcome::Cleanup(action)) => {
                self.cleanup_window_action(action, "Window late-build cleanup")
            }
            NativeOutcome::Build(BuildOutcome::Ignored) => {
                report_ignored_build();
                Ok(())
            }
            NativeOutcome::BuildFailed { outcome, .. } if outcome.diagnostic => {
                report_ignored_build_failure();
                Ok(())
            }
            NativeOutcome::Released(outcome) => {
                report_release(&outcome);
                Ok(())
            }
            NativeOutcome::Close(close) => report_native_close(close).map(|_| ()),
            NativeOutcome::Fact(fact) => {
                report_native_fact(fact);
                Ok(())
            }
            NativeOutcome::Build(_) | NativeOutcome::BuildFailed { .. } => Ok(()),
        }
    }
}

impl WindowEmitter<'_, '_> {
    fn native(&self, source: WindowRef, outcome: NativeOutcome) -> PublicationResult<()> {
        match outcome {
            NativeOutcome::Build(BuildOutcome::Created { fact, reply }) => {
                self.fact(&fact)?;
                self.owned_reply(reply, source)?;
            }
            NativeOutcome::Build(BuildOutcome::Cleanup(action)) => self
                .router
                .cleanup_window_action(action, "Window late-build cleanup")?,
            NativeOutcome::Build(BuildOutcome::Ignored) => report_ignored_build(),
            NativeOutcome::BuildFailed { outcome, stage } => {
                if outcome.diagnostic {
                    report_ignored_build_failure();
                }
                if let Some(call) = outcome.call.filter(|call| call.owner == self.owner()) {
                    self.refusal(call.correlation, build_refusal(stage), true, Some(source))?;
                }
                self.last_window(outcome.last_window, source)?;
            }
            NativeOutcome::Close(close) => {
                if let Some(fact) = report_native_close(close)? {
                    self.fact(&fact)?;
                }
            }
            NativeOutcome::Released(outcome) => {
                report_release(&outcome);
                if let Some(fact) = outcome.fact {
                    self.fact(&fact)?;
                }
                self.last_window(outcome.last_window, source)?;
                for call in outcome.destroy_calls {
                    self.owned_reply(
                        WindowReply {
                            owner: call.owner,
                            correlation: call.correlation,
                            response: WindowResponse::Destroyed,
                        },
                        source,
                    )?;
                }
            }
            NativeOutcome::Fact(fact) => {
                if let Some(fact) = report_native_fact(fact) {
                    self.fact(&fact)?;
                }
            }
        }
        Ok(())
    }
}
