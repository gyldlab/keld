//! D-90 negative admission against KEL-53's real native selected source.
//! Native filesystem selection proves one installed source, not repository census.

use super::selection::record_bytes;
use super::transaction::verifier;
use super::writer::seed_per_user_baseline;
use crate::windows_baseline::repository_census::WindowsRepositoryOperation;
use crate::windows_baseline::repository_source_reader::refuse_unqualified_selected_source;
use crate::windows_baseline::{
    load_windows_activation_write_snapshot, select_windows_active_package,
};
use crate::{ActivationEffect, Channel, UpdateError};

fn refusal(
    source: &crate::ActivePackageSelection,
    channel: Option<Channel>,
    operation: WindowsRepositoryOperation,
) -> (&'static str, String) {
    match refuse_unqualified_selected_source(source, channel, operation)
        .expect_err("unqualified source readers must never mint v2 authority")
    {
        UpdateError::Baseline { step, detail } => (step, detail),
        other => panic!("unexpected source-reader refusal: {other}"),
    }
}

#[test]
fn pinned_same_channel_source_still_requires_independent_reader_and_drain_proof() {
    let fixture = tempfile::tempdir().expect("source-reader fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let before = record_bytes(&trust);
    let source = select_windows_active_package(&trust).expect("native protected source selection");
    assert_eq!(source.install_identity(), &trust.installation);
    for operation in [
        WindowsRepositoryOperation::InitialInstallation,
        WindowsRepositoryOperation::OrdinaryUpdate,
        WindowsRepositoryOperation::Rollback,
        WindowsRepositoryOperation::PlannedTransition,
        WindowsRepositoryOperation::ManualRecovery,
    ] {
        let (step, detail) = refusal(&source, Some(trust.installation.channel), operation);
        assert_eq!(step, "repository reader qualification");
        assert!(
            detail.contains(&source.artifact().version),
            "diagnosis must name the actual selected installed version: {detail}"
        );
    }
    assert_eq!(
        record_bytes(&trust),
        before,
        "refusal may not mutate installed state"
    );
}

#[test]
fn retained_old_source_does_not_become_drained_when_a_new_writer_acquires_its_lease() {
    let fixture = tempfile::tempdir().expect("retained old-source fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let before = record_bytes(&trust);
    let old_source =
        select_windows_active_package(&trust).expect("old reader selects protected source");
    let old_version = old_source.artifact().version.clone();

    // Selection releases its short shared activation lease while retaining
    // the immutable version and ancestor handles. The next installed writer
    // can therefore take the same installation's share-zero lease while an
    // earlier source selection is still live. The real repository owner must
    // separately seal and drain old launch admission before a trust switch.
    let writer = load_windows_activation_write_snapshot(&trust, &verifier(&trust))
        .expect("real installation writer after the old selection");
    assert!(matches!(
        select_windows_active_package(&trust),
        Err(UpdateError::Activation {
            effect: ActivationEffect::WriterActive,
            ..
        })
    ));
    assert_eq!(old_source.install_identity(), &trust.installation);
    assert_eq!(old_source.artifact().version, old_version);

    for operation in [
        WindowsRepositoryOperation::OrdinaryUpdate,
        WindowsRepositoryOperation::Rollback,
        WindowsRepositoryOperation::PlannedTransition,
        WindowsRepositoryOperation::ManualRecovery,
    ] {
        let (step, detail) = refusal(&old_source, Some(trust.installation.channel), operation);
        assert_eq!(step, "repository reader qualification");
        assert!(
            detail.contains(&old_version),
            "a live, previously selected version must remain diagnosable: {detail}"
        );
    }

    drop(writer);
    assert!(
        select_windows_active_package(&trust).is_ok(),
        "the installation lease is available again, without proving old-reader drain"
    );
    assert_eq!(record_bytes(&trust), before);
}

#[test]
fn pinned_foreign_channel_and_uncommissioned_source_refuse_before_reader_qualification() {
    let fixture = tempfile::tempdir().expect("foreign-channel source fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let before = record_bytes(&trust);
    let source = select_windows_active_package(&trust).expect("native protected source");

    let (missing, _) = refusal(&source, None, WindowsRepositoryOperation::PlannedTransition);
    assert_eq!(missing, "repository source commissioning");
    let foreign = match source.install_identity().channel {
        Channel::Stable => Channel::Beta,
        Channel::Beta | Channel::Canary => Channel::Stable,
    };
    let (step, detail) = refusal(
        &source,
        Some(foreign),
        WindowsRepositoryOperation::ManualRecovery,
    );
    assert_eq!(step, "repository source channel");
    assert!(
        detail.contains(source.install_identity().channel.as_str()),
        "diagnosis must use the protected source channel, not the request: {detail}"
    );
    assert_eq!(
        record_bytes(&trust),
        before,
        "channel refusal must have no effects"
    );
}

#[test]
fn real_selected_source_blocks_new_enrollment_even_when_channel_matches() {
    let fixture = tempfile::tempdir().expect("fresh-enrollment source fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let before = record_bytes(&trust);
    let source = select_windows_active_package(&trust).expect("native protected source");
    let (step, _) = refusal(
        &source,
        Some(source.install_identity().channel),
        WindowsRepositoryOperation::FreshEnrollment,
    );
    assert_eq!(step, "repository fresh membership");
    assert_eq!(
        record_bytes(&trust),
        before,
        "fresh refusal must have no effects"
    );
}
