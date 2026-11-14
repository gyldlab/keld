//! A native fact retains its original source through the bounded turn wait.
//! Failure still applies the canonical transition exactly once and performs
//! required physical cleanup, without publishing through somebody else's turn.
use super::super::super::window_registry::NativePublicationSource;
use super::super::native_outcome::NativeOutcome;
use super::super::publication_turn::WindowPublicationTurn;
use super::{
    AppWindowEvent, Arc, BuildOutcome, CloseOutcome, HostAppError, NativePublicationDisposition,
    Ordering, PrimaryCallerContext, PrimaryRouterHandle, PublicationError, PublicationResult,
    WindowOwner, WriterLink, app_detail, owner,
};

pub(super) struct CapturedNativePublication {
    pub(super) source: Option<NativePublicationSource>,
    pub(super) target: Option<Arc<WriterLink>>,
}

impl PrimaryRouterHandle {
    pub(super) fn capture_window_native_source(
        &self,
        event: &AppWindowEvent,
    ) -> Result<CapturedNativePublication, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("Window native source", "current lock poisoned"))?;
        let windows = self
            .windows
            .as_ref()
            .ok_or_else(|| app_detail("Window native source", "Registry disappeared"))?;
        let session = windows
            .lock()
            .map_err(|_| app_detail("Window native source", "Registry lock poisoned"))?;
        let source = session.registry.native_publication_source(event);
        let target = if let Some(active) = current.as_ref().filter(|active| {
            self.shutdown.is_running()
                && active.identity.is_primary()
                && !active.reader_stop.load(Ordering::Acquire)
                && session.subscribed == Some(owner(active))
        }) {
            let caller = PrimaryCallerContext {
                identity: active.identity,
                attempt: active.attempt,
                reader_stop: Arc::clone(&active.reader_stop),
            };
            if source.as_ref().is_some_and(|source| {
                session
                    .registry
                    .retains_caller_window(&caller, source.window)
            }) {
                Some(Arc::clone(
                    active
                        .writer_link
                        .as_ref()
                        .filter(|link| link.matches(&caller))
                        .ok_or_else(|| {
                            app_detail(
                                "Window native source",
                                "exact original writer link is unavailable",
                            )
                        })?,
                ))
            } else {
                None
            }
        } else {
            None
        };
        Ok(CapturedNativePublication { source, target })
    }

    pub(super) fn prepare_window_native_source(
        &self,
        event: AppWindowEvent,
        turn: Option<&WindowPublicationTurn<'_>>,
    ) -> Result<NativeOutcome, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let _current = self
            .current
            .lock()
            .map_err(|_| app_detail("Window native source", "current lock poisoned"))?;
        let windows = self
            .windows
            .as_ref()
            .ok_or_else(|| app_detail("Window native source", "Registry disappeared"))?;
        let mut session = windows
            .lock()
            .map_err(|_| app_detail("Window native source", "Registry lock poisoned"))?;
        if turn.is_some_and(|turn| !turn.owns(windows, &session)) {
            return Err(app_detail("Window native source", "foreign source turn"));
        }
        // None is the failed ordering-claim path, never a publication grant.
        Self::prepare_window_native_locked(&mut session, event)
    }

    pub(super) fn finish_captured_window_native(
        &self,
        captured: &CapturedNativePublication,
        result: PublicationResult<()>,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let (Some(source), Some(target)) = (captured.source.as_ref(), captured.target.as_ref())
        else {
            return Ok(NativePublicationDisposition::Handled);
        };
        self.finish_native_publication(
            target.caller(),
            |session| {
                session.subscribed
                    == Some(WindowOwner::new(
                        target.caller().identity,
                        target.caller().attempt,
                    ))
                    && session
                        .registry
                        .retains_native_publication(target.caller(), source)
            },
            result,
        )
    }

    pub(super) fn window_native_turn_refused(
        &self,
        event: AppWindowEvent,
        captured: &CapturedNativePublication,
        error: HostAppError,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let outcome = self.prepare_window_native_source(event, None);
        if captured.target.is_some() {
            let cleanup =
                outcome.and_then(|outcome| self.cleanup_unpublished_window_native(outcome));
            return self.finish_captured_window_native(
                captured,
                Err(PublicationError::from(error).with_cleanup(cleanup)),
            );
        }
        // A stopped/superseded role still owes late-build cleanup. Its expired
        // publication alone cannot enqueue Fatal for the replacement role.
        match outcome {
            Ok(outcome) => self.cleanup_unpublished_window_native(outcome)?,
            Err(cause) if captured.source.is_none() => {
                return Err(crate::app_session::append_app_cleanup(error, [Err(cause)]));
            }
            Err(_) => {}
        }
        Ok(NativePublicationDisposition::Handled)
    }

    pub(super) fn cleanup_unpublished_window_native(
        &self,
        outcome: NativeOutcome,
    ) -> Result<(), HostAppError> {
        match outcome {
            NativeOutcome::Build(BuildOutcome::Cleanup(action))
            | NativeOutcome::Close(CloseOutcome::Teardown(action)) => {
                self.cleanup_window_action(action, "Window unpublished native cleanup")
            }
            NativeOutcome::OriginalDestroy {
                resolution: _,
                admission,
            } => {
                // The same Registry transition already resolved the actual
                // native ticket. No publication result is a Close receipt.
                drop(admission);
                Ok(())
            }
            NativeOutcome::Released(outcome) => {
                // The actual release was applied to the same Registry. This is
                // only its bounded fixed correlations/facts, not a native owner.
                drop(outcome);
                Ok(())
            }
            NativeOutcome::Committed(committed) => {
                let _ = committed;
                Ok(())
            }
            NativeOutcome::Finished(completed) => {
                drop(completed);
                Ok(())
            }
            NativeOutcome::None
            | NativeOutcome::Fact(_)
            | NativeOutcome::Build(BuildOutcome::Created { .. } | BuildOutcome::Ignored)
            | NativeOutcome::BuildFailed { .. }
            | NativeOutcome::Close(
                CloseOutcome::Requested { .. }
                | CloseOutcome::Unmapped
                | CloseOutcome::Exhausted
                | CloseOutcome::AlreadyClosing
                | CloseOutcome::Merged { .. }
                | CloseOutcome::Vetoed
                | CloseOutcome::Stale,
            )
            | NativeOutcome::OriginalFailure { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
