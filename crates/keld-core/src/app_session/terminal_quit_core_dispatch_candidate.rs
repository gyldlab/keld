//! A test-only adapter of the draft transaction to the real Core reader.
//! One existing Quit frame is a fixture carrier; outcomes travel out of band.
//! No conditional wire tag is allocated. The cut-only witness has no wire
//! reply; the separate legacy-tail witness uses the existing real reply and
//! shutdown sequence. The opt-in is owned by the exact test router/link.

use super::super::super::super::{
    ActivePrimaryGeneration, ConditionalQuitFixtureDispatch, ConditionalQuitFixtureDisposition,
    HostAppError, PrimaryCallerContext, PrimaryLinkIdentity, PrimaryRouterHandle,
    SESSION_LIFECYCLE_QUIT, app_detail, with_primary_caller_locked,
};
use super::super::super::WindowOwner;
use super::{HostFacts, Outcome, TerminalQuit};
use keld_ipc::CorrelationId;
use std::num::NonZeroU32;
use std::sync::{Arc, Weak, atomic::Ordering, mpsc};

/// The real fixture's transition -> current -> `WindowSession` order. The FS
/// state is read while the same transition excludes admission and writes.
fn owner_cut<R>(
    handle: &PrimaryRouterHandle,
    caller: &PrimaryCallerContext,
    apply: impl FnOnce(HostFacts, u64, &mut ActivePrimaryGeneration) -> Result<R, HostAppError>,
) -> Result<Option<R>, HostAppError> {
    let mut result = None;
    with_primary_caller_locked(&handle.current, &handle.shutdown, caller, |current| {
        let active = current
            .as_mut()
            .ok_or_else(|| app_detail("conditional Quit fixture", "validated owner disappeared"))?;
        let windows = handle
            .windows
            .as_ref()
            .ok_or_else(|| app_detail("conditional Quit fixture", "managed registry absent"))?;
        let windows = windows
            .lock()
            .map_err(|_| app_detail("conditional Quit fixture", "registry lock poisoned"))?;
        let (qualified, empty, incarnation) = windows.fixture_terminal_observation(
            WindowOwner::new(active.identity, active.attempt),
            active.attempt,
        );
        let drained = if let Some(owner) = &handle.fs {
            let fs = owner.upgrade().ok_or_else(|| {
                app_detail(
                    "conditional Quit fixture",
                    "configured FS owner disappeared",
                )
            })?;
            let state = fs
                .state
                .lock()
                .map_err(|_| app_detail("conditional Quit fixture", "FS lock poisoned"))?;
            state.in_flight == 0
                && !state.call_outstanding
                && state.pending_call.is_none()
                && state.failed_write_attempt.is_none()
        } else {
            true
        };
        result = Some(apply(
            HostFacts {
                generation: NonZeroU32::new(active.attempt),
                qualified: qualified
                    && matches!(active.identity, PrimaryLinkIdentity::Role(_))
                    && handle.window_ready.load(Ordering::Acquire),
                running: handle.shutdown.is_running(),
                registry_empty: empty,
                destruction_complete: empty,
                fs_terminal_drained: drained,
            },
            incarnation,
            active,
        )?);
        Ok(())
    })?;
    Ok(result)
}

impl PrimaryRouterHandle {
    /// Prepare on the actual selected, Ready, subscribed registry, then arm
    /// one explicitly controlled fixture carrier on that same accepted link.
    pub(in crate::app_session) fn arm_conditional_quit_fixture(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
    ) -> Result<mpsc::Receiver<(CorrelationId, String)>, HostAppError> {
        self.arm_conditional_quit_fixture_mode(caller, correlation, false)
    }

    /// Separate clean-cut witness for the existing reserved legacy tail.
    /// It does not implement conditional reply semantics.
    pub(in crate::app_session) fn arm_conditional_quit_tail_fixture(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
    ) -> Result<mpsc::Receiver<(CorrelationId, String)>, HostAppError> {
        self.arm_conditional_quit_fixture_mode(caller, correlation, true)
    }

