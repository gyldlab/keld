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
    let mut journal = ActivationJournal {
        attempt_id: [0x11; 32],
        candidate,
        rollback_target: baseline.clone(),
        prior_floor: baseline.version.clone(),
        prior_last_known_good: baseline,
        prior_previous_known_good: Some(previous),
        helper_image_blake3: [0x55; 32],
        health_channel_id: [0x66; 32],
        lifecycle_channel_id: [0x88; 32],
        phase: ActivationPhase::PublishPending,
    };
    set_phase(&mut journal, phase);
    journal
}

/// Sets `phase`; an accepted-health phase records the digest of this journal's own
/// attempt, health channel and candidate, as the transaction does.
fn set_phase(journal: &mut ActivationJournal, phase: ActivationPhase) {
    journal.phase = match phase {
        ActivationPhase::HealthAccepted { .. } => ActivationPhase::HealthAccepted {
            health_receipt_digest: crate::records::activation_health_receipt_digest(
                &journal.attempt_id,
                &journal.health_channel_id,
                &journal.candidate,
            )
            .expect("canonical receipt digest"),
        },
        other => other,
    };
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

/// Owned protected slots for the pure trace model.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Slots {
    floor: String,
    current: ArtifactIdentity,
    last_known_good: ArtifactIdentity,
    previous_known_good: Option<ArtifactIdentity>,
    candidate: CandidateLocation,
    retiree_present: bool,
}

impl Slots {
    fn view(&self) -> ProtectedSlots<'_> {
        ProtectedSlots {
            version_floor: &self.floor,
            current: &self.current,
            last_known_good: &self.last_known_good,
            previous_known_good: self.previous_known_good.as_ref(),
        }
    }
}

fn prior_slots(journal: &ActivationJournal) -> Slots {
    Slots {
        floor: journal.prior_floor.clone(),
        current: journal.rollback_target.clone(),
        last_known_good: journal.prior_last_known_good.clone(),
        previous_known_good: journal.prior_previous_known_good.clone(),
        candidate: CandidateLocation::Staged,
        retiree_present: true,
    }
}

/// Applies one step with the effect the approved spec assigns to it (§4 steps 3-8).
/// This is the specification's write list, not the Windows executor under test.
fn apply_spec_effect(journal: &mut ActivationJournal, slots: &mut Slots, step: ActivationStep) {
    match step {
        ActivationStep::PublishCandidate => slots.candidate = CandidateLocation::Published,
        ActivationStep::AdvanceFloor => slots.floor = journal.candidate.version.clone(),
        ActivationStep::SelectCandidate => slots.current = journal.candidate.clone(),
        ActivationStep::EnterAwaitingHealth => journal.phase = ActivationPhase::AwaitingHealth,
        ActivationStep::PreservePriorKnownGood => {
            slots.previous_known_good = Some(journal.prior_last_known_good.clone());
        }
        ActivationStep::CommitCandidate => slots.last_known_good = journal.candidate.clone(),
        ActivationStep::RestoreRollbackTarget => slots.current = journal.rollback_target.clone(),
        ActivationStep::RetireVersion => slots.retiree_present = false,
        ActivationStep::AwaitHealth
        | ActivationStep::RemoveJournal
        | ActivationStep::AbandonAttempt => {
            panic!("{step:?} is a terminal decision, not a slot write")
        }
    }
}

/// Runs the step function from `slots` until a terminal decision, recording every step.
fn trace(journal: &mut ActivationJournal, slots: &mut Slots) -> Vec<ActivationStep> {
    let mut steps = Vec::new();
    loop {
        let step = next_activation_step(
            journal,
            &slots.view(),
            slots.candidate,
            slots.retiree_present,
        )
        .unwrap_or_else(|refusal| panic!("on-trace state refused: {refusal:?} at {slots:?}"));
        steps.push(step);
        if matches!(
            step,
            ActivationStep::AwaitHealth
                | ActivationStep::RemoveJournal
                | ActivationStep::AbandonAttempt
        ) {
            return steps;
        }
        apply_spec_effect(journal, slots, step);
        assert!(
            steps.len() <= 8,
            "step function did not terminate: {steps:?}"
        );
    }
}

