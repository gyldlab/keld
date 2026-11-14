//! Windows lifecycle publication through the one charged primary writer.
//! The existing transition owner retains only the in-progress Ready commit.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use super::primary_writer::{
    self, ControlMessage, PreparedFrame, PrimaryReadyFailure, WriteReceipt, WriterLink,
};
use super::{
    APP_LINK_IO_DEADLINE, ActivePrimaryGeneration, HostAppError, LifecycleEvent,
    PlatformPrimaryOwnerHandle, PrimaryRetirementMode, PrimaryRouterHandle, SessionTransition,
    app_detail, append_app_cleanup,
};

pub(super) struct PendingReady {
    target: Arc<WriterLink>,
    deadline: Instant,
}

struct ReadyRefusal {
    failure: PrimaryReadyFailure,
    cleanup: Result<(), HostAppError>,
}

fn exact_target(active: &ActivePrimaryGeneration, target: &Arc<WriterLink>) -> bool {
    active
        .writer_link
        .as_ref()
        .is_some_and(|link| Arc::ptr_eq(link, target))
        && target.matches(&super::PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        })
}

fn owns_target(active: &ActivePrimaryGeneration, target: &Arc<WriterLink>) -> bool {
    !active.reader_stop.load(Ordering::Acquire) && exact_target(active, target)
}

impl PrimaryRouterHandle {
    /// Observe the actual receipt before dropping it. Only its opaque failure
    /// evidence can ask the existing generation owner for recovery/Ready denial.
    pub(super) fn complete_lifecycle_write(
        &self,
        receipt: WriteReceipt,
        deadline: Instant,
        phase: &'static str,
    ) -> Result<Instant, HostAppError> {
        let observed = receipt
            .wait(deadline)
            .map_err(|error| error.into_host_error(phase))
            .and_then(|completion| completion.into_result(phase));
        if let Err(primary) = observed {
            let evidence = receipt
                .failure_evidence()
                .map_err(|error| error.into_host_error("lifecycle failure evidence"));
            // Never hold a completed charged slot while its owner may bind B.
            // An admitted pending slot stays charged through actual completion.
            drop(receipt);
            let reported = evidence.and_then(|evidence| {
                evidence.map_or(Ok(()), |failure| self.report_primary_write_failure(failure))
            });
            return Err(append_app_cleanup(primary, [reported]));
        }
        receipt
            .written_at()
            .map_err(|error| error.into_host_error(phase))?
            .ok_or_else(|| app_detail(phase, "written completion has no physical timestamp"))
    }
}

impl PrimaryRouterHandle {
    pub(super) fn signal_ready_with_writer(&self) -> Result<(), HostAppError> {
        let deadline = Instant::now() + APP_LINK_IO_DEADLINE;
        let Some(target) = self.current_primary_writer_target()? else {
            return Ok(());
        };
        let prepared = (|| {
            let frame = self.prepare_primary_control(
                target.caller(),
                Arc::clone(&target),
                ControlMessage::LifecycleEvent(LifecycleEvent::Ready),
                deadline,
                "lifecycle Ready reservation",
            )?;
            self.admit_ready(&target, frame, deadline)
        })();
        let receipt = match prepared {
            Ok(Some(receipt)) => receipt,
            Ok(None) => return Ok(()),
            Err(primary) => return self.abort_live_ready(&target, primary),
        };
        // Unknown/expired admitted writes retain actual receipt evidence;
        // their exact existing owner decides recovery or startup denial.
        self.complete_lifecycle_write(receipt, deadline, "lifecycle Ready write")?;
        let committed = (|| {
            #[cfg(test)]
            self.hold_ready_written_for_test()?;
            let reply = self.commit_written_ready(&target)?;
            if let Some(reply) = reply {
                PlatformPrimaryOwnerHandle::await_recovery_arm(&reply)?;
                self.recovery_armed.store(true, Ordering::Release);
            }
            Ok(())
        })();
        committed.or_else(|primary| self.abort_live_ready(&target, primary))
    }

    /// A known-not-admitted or known-Written Ready failure may end live startup.
    /// Once the existing terminal owner wins, its admitted frames/Close remain
    /// its custody; this source only clears its own pending Ready record.
    fn abort_live_ready(
        &self,
        target: &Arc<WriterLink>,
        primary: HostAppError,
    ) -> Result<(), HostAppError> {
        let Some(refused) = self.record_live_ready_refusal(target)? else {
            return Ok(());
        };
        let reported = self.report_ready_refusal(refused.failure);
        Err(append_app_cleanup(primary, [refused.cleanup, reported]))
    }

