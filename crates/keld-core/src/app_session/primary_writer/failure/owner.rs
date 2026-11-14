//! The existing direct owner consumes one exact canonical failure fact.

use super::{
    Arc, FailureDisposition, FailureKind, PrimaryReadyFailure, PrimaryWriteFailure,
    PrimaryWriterFailure, RetiredWriterFailure, Shared, Weak, WriterLink,
};
use crate::app_session::{
    ActivePrimaryGeneration, DirectPrimaryOwnerCommand, HostAppError, PrimaryRetirementMode,
    PrimaryRouterHandle, SessionTransition, app_detail, receive_direct_owner_reply,
};
use std::sync::atomic::Ordering;
use std::sync::mpsc;

fn exact(active: &ActivePrimaryGeneration, target: &Arc<WriterLink>) -> bool {
    active.attempt == target.caller().attempt
        && active.identity == target.caller().identity
        && Arc::ptr_eq(&active.reader_stop, &target.caller().reader_stop)
        && active
            .writer_link
            .as_ref()
            .is_some_and(|link| Arc::ptr_eq(link, target))
}

impl PrimaryRouterHandle {
    fn owns_failure_writer(&self, owner: &Weak<Shared>) -> bool {
        self.primary_writer
            .as_ref()
            .is_some_and(|writer| Weak::ptr_eq(&Arc::downgrade(&writer.shared), owner))
    }

