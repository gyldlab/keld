//! Normal retirement stages fixed terminals before the canonical revocation cut.
//! The retired link, charged receipts and five payload-free observations remain
//! with this synchronous source until actual writes quiesce and that exact link closes.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Instant;

use super::exact_target;
use super::tail::AcceptedTail;
use crate::app_session::compat_router::{CompatRetirementReply, PreparedCompatRetirement};
use crate::app_session::primary_writer::{
    self, ControlMessage, PreparedFrame, Reservation, WriteCompletion, WriteObservation,
    WriteReceipt, WriterLink,
};
use crate::app_session::{
    ActivePrimaryGeneration, CorrelationId, FS_CHANNEL, FsDispatchSession, FsTerminalWrite,
    HOST_ANSWER_BUDGET, HostAppError, LIFECYCLE_CHANNEL, PendingFsCall, PrimaryRetirementMode,
    PrimaryRouterHandle, RetiredPrimaryLink, app_detail, append_app_cleanup, collapse_app_results,
};

/// The enclosing existing owner chooses a live revocation or its already
/// accepted tail. This carries no independent permission; every cut rechecks it.
enum RetirementScope<'a> {
    Live(&'a Arc<WriterLink>),
    Tail(&'a AcceptedTail),
}
impl RetirementScope<'_> {
    fn target(&self) -> &Arc<WriterLink> {
        match self {
            Self::Live(target) => target,
            Self::Tail(tail) => &tail.target,
        }
    }
    fn owns_record(&self, router: &PrimaryRouterHandle, active: &ActivePrimaryGeneration) -> bool {
        match self {
            Self::Live(target) => router.shutdown.is_running() && exact_target(active, target),
            Self::Tail(tail) => tail.matches(router, active),
        }
    }
    fn matches(&self, router: &PrimaryRouterHandle, active: &ActivePrimaryGeneration) -> bool {
        self.owns_record(router, active)
            && match self {
                Self::Live(_) => !active.reader_stop.load(Ordering::Acquire),
                Self::Tail(_) => true,
            }
    }
    fn reserve(
        &self,
        router: &PrimaryRouterHandle,
        deadline: Instant,
    ) -> Result<Reservation, HostAppError> {
        match self {
            Self::Live(target) => router.reserve_primary_retirement_control(
                target.caller(),
                deadline,
                "fixed primary retirement reservation",
            ),
            Self::Tail(_) => router
                .reserve_primary_tail_control(deadline, "fixed primary retirement reservation"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PendingTerminals {
    fs: Option<PendingFsCall>,
    quit: Option<CorrelationId>,
    dialog: Option<CorrelationId>,
}

struct Snapshot {
    target: Arc<WriterLink>,
    fs: Option<Arc<FsDispatchSession>>,
    pending: PendingTerminals,
    compat: Option<CompatRetirementReply>,
}

struct PreparedRetirement {
    snapshot: Snapshot,
    fixed: [Option<PreparedFrame>; 3],
    compat: Option<PreparedCompatRetirement>,
}

struct RetirementDrain {
    link: RetiredPrimaryLink,
    receipts: [Option<WriteReceipt>; 4],
    writes: [Option<WriteObservation>; 5],
    fs: Option<(Arc<FsDispatchSession>, PendingFsCall)>,
}

fn same_compat(a: Option<&CompatRetirementReply>, b: Option<&CompatRetirementReply>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.same_record(b),
        (None, None) => true,
        _ => false,
    }
}

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn retire_generation_with_writer(
        &self,
        attempt: u32,
    ) -> Result<(), HostAppError> {
        // Numeric Revoked belongs only to the trusted generation owner here.
        // Every unlocked preparation/wait keeps this exact captured A link.
        let target = {
            let _transition = self.shutdown.transition_guard();
            if !self.shutdown.is_running() {
                return Ok(());
            }
            let current = self
                .current
                .lock()
                .map_err(|_| app_detail("primary retirement capture", "current lock poisoned"))?;
            let Some(active) = current.as_ref().filter(|active| active.attempt == attempt) else {
                return Ok(());
            };
            active.writer_link.as_ref().map(Arc::clone).ok_or_else(|| {
                app_detail(
                    "primary retirement capture",
                    "original writer link is unavailable",
                )
            })?
        };
        self.retire_exact_link_with_writer(RetirementScope::Live(&target))
    }

    pub(super) fn retire_accepted_tail_with_writer(
        &self,
        tail: &AcceptedTail,
    ) -> Result<(), HostAppError> {
        self.retire_exact_link_with_writer(RetirementScope::Tail(tail))
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "Consume the captured retirement scope once across its actual drain and terminal operations."
    )]
    fn retire_exact_link_with_writer(
        &self,
        scope: RetirementScope<'_>,
    ) -> Result<(), HostAppError> {
        let deadline = Instant::now() + HOST_ANSWER_BUDGET;
        let turn = match self.claim_window_publication(deadline, None) {
            Ok(turn) => turn,
            Err(primary) => {
                // Ordering failure cannot skip the same captured scope's
                // canonical revocation and actual already-admitted write drain.
                let cleanup = self.retire_scope_without_terminals(&scope);
                return Err(append_app_cleanup(primary, [cleanup]));
            }
        };
        let prepared = self.prepare_and_cut_retirement(&scope, deadline);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let retirement_refusal = prepared
            .as_ref()
            .err()
            .and_then(Self::observed_selected_retirement_refusal);
        let result = prepared.map_err(|primary| {
            let cleanup = self.retire_scope_without_terminals(&scope);
            append_app_cleanup(primary, [cleanup])
        });
        let routes = turn.as_ref().map_or(Ok(()), |turn| {
            self.publish_retired_compat_routes(turn, scope.target().caller(), deadline)
        });
        drop(turn);
        let result = collapse_app_results([result, routes]);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_selected_diagnostic(
            "role-retirement-return",
            None,
            false,
            Some(result.is_err()),
            retirement_refusal,
        );
        result
    }

    fn prepare_and_cut_retirement(
        &self,
        scope: &RetirementScope<'_>,
        deadline: Instant,
    ) -> Result<(), HostAppError> {
        loop {
            if Instant::now() >= deadline {
                return Err(app_detail(
                    "primary retirement",
                    "the original terminal deadline expired",
                ));
            }
            let Some(snapshot) = self.capture_retirement(scope)? else {
                return self.retire_scope_without_terminals(scope);
            };
            let prepared = self.prepare_retirement(snapshot, deadline, scope)?;
            #[cfg(test)]
            self.hold_retirement_prepared_for_test()?;
            if let Some(drain) = self.cut_retirement(prepared, scope)? {
                return finish_retirement(drain);
            }
            // Only never-admitted metadata changed; no effect is retried and
            // the original absolute budget remains unchanged.
        }
    }

    /// A stopped ordinary role cannot publish new terminals, but its trusted
    /// revocation still removes exact A. A different accepted terminal owner or
    /// successor must retain its own custody. Earlier admitted A frames keep
    /// their original deadlines and charges until actual physical completion.
    fn retire_scope_without_terminals(
        &self,
        scope: &RetirementScope<'_>,
    ) -> Result<(), HostAppError> {
        let drain = {
            let mut transition = self.shutdown.transition_guard();
            let mut current = self
                .current
                .lock()
                .map_err(|_| app_detail("primary retirement cleanup", "current lock poisoned"))?;
            if current
                .as_ref()
                .is_none_or(|active| !scope.owns_record(self, active))
            {
                return Ok(());
            }
            let writes = self
                .primary_writer_owner()?
                .snapshot_target_writes(scope.target())
                .map_err(|error| error.into_host_error("primary retirement cleanup drain"))?;
            self.retire_pending_ready_locked(&mut transition, scope.target().caller());
            let link = self
                .revoke_current_generation_locked(
                    &mut current,
                    scope.target().caller().attempt,
                    "application call retired without a fresh terminal publication",
                    PrimaryRetirementMode::FailedWrite,
                )?
                .ok_or_else(|| {
                    app_detail("primary retirement cleanup", "exact original disappeared")
                })?;
            RetirementDrain {
                link,
                receipts: [None, None, None, None],
                writes,
                fs: None,
            }
        };
        // Canonical revocation and its current/transition guards have ended.
        // This is not retirement completion: the same admitted writes still drain below.
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        self.observe_selected_diagnostic("role-revoked-before-drain", None, false, None, None);
        finish_retirement(drain)
    }

    #[cfg(test)]
    fn hold_retirement_prepared_for_test(&self) -> Result<(), HostAppError> {
        let hold = self
            .quit_drain_hooks
            .lock()
            .map_err(|_| app_detail("retirement admission fixture", "hook lock poisoned"))?
            .retirement_prepared
            .take();
        if let Some((observed, resume)) = hold {
            observed
                .send(())
                .map_err(|error| app_detail("retirement admission fixture", error.to_string()))?;
            resume
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| app_detail("retirement admission fixture", error.to_string()))?;
        }
        Ok(())
    }

    fn retirement_pending_locked(
        &self,
        active: &ActivePrimaryGeneration,
        fs: Option<&Arc<FsDispatchSession>>,
    ) -> Result<PendingTerminals, HostAppError> {
        let fs = fs
            .map(|fs| {
                fs.state
                    .lock()
                    .map_err(|_| {
                        app_detail("primary retirement snapshot", "filesystem state poisoned")
                    })
                    .map(|state| {
                        state
                            .pending_call
                            .filter(|pending| pending.attempt == active.attempt)
                    })
            })
            .transpose()?
            .flatten();
        Ok(PendingTerminals {
            fs,
            quit: active.pending_quit,
            dialog: self
                .dialog
                .as_ref()
                .and_then(|dialog| dialog.pending_original_for(active.identity, active.attempt)),
        })
    }

    fn capture_retirement(
        &self,
        scope: &RetirementScope<'_>,
    ) -> Result<Option<Snapshot>, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("primary retirement snapshot", "current lock poisoned"))?;
        let Some(active) = current
            .as_ref()
            .filter(|active| scope.matches(self, active))
        else {
            return Ok(None);
        };
        let target = active.writer_link.as_ref().ok_or_else(|| {
            app_detail(
                "primary retirement snapshot",
                "exact original writer link is unavailable",
            )
        })?;
        let fs = self.fs.as_ref().and_then(Weak::upgrade);
        let pending = self.retirement_pending_locked(active, fs.as_ref())?;
        let compat = self.compat_retirement_reply_locked(target.caller())?;
        Ok(Some(Snapshot {
            target: Arc::clone(target),
            fs,
            pending,
            compat,
        }))
    }

    fn prepare_retirement(
        &self,
        snapshot: Snapshot,
        deadline: Instant,
        scope: &RetirementScope<'_>,
    ) -> Result<PreparedRetirement, HostAppError> {
        let mut fixed = [None, None, None];
        let pending = [
            snapshot
                .pending
                .fs
                .map(|call| (FS_CHANNEL, call.correlation)),
            snapshot.pending.quit.map(|corr| (LIFECYCLE_CHANNEL, corr)),
            snapshot
                .pending
                .dialog
                .map(|corr| (keld_ipc::channel_table::DIALOG.id(), corr)),
        ];
        for (index, terminal) in pending.into_iter().enumerate() {
            if let Some((channel, correlation)) = terminal {
                let reservation = scope.reserve(self, deadline)?;
                fixed[index] = Some(
                    ControlMessage::GenerationRetired {
                        channel,
                        correlation,
                    }
                    .encode(reservation, Arc::clone(&snapshot.target))?,
                );
            }
        }
        let compat = snapshot
            .compat
            .as_ref()
            .map(|pending| {
                Self::prepare_compat_retirement_reply_reserved(
                    pending.clone(),
                    Arc::clone(&snapshot.target),
                    scope.reserve(self, deadline)?,
                )
            })
            .transpose()?;
        Ok(PreparedRetirement {
            snapshot,
            fixed,
            compat,
        })
    }

    fn cut_retirement(
        &self,
        prepared: PreparedRetirement,
        scope: &RetirementScope<'_>,
    ) -> Result<Option<RetirementDrain>, HostAppError> {
        let PreparedRetirement {
            snapshot,
            mut fixed,
            compat,
        } = prepared;
        let mut transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("primary retirement admission", "current lock poisoned"))?;
        let Some(active) = current
            .as_mut()
            .filter(|active| scope.matches(self, active) && exact_target(active, &snapshot.target))
        else {
            return Ok(None);
        };
        let now = self.retirement_pending_locked(active, snapshot.fs.as_ref())?;
        let now_compat = self.compat_retirement_reply_locked(snapshot.target.caller())?;
        if now != snapshot.pending || !same_compat(now_compat.as_ref(), snapshot.compat.as_ref()) {
            return Ok(None);
        }
        let mut receipts = [None, None, None, None];
        let mut fs_terminal = None;
        if let (Some(frame), Some(pending), Some(fs)) =
            (fixed[0].take(), snapshot.pending.fs, snapshot.fs.as_ref())
        {
            let mut native = fs.state.lock().map_err(|_| {
                app_detail("filesystem retirement admission", "native state poisoned")
            })?;
            if native.pending_call == Some(pending) && native.terminal_write.is_none() {
                let receipt = primary_writer::admit_locked(active, frame)?;
                native.pending_call = None;
                native.terminal_write = Some(FsTerminalWrite {
                    pending,
                    completion: receipt.observation(),
                });
                receipts[0] = Some(receipt);
                fs_terminal = Some((Arc::clone(fs), pending));
            }
        }
        if let Some(frame) = fixed[1].take() {
            let receipt = primary_writer::admit_locked(active, frame)?;
            active.pending_quit = None;
            receipts[1] = Some(receipt);
        }
        if let Some(frame) = fixed[2].take() {
            let receipt = primary_writer::admit_locked(active, frame)?;
            if let Some(dialog) = &self.dialog {
                dialog.retire(active.identity, active.attempt);
            }
            receipts[2] = Some(receipt);
        }
        if let Some(prepared) = compat {
            receipts[3] = self.admit_compat_retirement_reply_locked(active, prepared)?;
        }
        // Include every earlier A frame too. These bounded references cannot
        // deliver bytes, retarget a frame, or release another owner's receipt.
        let writes = self
            .primary_writer_owner()?
            .snapshot_target_writes(&snapshot.target)
            .map_err(|error| error.into_host_error("primary retirement drain snapshot"))?;
        self.retire_pending_ready_locked(&mut transition, snapshot.target.caller());
        let link = self
            .revoke_current_generation_locked(
                &mut current,
                snapshot.target.caller().attempt,
                "application call retired with the primary generation",
                PrimaryRetirementMode::Normal,
            )?
            .ok_or_else(|| {
                app_detail(
                    "primary retirement",
                    "the fenced original generation disappeared",
                )
            })?;
        Ok(Some(RetirementDrain {
            link,
            receipts,
            writes,
            fs: fs_terminal,
        }))
    }
}

