//! Native generated pending-record preparation and slot replacement contracts.
//! Real Windows filesystem admission, protected files and refusal effects remain observable.

use super::{seed_per_user_baseline, support, verifier};
use std::io::Write as _;

#[test]
fn record_replacement_requires_a_generated_sibling_and_a_file_destination() {
    use crate::windows_fs::{RecordSlot, replace_record_slot};
    let fixture = tempfile::tempdir().expect("record slot fixture");
    let path = fixture.path();
    let pending = format!("pending-{}", "a".repeat(64));
    std::fs::write(path.join("current"), b"previous").expect("existing slot");
    std::fs::write(path.join("last-known-good"), b"known-good").expect("another slot");
    std::fs::write(path.join("install-provenance"), b"protected").expect("provenance");
    std::fs::write(path.join(&pending), b"replacement").expect("prepared sibling");
    std::fs::create_dir(path.join("version-floor")).expect("directory at a slot name");
    let parent = support::directory(path);

    for source in ["last-known-good", "install-provenance", "pending-short"] {
        assert!(
            replace_record_slot(&parent, source, RecordSlot::Current).is_err(),
            "{source} is not a generated pending sibling"
        );
    }
    assert!(
        replace_record_slot(&parent, &pending, RecordSlot::Floor).is_err(),
        "typed handle-relative replacement refuses a directory destination"
    );
    for (leaf, bytes) in [
        ("current", &b"previous"[..]),
        ("last-known-good", b"known-good"),
        ("install-provenance", b"protected"),
        (pending.as_str(), b"replacement"),
    ] {
        assert_eq!(
            std::fs::read(path.join(leaf)).expect("record after refusals"),
            bytes,
            "{leaf} is untouched by every refused replacement"
        );
    }
    assert!(path.join("version-floor").is_dir());

    replace_record_slot(&parent, &pending, RecordSlot::Current)
        .expect("a generated sibling replaces an existing fixed file slot");
    assert_eq!(
        std::fs::read(path.join("current")).expect("replaced slot"),
        b"replacement"
    );
    assert!(!path.join(&pending).exists());
}

/// The admission refusals and removal are observable; binding admission and deletion to
/// one handle is a construction property, because no deterministic swap fits between them.
#[test]
fn stale_record_preparations_refuse_links_and_directories_and_remove_genuine_files() {
    let profile = keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;
    for case in ["genuine", "linked", "directory"] {
        let fixture = tempfile::tempdir().expect("stale preparation fixture");
        let elsewhere = tempfile::tempdir().expect("same-volume link fixture");
        let trust = seed_per_user_baseline(fixture.path());
        let update = trust.installation.update_root.clone();
        let leaf = format!("pending-{}", "b".repeat(64));
        if case == "directory" {
            std::fs::create_dir(update.join(&leaf)).expect("directory at a pending name");
        } else {
            let parent = support::directory(&update);
            let mut file =
                crate::windows_fs::create_file_relative_with_profile(&parent, &leaf, profile)
                    .expect("profiled preparation");
            file.write_all(b"prepared").expect("prepared bytes");
            drop(file);
            if case == "linked" {
                std::fs::hard_link(update.join(&leaf), elsewhere.path().join("second-link"))
                    .expect("second link outside the update root");
            }
        }
        let result = crate::repair_windows_unjournaled_versions(&trust, &verifier(&trust));
        if case == "genuine" {
            result.expect("a genuine preparation is removed");
            assert!(!update.join(&leaf).exists());
        } else {
            let error = result.expect_err("a non-genuine preparation must refuse");
            assert!(
                format!("{error:?}").contains("stale record admission"),
                "{case} refuses at admission, not elsewhere: {error:?}"
            );
            assert!(
                update.join(&leaf).exists(),
                "{case} preparation stays for manual recovery"
            );
        }
    }
}