    fn arm_conditional_quit_fixture_mode(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        finish_legacy_tail: bool,
    ) -> Result<mpsc::Receiver<(CorrelationId, String)>, HostAppError> {
        if correlation.0 == 0 {
            return Err(app_detail(
                "conditional Quit fixture",
                "zero carrier correlation",
            ));
        }
        let mut transaction = TerminalQuit::new();
        let Some((Outcome::Prepared(token), initial_incarnation)) =
            owner_cut(self, caller, |facts, incarnation, _| {
                Ok((transaction.prepare(facts), incarnation))
            })?
        else {
            return Err(app_detail(
                "conditional Quit fixture",
                "real Prepare was not admitted",
            ));
        };
        let (report, received) = mpsc::sync_channel(1);
        let mut hooks = self
            .quit_drain_hooks
            .lock()
            .map_err(|_| app_detail("conditional Quit fixture", "hook lock poisoned"))?;
        if hooks.conditional_dispatch.is_some() || hooks.conditional_dispatch_used {
            return Err(app_detail(
                "conditional Quit fixture",
                "carrier already armed",
            ));
        }
        hooks.conditional_dispatch = Some(ConditionalQuitFixtureDispatch {
            caller: caller.clone(),
            correlation,
            run: Box::new(move |handle, caller, correlation| {
                let initial = owner_cut(handle, caller, |facts, incarnation, active| {
                    // From an empty Prepare cut a later live/adoptable window
                    // requires a new Create identity. Reuse the registry's
                    // existing non-recycled incarnation for this fixture.
                    // Shipping integration must invalidate at the successful
                    // common Create/adoption cut itself, not add this sampler.
                    if incarnation != initial_incarnation {
                        transaction.admit_create();
                    }
                    let initial = transaction.begin_commit(token, facts);
                    if finish_legacy_tail && initial == Outcome::DrainRequired {
                        if active.pending_quit.is_some() {
                            return Err(app_detail(
                                "conditional Quit tail fixture",
                                "legacy reply already pending",
                            ));
                        }
                        active.pending_quit = Some(correlation);
                    }
                    Ok(initial)
                })?
                .unwrap_or(Outcome::Unknown);
                let outcome = if initial == Outcome::DrainRequired {
                    // We are on read_primary_frames itself. No new Call can
                    // enter on this reader during the real running-state drain.
                    // No transition/current/windows/hook lock spans this wait.
                    if let Some(fs) = handle.fs.as_ref().and_then(Weak::upgrade) {
                        fs.drain()?;
                    } else if handle.fs.is_some() {
                        return Err(app_detail(
                            "conditional Quit fixture",
                            "FS owner lost during drain",
                        ));
                    }
                    owner_cut(handle, caller, |facts, incarnation, _| {
                        if incarnation != initial_incarnation {
                            transaction.admit_create();
                        }
                        Ok(transaction.finish_commit(token, facts, || {
                            handle.shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT)
                        }))
                    })?
                    .unwrap_or(Outcome::Unknown)
                } else {
                    initial
                };
                report
                    .send((correlation, format!("{outcome:?}")))
                    .map_err(|error| {
                        app_detail("conditional Quit fixture receipt", error.to_string())
                    })?;
                if finish_legacy_tail {
                    if outcome != Outcome::Committed {
                        return Err(app_detail(
                            "conditional Quit tail fixture",
                            "clean legacy-tail witness did not commit",
                        ));
                    }
                    // The same reader enters the already-reserved production
                    // tail; it must not try to reserve a second time.
                    return Ok(ConditionalQuitFixtureDisposition::FinishReservedLegacyTail);
                }
                // The original cut-only fixture never sends a wire reply.
                Ok(if outcome == Outcome::Retained {
                    ConditionalQuitFixtureDisposition::Resume
                } else {
                    ConditionalQuitFixtureDisposition::Stop
                })
            }),
        });
        Ok(received)
    }

    pub(in crate::app_session) fn dispatch_conditional_quit_fixture(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
    ) -> Result<Option<ConditionalQuitFixtureDisposition>, HostAppError> {
        let dispatch = {
            let mut hooks = self
                .quit_drain_hooks
                .lock()
                .map_err(|_| app_detail("conditional Quit fixture", "hook lock poisoned"))?;
            if hooks.conditional_dispatch.as_ref().is_some_and(|armed| {
                armed.caller.identity == caller.identity
                    && armed.caller.attempt == caller.attempt
                    && Arc::ptr_eq(&armed.caller.reader_stop, &caller.reader_stop)
                    && armed.correlation == correlation
            }) {
                hooks.conditional_dispatch_used = true;
                hooks.conditional_dispatch.take()
            } else {
                None
            }
        };
        dispatch
            .map(|armed| (armed.run)(self, caller, correlation))
            .transpose()
    }
}