fn clear_fs_terminal(
    fs: &FsDispatchSession,
    pending: PendingFsCall,
    observation: &WriteObservation,
) -> Result<(), HostAppError> {
    let mut native = fs
        .state
        .lock()
        .map_err(|_| app_detail("filesystem retirement completion", "native state poisoned"))?;
    if native.terminal_write.as_ref().is_some_and(|terminal| {
        terminal.pending == pending && terminal.completion.same_record(observation)
    }) {
        native.terminal_write = None;
        if native.pending_call.is_none() && native.in_flight == 0 {
            native.call_outstanding = false;
        }
    }
    Ok(())
}

/// A terminating source already owns its exact accepted link. It waits for
/// actual return with the packet's original I/O deadline and never asks the
/// recovery owner to restart a link that this source is closing.
/// The receipt stays charged through quiescence and actual outcome readback.
pub(super) fn finish_terminal_receipt(
    receipt: WriteReceipt,
    phase: &'static str,
) -> Result<WriteCompletion, HostAppError> {
    let drained = receipt
        .observation()
        .wait_quiesced()
        .map_err(|error| error.into_host_error(phase));
    let outcome = receipt
        .poll()
        .map_err(|error| error.into_host_error(phase))
        .and_then(|outcome| {
            outcome.ok_or_else(|| {
                app_detail(phase, "quiesced primary receipt has no actual completion")
            })
        });
    drop(receipt);
    match outcome {
        Ok(outcome) => drained.map(|()| outcome),
        Err(primary) => Err(append_app_cleanup(primary, [drained])),
    }
}

