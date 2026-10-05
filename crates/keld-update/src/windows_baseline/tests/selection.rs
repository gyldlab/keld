//! Read-only active package selection (KEL-254 AC4) over real per-user Windows state.
//!
//! Oracles are the selected artifact and canonical tree path, the exact bytes of every
//! protected record before and after selection, typed refusals, and OS sharing behavior
//! of the retained version handles.

use std::collections::BTreeMap;

use super::transaction::{begin, commit, crash_after_publish_pending, retirement};
use super::writer::seed_per_user_baseline;
use crate::records::PointerKind;
use crate::windows_baseline::{
    WindowsBaselineTrust, load_windows_activation_write_snapshot, select_windows_active_package,
};
use crate::{ActivationEffect, ActivationFailureClass, UpdateError};

/// Every regular file directly under the update root, by name, with its exact bytes.
///
/// The empty `activation.lock` is the lease handle's target, not a record, and a held
/// writer lease shares it with nobody, so it is left out.
fn record_bytes(trust: &WindowsBaselineTrust) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(&trust.installation.update_root)
        .expect("update census")
        .map(|entry| entry.expect("update entry").path())
        .filter(|path| path.is_file() && !path.ends_with("activation.lock"))
        .map(|path| {
            (
                path.file_name()
                    .expect("record name")
                    .to_string_lossy()
                    .into_owned(),
                std::fs::read(&path).expect("record bytes"),
            )
        })
        .collect()
}

fn selected_version(trust: &WindowsBaselineTrust) -> String {
    let selection = select_windows_active_package(trust).expect("a committed state selects");
    let version = selection.artifact().version.clone();
    assert_eq!(
        selection.tree_root(),
        trust
            .installation
            .update_root
            .join("versions")
            .join(&version)
            .join("tree"),
        "the selection names exactly the canonical immutable tree"
    );
    assert!(selection.tree_root().is_dir());
    assert_eq!(
        selection.install_identity(),
        &trust.installation,
        "the selection carries the protected installation identity"
    );
    assert_eq!(selection.publisher_scope(), &trust.publisher_scope);
    version
}

/// Points `current` at `artifact`, keeping the record's protected descriptor.
fn write_current(trust: &WindowsBaselineTrust, artifact: &crate::ArtifactIdentity) {
    std::fs::write(
        trust.installation.update_root.join("current"),
        crate::records::encode_pointer(PointerKind::Current, artifact).expect("encode pointer"),
    )
    .expect("rewrite current");
}

fn last_known_good(trust: &WindowsBaselineTrust) -> crate::ArtifactIdentity {
    crate::records::decode_pointer(
        PointerKind::LastKnownGood,
        &std::fs::read(trust.installation.update_root.join("last-known-good")).expect("LKG"),
    )
    .expect("canonical LKG")
}

fn assert_refuses(trust: &WindowsBaselineTrust, why: &str) -> UpdateError {
    let before = record_bytes(trust);
    let error = select_windows_active_package(trust).expect_err(why);
    assert_eq!(
        record_bytes(trust),
        before,
        "{why}: selection writes nothing"
    );
    error
}

#[test]
fn the_installed_baseline_is_selected_before_any_update() {
    let fixture = tempfile::tempdir().expect("baseline selection fixture");
    let trust = seed_per_user_baseline(fixture.path());
    assert_eq!(selected_version(&trust), "1.0.0");
}

