//! Native attempt-mint, remint and owner-fact admission oracles.
//!
//! These authentic Windows transaction tests preserve on-disk journal state,
//! process ownership, crash cuts and channel identity, without fake mocks.

use std::io::Write as _;

use super::support;
use super::{
    ATTEMPT_OWNER, CRASH_EXIT, CRASH_HELPER, INITIATING_LOGON, OWNERSHIP, assert_refusal,
    assert_resolved, awaiting_health, commit, complete, coordinator_image, observe, receipt,
    recover_exact, seed_per_user_baseline, verifier,
};
use crate::records::ActivationPhase;
use crate::windows_baseline::{
    ProcessFamilyRetirement, WindowsActivationOutcome, WindowsBaselineTrust,
    WindowsHealthAcceptedAttempt, WindowsRecoveryOutcome, load_windows_recovery_inspection,
};
use crate::{ActivationEffect, ActivationFailureClass, UpdateError};

#[test]
fn an_unlaunched_attempt_resumes_under_the_lease_with_fresh_channels() {
    let fixture = tempfile::tempdir().expect("unlaunched resume fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let stdout = support::child(
        CRASH_HELPER,
        fixture.path(),
        "first-commit",
        "floor-advanced",
        CRASH_EXIT,
    );
    assert!(
        stdout.contains("KELD_ACTIVATION_CUT=floor-advanced"),
        "{stdout}"
    );
    let journal_path = trust.installation.update_root.join("activation-journal");
    let lost_bytes = std::fs::read(&journal_path).expect("the lost attempt is journaled");
    let lost = observe(&trust)
        .journal
        .expect("the lost attempt is journaled");
    assert_eq!(lost.phase, ActivationPhase::PublishPending);

    let inspection = load_windows_recovery_inspection(&trust, &verifier(&trust))
        .expect("inspect the unlaunched attempt");
    let minted = inspection
        .resume_unlaunched_with_image_file(&coordinator_image(&trust))
        .expect("a never-launched attempt resumes under the writer lease alone");
    assert_eq!(minted.attempt_id(), &lost.attempt_id);
    assert_ne!(minted.health_channel_id(), &lost.health_channel_id);
    assert_eq!(
        std::fs::read(&journal_path).expect("journal after minting"),
        lost_bytes,
        "re-minting writes nothing until the resumed owner journals its facts"
    );
    let minted_health = *minted.health_channel_id();
    let attempt = awaiting_health(minted.journal(ATTEMPT_OWNER, INITIATING_LOGON));
    assert_eq!(attempt.attempt_id(), &lost.attempt_id);
    assert_eq!(attempt.health_channel_id(), &minted_health);
    assert_ne!(attempt.lifecycle_channel_id(), &lost.lifecycle_channel_id);
    let resumed = observe(&trust)
        .journal
        .expect("the resumed attempt stays journaled");
    assert_eq!(resumed.phase, ActivationPhase::AwaitingHealth);
    assert_eq!(resumed.health_channel_id, minted_health);
    assert_eq!(
        &resumed.lifecycle_channel_id,
        attempt.lifecycle_channel_id()
    );
    assert_eq!(resumed.ownership, Some(OWNERSHIP));

    let stale = ProcessFamilyRetirement::from_exact_zero_observation(
        *attempt.lifecycle_installation_id(),
        lost.attempt_id,
        lost.lifecycle_channel_id,
    );
    assert_refusal(
        attempt.roll_back(ActivationFailureClass::ProcessCrash, &stale),
        "process-family retirement binding",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(
        recover_exact(&trust).expect("the re-minted channel binds recovery"),
        WindowsActivationOutcome::RolledBack
    );
    assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);
}

/// Plants a genuine stale `pending-*` record sibling, which only a journal step removes.
fn plant_stale_record_preparation(trust: &WindowsBaselineTrust) {
    let parent = support::directory(&trust.installation.update_root);
    let mut file = crate::windows_fs::create_file_relative_with_profile(
        &parent,
        &format!("pending-{}", "d".repeat(64)),
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    )
    .expect("profiled stale preparation");
    file.write_all(b"prepared").expect("prepared bytes");
}

/// Crashes a child first update of 2.0.0 at `cut` and returns the lost journal's bytes.
pub(super) fn lost_first_attempt(
    fixture: &std::path::Path,
    trust: &WindowsBaselineTrust,
    cut: &str,
) -> Vec<u8> {
    let stdout = support::child(CRASH_HELPER, fixture, "first-commit", cut, CRASH_EXIT);
    assert!(
        stdout.contains(&format!("KELD_ACTIVATION_CUT={cut}")),
        "{stdout}"
    );
    std::fs::read(trust.installation.update_root.join("activation-journal"))
        .expect("the lost attempt is journaled")
}

