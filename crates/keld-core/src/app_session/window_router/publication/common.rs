//! Shared result handling for the existing synchronous Window/Compat sources.
//! No owner RPC occurs while a source still holds its publication turn.
use crate::app_session::primary_writer::{PrimaryWriteFailure, WriteReceipt, WriterLink};
use crate::app_session::window_router::WindowSession;
use crate::app_session::{
    HostAppError, PrimaryCallerContext, PrimaryRouterHandle, app_detail, append_app_cleanup,
    with_primary_caller_locked,
};
use std::sync::Arc;
use std::time::Instant;

/// The outer native coordinator preserves the typed cause without a second Fatal.
#[must_use = "the native coordinator must preserve an already queued fatal cause"]
pub(in crate::app_session) enum NativePublicationDisposition {
    Handled,
    FatalQueued(HostAppError),
}

/// One source retains at most its own actual failure through turn release.
pub(in crate::app_session) struct PublicationError {
    cause: HostAppError,
    write: Option<PrimaryWriteFailure>,
}
pub(in crate::app_session) type PublicationResult<T> = Result<T, PublicationError>;
impl From<HostAppError> for PublicationError {
    fn from(cause: HostAppError) -> Self {
        Self { cause, write: None }
    }
}
impl PublicationError {
    /// Required physical cleanup adds no writer evidence and cannot replace it.
    pub(in crate::app_session) fn with_cleanup(self, cleanup: Result<(), HostAppError>) -> Self {
        Self {
            cause: append_app_cleanup(self.cause, [cleanup]),
            write: self.write,
        }
    }
}

impl PrimaryRouterHandle {
    /// Observe the original receipt under a source turn, outside every mutex.
    /// The returned sealed fact is reported only after that turn has been dropped.
    pub(in crate::app_session) fn observe_publication(
        &self,
        target: &Arc<WriterLink>,
        receipt: WriteReceipt,
        deadline: Instant,
        phase: &'static str,
    ) -> PublicationResult<()> {
        let (result, write) = self
            .observe_primary_write(target, receipt, deadline, phase)
            .into_parts();
        match (result, write) {
            (Ok(_), None) => Ok(()),
            (Err(cause), write) => Err(PublicationError { cause, write }),
            (Ok(_), Some(write)) => Err(PublicationError {
                cause: app_detail(phase, "writer returned contradictory completion evidence"),
                write: Some(write),
            }),
        }
    }

    /// The exact original reader receives its cause after the source turn ends.
    pub(in crate::app_session) fn finish_call_publication(
        &self,
        result: PublicationResult<()>,
    ) -> Result<(), HostAppError> {
        match result {
            Ok(()) => Ok(()),
            Err(PublicationError { cause, write }) => {
                let reported =
                    write.map_or(Ok(()), |failure| self.report_primary_write_failure(failure));
                Err(append_app_cleanup(cause, [reported]))
            }
        }
    }

    /// Invoke after turn Drop. A never-admitted source error uses only the actual
    /// original caller and retained Registry source at the existing final cut.
    /// Superseded work is ignored; a current invariant failure uses the existing
    /// Fatal command. Actual writer failures use their separate sealed owner fact.
    pub(in crate::app_session) fn finish_native_publication(
        &self,
        caller: &PrimaryCallerContext,
        source_current: impl FnOnce(&WindowSession) -> bool,
        result: PublicationResult<()>,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let Err(PublicationError { cause, write }) = result else {
            return Ok(NativePublicationDisposition::Handled);
        };
        if let Some(write) = write {
            return match self.report_primary_write_failure(write) {
                Ok(()) => Ok(NativePublicationDisposition::Handled),
                // This exact owner distinguishes stale/duplicate success from
                // terminal denial. Preserve its typed failure for the coordinator.
                Err(cleanup) => Err(append_app_cleanup(cause, [Err(cleanup)])),
            };
        }
        let mut predicate = Some(source_current);
        let mut queued = false;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let Some(windows) = &self.windows else {
                return Ok(());
            };
            let windows = windows
                .lock()
                .map_err(|_| app_detail("native publication failure", "registry lock poisoned"))?;
            let Some(predicate) = predicate.take() else {
                return Err(app_detail(
                    "native publication failure",
                    "source predicate entered twice",
                ));
            };
            if !predicate(&windows) {
                return Ok(());
            }
            self.window_commands
                .send(keld_wv::AppWindowCommand::Fatal)
                .map_err(|_| {
                    app_detail("native publication failure", "window command owner ended")
                })?;
            queued = true;
            Ok(())
        })?;
        Ok(if queued {
            NativePublicationDisposition::FatalQueued(cause)
        } else {
            NativePublicationDisposition::Handled
        })
    }
}
