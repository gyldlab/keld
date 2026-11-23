//! D-64/D-68 restart cuts as an independent oracle for the native journal reader.
//!
//! These contract checks exercise the exact published record decoder and the
//! restart classification that native KEL-53 reads after each durable cut.
//! Real process termination, retained NT handles and physical power loss still
//! require the qualified T-82 repository owner and separate Windows oracles.

use super::{anchor, assert_refuses, pointer, record};
use crate::records::initial_admission::{
    AdmissionInspection, InitialAdmissionPhase, PendingOperationKind,
    decode_initial_admission_record, decode_pending_repository_operation,
    encode_initial_admission_record, encode_pending_repository_operation,
    inspect_initial_admission,
};

#[test]
fn every_pointed_phase_refuses_a_foreign_d68_identity_before_classification() {
    for phase in [
        InitialAdmissionPhase::Admitted,
        InitialAdmissionPhase::CreationArmed,
        InitialAdmissionPhase::BootstrapOwned,
        InitialAdmissionPhase::ProvenancePublished,
        InitialAdmissionPhase::Complete,
        InitialAdmissionPhase::Aborted,
    ] {
        let mut journal = record();
        journal.phase = phase;
        let matching = pointer(&journal);

        let mut wrong_attempt = matching.clone();
        wrong_attempt.attempt_id[0] ^= 1;
        let mut wrong_repository = matching.clone();
        wrong_repository.repository_id[0] ^= 1;
        let mut wrong_owner = matching.clone();
        wrong_owner.owner_sid = "S-1-5-21-100-200-300-1002".to_owned();
        let mut wrong_app = matching.clone();
        wrong_app.app_id = "com.gyldlab.other".to_owned();
        let mut wrong_kind = matching;
        wrong_kind.kind = PendingOperationKind::ManualRecovery;

        for foreign in [
            wrong_attempt,
            wrong_repository,
            wrong_owner,
            wrong_app,
            wrong_kind,
        ] {
            assert_refuses(inspect_initial_admission(
                &anchor(),
                Some(&journal),
                Some(&foreign),
                &[],
            ));
        }
    }
}

#[test]
fn every_d64_phase_cut_requires_the_exact_d68_pointer_or_a_terminal_diagnosis() {
    let expected = anchor();
    let cases = [
        (
            InitialAdmissionPhase::Admitted,
            AdmissionInspection::AdmittedUnarmed,
            Some(AdmissionInspection::OrphanAdmittedNeedsIndependentNoEffectProof),
        ),
        (
            InitialAdmissionPhase::CreationArmed,
            AdmissionInspection::CreationArmedOperatorDispositionRequired,
            None,
        ),
        (
            InitialAdmissionPhase::BootstrapOwned,
            AdmissionInspection::BoundRecoveryRequired,
            None,
        ),
        (
            InitialAdmissionPhase::ProvenancePublished,
            AdmissionInspection::BoundRecoveryRequired,
            None,
        ),
        (
            InitialAdmissionPhase::Complete,
            AdmissionInspection::TerminalCoherenceProofRequired,
            Some(AdmissionInspection::RetainedTerminalDiagnostic),
        ),
        (
            InitialAdmissionPhase::Aborted,
            AdmissionInspection::TerminalCoherenceProofRequired,
            Some(AdmissionInspection::RetainedTerminalDiagnostic),
        ),
    ];

    for (phase, pointed, unpointed) in cases {
        let mut journal = record();
        journal.phase = phase;
        let journal_bytes = encode_initial_admission_record(&journal).expect("canonical D64");
        let journal =
            decode_initial_admission_record(&journal_bytes).expect("protected D64 readback");
        let reference = pointer(&journal);
        let pointer_bytes = encode_pending_repository_operation(&reference).expect("canonical D68");
        let reference =
            decode_pending_repository_operation(&pointer_bytes).expect("protected D68 readback");

        assert_eq!(
            inspect_initial_admission(&expected, Some(&journal), Some(&reference), &[])
                .expect("exact pointed recovery must be classified"),
            pointed,
            "after durable phase {phase:?} and before pointer retirement",
        );

        match unpointed {
            Some(unpointed) => assert_eq!(
                inspect_initial_admission(&expected, Some(&journal), None, &[])
                    .expect("admitted orphan or retained terminal diagnostic"),
                unpointed,
                "after cut with absent D68 pointer at {phase:?}",
            ),
            None => assert_refuses(inspect_initial_admission(
                &expected,
                Some(&journal),
                None,
                &[],
            )),
        }
    }
}

#[test]
fn terminated_admitted_attempt_cannot_be_mistaken_for_fresh_enrollment() {
    let expected = anchor();
    let frozen = record();

    // Cut after durable D64 admitted, before D68 pending publication.
    assert_eq!(
        inspect_initial_admission(&expected, Some(&frozen), None, &[])
            .expect("unpointed admission"),
        AdmissionInspection::OrphanAdmittedNeedsIndependentNoEffectProof,
    );
    // Even with identical requested bytes, a later worker may not synthesize
    // a creation-armed record or silently discard a partially armed attempt.
    let mut falsely_armed = frozen.clone();
    falsely_armed.phase = InitialAdmissionPhase::CreationArmed;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&falsely_armed),
        None,
        &[],
    ));

    // Cut after D68 pointer publication, before durable creation-armed phase.
    assert_eq!(
        inspect_initial_admission(&expected, Some(&frozen), Some(&pointer(&frozen)), &[])
            .expect("same-attempt D68 readback"),
        AdmissionInspection::AdmittedUnarmed,
    );
    // On restart the same pointer remains authoritative until an independent
    // no-effect witness allows terminal aborted and exact pointer removal.
    assert_refuses(inspect_initial_admission(
        &expected,
        None,
        Some(&pointer(&frozen)),
        &[],
    ));
}

#[test]
fn retired_terminal_and_new_orphan_have_bounded_separate_attempts() {
    let expected = anchor();
    let orphan = record();
    let mut old_terminal = record();
    old_terminal.attempt_id = [0x9b; 32];
    old_terminal.phase = InitialAdmissionPhase::Complete;

    // One retained terminal plus one new unpointed admission is diagnostic;
    // neither a second terminal nor a foreign pointer can be substituted.
    assert_eq!(
        inspect_initial_admission(
            &expected,
            Some(&orphan),
            None,
            std::slice::from_ref(&old_terminal),
        )
        .expect("one bounded retained terminal"),
        AdmissionInspection::OrphanAdmittedNeedsIndependentNoEffectProof,
    );
    assert_eq!(
        inspect_initial_admission(
            &expected,
            Some(&orphan),
            Some(&pointer(&orphan)),
            std::slice::from_ref(&old_terminal),
        )
        .expect("pointer resumed exact orphan"),
        AdmissionInspection::AdmittedUnarmed,
    );

    let mut different = orphan.clone();
    different.attempt_id = [0x8d; 32];
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&orphan),
        Some(&pointer(&different)),
        std::slice::from_ref(&old_terminal),
    ));

    // If the current attempt completes, the former retained terminal must
    // first be retired under D-68 before this one can remain as the sole
    // diagnostic record after pointer removal.
    let mut completed = orphan;
    completed.phase = InitialAdmissionPhase::Complete;
    assert_refuses(inspect_initial_admission(
        &expected,
        Some(&completed),
        None,
        std::slice::from_ref(&old_terminal),
    ));
}