#[test]
fn minting_writes_nothing_until_the_owner_journals_its_facts() {
    let fixture = tempfile::tempdir().expect("mint-then-journal fixture");
    let trust = seed_per_user_baseline(fixture.path());
    plant_stale_record_preparation(&trust);
    let (root, stage) = complete(&trust, "2.0.0");
    let before = observe(&trust);
    assert_eq!((before.journal.as_ref(), before.pending_records), (None, 1));

    let minted = root
        .begin_activation_with_image_file(stage, &coordinator_image(&trust))
        .expect("mint the attempt identities");
    assert_eq!(
        observe(&trust),
        before,
        "minting writes no journal, renames no stage and removes no record sibling"
    );
    let attempt_id = *minted.attempt_id();
    let health_channel_id = *minted.health_channel_id();
    assert_ne!(attempt_id, health_channel_id);
    assert_eq!(
        minted.lifecycle_installation_id(),
        &trust
            .lifecycle_installation_id()
            .expect("trusted installation ID")
    );

    let attempt = awaiting_health(minted.journal(ATTEMPT_OWNER, INITIATING_LOGON));
    assert_eq!(attempt.attempt_id(), &attempt_id);
    assert_eq!(attempt.health_channel_id(), &health_channel_id);
    let journaled = observe(&trust);
    assert_eq!(
        journaled.pending_records, 0,
        "the journal step removes stale siblings before its first record"
    );
    let journal = journaled.journal.expect("the attempt is journaled");
    assert_eq!(
        (journal.attempt_id, journal.health_channel_id),
        (attempt_id, health_channel_id),
        "the first record reveals exactly the minted identities"
    );
    assert_eq!(journal.ownership, Some(OWNERSHIP));
    assert_eq!(journal.phase, ActivationPhase::AwaitingHealth);
    assert!(
        std::fs::read(trust.installation.update_root.join("activation-journal"))
            .expect("journal bytes")
            .starts_with(br#"{"schema":"keld.activation-journal/v2","#)
    );
    let health = receipt(&attempt);
    assert_eq!(
        attempt
            .accept_health(&health)
            .and_then(WindowsHealthAcceptedAttempt::complete)
            .expect("exact health commits")
            .outcome(),
        WindowsActivationOutcome::Committed
    );
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);
}

#[test]
fn journal_bound_recovery_re_mints_an_unlaunched_attempt_without_writing() {
    let fixture = tempfile::tempdir().expect("recovery re-mint fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let lost = lost_first_attempt(fixture.path(), &trust, "floor-advanced");
    plant_stale_record_preparation(&trust);
    let before = observe(&trust);
    assert_eq!(before.pending_records, 1);
    let lost_journal = before
        .journal
        .clone()
        .expect("the lost attempt is journaled");
    assert_eq!(lost_journal.phase, ActivationPhase::PublishPending);

    let inspection = load_windows_recovery_inspection(&trust, &verifier(&trust))
        .expect("inspect the unlaunched attempt");
    let retirement = ProcessFamilyRetirement::from_exact_zero_observation(
        *inspection.lifecycle_installation_id(),
        *inspection.attempt_id(),
        *inspection.lifecycle_channel_id(),
    );
    let WindowsRecoveryOutcome::Reminted(minted) = inspection
        .recover_with_image_file(&retirement, &coordinator_image(&trust))
        .expect("an exact binding admits journal-bound recovery")
    else {
        panic!("a publish-pending journal re-mints its channels");
    };
    assert_eq!(
        observe(&trust),
        before,
        "re-minting through recovery writes no record and removes no record sibling"
    );
    assert_eq!(
        std::fs::read(trust.installation.update_root.join("activation-journal"))
            .expect("journal after re-minting"),
        lost,
        "the lost owner's journal stays byte-identical until the resumed owner journals"
    );
    assert_eq!(minted.attempt_id(), &lost_journal.attempt_id);
    assert_ne!(minted.health_channel_id(), &lost_journal.health_channel_id);
    let minted_health = *minted.health_channel_id();

    let attempt = awaiting_health(minted.journal(ATTEMPT_OWNER, INITIATING_LOGON));
    let resumed = observe(&trust);
    assert_eq!(
        resumed.pending_records, 0,
        "the journal step removes stale siblings before its first record"
    );
    let journal = resumed.journal.expect("the resumed attempt is journaled");
    assert_eq!(
        (journal.attempt_id, journal.health_channel_id),
        (lost_journal.attempt_id, minted_health),
        "the re-mint record reveals the kept attempt and the freshly minted channel"
    );
    assert_ne!(
        journal.lifecycle_channel_id,
        lost_journal.lifecycle_channel_id
    );
    assert_eq!(journal.ownership, Some(OWNERSHIP));
    let health = receipt(&attempt);
    assert_eq!(
        attempt
            .accept_health(&health)
            .and_then(WindowsHealthAcceptedAttempt::complete)
            .expect("exact health commits")
            .outcome(),
        WindowsActivationOutcome::Committed
    );
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);
}

