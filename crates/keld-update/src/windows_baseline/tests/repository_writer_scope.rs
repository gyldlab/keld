//! Native KEL-53 writer-lease negative proof for D-90.

use super::selection::record_bytes;
use super::transaction::verifier;
use super::writer::seed_per_user_baseline;
use crate::windows_baseline::repository_census::{
    WindowsRepositoryEligibilityRequest, WindowsRepositoryOperation,
};
use crate::windows_baseline::repository_writer_scope::refuse_installation_lease_as_repository_owner;
use crate::windows_baseline::{
    load_windows_activation_write_snapshot, select_windows_active_package,
};
use crate::{Channel, UpdateError};

const REPOSITORY: [u8; 32] = [0x91; 32];

fn refusal_step(
    writer: &crate::WindowsActivationWriteSnapshot,
    request: &WindowsRepositoryEligibilityRequest<'_>,
) -> &'static str {
    match refuse_installation_lease_as_repository_owner(writer, request)
        .expect_err("an installation lease cannot issue repository authority")
    {
        UpdateError::Baseline { step, .. } => step,
        other => panic!("unexpected installation lease refusal: {other}"),
    }
}

#[test]
fn genuine_exclusive_installation_lease_is_not_repository_membership_authority() {
    let fixture = tempfile::tempdir().expect("native installation lease fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let before = record_bytes(&trust);
    let writer = load_windows_activation_write_snapshot(&trust, &verifier(&trust))
        .expect("existing KEL-53 native writer lease");
    let request = WindowsRepositoryEligibilityRequest {
        repository_id: &REPOSITORY,
        app_id: &trust.installation.app_id,
        owner_sid: "S-1-5-21-123-456-789-1001",
        commissioned_channel: Some(trust.installation.channel),
        requested_channel: trust.installation.channel,
        source_channel: Some(trust.installation.channel),
        destination_channel: Some(trust.installation.channel),
        operation: WindowsRepositoryOperation::PlannedTransition,
    };
    assert_eq!(
        refusal_step(&writer, &request),
        "repository writer scope",
        "the real installation lease cannot cover unrelated roots or source readers"
    );
    assert!(
        select_windows_active_package(&trust).is_err(),
        "the writer still has the exclusive per-install activation lease"
    );
    drop(writer);
    assert!(
        select_windows_active_package(&trust).is_ok(),
        "the installation selector works again after the existing lease closes"
    );
    assert_eq!(
        record_bytes(&trust),
        before,
        "D-90 refusal must not mutate records"
    );
}

#[test]
fn protected_installation_app_and_channel_mismatches_refuse_before_scope_check() {
    let fixture = tempfile::tempdir().expect("native writer identity fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let writer = load_windows_activation_write_snapshot(&trust, &verifier(&trust))
        .expect("existing KEL-53 native writer");
    let mut request = WindowsRepositoryEligibilityRequest {
        repository_id: &REPOSITORY,
        app_id: &trust.installation.app_id,
        owner_sid: "S-1-5-21-123-456-789-1001",
        commissioned_channel: Some(trust.installation.channel),
        requested_channel: trust.installation.channel,
        source_channel: Some(trust.installation.channel),
        destination_channel: None,
        operation: WindowsRepositoryOperation::OrdinaryUpdate,
    };
    request.app_id = "org.example.foreign";
    assert_eq!(
        refusal_step(&writer, &request),
        "repository writer app binding"
    );
    request.app_id = &trust.installation.app_id;
    request.commissioned_channel = None;
    assert_eq!(
        refusal_step(&writer, &request),
        "repository writer commissioning"
    );
    let foreign = match trust.installation.channel {
        Channel::Stable => Channel::Beta,
        Channel::Beta | Channel::Canary => Channel::Stable,
    };
    request.commissioned_channel = Some(foreign);
    assert_eq!(refusal_step(&writer, &request), "repository writer channel");
}

#[test]
fn simultaneous_native_installation_writers_cannot_mint_repository_eligibility() {
    let first_root = tempfile::tempdir().expect("first independent NTFS installation");
    let second_root = tempfile::tempdir().expect("second independent NTFS installation");
    let first = seed_per_user_baseline(first_root.path());
    let second = seed_per_user_baseline(second_root.path());
    assert_ne!(
        first.installation.install_root, second.installation.install_root,
        "distinct native installation roots are required for the lease scope falsifier"
    );
    assert_eq!(
        first.installation.app_id, second.installation.app_id,
        "the negative control uses the same application identity"
    );
    assert_eq!(
        first.installation.channel, second.installation.channel,
        "the negative control uses the same installed channel"
    );
    let first_before = record_bytes(&first);
    let second_before = record_bytes(&second);

    // This is the actual OS-level counterexample to treating an existing
    // installation's activation.lock as a repository-wide writer lease.
    // Each root has its own share-zero file; holding the first does NOT block
    // a second independent installation's writer. No repository identifier
    // is present in v1 provenance, so identical caller claims cannot prove
    // that either writer covers the missing acquisition-v1 repository.
    let first_writer = load_windows_activation_write_snapshot(&first, &verifier(&first))
        .expect("first real exclusive installation writer");
    assert!(
        select_windows_active_package(&first).is_err(),
        "first exclusive writer blocks its own installation reader"
    );
    assert!(
        select_windows_active_package(&second).is_ok(),
        "first installation lease cannot fence a different installed root"
    );
    let second_writer = load_windows_activation_write_snapshot(&second, &verifier(&second))
        .expect("second installation owns an independent exclusive activation lease");
    assert!(
        select_windows_active_package(&second).is_err(),
        "second exclusive writer blocks only its own installation reader"
    );

    for (trust, writer) in [(&first, &first_writer), (&second, &second_writer)] {
        let request = WindowsRepositoryEligibilityRequest {
            repository_id: &REPOSITORY,
            app_id: &trust.installation.app_id,
            owner_sid: "S-1-5-21-123-456-789-1001",
            commissioned_channel: Some(trust.installation.channel),
            requested_channel: trust.installation.channel,
            source_channel: Some(trust.installation.channel),
            destination_channel: Some(trust.installation.channel),
            operation: WindowsRepositoryOperation::PlannedTransition,
        };
        assert_eq!(
            refusal_step(writer, &request),
            "repository writer scope",
            "neither exclusive installation lease is a repository-wide D-90 census witness"
        );
    }

    drop(first_writer);
    assert!(
        select_windows_active_package(&first).is_ok(),
        "releasing the first installation writer restores only its own reader"
    );
    assert!(
        select_windows_active_package(&second).is_err(),
        "the second installation writer remains independently exclusive"
    );
    drop(second_writer);
    assert!(
        select_windows_active_package(&second).is_ok(),
        "releasing the second installation writer restores its own reader"
    );
    assert_eq!(
        record_bytes(&first),
        first_before,
        "D-90 refusal must not write protected records in the first installation"
    );
    assert_eq!(
        record_bytes(&second),
        second_before,
        "D-90 refusal must not write protected records in the second installation"
    );
}
