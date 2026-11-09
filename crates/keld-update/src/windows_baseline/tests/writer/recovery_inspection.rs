//! Real Windows protected recovery inspection and durable crash-cut admissions.
//!
//! These native tests preserve the original reader lease, pending journal,
//! floor/current/last-known-good historical binding, and read-only assertions.
//! Exact subprocess-helper selectors remain in the parent writer test owner.

use super::{
    ActivationPhase, GOLDEN, OpenOptions, load_windows_activation_write_snapshot,
    load_windows_recovery_inspection, seed_pending_activation_journal, seed_per_user_baseline,
    support, write_profiled_record,
};

#[test]
fn recovery_inspection_retains_writer_lease_and_preserves_pending_state() {
    let fixture = tempfile::tempdir().expect("per-user recovery inspection fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (attempt_id, lifecycle_channel_id) =
        seed_pending_activation_journal(&trust, ActivationPhase::PublishPending);
    let update_root = &trust.installation.update_root;
    let journal_path = update_root.join("activation-journal");
    let journal_before = std::fs::read(&journal_path).expect("pending journal bytes");
    let pointer_names = ["version-floor", "current", "last-known-good"];
    let pointers_before = pointer_names
        .map(|name| std::fs::read(update_root.join(name)).expect("protected pointer bytes"));

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("read-only recovery inspection accepts consistent protected pending journal");
    assert_eq!(inspection.identity(), &trust.installation);
    assert_eq!(
        inspection.lifecycle_installation_id(),
        &trust
            .lifecycle_installation_id()
            .expect("trusted installation binding")
    );
    assert_eq!(inspection.attempt_id(), &attempt_id);
    assert_eq!(inspection.lifecycle_channel_id(), &lifecycle_channel_id);
    assert_eq!(inspection.version_floor(), "1.0.0");
    assert_eq!(inspection.current().version, "1.0.0");
    assert_eq!(inspection.last_known_good().version, "1.0.0");
    assert!(inspection.previous_known_good().is_none());
    assert!(
        OpenOptions::new()
            .read(true)
            .open(update_root.join("activation.lock"))
            .is_err(),
        "inspection retains the exclusive writer lease during witness comparison"
    );
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "the ordinary writer loader still refuses pending journals"
    );
    assert_eq!(
        std::fs::read(&journal_path).expect("journal while inspection lives"),
        journal_before,
        "inspection cannot change the activation journal"
    );
    for (name, before) in pointer_names.into_iter().zip(&pointers_before) {
        assert_eq!(
            std::fs::read(update_root.join(name)).expect("pointer while inspection lives"),
            *before,
            "inspection cannot change {name}"
        );
    }
    drop(inspection);
    assert!(
        OpenOptions::new()
            .read(true)
            .open(update_root.join("activation.lock"))
            .is_ok(),
        "dropping the inspection releases its exact writer lease"
    );
    assert_eq!(
        std::fs::read(&journal_path).expect("journal after inspection drop"),
        journal_before
    );
    for (name, before) in pointer_names.into_iter().zip(&pointers_before) {
        assert_eq!(
            std::fs::read(update_root.join(name)).expect("pointer after inspection drop"),
            *before
        );
    }
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "the ordinary writer loader must continue refusing pending recovery after inspection"
    );
    std::fs::write(&journal_path, b"malformed-journal")
        .expect("install malformed protected journal negative control");
    assert!(
        load_windows_recovery_inspection(&trust, &verifier).is_err(),
        "malformed journal bytes must not produce a recovery inspection"
    );
    let impossible_history = String::from_utf8(journal_before.clone())
        .expect("canonical journal UTF-8")
        .replace(r#""prior_floor":"1.0.0""#, r#""prior_floor":"0.5.0""#);
    assert_ne!(impossible_history.as_bytes(), journal_before);
    std::fs::write(&journal_path, impossible_history)
        .expect("install journal with an impossible historical floor");
    assert!(
        load_windows_recovery_inspection(&trust, &verifier).is_err(),
        "prior floor below the recorded rollback/LKG artifacts must refuse inspection"
    );
    std::fs::write(&journal_path, &journal_before).expect("restore exact pending journal");
}

#[test]
fn recovery_inspection_accepts_candidate_selected_while_health_is_pending() {
    let fixture = tempfile::tempdir().expect("awaiting-health recovery fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (attempt_id, lifecycle_channel_id) =
        seed_pending_activation_journal(&trust, ActivationPhase::AwaitingHealth);

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("valid awaiting-health cut has candidate current and prior LKG");
    assert_eq!(inspection.attempt_id(), &attempt_id);
    assert_eq!(inspection.lifecycle_channel_id(), &lifecycle_channel_id);
    assert_eq!(inspection.version_floor(), "2.0.0");
    assert_eq!(inspection.current().version, "2.0.0");
    assert_eq!(inspection.last_known_good().version, "1.0.0");
    assert!(inspection.previous_known_good().is_none());
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "ordinary writer admission continues to refuse this pending transaction"
    );
}

#[test]
fn recovery_inspection_accepts_health_commit_cut_with_previous_published_first() {
    let fixture = tempfile::tempdir().expect("health commit-cut fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (attempt_id, lifecycle_channel_id) = seed_pending_activation_journal(
        &trust,
        ActivationPhase::HealthAccepted {
            health_receipt_digest: [0x99; 32],
        },
    );
    let baseline = trust.installation.baseline.clone();
    let update = support::directory(&trust.installation.update_root);
    write_profiled_record(
        &update,
        "previous-known-good",
        &crate::records::encode_pointer(crate::records::PointerKind::PreviousKnownGood, &baseline)
            .expect("previous known-good intermediate pointer"),
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    );

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("health-accepted journal accepts its durable previous-pointer cut");
    assert_eq!(inspection.attempt_id(), &attempt_id);
    assert_eq!(inspection.lifecycle_channel_id(), &lifecycle_channel_id);
    assert_eq!(inspection.version_floor(), "2.0.0");
    assert_eq!(inspection.current().version, "2.0.0");
    assert_eq!(inspection.last_known_good().version, "1.0.0");
    assert_eq!(
        inspection.previous_known_good(),
        Some(&trust.installation.baseline)
    );
}

