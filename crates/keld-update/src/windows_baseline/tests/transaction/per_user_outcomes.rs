//! Real native per-user activation commit, durable-health, rollback and floor outcomes.
//!
//! These end-to-end tests preserve the original on-disk journal, lease,
//! rollback and signed-manifest assertions; shared fixture setup stays in parent.

use super::{
    assert_resolved, begin, commit, observe, receipt, retirement, seed_per_user_baseline, support,
    verifier,
};
use crate::ActivationFailureClass;
use crate::records::ActivationPhase;
use crate::windows_baseline::{
    WindowsActivationOutcome, load_windows_activation_write_snapshot,
    load_windows_recovery_inspection,
};

#[test]
fn per_user_updates_commit_through_the_common_trace_and_retire_superseded_versions() {
    support::assert_user_principal_token();
    let fixture = tempfile::tempdir().expect("per-user activation fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = verifier(&trust);

    let attempt = begin(&trust, "2.0.0");
    let awaiting = observe(&trust);
    assert_eq!(awaiting.floor, "2.0.0", "floor advances before selection");
    assert_eq!(awaiting.current, "2.0.0");
    assert_eq!(
        awaiting.last_known_good, "1.0.0",
        "the prior known-good stays until exact health"
    );
    let journal = awaiting.journal.expect("live attempt is journaled");
    assert_eq!(journal.phase, ActivationPhase::AwaitingHealth);
    assert_eq!(&journal.attempt_id, attempt.attempt_id());
    assert_eq!(&journal.health_channel_id, attempt.health_channel_id());
    assert_eq!(
        &journal.lifecycle_channel_id,
        attempt.lifecycle_channel_id()
    );
    assert_eq!(
        journal.helper_image_blake3,
        support::COORDINATOR_IMAGE_BLAKE3
    );
    assert_eq!(journal.rollback_target.version, "1.0.0");
    assert_eq!(journal.prior_floor, "1.0.0");
    assert_eq!(
        attempt.lifecycle_installation_id(),
        &trust
            .lifecycle_installation_id()
            .expect("trusted lifecycle installation ID")
    );
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "the live attempt keeps the share-zero writer lease through health"
    );
    assert!(
        load_windows_recovery_inspection(&trust, &verifier).is_err(),
        "recovery cannot race a live owner"
    );

    let health = receipt(&attempt);

    let accepted = attempt
        .accept_health(&health)
        .expect("exact health is durably accepted");
    let durable = observe(&trust);
    assert!(
        matches!(
            durable.journal.as_ref().map(|journal| &journal.phase),
            Some(ActivationPhase::HealthAccepted { .. })
        ),
        "the first health step returns at the durable HealthAccepted record: {durable:?}"
    );
    assert_eq!(
        (durable.current.as_str(), durable.last_known_good.as_str()),
        ("2.0.0", "1.0.0"),
        "the first health step commits nothing"
    );
    let resolution = accepted
        .complete()
        .expect("exact health commits the first update");
    assert_eq!(resolution.outcome(), WindowsActivationOutcome::Committed);
    assert!(resolution.cleanup_error().is_none());
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);

    commit(&trust, "3.0.0");
    assert_resolved(&trust, "3.0.0", Some("2.0.0"), "3.0.0", &["2.0.0", "3.0.0"]);
}

#[test]
fn per_user_rollback_restores_the_target_retires_the_candidate_and_keeps_the_floor() {
    support::assert_user_principal_token();
    let fixture = tempfile::tempdir().expect("per-user rollback fixture");
    let trust = seed_per_user_baseline(fixture.path());

    let attempt = begin(&trust, "2.0.0");
    let proof = retirement(&attempt);
    let resolution = attempt
        .roll_back(ActivationFailureClass::HealthRejected, &proof)
        .expect("bound failure rolls back");
    assert_eq!(resolution.outcome(), WindowsActivationOutcome::RolledBack);
    assert_eq!(resolution.current().version, "1.0.0");
    assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);

    let verifier = verifier(&trust);
    let admitted = verifier
        .admit(&crate::ProvenanceObservation::Protected {
            record: crate::InstallProvenance {
                identity: trust.installation.clone(),
                owner: crate::InstallOwner::Direct,
            },
            version_floor: Some(observe(&trust).floor),
        })
        .expect("admit the rolled-back installation");
    let manifest = crate::tests::manifest_json(&crate::tests::release_json(
        "2.0.0",
        "1",
        &crate::tests::digest_hex(b"x"),
        "1",
        &crate::tests::digest_hex(b"x"),
        "",
    ));
    assert!(
        matches!(
            admitted
                .verify_manifest(&manifest, &crate::tests::sign(&manifest))
                .expect("authentic replayed manifest"),
            crate::ManifestDecision::NoUpdate
        ),
        "the failed signed version is never automatically reselected"
    );

    commit(&trust, "3.0.0");
    assert_resolved(&trust, "3.0.0", Some("1.0.0"), "3.0.0", &["1.0.0", "3.0.0"]);
}