#[test]
fn every_persisted_cut_resumes_the_exact_next_spec_step_for_commit_and_rollback() {
    use ActivationStep::{
        AdvanceFloor, AwaitHealth, CommitCandidate, EnterAwaitingHealth, PreservePriorKnownGood,
        PublishCandidate, RemoveJournal, RestoreRollbackTarget, RetireVersion, SelectCandidate,
    };
    let start = journal(ActivationPhase::PublishPending);
    let mut forward = start.clone();
    let mut slots = prior_slots(&forward);
    let publish = trace(&mut forward, &mut slots);
    assert_eq!(
        publish,
        [
            PublishCandidate,
            AdvanceFloor,
            SelectCandidate,
            EnterAwaitingHealth,
            AwaitHealth
        ]
    );

    let mut committed = forward.clone();
    set_phase(
        &mut committed,
        ActivationPhase::HealthAccepted {
            health_receipt_digest: [0; 32],
        },
    );
    let mut commit_slots = slots.clone();
    let commit = trace(&mut committed, &mut commit_slots);
    assert_eq!(
        commit,
        [
            PreservePriorKnownGood,
            CommitCandidate,
            RetireVersion,
            RemoveJournal
        ]
    );
    assert_eq!(commit_slots.current, start.candidate);
    assert_eq!(commit_slots.last_known_good, start.candidate);
    assert_eq!(
        commit_slots.previous_known_good.as_ref(),
        Some(&start.prior_last_known_good)
    );
    assert_eq!(
        commit_slots.floor, start.candidate.version,
        "commit keeps the advanced floor"
    );

    let mut rolled_back = forward;
    rolled_back.phase = ActivationPhase::RollbackPending {
        failure: ActivationFailureClass::HealthTimeout,
    };
    let mut rollback_slots = slots;
    let rollback = trace(&mut rolled_back, &mut rollback_slots);
    assert_eq!(
        rollback,
        [RestoreRollbackTarget, RetireVersion, RemoveJournal]
    );
    assert_eq!(rollback_slots.current, start.rollback_target);
    assert_eq!(rollback_slots.last_known_good, start.prior_last_known_good);
    assert_eq!(
        rollback_slots.floor, start.candidate.version,
        "rollback never lowers the trust floor"
    );

    // Re-entering at each intermediate persisted cut must resume the identical suffix:
    // forward progress and recovery are one function, so a resumed owner cannot diverge.
    for (phase, expected) in [
        (ActivationPhase::PublishPending, &publish),
        (
            ActivationPhase::HealthAccepted {
                health_receipt_digest: [0x77; 32],
            },
            &commit,
        ),
        (
            ActivationPhase::RollbackPending {
                failure: ActivationFailureClass::HealthTimeout,
            },
            &rollback,
        ),
    ] {
        let mut journal = start.clone();
        set_phase(&mut journal, phase.clone());
        let mut slots = prior_slots(&journal);
        if !matches!(phase, ActivationPhase::PublishPending) {
            slots.floor = journal.candidate.version.clone();
            slots.current = journal.candidate.clone();
            slots.candidate = CandidateLocation::Published;
        }
        for cut in 0..expected.len() {
            let mut resumed_journal = journal.clone();
            let mut resumed_slots = slots.clone();
            for step in &expected[..cut] {
                apply_spec_effect(&mut resumed_journal, &mut resumed_slots, *step);
            }
            assert_eq!(
                trace(&mut resumed_journal, &mut resumed_slots),
                expected[cut..],
                "{phase:?} cut {cut} must resume the uninterrupted suffix"
            );
        }
    }
}

#[test]
fn retirement_targets_only_the_unreferenced_version_after_its_pointer_steps() {
    let mut accepted = journal(ActivationPhase::HealthAccepted {
        health_receipt_digest: [0x77; 32],
    });
    let mut slots = prior_slots(&accepted);
    slots.floor = accepted.candidate.version.clone();
    slots.current = accepted.candidate.clone();
    assert_eq!(
        retirement_due(&accepted, &slots.view()),
        None,
        "the older version stays referenced until both known-good publications land"
    );
    slots.previous_known_good = Some(accepted.prior_last_known_good.clone());
    assert_eq!(retirement_due(&accepted, &slots.view()), None);
    slots.last_known_good = accepted.candidate.clone();
    assert_eq!(
        retirement_due(&accepted, &slots.view()),
        accepted.prior_previous_known_good.as_ref(),
        "commit retires exactly the superseded previous-known-good"
    );

    accepted.prior_previous_known_good = None;
    assert_eq!(
        retirement_due(&accepted, &slots.view()),
        None,
        "the first update has no superseded version to retire"
    );
    assert_eq!(
        next_activation_step(&accepted, &slots.view(), slots.candidate, true),
        Ok(ActivationStep::RemoveJournal)
    );

    let rollback = journal(ActivationPhase::RollbackPending {
        failure: ActivationFailureClass::CandidateLaunch,
    });
    let mut slots = prior_slots(&rollback);
    slots.floor = rollback.candidate.version.clone();
    slots.current = rollback.candidate.clone();
    assert_eq!(
        retirement_due(&rollback, &slots.view()),
        None,
        "a selected candidate is never retired"
    );
    slots.current = rollback.rollback_target.clone();
    assert_eq!(
        retirement_due(&rollback, &slots.view()),
        Some(&rollback.candidate)
    );
    assert_eq!(
        next_activation_step(&rollback, &slots.view(), slots.candidate, false),
        Ok(ActivationStep::RemoveJournal),
        "an already-retired candidate resolves by removing the journal"
    );
    for phase in [
        ActivationPhase::PublishPending,
        ActivationPhase::AwaitingHealth,
    ] {
        let journal = journal(phase);
        let mut slots = prior_slots(&journal);
        slots.floor = journal.candidate.version.clone();
        slots.current = journal.candidate.clone();
        assert_eq!(retirement_due(&journal, &slots.view()), None);
    }
}