#[test]
fn a_committed_update_is_selected_and_nothing_is_written() {
    let fixture = tempfile::tempdir().expect("committed selection fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let before = record_bytes(&trust);
    let selection = select_windows_active_package(&trust).expect("committed state selects");
    assert_eq!(selection.artifact().version, "2.0.0");
    assert_eq!(record_bytes(&trust), before, "selection writes nothing");
    // The snapshot lease closed before return: the writer is admitted while the
    // selection (and its version pins) is still held.
    drop(
        load_windows_activation_write_snapshot(&trust, &super::transaction::verifier(&trust))
            .expect("a held selection never blocks the writer lease"),
    );
    drop(selection);
}

#[test]
fn current_at_previous_known_good_is_selected() {
    let fixture = tempfile::tempdir().expect("previous-known-good selection fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let previous = last_known_good(&trust);
    commit(&trust, "3.0.0");
    write_current(&trust, &previous);
    assert_eq!(selected_version(&trust), "2.0.0");
}

#[test]
fn a_rolled_back_attempt_selects_its_rollback_target() {
    let fixture = tempfile::tempdir().expect("rollback selection fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let attempt = begin(&trust, "3.0.0");
    let proof = retirement(&attempt);
    attempt
        .roll_back(ActivationFailureClass::HealthRejected, &proof)
        .expect("the failed candidate rolls back");
    assert_eq!(selected_version(&trust), "2.0.0");
}

#[test]
fn a_pending_journal_selects_nothing() {
    super::support::assert_user_principal_token();
    let fixture = tempfile::tempdir().expect("pending journal selection fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    crash_after_publish_pending(fixture.path(), &trust);
    match assert_refuses(&trust, "a pending journal must not select a tree") {
        UpdateError::Activation { effect, .. } => {
            assert_eq!(effect, ActivationEffect::JournalBoundRecoveryRequired);
        }
        other => panic!("a pending journal must refuse as journal-bound: {other:?}"),
    }
}

#[test]
fn current_naming_an_artifact_that_is_not_known_good_refuses() {
    let fixture = tempfile::tempdir().expect("unknown current fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let mut foreign = last_known_good(&trust);
    foreign.content_blake3[0] ^= 1;
    write_current(&trust, &foreign);
    let error = assert_refuses(&trust, "current must be a known-good artifact");
    assert!(
        error
            .to_string()
            .contains("current is not a known-good artifact"),
        "{error}"
    );
}

#[test]
fn a_floor_below_the_selected_version_refuses() {
    let fixture = tempfile::tempdir().expect("lowered floor fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    std::fs::write(
        trust.installation.update_root.join("version-floor"),
        crate::records::encode_floor("1.0.0").expect("encode floor"),
    )
    .expect("lower the floor");
    let error = assert_refuses(&trust, "a selected version above the floor refuses");
    assert!(
        error.to_string().contains("above the protected floor"),
        "{error}"
    );
}

#[test]
fn an_unreferenced_or_damaged_version_refuses() {
    let fixture = tempfile::tempdir().expect("version census fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let versions = trust.installation.update_root.join("versions");
    std::fs::create_dir(versions.join("9.9.9")).expect("an unreferenced version entry");
    assert_refuses(&trust, "an orphan version directory refuses");
    std::fs::remove_dir(versions.join("9.9.9")).expect("remove the orphan");
    assert_eq!(selected_version(&trust), "2.0.0");

    std::fs::remove_file(versions.join("2.0.0").join(".complete"))
        .expect("damage the selected version");
    assert_refuses(
        &trust,
        "a selected version without its completion record refuses",
    );
}

#[test]
fn a_damaged_known_good_version_refuses_even_when_current_is_sound() {
    let fixture = tempfile::tempdir().expect("damaged known-good fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    // current = last-known-good = 2.0.0; previous-known-good = 1.0.0.
    std::fs::remove_file(
        trust
            .installation
            .update_root
            .join("versions")
            .join("1.0.0")
            .join(".complete"),
    )
    .expect("damage the previous-known-good version");
    assert_refuses(
        &trust,
        "both known-good slots must pass metadata admission before any tree is selected",
    );
}

#[test]
fn a_writer_holding_the_lease_blocks_selection() {
    let fixture = tempfile::tempdir().expect("busy lease fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let writer =
        load_windows_activation_write_snapshot(&trust, &super::transaction::verifier(&trust))
            .expect("hold the exclusive writer lease");
    match assert_refuses(&trust, "a held writer lease refuses selection") {
        UpdateError::Activation { effect, .. } => {
            assert_eq!(effect, ActivationEffect::WriterActive);
        }
        other => panic!("a busy lease must refuse as a typed writer-active effect: {other:?}"),
    }
    drop(writer);
    assert_eq!(selected_version(&trust), "1.0.0");
}

#[test]
fn a_held_selection_pins_its_version_against_retirement() {
    let fixture = tempfile::tempdir().expect("selection pin fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let versions = trust.installation.update_root.join("versions");
    let selection = select_windows_active_package(&trust).expect("select 2.0.0");
    let retired = versions.join(format!("retired-{}", "d".repeat(64)));
    assert!(
        std::fs::rename(versions.join("2.0.0"), &retired).is_err(),
        "a running selection's version cannot be renamed away"
    );
    drop(selection);
    std::fs::rename(versions.join("2.0.0"), &retired)
        .expect("the version is movable once the selection is released");
}

#[test]
fn a_held_selection_pins_its_protected_ancestry() {
    let fixture = tempfile::tempdir().expect("ancestry pin fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let update_root = &trust.installation.update_root;
    let moved = update_root.with_file_name("moved-update-root");
    let selection = select_windows_active_package(&trust).expect("select the baseline");
    assert!(
        std::fs::rename(update_root, &moved).is_err(),
        "the update root above a running selection cannot be renamed"
    );
    drop(selection);
    std::fs::rename(update_root, &moved)
        .expect("the update root is movable once the selection is released");
}

#[test]
fn concurrent_snapshot_readers_share_the_lease() {
    let fixture = tempfile::tempdir().expect("shared reader fixture");
    let trust = seed_per_user_baseline(fixture.path());
    // The baseline loader keeps its shared snapshot lease for its whole lifetime.
    let reader = crate::windows_baseline::load_windows_baseline(&trust)
        .expect("hold a shared snapshot lease");
    assert_eq!(
        selected_version(&trust),
        "1.0.0",
        "a second snapshot reader is admitted beside the first"
    );
    drop(reader);
}

#[test]
fn an_absent_or_changed_package_policy_refuses() {
    let fixture = tempfile::tempdir().expect("package policy fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let policy = |version: &str| {
        trust
            .installation
            .update_root
            .join("versions")
            .join(version)
            .join("tree")
            .join(keld_pack::UPDATE_POLICY_PATH)
    };
    let original = std::fs::read(policy("2.0.0")).expect("selected policy bytes");
    assert_eq!(original, keld_pack::NO_MIGRATION_POLICY);
    std::fs::write(
        policy("2.0.0"),
        b"{\"schema\":1,\"dataMigration\":\"copy\"}\n",
    )
    .expect("change the selected tree's policy");
    let error = assert_refuses(&trust, "a changed policy refuses before launch");
    assert!(error.to_string().contains("package policy"), "{error}");
    std::fs::write(policy("2.0.0"), &original).expect("restore the selected policy");
    assert_eq!(selected_version(&trust), "2.0.0");

    std::fs::remove_file(policy("1.0.0")).expect("drop the previous-known-good policy");
    assert_refuses(&trust, "an absent policy in a known-good slot refuses");
}

#[test]
fn generated_leftovers_never_change_the_selection() {
    let fixture = tempfile::tempdir().expect("generated leftovers fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let update = &trust.installation.update_root;
    let versions = update.join("versions");
    std::fs::create_dir(versions.join(format!("incomplete-{}", "e".repeat(64))))
        .expect("an incomplete diagnostic stage");
    std::fs::create_dir(versions.join(format!("retired-{}", "f".repeat(64))))
        .expect("a retired tree awaiting deletion");
    std::fs::write(update.join(format!("pending-{}", "a".repeat(64))), b"stale")
        .expect("a stale record preparation");
    assert_eq!(selected_version(&trust), "2.0.0");
}

fn current_record(trust: &WindowsBaselineTrust) -> Option<Vec<u8>> {
    std::fs::read(trust.installation.update_root.join("current")).ok()
}

fn lkg_pointer_as_current(trust: &WindowsBaselineTrust) -> Vec<u8> {
    crate::records::encode_pointer(PointerKind::Current, &last_known_good(trust))
        .expect("encode the last-known-good pointer")
}

#[test]
fn an_absent_current_is_republished_from_last_known_good() {
    let fixture = tempfile::tempdir().expect("absent current fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    std::fs::remove_file(trust.installation.update_root.join("current"))
        .expect("lose the current record");
    assert_eq!(selected_version(&trust), "2.0.0");
    assert_eq!(
        current_record(&trust),
        Some(lkg_pointer_as_current(&trust)),
        "the repair durably republishes last-known-good as current"
    );
}

#[test]
fn an_undecodable_current_is_republished_from_last_known_good() {
    let fixture = tempfile::tempdir().expect("undecodable current fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    std::fs::write(
        trust.installation.update_root.join("current"),
        b"not a pointer",
    )
    .expect("corrupt the current record");
    let pending = trust
        .installation
        .update_root
        .join(format!("pending-{}", "b".repeat(64)));
    std::fs::write(&pending, b"stale").expect("a stale record preparation");
    assert_eq!(selected_version(&trust), "2.0.0");
    assert_eq!(current_record(&trust), Some(lkg_pointer_as_current(&trust)));
    assert!(
        !pending.exists(),
        "the repair removes stale preparations first"
    );
}

#[test]
fn an_invalid_current_is_not_repaired_from_a_damaged_last_known_good() {
    let fixture = tempfile::tempdir().expect("damaged LKG fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    std::fs::write(
        trust.installation.update_root.join("current"),
        b"not a pointer",
    )
    .expect("corrupt the current record");
    std::fs::remove_file(
        trust
            .installation
            .update_root
            .join("versions")
            .join("2.0.0")
            .join(".complete"),
    )
    .expect("damage last-known-good");
    assert_refuses(&trust, "a damaged last-known-good never becomes current");
    assert_eq!(
        current_record(&trust).as_deref(),
        Some(&b"not a pointer"[..])
    );
}

#[test]
fn an_invalid_current_beside_a_pending_journal_stays_journal_bound() {
    super::support::assert_user_principal_token();
    let fixture = tempfile::tempdir().expect("invalid current with journal fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    crash_after_publish_pending(fixture.path(), &trust);
    std::fs::write(
        trust.installation.update_root.join("current"),
        b"not a pointer",
    )
    .expect("corrupt the current record");
    match assert_refuses(&trust, "a journal outranks the current repair") {
        UpdateError::Activation { effect, .. } => {
            assert_eq!(effect, ActivationEffect::JournalBoundRecoveryRequired);
        }
        other => panic!("a pending journal must refuse as journal-bound: {other:?}"),
    }
}
