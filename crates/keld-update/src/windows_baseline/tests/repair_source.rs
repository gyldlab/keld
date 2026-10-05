//! The read-only validation of last-known-good as the source that replaces an invalid
//! `current` (KEL-254 AC4 and AC16 "valid last-known-good"; KEL-270 T4d slice S2).
//!
//! The per-user startup repair runs it under the exclusive writer lease before any write,
//! and a `MachineUacDirect` startup runs it under the snapshot lease before an invalid
//! `current` may become the typed recovery-required state. Oracles are the exact landed
//! refusal of the per-user repair for each damaged check, pinned before the validation was
//! extracted, and the exact bytes of every protected record before and after.

use std::fs;

use super::locate::state;
use super::recovery_required::per_user_host_install;
use super::support::host_package_content;
use super::transaction::commit_with;
use crate::UpdateError;
use crate::records::PointerKind;
use crate::windows_baseline::load::validate_current_repair_source;
use crate::windows_baseline::{WindowsBaselineTrust, select_windows_active_package};

const UNDECODABLE: &[u8] = b"not a pointer";
/// The landed refusal of a known-good version whose `.complete` record is gone.
pub(super) const COMPLETION_RECORD_REMOVED: &str = "unexpected state: wanted {\".complete\", \"content.tar\", \"tree\"}, found {\"content.tar\", \"tree\"}";

fn landed(step: &'static str, detail: &str) -> UpdateError {
    UpdateError::Baseline {
        step,
        detail: detail.to_owned(),
    }
}

/// One labelled damage to an installation and the exact refusal it must produce.
type Damage = (&'static str, fn(&WindowsBaselineTrust), UpdateError);

/// One damage per read-only check, after a committed update (last-known-good `2.0.0`,
/// previous-known-good `1.0.0`, floor `2.0.0`), with the exact refusal the landed per-user
/// repair returns for it.
fn damaged_last_known_good() -> [Damage; 4] {
    [
        (
            "floor below last-known-good",
            |trust| {
                fs::write(
                    trust.installation.update_root.join("version-floor"),
                    crate::records::encode_floor("1.0.0").expect("encode floor"),
                )
                .expect("lower the floor");
            },
            landed(
                "activation pointer version",
                "selected version is above the protected floor",
            ),
        ),
        (
            "unreferenced version",
            |trust| {
                fs::create_dir(
                    trust
                        .installation
                        .update_root
                        .join("versions")
                        .join("9.9.9"),
                )
                .expect("an unreferenced version entry");
            },
            landed(
                "activation versions",
                "unreferenced version entry \"9.9.9\" requires recovery",
            ),
        ),
        (
            "completion record removed",
            |trust| {
                fs::remove_file(
                    trust
                        .installation
                        .update_root
                        .join("versions")
                        .join("2.0.0")
                        .join(".complete"),
                )
                .expect("damage last-known-good");
            },
            landed("version contents", COMPLETION_RECORD_REMOVED),
        ),
        (
            "package policy changed",
            |trust| {
                fs::write(
                    trust
                        .installation
                        .update_root
                        .join("versions")
                        .join("2.0.0")
                        .join("tree")
                        .join(keld_pack::UPDATE_POLICY_PATH),
                    b"{\"schema\":1,\"dataMigration\":\"copy\"}\n",
                )
                .expect("change the last-known-good policy");
            },
            landed(
                "package policy",
                "the tree's update policy differs from the signed no-migration policy",
            ),
        ),
    ]
}

/// A per-user installation with one committed update, ready for one damage.
fn committed_install(fixture: &std::path::Path) -> WindowsBaselineTrust {
    let trust = per_user_host_install(fixture);
    commit_with(&trust, "2.0.0", &host_package_content());
    trust
}

/// Runs the shared validation over the protected records as they are on disk, decoded by
/// the canonical codecs independently of the loader.
fn validate_on_disk(trust: &WindowsBaselineTrust) -> Result<(), UpdateError> {
    let update = &trust.installation.update_root;
    let read = |leaf: &str| fs::read(update.join(leaf)).expect("protected record bytes");
    let floor = crate::records::decode_floor(&read("version-floor")).expect("canonical floor");
    let last_known_good =
        crate::records::decode_pointer(PointerKind::LastKnownGood, &read("last-known-good"))
            .expect("canonical last-known-good");
    let previous_known_good = update.join("previous-known-good").is_file().then(|| {
        crate::records::decode_pointer(PointerKind::PreviousKnownGood, &read("previous-known-good"))
            .expect("canonical previous-known-good")
    });
    let roots = crate::windows_baseline::open_roots(trust, false).expect("per-user roots");
    validate_current_repair_source(
        &trust.installation.baseline,
        &roots,
        &floor,
        &last_known_good,
        previous_known_good.as_ref(),
    )
}

#[test]
fn the_shared_validation_admits_a_valid_last_known_good_and_refuses_each_damage() {
    let fixture = tempfile::tempdir().expect("baseline-only fixture");
    let baseline_only = per_user_host_install(fixture.path());
    validate_on_disk(&baseline_only).expect("the installed baseline is a valid source");
    let fixture = tempfile::tempdir().expect("committed fixture");
    let committed = committed_install(fixture.path());
    let before = state(&committed);
    validate_on_disk(&committed).expect("a committed last-known-good is a valid source");
    assert_eq!(state(&committed), before, "the validation writes nothing");

    for (label, damage, expected) in damaged_last_known_good() {
        let fixture = tempfile::tempdir().expect("damaged last-known-good fixture");
        let trust = committed_install(fixture.path());
        damage(&trust);
        let before = state(&trust);
        assert_eq!(
            validate_on_disk(&trust),
            Err(expected),
            "{label}: the landed refusal is kept"
        );
        assert_eq!(
            state(&trust),
            before,
            "{label}: the validation writes nothing"
        );
    }
}

#[test]
fn the_per_user_repair_refuses_each_damaged_last_known_good_with_its_landed_error() {
    for (label, damage, expected) in damaged_last_known_good() {
        let fixture = tempfile::tempdir().expect("damaged last-known-good fixture");
        let trust = committed_install(fixture.path());
        damage(&trust);
        fs::write(trust.installation.update_root.join("current"), UNDECODABLE)
            .expect("corrupt the current record");
        let before = state(&trust);
        let error = select_windows_active_package(&trust)
            .expect_err("a damaged last-known-good never becomes current");
        assert_eq!(error, expected, "{label}: {error}");
        assert_eq!(state(&trust), before, "{label}: the refusal writes nothing");
    }
}