fn finish_retirement(drain: RetirementDrain) -> Result<(), HostAppError> {
    let RetirementDrain {
        link,
        receipts,
        writes,
        fs,
    } = drain;
    let mut outcomes: [Result<(), HostAppError>; 4] = std::array::from_fn(|_| Ok(()));
    for (index, receipt) in receipts.into_iter().enumerate() {
        if let Some(receipt) = receipt {
            let observation = receipt.observation();
            let completion = finish_terminal_receipt(receipt, "primary retirement terminal");
            let quiesced = completion.is_ok();
            outcomes[index] = completion.and_then(|written| {
                if index < 3 {
                    // Existing fixed 023 answers are best effort (GH527 section 4.9).
                    // Unknown is not Written; exact revocation/drain/close still
                    // succeeds if the peer can no longer accept its final answer.
                    Ok(())
                } else {
                    // Compat keeps its existing strict terminal disposition.
                    written.into_result("primary retirement terminal")
                }
            });
            if index == 0
                && quiesced
                && let Some((fs, pending)) = &fs
            {
                let previous = std::mem::replace(&mut outcomes[index], Ok(()));
                outcomes[index] =
                    collapse_app_results([previous, clear_fs_terminal(fs, *pending, &observation)]);
            }
        }
    }
    let written = collapse_app_results(outcomes.into_iter().chain(
        writes.into_iter().flatten().map(|write| {
            write
                .wait_quiesced_result()
                .map_err(|error| error.into_host_error("original primary write drain"))
                .and_then(|outcome| {
                    outcome.map_or(Ok(()), |outcome| {
                        outcome.into_result("original primary write drain")
                    })
                })
        }),
    ));
    // Closing this retained handle never performs a current/successor lookup.
    let closed = link.close("retired primary generation link close");
    collapse_app_results([written, closed])
}
