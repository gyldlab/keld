//! The admitted Quit frame retains its original correlation/link through real completion.

use std::sync::{Arc, Weak};
use std::time::Instant;

use super::retirement::finish_terminal_receipt;
use super::tail::AcceptedTail;
use super::{exact_target, owns_target};
use crate::app_session::primary_writer::{
    self, ControlMessage, PreparedFrame, WriteReceipt, WriterLink,
};
use crate::app_session::{
    APP_LINK_IO_DEADLINE, BootstrapStream, CorrelationId, HostAppError, LifecycleResponse,
    PrimaryCallerContext, PrimaryRouterHandle, QuitAdmission, SESSION_CLI_LEASE_LOST,
    SESSION_LIFECYCLE_QUIT, app_detail, append_app_cleanup, collapse_app_results,
    window_registry::WindowOwner,
};

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn lifecycle_quit_with_writer(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        reader: &mut BootstrapStream,
    ) -> Result<(), HostAppError> {
        let quit_frame = Instant::now();
        let Some((target, admission)) = self.begin_quit_with_writer(caller, correlation)? else {
            return self.finish_lost_quit_claim();
        };
        // This same reader cannot admit later work while its original Quit waits.
        if let Some(fs) = self.fs.as_ref().and_then(Weak::upgrade) {
            fs.drain()?;
        }
        let deadline = Instant::now() + APP_LINK_IO_DEADLINE;
        let frame = self.prepare_primary_control(
            caller,
            Arc::clone(&target),
            ControlMessage::LifecycleReply {
                correlation,
                response: LifecycleResponse::Quit,
            },
            deadline,
            "lifecycle Quit reservation",
        )?;
        if !self.claim_quit_with_writer(&target, correlation, admission, quit_frame, deadline)? {
            return self.finish_lost_quit_claim();
        }
        let _run = self.shutdown.tail_run();
        let pending = self.fail_pending_echo_for_attempt(
            caller.attempt,
            "application call retired by accepted lifecycle Quit",
        );
        if let Err(primary) = self.guardian.prepare_accepted_shutdown() {
            let ending = {
                let _transition = self.shutdown.transition_guard();
                self.end_window_session_guarded()
            };
            let fs = self.quiesce_and_drain_fs();
            let retired = self.retire_generation_after_write_failure(caller);
            let finished = self.finish_tail("lifecycle Quit preparation failure");
            return Err(append_app_cleanup(
                primary,
                [pending, ending, fs, retired, finished],
            ));
        }
        let tail = AcceptedTail {
            target,
            cause: SESSION_LIFECYCLE_QUIT,
        };
        let reply = self
            .accept_quit_reply_with_writer(&tail, correlation, frame)
            .and_then(|receipt| {
                // accept_quit_reply_with_writer has already committed the exact
                // terminal cause and admitted this reply. Delivery stays strict,
                // but a physical failure cannot re-enter recovery after acceptance.
                finish_terminal_receipt(receipt, "lifecycle Quit reply")?
                    .into_result("lifecycle Quit reply")
            });
        // The physical reply attempt has returned before any Ending/UI command.
        let ending = {
            let _transition = self.shutdown.transition_guard();
            self.end_window_session_guarded()
        };
        let fs = self.quiesce_and_drain_fs();
        if reply.is_ok() && fs.is_ok() {
            self.answer_calls_after_quit_with_writer(&tail, reader);
        }
        let retired = self.retire_accepted_tail_with_writer(&tail);
        let finished = self.finish_tail("lifecycle Quit");
        collapse_app_results([reply, pending, ending, fs, retired, finished])
    }

    fn finish_lost_quit_claim(&self) -> Result<(), HostAppError> {
        if self.shutdown.cause() == SESSION_CLI_LEASE_LOST {
            self.cli_lease_lost()
        } else {
            Ok(())
        }
    }

    fn begin_quit_with_writer(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
    ) -> Result<Option<(Arc<WriterLink>, QuitAdmission<'_>)>, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        if !self.shutdown.is_running() {
            return Ok(None);
        }
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("lifecycle Quit admission", "current lock poisoned"))?;
        let Some(active) = current.as_mut() else {
            return Ok(None);
        };
        let Some(target) = active
            .writer_link
            .as_ref()
            .filter(|target| target.matches(caller))
            .map(Arc::clone)
        else {
            return Ok(None);
        };
        if !owns_target(active, &target) {
            return Ok(None);
        }
        active.pending_quit = Some(correlation);
        let admission = self.shutdown.admit_quit_guarded(caller.attempt);
        Ok(Some((target, admission)))
    }

    fn claim_quit_with_writer(
        &self,
        target: &Arc<WriterLink>,
        correlation: CorrelationId,
        admission: QuitAdmission<'_>,
        quit_frame: Instant,
        deadline: Instant,
    ) -> Result<bool, HostAppError> {
        let mut transition = self.shutdown.transition_guard();
        while transition
            .pending_ready
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(&pending.target, target))
        {
            if !self.shutdown.is_running()
                || target
                    .caller()
                    .reader_stop
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                break;
            }
            let original_deadline = transition
                .pending_ready
                .as_ref()
                .map_or(deadline, |pending| pending.deadline.min(deadline));
            let Some(remaining) = original_deadline
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
            else {
                self.retire_pending_ready_locked(&mut transition, target.caller());
                return Err(app_detail(
                    "lifecycle Quit Ready ordering",
                    "the original pending Ready deadline expired before Quit could claim",
                ));
            };
            #[cfg(test)]
            {
                let observed = self
                    .quit_drain_hooks
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .quit_ready_wait
                    .take();
                if let Some(observed) = observed {
                    let _ = observed.try_send(());
                }
            }
            // The original Ready owner must commit or retire before this Quit
            // claims. No current or Registry guard survives this actual wait.
            transition = self
                .shutdown
                .changed
                .wait_timeout(transition, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        admission.release_guarded();
        if self
            .pre_ready_link_failed
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(app_detail(
                "lifecycle Quit Ready ordering",
                "initial Ready failure preceded the Quit claim",
            ));
        }
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("lifecycle Quit claim", "current lock poisoned"))?;
        let Some(active) = current.as_ref().filter(|active| {
            self.shutdown.is_running()
                && owns_target(active, target)
                && active.pending_quit == Some(correlation)
        }) else {
            return Ok(false);
        };
        self.cancel_window_native_admissions(WindowOwner::new(active.identity, active.attempt))?;
        if !self.shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT) {
            return Ok(false);
        }
        let _ = self.shutdown.handover_clock.quit_frame.set(quit_frame);
        Ok(true)
    }

    fn accept_quit_reply_with_writer(
        &self,
        tail: &AcceptedTail,
        correlation: CorrelationId,
        frame: PreparedFrame,
    ) -> Result<WriteReceipt, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        if !self
            .shutdown
            .accept_reserved_guarded(SESSION_LIFECYCLE_QUIT)
            || !self.shutdown.begin_tail()
        {
            return Err(app_detail(
                "lifecycle Quit reply",
                "the original Quit claimant is no longer accepted",
            ));
        }
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("lifecycle Quit reply", "current lock poisoned"))?;
        let active = current
            .as_mut()
            .filter(|active| {
                exact_target(active, &tail.target) && active.pending_quit == Some(correlation)
            })
            .ok_or_else(|| {
                app_detail(
                    "lifecycle Quit reply",
                    "the original Quit link/correlation disappeared",
                )
            })?;
        let receipt = primary_writer::admit_locked(active, frame)?;
        active.pending_quit = None;
        Ok(receipt)
    }
}