#[test]
fn an_endpoint_refusal_between_mint_and_journal_writes_nothing() {
    let fixture = tempfile::tempdir().expect("fresh endpoint refusal fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let (root, stage) = complete(&trust, "2.0.0");
    let before = observe(&trust);
    let minted = root
        .begin_activation_with_image_file(stage, &coordinator_image(&trust))
        .expect("mint the attempt identities");
    // Seam-injected: the owner found its endpoint name already present.
    assert_refusal(
        Err::<(), _>(minted.refuse("attempt endpoint", "the endpoint name already exists")),
        "attempt endpoint",
        ActivationEffect::ProtectedStateUnchanged,
    );
    assert_eq!(
        observe(&trust),
        before,
        "a fresh refusal leaves only the stage"
    );
    commit(&trust, "2.0.0");
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);

    let fixture = tempfile::tempdir().expect("resumed endpoint refusal fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let lost = lost_first_attempt(fixture.path(), &trust, "floor-advanced");
    let minted = load_windows_recovery_inspection(&trust, &verifier(&trust))
        .expect("inspect the unlaunched attempt")
        .resume_unlaunched_with_image_file(&coordinator_image(&trust))
        .expect("re-mint the channels");
    assert_refusal(
        Err::<(), _>(minted.refuse("attempt endpoint", "the endpoint name already exists")),
        "attempt endpoint",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(
        std::fs::read(trust.installation.update_root.join("activation-journal"))
            .expect("journal after the refusal"),
        lost,
        "a resumed refusal leaves the journal unchanged"
    );
    assert_eq!(
        recover_exact(&trust).expect("the journal stays authoritative"),
        WindowsActivationOutcome::Committed
    );
}

/// Asserts an owner-fact refusal at `step` with `effect` whose detail names `reason`.
fn assert_owner_fact_refusal(
    result: Result<crate::WindowsJournaledAttempt, UpdateError>,
    step: &str,
    effect: ActivationEffect,
    reason: &str,
) {
    match result.expect_err(reason) {
        UpdateError::Activation {
            step: refused,
            effect: observed,
            detail,
        } => {
            assert_eq!((refused, observed), (step, effect), "{reason}");
            assert!(
                detail.contains(reason),
                "refused for `{detail}`, not `{reason}`"
            );
        }
        other => panic!("expected a {step} refusal for `{reason}`, got {other:?}"),
    }
}

#[test]
fn owner_facts_that_name_no_process_or_session_refuse_before_any_write() {
    const ZERO_PROCESS_ID: &str = "attempt owner process ID must be in 1..=4294967295";
    const ZERO_LOGON_TIME: &str = "initiating logon time must be positive";
    // KEL-53 §4, owner decision 2026-10-06 (zero LUID refused).
    const ZERO_LUID: &str = "initiating logon authentication ID must be nonzero";
    let zero_luid = crate::InitiatingLogon::new(0, 0x01dd_5568_af7a_c000);

    for (attempt_owner, initiating_logon, reason) in [
        (
            crate::AttemptOwner::new(0, 1),
            INITIATING_LOGON,
            ZERO_PROCESS_ID,
        ),
        (ATTEMPT_OWNER, zero_luid, ZERO_LUID),
    ] {
        let fixture = tempfile::tempdir().expect("fresh owner-fact refusal fixture");
        let trust = seed_per_user_baseline(fixture.path());
        plant_stale_record_preparation(&trust);
        let (root, stage) = complete(&trust, "2.0.0");
        let before = observe(&trust);
        let minted = root
            .begin_activation_with_image_file(stage, &coordinator_image(&trust))
            .expect("mint the attempt identities");
        assert_owner_fact_refusal(
            minted.journal(attempt_owner, initiating_logon),
            "start",
            ActivationEffect::ProtectedStateUnchanged,
            reason,
        );
        assert_eq!(
            observe(&trust),
            before,
            "{reason}: a refused owner fact writes no journal and removes no record sibling"
        );
    }

    let fixture = tempfile::tempdir().expect("resumed owner-fact refusal fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let lost = lost_first_attempt(fixture.path(), &trust, "floor-advanced");
    for (initiating_logon, reason) in [
        (crate::InitiatingLogon::new(1, 0), ZERO_LOGON_TIME),
        (zero_luid, ZERO_LUID),
    ] {
        let minted = load_windows_recovery_inspection(&trust, &verifier(&trust))
            .expect("inspect the unlaunched attempt")
            .resume_unlaunched_with_image_file(&coordinator_image(&trust))
            .expect("re-mint the channels");
        assert_owner_fact_refusal(
            minted.journal(ATTEMPT_OWNER, initiating_logon),
            "local record",
            ActivationEffect::JournalBoundRecoveryRequired,
            reason,
        );
        assert_eq!(
            std::fs::read(trust.installation.update_root.join("activation-journal"))
                .expect("journal after the refusal"),
            lost,
            "{reason}: a resumed refusal leaves the journal unchanged"
        );
    }
}

/// Replaces the protected journal with the landed v1 encoding of the same attempt, which
/// carries no owner facts.
pub(in super::super) fn downgrade_journal_to_v1(trust: &WindowsBaselineTrust) {
    let path = trust.installation.update_root.join("activation-journal");
    let mut journal = crate::records::decode_activation_journal(
        &std::fs::read(&path).expect("protected journal bytes"),
    )
    .expect("canonical v2 journal");
    journal.ownership = None;
    let bytes = crate::records::encode_activation_journal(&journal).expect("v1 encoding");
    assert!(bytes.starts_with(br#"{"schema":"keld.activation-journal/v1","#));
    std::fs::remove_file(&path).expect("remove the v2 journal");
    let parent = support::directory(&trust.installation.update_root);
    let mut file = crate::windows_fs::create_file_relative_with_profile(
        &parent,
        "activation-journal",
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    )
    .expect("profiled v1 journal");
    file.write_all(&bytes).expect("v1 journal bytes");
    file.sync_all().expect("flush the v1 journal");
}

#[test]
fn a_landed_v1_journal_finishes_and_its_re_mint_record_is_v2() {
    support::assert_user_principal_token();
    // An unlaunched v1 attempt resumes; its re-mint record supplies the owner facts.
    let fixture = tempfile::tempdir().expect("v1 publish-pending fixture");
    let trust = seed_per_user_baseline(fixture.path());
    lost_first_attempt(fixture.path(), &trust, "floor-advanced");
    downgrade_journal_to_v1(&trust);
    assert_eq!(
        recover_exact(&trust).expect("a v1 publish-pending attempt resumes"),
        WindowsActivationOutcome::Committed,
        "recovery asserts the resumed journal carries the owner facts"
    );
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);

    // A launched v1 attempt rolls back; its phase writes keep the v1 encoding.
    let fixture = tempfile::tempdir().expect("v1 awaiting-health fixture");
    let trust = seed_per_user_baseline(fixture.path());
    lost_first_attempt(fixture.path(), &trust, "awaiting-health");
    downgrade_journal_to_v1(&trust);
    let stdout = support::child(
        CRASH_HELPER,
        fixture.path(),
        "recover",
        "rollback-pending",
        CRASH_EXIT,
    );
    assert!(
        stdout.contains("KELD_ACTIVATION_CUT=rollback-pending"),
        "{stdout}"
    );
    let bytes = std::fs::read(trust.installation.update_root.join("activation-journal"))
        .expect("rollback-pending journal");
    assert!(bytes.starts_with(br#"{"schema":"keld.activation-journal/v1","#));
    let journal = crate::records::decode_activation_journal(&bytes).expect("v1 journal");
    assert_eq!(
        journal.ownership, None,
        "a v1 attempt never gains claimable facts"
    );
    assert!(matches!(
        journal.phase,
        ActivationPhase::RollbackPending { .. }
    ));
    assert_eq!(
        recover_exact(&trust).expect("a v1 rollback finishes"),
        WindowsActivationOutcome::RolledBack
    );
    assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);
}
