use super::*;
use crate::records::{ActivationFailureClass, ActivationJournal, ActivationPhase};
use crate::tests::expected_identity;

fn journal(phase: ActivationPhase) -> ActivationJournal {
    let baseline = expected_identity().baseline;
    let mut previous = baseline.clone();
    previous.version = "0.9.0".to_owned();
    previous.content_blake3 = [0x33; 32];
    let mut candidate = baseline.clone();
    candidate.version = "1.1.0".to_owned();
    candidate.content_blake3 = [0x44; 32];
    ActivationJournal {
        attempt_id: [0x11; 32],
        candidate,
        rollback_target: baseline.clone(),
        prior_floor: baseline.version.clone(),
        prior_last_known_good: baseline,
        prior_previous_known_good: Some(previous),
        helper_image_blake3: [0x55; 32],
        health_channel_id: [0x66; 32],
        lifecycle_channel_id: [0x88; 32],
        phase,
    }
}

fn snapshot(journal: &ActivationJournal) -> RecoverySnapshot {
    RecoverySnapshot {
        current: journal.rollback_target.clone(),
        version_floor: journal.prior_floor.clone(),
        last_known_good: journal.prior_last_known_good.clone(),
        previous_known_good: journal.prior_previous_known_good.clone(),
        process_family: ProcessFamilyObservation::Exited,
    }
}

fn candidate_selected(journal: &ActivationJournal) -> RecoverySnapshot {
    let mut state = snapshot(journal);
    state.current = journal.candidate.clone();
    state.version_floor = journal.candidate.version.clone();
    state
}

#[test]
fn publish_pending_resumes_only_exact_prior_or_candidate_floor_states() {
    let journal = journal(ActivationPhase::PublishPending);
    let prior = snapshot(&journal);
    assert_eq!(
        recovery_decision(&journal, &prior),
        RecoveryDecision::ResumePublishPending {
            advance_floor: true
        }
    );

    let mut floor_published = prior.clone();
    floor_published.version_floor = journal.candidate.version.clone();
    assert_eq!(
        recovery_decision(&journal, &floor_published),
        RecoveryDecision::ResumePublishPending {
            advance_floor: false
        }
    );

    let mut current_published = floor_published;
    current_published.current = journal.candidate.clone();
    assert_eq!(
        recovery_decision(&journal, &current_published),
        RecoveryDecision::ResumeCandidateHealth
    );
}

#[test]
fn health_accepted_finishes_commit_at_each_durable_pointer_cut() {
    let journal = journal(ActivationPhase::HealthAccepted {
        health_receipt_digest: [0x77; 32],
    });
    let before = candidate_selected(&journal);
    let mut previous_published = before.clone();
    previous_published.previous_known_good = Some(journal.prior_last_known_good.clone());
    let mut lkg_published = previous_published.clone();
    lkg_published.last_known_good = journal.candidate.clone();
    for state in [before, previous_published, lkg_published] {
        assert_eq!(
            recovery_decision(&journal, &state),
            RecoveryDecision::FinishCommit
        );
    }
}

#[test]
fn awaiting_health_rolls_back_after_family_exit_and_never_accepts_old_health() {
    let journal = journal(ActivationPhase::AwaitingHealth);
    let state = candidate_selected(&journal);
    assert_eq!(
        recovery_decision(&journal, &state),
        RecoveryDecision::Rollback {
            target: journal.rollback_target.clone()
        }
    );
}

#[test]
fn rollback_pending_finishes_only_its_recorded_context() {
    let journal = journal(ActivationPhase::RollbackPending {
        failure: ActivationFailureClass::HealthRejected,
    });
    let state = candidate_selected(&journal);
    assert_eq!(
        recovery_decision(&journal, &state),
        RecoveryDecision::FinishRollback {
            target: journal.rollback_target.clone()
        }
    );
}

#[test]
fn every_phase_refuses_without_authenticated_lifecycle_retirement() {
    let phases = [
        ActivationPhase::PublishPending,
        ActivationPhase::AwaitingHealth,
        ActivationPhase::HealthAccepted {
            health_receipt_digest: [0x77; 32],
        },
        ActivationPhase::RollbackPending {
            failure: ActivationFailureClass::ProcessCrash,
        },
    ];
    for phase in phases {
        let journal = journal(phase);
        let mut state = snapshot(&journal);
        state.process_family = ProcessFamilyObservation::NotProvenDead;
        assert_eq!(
            recovery_decision(&journal, &state),
            RecoveryDecision::Refuse(RecoveryRefusal::ProcessFamilyNotProven),
            "recovery must preserve state when retirement evidence is lost or ambiguous"
        );
    }
}

#[test]
fn mixed_or_advanced_state_refuses_without_lowering_floor() {
    let journal = journal(ActivationPhase::PublishPending);
    let mut state = snapshot(&journal);
    state.version_floor = "1.2.0".to_owned();
    assert_eq!(
        recovery_decision(&journal, &state),
        RecoveryDecision::Refuse(RecoveryRefusal::FloorMismatch)
    );

    let mut mixed = snapshot(&journal);
    mixed.current = journal.candidate.clone();
    mixed.version_floor = journal.prior_floor.clone();
    assert_eq!(
        recovery_decision(&journal, &mixed),
        RecoveryDecision::Refuse(RecoveryRefusal::CurrentFloorMismatch)
    );

    let mut changed_lkg = snapshot(&journal);
    changed_lkg.last_known_good.content_blake3 = [0x99; 32];
    assert_eq!(
        recovery_decision(&journal, &changed_lkg),
        RecoveryDecision::Refuse(RecoveryRefusal::KnownGoodMismatch)
    );
}

#[test]
fn recovery_rejects_a_prior_floor_below_journaled_known_good_artifacts() {
    let mut journal = journal(ActivationPhase::PublishPending);
    journal.prior_floor = "0.5.0".to_owned();
    let mut observed = snapshot(&journal);
    observed.version_floor = journal.candidate.version.clone();
    assert_eq!(
        recovery_decision(&journal, &observed),
        RecoveryDecision::Refuse(RecoveryRefusal::InvalidJournalContext),
        "a transaction cannot claim a historical floor below its selected rollback state"
    );
}