    /// Worker and timed-out receipt holders use the same canonical non-writing
    /// revocation. This records no permission when A has already been replaced.
    pub(in crate::app_session) fn record_primary_write_failure(
        &self,
        failure: &PrimaryWriteFailure,
    ) -> Result<(), HostAppError> {
        if !self.owns_failure_writer(&failure.owner) {
            return Err(app_detail(
                "primary write failure",
                "foreign physical writer evidence",
            ));
        }
        let mut transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("primary write failure", "generation lock poisoned"))?;
        self.record_write_failure_locked(
            &mut transition,
            &mut current,
            &failure.target,
            &failure.owner,
        )
    }

    fn record_write_failure_locked(
        &self,
        transition: &mut SessionTransition,
        current: &mut Option<ActivePrimaryGeneration>,
        target: &Arc<WriterLink>,
        owner: &Weak<Shared>,
    ) -> Result<(), HostAppError> {
        if !self.shutdown.is_running() {
            transition.failed_writer = None;
        }
        if current.as_ref().is_none_or(|active| !exact(active, target)) {
            return Ok(());
        }
        if self.shutdown.is_running() {
            transition.failed_writer = Some(RetiredWriterFailure {
                target: Arc::downgrade(target),
                owner: owner.clone(),
                kind: FailureKind::UnknownWrite,
            });
        }
        self.retire_pending_ready_locked(transition, target.caller());
        self.retire_current_generation_with_mode_locked(
            current,
            target.caller().attempt,
            "application call retired after an unknown primary write outcome",
            "failed primary writer link close",
            PrimaryRetirementMode::FailedWrite,
        )
    }

    /// A Ready producer calls this while holding the actual transition and
    /// current owner. The fact is metadata only and must be reported after both
    /// guards are released; the owner will revalidate before denying recovery.
    pub(in crate::app_session) fn record_ready_refusal_locked(
        &self,
        transition: &mut SessionTransition,
        active: &ActivePrimaryGeneration,
        target: &Arc<WriterLink>,
    ) -> Result<PrimaryReadyFailure, HostAppError> {
        if !self.shutdown.is_running() || !exact(active, target) {
            return Err(app_detail(
                "Ready refusal",
                "original live Ready owner is unavailable",
            ));
        }
        let writer = self.primary_writer_owner()?;
        let owner = Arc::downgrade(&writer.shared);
        transition.failed_writer = Some(RetiredWriterFailure {
            target: Arc::downgrade(target),
            owner: owner.clone(),
            kind: FailureKind::ReadyRefused,
        });
        Ok(PrimaryReadyFailure {
            target: Arc::clone(target),
            owner,
        })
    }

    pub(in crate::app_session) fn report_primary_write_failure(
        &self,
        failure: PrimaryWriteFailure,
    ) -> Result<(), HostAppError> {
        let retirement = self.record_primary_write_failure(&failure);
        let recovery = self.report_writer_failure(PrimaryWriterFailure::Write(failure));
        crate::app_session::collapse_app_results([retirement, recovery])
    }

    pub(in crate::app_session) fn report_ready_refusal(
        &self,
        failure: PrimaryReadyFailure,
    ) -> Result<(), HostAppError> {
        self.report_writer_failure(PrimaryWriterFailure::Ready(failure))
    }

    fn report_writer_failure(&self, failure: PrimaryWriterFailure) -> Result<(), HostAppError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.guardian
            .command_tx
            .send(DirectPrimaryOwnerCommand::FailPrimaryWriter(failure, reply))
            .map_err(|_| app_detail("primary writer recovery", "primary owner stopped"))?;
        receive_direct_owner_reply(&response, "primary writer recovery", false)
    }

    /// Called only on the existing `DirectPrimaryOwner`. The actual restart/deny
    /// operation runs inside this final canonical cut, never after a precheck.
    pub(in crate::app_session) fn apply_primary_writer_failure(
        &self,
        failure: &PrimaryWriterFailure,
        restart: impl FnOnce(u32),
        deny: impl FnOnce(),
    ) -> Result<FailureDisposition, String> {
        let (target, owner, kind) = failure.parts();
        self.apply_writer_failure_parts(target, owner, kind, restart, deny)
    }

    /// Bound replay already runs on this owner. Its returned fact is consumed
    /// inline, without recursively submitting a command to the same thread.
    pub(in crate::app_session) fn consume_bound_writer_failure(
        &self,
        attempt: u32,
        failure: &crate::app_session::PrimaryGenerationFailure,
        restart: impl FnOnce(u32),
        deny: impl FnOnce(),
    ) -> Result<Option<FailureDisposition>, String> {
        let Some(failure) = failure.writer_failure.as_ref() else {
            return Ok(None);
        };
        let (target, owner, kind) = failure.parts();
        if target.caller().attempt != attempt {
            return Err(String::from(
                "bound replay failure does not belong to its original caller",
            ));
        }
        self.apply_writer_failure_parts(target, owner, kind, restart, deny)
            .map(Some)
    }

    fn apply_writer_failure_parts(
        &self,
        target: &Arc<WriterLink>,
        owner: &Weak<Shared>,
        kind: FailureKind,
        restart: impl FnOnce(u32),
        deny: impl FnOnce(),
    ) -> Result<FailureDisposition, String> {
        if !self.owns_failure_writer(owner) {
            return Err(String::from("foreign primary writer failure evidence"));
        }
        let mut transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| String::from("primary current lock poisoned"))?;
        if kind == FailureKind::UnknownWrite {
            self.record_write_failure_locked(&mut transition, &mut current, target, owner)
                .map_err(|error| error.to_string())?;
        }
        if !self.shutdown.is_running() || self.shutdown.reader_stop.load(Ordering::Acquire) {
            transition.failed_writer = None;
            return Ok(FailureDisposition::Ignored);
        }
        if current.is_some()
            || target.caller().attempt <= self.last_revoked_attempt.load(Ordering::Acquire)
        {
            if transition
                .failed_writer
                .as_ref()
                .is_some_and(|record| record.matches(target, owner, kind))
            {
                transition.failed_writer = None;
            }
            return Ok(FailureDisposition::Ignored);
        }
        if !transition
            .failed_writer
            .as_ref()
            .is_some_and(|record| record.matches(target, owner, kind))
        {
            return Ok(FailureDisposition::Ignored);
        }
        // Taking the one actual record makes duplicate late reports inert. A
        // successor installation clears it under this same transition guard.
        transition.failed_writer = None;
        let disposition =
            if kind == FailureKind::ReadyRefused || !self.window_ready.load(Ordering::Acquire) {
                self.pre_ready_link_failed.store(true, Ordering::Release);
                deny();
                FailureDisposition::RecoveryDenied
            } else {
                // Eligibility belongs to the actual Supervisor/RecoveryGate,
                // not a producer's delayed copy of its acknowledgement.
                restart(target.caller().attempt);
                FailureDisposition::RestartRequested
            };
        Ok(disposition)
    }

    pub(in crate::app_session) fn clear_primary_writer_failure(&self) {
        self.shutdown.transition_guard().failed_writer = None;
    }
}
