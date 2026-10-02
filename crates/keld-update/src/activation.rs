//! Common activation recovery decisions, independent of the mode-specific write lease.

use crate::ArtifactIdentity;
use crate::records::{ActivationJournal, ActivationPhase};

/// Independent result of the native process-family owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessFamilyObservation {
    /// The exact journaled coordinator and candidate family is proven exited.
    Exited,
    /// A family member may remain live, or authenticated retirement evidence
    /// was lost/ambiguous (including across a reboot or hibernate).
    NotProvenDead,
}

/// Protected state observed under the single-writer lease before recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecoverySnapshot {
    /// Currently selected runnable artifact.
    pub(crate) current: ArtifactIdentity,
    /// Monotonic trust floor; rollback never lowers it.
    pub(crate) version_floor: String,
    /// Current last-known-good artifact.
    pub(crate) last_known_good: ArtifactIdentity,
    /// Older health-confirmed artifact, if present.
    pub(crate) previous_known_good: Option<ArtifactIdentity>,
    /// Native process-family observation, independent of journal contents.
    pub(crate) process_family: ProcessFamilyObservation,
}

/// Common state-machine action; a mode adapter supplies only the write lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecoveryDecision {
    /// Continue the exact publish-pending attempt, advancing the floor only if needed.
    ResumePublishPending { advance_floor: bool },
    /// Current and floor already name the candidate; move to awaiting-health and launch it.
    ResumeCandidateHealth,
    /// Candidate failed and the validated prior known-good must be restored.
    Rollback { target: ArtifactIdentity },
    /// Exact health was accepted; finish the ordered known-good pointer commit, retire
    /// the superseded version (if any) and remove the journal.
    FinishCommit,
    /// Exact rollback was durably chosen; finish the pointer, retire the candidate and
    /// remove the journal.
    FinishRollback { target: ArtifactIdentity },
    /// The recorded phase and observed protected state are inconsistent or unsafe.
    Refuse(RecoveryRefusal),
}

/// Typed, mode-independent reason recovery must stop without altering protected state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryRefusal {
    /// The OS could not prove that every prior process-family member exited.
    ProcessFamilyNotProven,
    /// The floor is not the exact phase-authorized value.
    FloorMismatch,
    /// Current does not match the phase-authorized pointer/floor pair.
    CurrentFloorMismatch,
    /// Last-known-good slots differ from the journaled transaction context.
    KnownGoodMismatch,
    /// The candidate is not a higher strict-SemVer release than the prior floor.
    InvalidJournalContext,
}

/// Classifies a crash-cut snapshot without guessing or repairing unrelated state.
///
/// This function proves only the common journal decision. The caller must already hold
/// the installation-wide writer lease and supply an OS-backed process-family observation.
#[must_use]
pub(crate) fn recovery_decision(
    journal: &ActivationJournal,
    observed: &RecoverySnapshot,
) -> RecoveryDecision {
    if observed.process_family != ProcessFamilyObservation::Exited {
        return RecoveryDecision::Refuse(RecoveryRefusal::ProcessFamilyNotProven);
    }
    classify_protected_recovery_state(
        journal,
        &observed.current,
        &observed.version_floor,
        &observed.last_known_good,
        observed.previous_known_good.as_ref(),
    )
}

/// Protected pointer/floor slots observed (or just written) under the writer lease.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProtectedSlots<'a> {
    pub(crate) version_floor: &'a str,
    pub(crate) current: &'a ArtifactIdentity,
    pub(crate) last_known_good: &'a ArtifactIdentity,
    pub(crate) previous_known_good: Option<&'a ArtifactIdentity>,
}

/// The single next durable action of the common transaction.
///
/// Forward progress and crash recovery both call [`next_activation_step`], so a
/// resumed attempt can only take the step an uninterrupted owner would have taken.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivationStep {
    /// Replace the trust floor with the candidate version.
    AdvanceFloor,
    /// Replace `current` with the candidate.
    SelectCandidate,
    /// Replace the journal phase with `AwaitingHealth`.
    EnterAwaitingHealth,
    /// Decision point: the live owner launches the candidate and resolves health;
    /// recovery without a live owner rolls back.
    AwaitHealth,
    /// Publish the prior last-known-good as `previous-known-good`.
    PreservePriorKnownGood,
    /// Replace `last-known-good` with the health-accepted candidate.
    CommitCandidate,
    /// Replace `current` with the journaled rollback target.
    RestoreRollbackTarget,
    /// Retire the journal-authorized unreferenced version before journal removal.
    RetireVersion,
    /// Remove the resolved journal.
    RemoveJournal,
}