#[test]
fn recovery_inspection_accepts_health_commit_cut_after_lkg_publication() {
    let fixture = tempfile::tempdir().expect("health commit-final-cut fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (_, _) = seed_pending_activation_journal(
        &trust,
        ActivationPhase::HealthAccepted {
            health_receipt_digest: [0x9a; 32],
        },
    );
    let baseline = trust.installation.baseline.clone();
    let update = support::directory(&trust.installation.update_root);
    write_profiled_record(
        &update,
        "previous-known-good",
        &crate::records::encode_pointer(crate::records::PointerKind::PreviousKnownGood, &baseline)
            .expect("previous known-good pointer"),
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    );
    let mut candidate = baseline.clone();
    candidate.version = "2.0.0".to_owned();
    candidate.content_blake3 = *blake3::hash(GOLDEN).as_bytes();
    std::fs::write(
        trust.installation.update_root.join("last-known-good"),
        crate::records::encode_pointer(crate::records::PointerKind::LastKnownGood, &candidate)
            .expect("candidate LKG pointer"),
    )
    .expect("publish candidate LKG pointer in fixture");

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("health-accepted journal accepts the durable candidate-LKG commit cut");
    assert_eq!(inspection.version_floor(), "2.0.0");
    assert_eq!(inspection.current().version, "2.0.0");
    assert_eq!(inspection.last_known_good().version, "2.0.0");
    assert_eq!(inspection.previous_known_good(), Some(&baseline));
}

#[test]
fn recovery_inspection_accepts_rollback_pending_before_and_after_current_replacement() {
    for current_candidate in [true, false] {
        let fixture = tempfile::tempdir().expect("rollback crash-cut fixture");
        let trust = seed_per_user_baseline(fixture.path());
        let verifier = crate::UpdateVerifier::new(
            trust.installation.clone(),
            crate::tests::signing_key().verifying_key().to_bytes(),
        )
        .expect("trusted test verifier");
        seed_pending_activation_journal(
            &trust,
            ActivationPhase::RollbackPending {
                failure: crate::records::ActivationFailureClass::HealthRejected,
            },
        );
        let expected_current = if current_candidate {
            let mut candidate = trust.installation.baseline.clone();
            candidate.version = "2.0.0".to_owned();
            candidate.content_blake3 = *blake3::hash(GOLDEN).as_bytes();
            candidate
        } else {
            let baseline = trust.installation.baseline.clone();
            std::fs::write(
                trust.installation.update_root.join("current"),
                crate::records::encode_pointer(crate::records::PointerKind::Current, &baseline)
                    .expect("rollback-target current pointer"),
            )
            .expect("publish rollback target in fixture");
            baseline
        };

        let inspection = load_windows_recovery_inspection(&trust, &verifier)
            .expect("rollback-pending journal accepts each recorded current-pointer cut");
        assert_eq!(inspection.version_floor(), "2.0.0");
        assert_eq!(inspection.current(), &expected_current);
        assert_eq!(inspection.last_known_good(), &trust.installation.baseline);
        assert!(inspection.previous_known_good().is_none());
    }
}

#[test]
fn recovery_inspection_rejects_superseded_lkg_without_historical_previous() {
    let fixture = tempfile::tempdir().expect("missing historical previous fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    seed_pending_activation_journal(&trust, ActivationPhase::PublishPending);

    let journal_path = trust.installation.update_root.join("activation-journal");
    let mut journal = crate::records::decode_activation_journal(
        &std::fs::read(&journal_path).expect("seed pending journal"),
    )
    .expect("canonical seeded journal");
    let prior_lkg = journal.candidate.clone();
    journal.prior_floor = prior_lkg.version.clone();
    journal.prior_last_known_good = prior_lkg.clone();
    journal.rollback_target = prior_lkg.clone();
    journal.candidate.version = "3.0.0".to_owned();
    let journal_bytes = crate::records::encode_activation_journal(&journal)
        .expect("journal with valid floor but missing required history");
    std::fs::write(journal_path, journal_bytes).expect("write history-gap journal");

    let update_root = &trust.installation.update_root;
    std::fs::write(
        update_root.join("version-floor"),
        crate::records::encode_floor(&prior_lkg.version).expect("prior floor"),
    )
    .expect("write prior floor");
    std::fs::write(
        update_root.join("current"),
        crate::records::encode_pointer(crate::records::PointerKind::Current, &prior_lkg)
            .expect("current pointer"),
    )
    .expect("write current pointer");
    std::fs::write(
        update_root.join("last-known-good"),
        crate::records::encode_pointer(crate::records::PointerKind::LastKnownGood, &prior_lkg)
            .expect("LKG pointer"),
    )
    .expect("write LKG pointer");

    let error = load_windows_recovery_inspection(&trust, &verifier)
        .expect_err("journal cannot omit the older known-good artifact after baseline moved");
    assert!(
        matches!(
            error,
            crate::UpdateError::Baseline {
                step: "activation journal previous-known-good history",
                ..
            }
        ),
        "history gap has a direct refusal, got {error:?}"
    );
}
