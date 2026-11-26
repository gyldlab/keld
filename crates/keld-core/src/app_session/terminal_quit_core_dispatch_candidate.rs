//! A test-only adapter of the draft transaction to the real Core reader.
//! One existing Quit frame is a fixture carrier; outcomes travel out of band.
//! No conditional wire tag, accepted legacy Quit reply or terminal tail is
//! synthesized. The opt-in is owned by the exact test router/link.

use super::super::super::super::{
    ConditionalQuitFixtureDispatch, HostAppError, PrimaryCallerContext, PrimaryLinkIdentity,
    PrimaryRouterHandle, SESSION_LIFECYCLE_QUIT, app_detail, with_primary_caller_locked,
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
    apply: impl FnOnce(HostFacts, u64) -> R,
) -> Result<Option<R>, HostAppError> {
    let mut result = None;
    with_primary_caller_locked(&handle.current, &handle.shutdown, caller, |current| {
        let active = current
            .as_ref()
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
        ));
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
        if correlation.0 == 0 {
            return Err(app_detail(
                "conditional Quit fixture",
                "zero carrier correlation",
            ));
        }
        let mut transaction = TerminalQuit::new();
        let Some((Outcome::Prepared(token), initial_incarnation)) =
            owner_cut(self, caller, |facts, incarnation| {
                (transaction.prepare(facts), incarnation)
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
                let initial = owner_cut(handle, caller, |facts, incarnation| {
                    // From an empty Prepare cut a later live/adoptable window
                    // requires a new Create identity. Reuse the registry's
                    // existing non-recycled incarnation for this fixture.
                    // Shipping integration must invalidate at the successful
                    // common Create/adoption cut itself, not add this sampler.
                    if incarnation != initial_incarnation {
                        transaction.admit_create();
                    }
                    transaction.begin_commit(token, facts)
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
                    owner_cut(handle, caller, |facts, incarnation| {
                        if incarnation != initial_incarnation {
                            transaction.admit_create();
                        }
                        transaction.finish_commit(token, facts, || {
                            handle.shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT)
                        })
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
                // A committed cut stops this reader; no synthetic legacy
                // acknowledgement or production cleanup is invoked. Retained
                // resumes the same reader; all carrier settlement stays test-only.
                Ok(outcome != Outcome::Retained)
            }),
        });
        Ok(received)
    }

    pub(in crate::app_session) fn dispatch_conditional_quit_fixture(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
    ) -> Result<Option<bool>, HostAppError> {
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