    /// The live initial source and the bound replay source share this one
    /// exact known-refusal record/revocation cut. A stale source returns no fact.
    fn record_live_ready_refusal(
        &self,
        target: &Arc<WriterLink>,
    ) -> Result<Option<ReadyRefusal>, HostAppError> {
        let mut transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("lifecycle Ready refusal", "current lock poisoned"))?;
        if !self.shutdown.is_running()
            || current
                .as_ref()
                .is_none_or(|active| !owns_target(active, target))
        {
            self.clear_pending_ready_locked(&mut transition, target.caller());
            return Ok(None);
        }
        let active = current.as_ref().ok_or_else(|| {
            app_detail(
                "lifecycle Ready refusal",
                "validated original owner disappeared",
            )
        })?;
        let failure = self.record_ready_refusal_locked(&mut transition, active, target)?;
        self.retire_pending_ready_locked(&mut transition, target.caller());
        let retired = self.retire_current_generation_with_mode_locked(
            &mut current,
            target.caller().attempt,
            "application call retired after live Ready refused its canonical commit",
            "failed live Ready link close",
            PrimaryRetirementMode::FailedWrite,
        );
        Ok(Some(ReadyRefusal {
            failure,
            cleanup: retired,
        }))
    }

    fn admit_ready(
        &self,
        target: &Arc<WriterLink>,
        frame: PreparedFrame,
        deadline: Instant,
    ) -> Result<Option<WriteReceipt>, HostAppError> {
        let pending = PendingReady {
            target: Arc::clone(target),
            deadline,
        };
        let mut transition = self.shutdown.transition_guard();
        if !self.shutdown.is_running() || self.pre_ready_link_failed.load(Ordering::Acquire) {
            return Ok(None);
        }
        if transition.pending_ready.is_some() {
            return Err(app_detail(
                "lifecycle Ready admission",
                "another Ready commit is pending",
            ));
        }
        {
            let current = self
                .current
                .lock()
                .map_err(|_| app_detail("lifecycle Ready admission", "current lock poisoned"))?;
            if current
                .as_ref()
                .is_none_or(|active| !owns_target(active, target))
            {
                return Ok(None);
            }
        }
        // The registry uses this same transition/current order. No current guard
        // is held while entering its existing helper.
        if !self.mark_window_registry_ready_guarded()? {
            return Ok(None);
        }
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("lifecycle Ready admission", "current lock poisoned"))?;
        let active = current.as_ref().ok_or_else(|| {
            app_detail(
                "lifecycle Ready admission",
                "validated current generation disappeared",
            )
        })?;
        let receipt = primary_writer::admit_locked(active, frame)?;
        // Infallible commit before the physical worker can cross this fence.
        transition.pending_ready = Some(pending);
        Ok(Some(receipt))
    }

    fn commit_written_ready(
        &self,
        target: &Arc<WriterLink>,
    ) -> Result<Option<Receiver<Result<(), String>>>, HostAppError> {
        let mut transition = self.shutdown.transition_guard();
        if !self.shutdown.is_running() {
            self.clear_pending_ready_locked(&mut transition, target.caller());
            return Ok(None);
        }
        let valid = transition.pending_ready.as_ref().is_some_and(|pending| {
            Arc::ptr_eq(&pending.target, target) && Instant::now() < pending.deadline
        });
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("lifecycle Ready commit", "current lock poisoned"))?;
        if current
            .as_ref()
            .is_none_or(|active| !owns_target(active, target))
        {
            self.clear_pending_ready_locked(&mut transition, target.caller());
            return Ok(None);
        }
        if !valid || self.pre_ready_link_failed.load(Ordering::Acquire) {
            return Err(app_detail(
                "lifecycle Ready commit",
                "the original Ready publication expired or its generation retired",
            ));
        }
        let reply = if self.recovery_armed.load(Ordering::Acquire) {
            None
        } else {
            // Enqueue the existing arm before publishing readiness. Its owner
            // may need this fence to acknowledge, so that wait stays outside.
            Some(self.guardian.request_recovery_arm()?)
        };
        self.window_ready.store(true, Ordering::Release);
        transition.pending_ready = None;
        self.shutdown.changed.notify_all();
        Ok(reply)
    }

    fn clear_pending_ready_locked(
        &self,
        transition: &mut SessionTransition,
        caller: &super::PrimaryCallerContext,
    ) -> bool {
        if transition
            .pending_ready
            .as_ref()
            .is_none_or(|pending| !pending.target.matches(caller))
        {
            return false;
        }
        transition.pending_ready = None;
        self.shutdown.changed.notify_all();
        true
    }

    pub(super) fn retire_pending_ready_locked(
        &self,
        transition: &mut SessionTransition,
        caller: &super::PrimaryCallerContext,
    ) {
        if self.clear_pending_ready_locked(transition, caller)
            && !self.window_ready.load(Ordering::Acquire)
        {
            self.pre_ready_link_failed.store(true, Ordering::Release);
        }
    }

    /// The captured reader may wait only for its own Ready publication. Recheck
    /// its actual role allocation after every wake before touching global startup
    /// state; an equal-number successor is a different owner.
    pub(super) fn latch_ready_writer_failure_for(&self, caller: &super::PrimaryCallerContext) {
        let mut transition = self.shutdown.transition_guard();
        loop {
            if !self.shutdown.is_running() || caller.reader_stop.load(Ordering::Acquire) {
                return;
            }
            let owns_current = self.current.lock().is_ok_and(|current| {
                current.as_ref().is_some_and(|active| {
                    active.attempt == caller.attempt
                        && active.identity == caller.identity
                        && Arc::ptr_eq(&active.reader_stop, &caller.reader_stop)
                })
            });
            if !owns_current || self.window_ready.load(Ordering::Acquire) {
                return;
            }
            let Some(pending) = transition.pending_ready.as_ref() else {
                self.pre_ready_link_failed.store(true, Ordering::Release);
                return;
            };
            if !pending.target.matches(caller) {
                return;
            }
            let remaining = pending.deadline.checked_duration_since(Instant::now());
            if remaining.is_none_or(|duration| duration.is_zero()) {
                self.clear_pending_ready_locked(&mut transition, caller);
                self.pre_ready_link_failed.store(true, Ordering::Release);
                return;
            }
            let Some(remaining) = remaining else {
                return;
            };
            #[cfg(test)]
            self.observe_ready_failure_wait_for_test();
            // Condvar releases transition while waiting. Neither current nor
            // Registry is held, and physical quiescence alone cannot end this wait.
            transition = self
                .shutdown
                .changed
                .wait_timeout(transition, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    pub(super) fn signal_last_window_with_writer(&self) -> Result<Instant, HostAppError> {
        let deadline = Instant::now() + APP_LINK_IO_DEADLINE;
        let target = self.current_primary_writer_target()?.ok_or_else(|| {
            app_detail(
                "lifecycle LastWindowClosed",
                "no live original writer is available",
            )
        })?;

        (|| {
            let frame = self.prepare_primary_control(
                target.caller(),
                Arc::clone(&target),
                ControlMessage::LifecycleEvent(LifecycleEvent::LastWindowClosed),
                deadline,
                "lifecycle LastWindowClosed reservation",
            )?;
            let receipt = {
                let _transition = self.shutdown.transition_guard();
                let current = self.current.lock().map_err(|_| {
                    app_detail("lifecycle LastWindowClosed", "current lock poisoned")
                })?;
                let active = current
                    .as_ref()
                    .filter(|active| self.shutdown.is_running() && owns_target(active, &target))
                    .ok_or_else(|| {
                        app_detail(
                            "lifecycle LastWindowClosed",
                            "original generation retired before admission",
                        )
                    })?;
                let receipt = primary_writer::admit_locked(active, frame)?;
                self.last_window_closed.store(true, Ordering::Release);
                receipt
            };
            let written = self.complete_lifecycle_write(
                receipt,
                deadline,
                "lifecycle LastWindowClosed write",
            )?;
            #[cfg(test)]
            self.hold_last_window_written_for_test(written)?;
            Ok(written)
        })()
    }

    #[cfg(test)]
    fn hold_ready_written_for_test(&self) -> Result<(), HostAppError> {
        let hold = self
            .quit_drain_hooks
            .lock()
            .map_err(|_| app_detail("Ready write fixture", "hook lock poisoned"))?
            .ready_written
            .take();
        if let Some((observed, resume)) = hold {
            observed
                .send(())
                .map_err(|error| app_detail("Ready write fixture", error.to_string()))?;
            resume
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| app_detail("Ready write fixture", error.to_string()))?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn hold_last_window_written_for_test(&self, written: Instant) -> Result<(), HostAppError> {
        let hold = self
            .quit_drain_hooks
            .lock()
            .map_err(|_| app_detail("LastWindowClosed fixture", "hook lock poisoned"))?
            .last_window_written
            .take();
        if let Some((observed, resume)) = hold {
            observed
                .send(written)
                .map_err(|error| app_detail("LastWindowClosed fixture", error.to_string()))?;
            resume
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| app_detail("LastWindowClosed fixture", error.to_string()))?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn observe_ready_failure_wait_for_test(&self) {
        let observed = self
            .quit_drain_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ready_failure_wait
            .take();
        if let Some(observed) = observed {
            let _ = observed.send(());
        }
    }
}

mod quit;
mod replay;
mod retirement;
mod tail;

#[cfg(test)]
mod tests;