/// Version that the journal authorizes to retire once every pointer step is durable.
///
/// Commit retires the prior previous-known-good, which leaves both retained slots;
/// rollback retires the failed candidate after `current` is restored. No other
/// artifact is ever eligible, and the result is `None` until the pointer state proves
/// the retiree is no longer referenced by any slot.
#[cfg(any(windows, test))]
pub(crate) fn retirement_due<'journal>(
    journal: &'journal ActivationJournal,
    slots: &ProtectedSlots<'_>,
) -> Option<&'journal ArtifactIdentity> {
    match journal.phase {
        ActivationPhase::HealthAccepted { .. }
            if slots.last_known_good == &journal.candidate
                && slots.previous_known_good == Some(&journal.prior_last_known_good) =>
        {
            journal.prior_previous_known_good.as_ref()
        }
        ActivationPhase::RollbackPending { .. } if slots.current == &journal.rollback_target => {
            Some(&journal.candidate)
        }
        _ => None,
    }
}

/// Selects the exact next step for a validated journal and its protected slots.
///
/// `retiree_present` reports whether the [`retirement_due`] version directory still
/// exists. This function is pure: the caller must already hold the installation-wide
/// writer lease and, for recovery, authenticated process-family retirement evidence.
///
/// # Errors
/// Returns the typed refusal for malformed, mixed or phase-inconsistent state.
#[cfg(any(windows, test))]
pub(crate) fn next_activation_step(
    journal: &ActivationJournal,
    slots: &ProtectedSlots<'_>,
    retiree_present: bool,
) -> Result<ActivationStep, RecoveryRefusal> {
    let retire_or_remove = || {
        if retirement_due(journal, slots).is_some() && retiree_present {
            ActivationStep::RetireVersion
        } else {
            ActivationStep::RemoveJournal
        }
    };
    match classify_protected_recovery_state(
        journal,
        slots.current,
        slots.version_floor,
        slots.last_known_good,
        slots.previous_known_good,
    ) {
        RecoveryDecision::Refuse(reason) => Err(reason),
        RecoveryDecision::ResumePublishPending {
            advance_floor: true,
        } => Ok(ActivationStep::AdvanceFloor),
        RecoveryDecision::ResumePublishPending {
            advance_floor: false,
        } => Ok(ActivationStep::SelectCandidate),
        RecoveryDecision::ResumeCandidateHealth => Ok(ActivationStep::EnterAwaitingHealth),
        RecoveryDecision::Rollback { .. } => Ok(ActivationStep::AwaitHealth),
        RecoveryDecision::FinishCommit => {
            if slots.last_known_good == &journal.candidate {
                Ok(retire_or_remove())
            } else if slots.previous_known_good == Some(&journal.prior_last_known_good) {
                Ok(ActivationStep::CommitCandidate)
            } else {
                Ok(ActivationStep::PreservePriorKnownGood)
            }
        }
        RecoveryDecision::FinishRollback { .. } => {
            if slots.current == &journal.candidate {
                Ok(ActivationStep::RestoreRollbackTarget)
            } else {
                Ok(retire_or_remove())
            }
        }
    }
}

/// Validates the journal-authorized pointer/floor relationships without treating
/// that classification as family-retirement proof or a recovery command.
///
/// The protected snapshot must be read while the installation writer lease is held.
/// Callers may use this only to refuse malformed or phase-inconsistent state; the
/// returned classification from [`recovery_decision`] remains gated on independent
/// process-family evidence.
#[cfg(windows)]
pub(crate) fn validate_protected_recovery_state(
    journal: &ActivationJournal,
    current: &ArtifactIdentity,
    version_floor: &str,
    last_known_good: &ArtifactIdentity,
    previous_known_good: Option<&ArtifactIdentity>,
) -> Result<(), RecoveryRefusal> {
    match classify_protected_recovery_state(
        journal,
        current,
        version_floor,
        last_known_good,
        previous_known_good,
    ) {
        RecoveryDecision::Refuse(reason) => Err(reason),
        _ => Ok(()),
    }
}