#[test]
fn off_trace_slots_refuse_in_every_phase_without_a_write_step() {
    let publish = journal(ActivationPhase::PublishPending);
    let mut above = prior_slots(&publish);
    above.floor = "9.0.0".to_owned();
    assert_eq!(
        next_activation_step(&publish, &above.view(), above.candidate, true),
        Err(RecoveryRefusal::FloorMismatch)
    );

    let awaiting = journal(ActivationPhase::AwaitingHealth);
    let unselected = prior_slots(&awaiting);
    assert_eq!(
        next_activation_step(&awaiting, &unselected.view(), unselected.candidate, true),
        Err(RecoveryRefusal::FloorMismatch),
        "awaiting-health with the prior floor is not a persisted cut"
    );

    let accepted = journal(ActivationPhase::HealthAccepted {
        health_receipt_digest: [0x77; 32],
    });
    let mut wrong_lkg = prior_slots(&accepted);
    wrong_lkg.floor = accepted.candidate.version.clone();
    wrong_lkg.current = accepted.candidate.clone();
    wrong_lkg.last_known_good = accepted.candidate.clone();
    assert_eq!(
        next_activation_step(&accepted, &wrong_lkg.view(), wrong_lkg.candidate, true),
        Err(RecoveryRefusal::KnownGoodMismatch),
        "last-known-good cannot be committed before previous-known-good is preserved"
    );

    let rollback = journal(ActivationPhase::RollbackPending {
        failure: ActivationFailureClass::ProcessCrash,
    });
    let mut unrelated = prior_slots(&rollback);
    unrelated.floor = rollback.candidate.version.clone();
    unrelated.current.content_blake3 = [0xee; 32];
    assert_eq!(
        next_activation_step(&rollback, &unrelated.view(), unrelated.candidate, true),
        Err(RecoveryRefusal::CurrentFloorMismatch),
        "rollback never adopts an unrelated current artifact"
    );
}

#[test]
fn accepted_health_with_a_digest_for_other_attempt_fields_refuses() {
    let accepted = journal(ActivationPhase::HealthAccepted {
        health_receipt_digest: [0; 32],
    });
    let mut slots = prior_slots(&accepted);
    slots.floor = accepted.candidate.version.clone();
    slots.current = accepted.candidate.clone();
    assert_eq!(
        next_activation_step(&accepted, &slots.view(), slots.candidate, true),
        Ok(ActivationStep::PreservePriorKnownGood),
        "the digest of the journal's own attempt, channel and candidate is accepted"
    );
    for mutate in [
        |journal: &mut ActivationJournal| journal.attempt_id[0] ^= 1,
        |journal: &mut ActivationJournal| journal.health_channel_id[0] ^= 1,
        |journal: &mut ActivationJournal| journal.candidate.content_blake3[0] ^= 1,
    ] {
        let mut mixed = accepted.clone();
        mutate(&mut mixed);
        let mut mixed_slots = slots.clone();
        mixed_slots.current = mixed.candidate.clone();
        assert_eq!(
            next_activation_step(&mixed, &mixed_slots.view(), mixed_slots.candidate, true),
            Err(RecoveryRefusal::InvalidJournalContext),
            "a recorded digest that no longer matches its journal fields halts"
        );
    }
}

#[test]
fn a_publish_pending_attempt_publishes_a_staged_candidate_or_abandons_a_missing_one() {
    let publish = journal(ActivationPhase::PublishPending);
    let mut slots = prior_slots(&publish);
    let step = |slots: &Slots| next_activation_step(&publish, &slots.view(), slots.candidate, true);
    assert_eq!(step(&slots), Ok(ActivationStep::PublishCandidate));
    slots.candidate = CandidateLocation::Absent;
    assert_eq!(
        step(&slots),
        Ok(ActivationStep::AbandonAttempt),
        "a journal whose stage is gone changed nothing, so it is abandoned"
    );
    slots.candidate = CandidateLocation::Published;
    assert_eq!(step(&slots), Ok(ActivationStep::AdvanceFloor));

    // Once the floor names the candidate, publication must already have happened.
    slots.floor = publish.candidate.version.clone();
    assert_eq!(step(&slots), Ok(ActivationStep::SelectCandidate));
    for missing in [CandidateLocation::Staged, CandidateLocation::Absent] {
        slots.candidate = missing;
        assert_eq!(
            step(&slots),
            Err(RecoveryRefusal::CandidateUnpublished),
            "{missing:?} with an advanced floor is not a persisted cut"
        );
    }
}
