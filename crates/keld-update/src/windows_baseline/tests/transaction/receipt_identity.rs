//! Identity binding of journaled health receipts, channels, process retirement,
//! and the coordinator image. Product oracles observe protected disk records.

use super::{
    ActivationEffect, ActivationFailureClass, ActivationHealthReceipt, ActivationPhase,
    ProcessFamilyRetirement, WindowsActivationOutcome, assert_refusal, assert_resolved, begin,
    coordinator_image, fixture_root, load_windows_recovery_inspection, observe, receipt,
    recover_exact, seed_per_user_baseline, support, verifier,
};

#[test]
fn substituted_health_receipts_refuse_before_any_write_and_leave_recovery_authoritative() {
    for substitution in ["attempt", "channel", "candidate"] {
        let fixture = tempfile::tempdir().expect("receipt falsifier fixture");
        let trust = seed_per_user_baseline(fixture.path());
        let attempt = begin(&trust, "2.0.0");
        let before = observe(&trust);
        let mut attempt_id = *attempt.attempt_id();
        let mut health_channel_id = *attempt.health_channel_id();
        let mut candidate = attempt.candidate().clone();
        match substitution {
            "attempt" => attempt_id[0] ^= 1,
            // A real identity from the same attempt, but not its health channel.
            "channel" => health_channel_id = *attempt.lifecycle_channel_id(),
            _ => candidate.content_blake3[0] ^= 1,
        }
        let substituted = ActivationHealthReceipt::new(attempt_id, health_channel_id, candidate);
        assert_refusal(
            attempt.accept_health(&substituted),
            "health receipt binding",
            ActivationEffect::JournalBoundRecoveryRequired,
        );
        assert_eq!(
            observe(&trust),
            before,
            "{substitution}: a substituted receipt writes nothing"
        );
        assert_eq!(
            recover_exact(&trust).expect("exact recovery of the unresolved attempt"),
            WindowsActivationOutcome::RolledBack,
            "{substitution}: health that never matched the attempt is never committed"
        );
        assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);
    }
}

#[test]
fn the_expected_digest_is_the_receipt_digest_of_the_journaled_attempt_channel_and_candidate() {
    support::assert_user_principal_token();
    let fixture = tempfile::tempdir().expect("expected digest fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let attempt = begin(&trust, "2.0.0");
    let expected = *attempt.health_receipt_digest();
    let journal = observe(&trust).journal.expect("live attempt is journaled");
    let digest = |channel: &[u8; 32]| {
        crate::records::activation_health_receipt_digest(
            &journal.attempt_id,
            channel,
            &journal.candidate,
        )
        .expect("digest of a journaled attempt")
    };
    // The protected record, read back from disk, names the inputs; the accessor must
    // bind exactly the health channel, never the lifecycle channel that the same
    // journal also carries.
    assert_eq!(expected, digest(&journal.health_channel_id));
    assert_ne!(expected, digest(&journal.lifecycle_channel_id));

    let health = receipt(&attempt);
    // Dropping the accepted attempt releases the lease without completing.
    drop(
        attempt
            .accept_health(&health)
            .expect("exact health is durably accepted"),
    );
    assert_eq!(
        observe(&trust).journal.map(|journal| journal.phase),
        Some(ActivationPhase::HealthAccepted {
            health_receipt_digest: expected
        }),
        "HealthAccepted records the digest the owner compared with KELD-AB1"
    );
    // Recovery recomputes the digest from the journal and finishes the commit.
    assert_eq!(
        recover_exact(&trust).expect("recovery finishes an accepted attempt"),
        WindowsActivationOutcome::Committed
    );
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);
}

#[test]
fn substituted_retirement_or_coordinator_refuses_live_and_recovery_writes() {
    let fixture = tempfile::tempdir().expect("retirement falsifier fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = verifier(&trust);
    let attempt = begin(&trust, "2.0.0");
    let before = observe(&trust);
    let installation = *attempt.lifecycle_installation_id();
    let attempt_id = *attempt.attempt_id();
    let channel = *attempt.lifecycle_channel_id();
    let flip = |mut value: [u8; 32]| {
        value[31] ^= 1;
        value
    };
    let substituted = [
        ProcessFamilyRetirement::from_exact_zero_observation(
            flip(installation),
            attempt_id,
            channel,
        ),
        ProcessFamilyRetirement::from_exact_zero_observation(
            installation,
            flip(attempt_id),
            channel,
        ),
        ProcessFamilyRetirement::from_exact_zero_observation(
            installation,
            attempt_id,
            flip(channel),
        ),
    ];
    assert_refusal(
        attempt.roll_back(ActivationFailureClass::ProcessCrash, &substituted[0]),
        "process-family retirement binding",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(
        observe(&trust),
        before,
        "live rollback writes nothing without exact proof"
    );

    let image = coordinator_image(&trust);
    for wrong in &substituted {
        let inspection =
            load_windows_recovery_inspection(&trust, &verifier).expect("inspect the lost attempt");
        assert_refusal(
            inspection.recover_with_image_file(wrong, &image),
            "process-family retirement binding",
            ActivationEffect::JournalBoundRecoveryRequired,
        );
        assert_eq!(observe(&trust), before);
    }
    let exact =
        ProcessFamilyRetirement::from_exact_zero_observation(installation, attempt_id, channel);
    let inspection =
        load_windows_recovery_inspection(&trust, &verifier).expect("inspect the lost attempt");
    assert_refusal(
        inspection.recover_with_image_file(
            &exact,
            &support::other_coordinator_image(fixture_root(&trust)),
        ),
        "coordinator identity",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(observe(&trust), before);
    let inspection =
        load_windows_recovery_inspection(&trust, &verifier).expect("inspect the lost attempt");
    assert_refusal(
        inspection.resume_unlaunched_with_image_file(&image),
        "unlaunched resume",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(
        observe(&trust),
        before,
        "a launched attempt is never resumed without a retirement binding"
    );

    assert_eq!(
        recover_exact(&trust).expect("exact retirement recovers the lost attempt"),
        WindowsActivationOutcome::RolledBack,
        "an attempt that lost its owner during health rolls back"
    );
    assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);
}