fn classify_protected_recovery_state(
    journal: &ActivationJournal,
    current: &ArtifactIdentity,
    version_floor: &str,
    last_known_good: &ArtifactIdentity,
    previous_known_good: Option<&ArtifactIdentity>,
) -> RecoveryDecision {
    if crate::records::validate_activation_journal(journal).is_err() {
        return RecoveryDecision::Refuse(RecoveryRefusal::InvalidJournalContext);
    }

    let prior_slots_match = last_known_good == &journal.prior_last_known_good
        && previous_known_good == journal.prior_previous_known_good.as_ref();
    let candidate_floor = journal.candidate.version.as_str();
    let rollback_target = &journal.rollback_target;

    match &journal.phase {
        ActivationPhase::PublishPending => {
            if !prior_slots_match {
                return RecoveryDecision::Refuse(RecoveryRefusal::KnownGoodMismatch);
            }
            let floor_is_prior = version_floor == journal.prior_floor;
            let floor_is_candidate = version_floor == candidate_floor;
            if current == rollback_target && (floor_is_prior || floor_is_candidate) {
                RecoveryDecision::ResumePublishPending {
                    advance_floor: floor_is_prior,
                }
            } else if current == &journal.candidate && floor_is_candidate {
                RecoveryDecision::ResumeCandidateHealth
            } else if !floor_is_prior && !floor_is_candidate {
                RecoveryDecision::Refuse(RecoveryRefusal::FloorMismatch)
            } else {
                RecoveryDecision::Refuse(RecoveryRefusal::CurrentFloorMismatch)
            }
        }
        ActivationPhase::AwaitingHealth => {
            if !prior_slots_match {
                return RecoveryDecision::Refuse(RecoveryRefusal::KnownGoodMismatch);
            }
            if version_floor != candidate_floor {
                return RecoveryDecision::Refuse(RecoveryRefusal::FloorMismatch);
            }
            if current != &journal.candidate {
                return RecoveryDecision::Refuse(RecoveryRefusal::CurrentFloorMismatch);
            }
            RecoveryDecision::Rollback {
                target: rollback_target.clone(),
            }
        }
        ActivationPhase::HealthAccepted {
            health_receipt_digest,
        } => {
            let recorded = crate::records::activation_health_receipt_digest(
                &journal.attempt_id,
                &journal.health_channel_id,
                &journal.candidate,
            );
            if recorded.as_ref() != Ok(health_receipt_digest) {
                return RecoveryDecision::Refuse(RecoveryRefusal::InvalidJournalContext);
            }
            if version_floor != candidate_floor {
                return RecoveryDecision::Refuse(RecoveryRefusal::FloorMismatch);
            }
            if current != &journal.candidate {
                return RecoveryDecision::Refuse(RecoveryRefusal::CurrentFloorMismatch);
            }
            let commit_slots_match = last_known_good == &journal.prior_last_known_good
                && (previous_known_good == journal.prior_previous_known_good.as_ref()
                    || previous_known_good == Some(&journal.prior_last_known_good))
                || (last_known_good == &journal.candidate
                    && previous_known_good == Some(&journal.prior_last_known_good));
            if commit_slots_match {
                RecoveryDecision::FinishCommit
            } else {
                RecoveryDecision::Refuse(RecoveryRefusal::KnownGoodMismatch)
            }
        }
        ActivationPhase::RollbackPending { .. } => {
            if version_floor != candidate_floor {
                return RecoveryDecision::Refuse(RecoveryRefusal::FloorMismatch);
            }
            if !prior_slots_match {
                return RecoveryDecision::Refuse(RecoveryRefusal::KnownGoodMismatch);
            }
            if current == &journal.candidate || current == rollback_target {
                RecoveryDecision::FinishRollback {
                    target: rollback_target.clone(),
                }
            } else {
                RecoveryDecision::Refuse(RecoveryRefusal::CurrentFloorMismatch)
            }
        }
    }
}

#[cfg(test)]
mod tests;
