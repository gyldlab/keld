//! SYSTEM fixture mutations isolate the loader and initial-seed proof predicates.

use std::fs;
use std::path::Path;

use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::wrappers::SetSecurityInfo;
use windows_permissions::{LocalBox, SecurityDescriptor};

use super::support::{self, baseline, dacl_handle, provision};
use crate::windows_baseline::{initialize_windows_baseline, load_windows_baseline};

pub(super) fn set_dacl(path: &Path, text: &str) {
    let descriptor: LocalBox<SecurityDescriptor> = text.parse().expect("independent SDDL mutation");
    let mut object = dacl_handle(path);
    SetSecurityInfo(
        &mut object,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        descriptor.dacl(),
        None,
    )
    .expect("SYSTEM mutates one isolated descriptor");
}

pub(super) fn run(root: &Path) {
    for (label, relative) in [
        ("bad-provenance", "install-provenance"),
        ("bad-floor", "updates/version-floor"),
    ] {
        let trust = provision(root, label);
        drop(
            initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
                .expect("valid complete initial state before one byte mutation"),
        );
        let target = trust.installation.install_root.join(relative);
        let original = fs::read(&target).expect("record bytes before mutation");
        fs::write(&target, b"{}").expect("one corrupt record");
        assert!(
            load_windows_baseline(&trust).is_err(),
            "corrupt protected record must refuse: {label}"
        );
        fs::write(&target, &original).expect("restore exact record bytes");
        drop(load_windows_baseline(&trust).expect("matched restored-record positive control"));
    }
    let trust = provision(root, "bad-file-acl");
    drop(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
            .expect("initial commit"),
    );
    let record = trust.installation.install_root.join("install-provenance");
    set_dacl(
        &record,
        "O:SYD:P(A;;FA;;;SY)(A;;0x1200a9;;;BU)(A;;0x2;;;BU)",
    );
    assert!(
        load_windows_baseline(&trust).is_err(),
        "one extra data-write ACE refuses"
    );
    set_dacl(&record, "O:SYD:P(A;;FA;;;SY)(A;;0x1200a9;;;BU)");
    drop(load_windows_baseline(&trust).expect("exact ACL restoration is positive control"));

    for (label, relative) in [
        ("bad-marker", "updates/versions/1.0.0/.complete"),
        ("bad-tree", "updates/versions/1.0.0/tree/nest/one"),
        ("bad-current", "updates/current"),
        ("bad-lkg", "updates/last-known-good"),
        ("advanced-seed-floor", "updates/version-floor"),
    ] {
        let trust = provision(root, label);
        drop(
            initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
                .expect("initial commit"),
        );
        let target = trust.installation.install_root.join(relative);
        let original = fs::read(&target).expect("initial seed bytes");
        let changed = if label == "bad-tree" {
            let mut bytes = original.clone();
            bytes[0] ^= 1;
            bytes
        } else {
            let text = std::str::from_utf8(&original).expect("canonical UTF-8 record");
            assert_eq!(
                text.matches("1.0.0").count(),
                1,
                "one artifact version field"
            );
            text.replace("1.0.0", "2.0.0").into_bytes()
        };
        assert_eq!(
            changed.len(),
            original.len(),
            "length cannot be the negative oracle"
        );
        assert_ne!(changed, original);
        fs::write(&target, changed).expect("one canonical wrong identity or same-length payload");
        if label == "bad-tree" {
            let loaded = load_windows_baseline(&trust)
                .expect("metadata admission does not rehash runnable payload");
            assert!(
                super::super::load::validate_initial_seed(&loaded.roots).is_err(),
                "full initializer proof must reject same-length payload corruption"
            );
        } else {
            assert!(
                load_windows_baseline(&trust).is_err(),
                "public loader must refuse mixed initial metadata before observation: {label}"
            );
        }
        fs::write(&target, original).expect("restore initial seed");
        let loaded = load_windows_baseline(&trust).expect("protected metadata");
        super::super::load::validate_initial_seed(&loaded.roots)
            .expect("restored seed positive control");
    }

    reject_completion_object_mismatches(root);
    reject_missing_metadata(root);
    reject_unknown_state(root);
    admit_only_named_diagnostic_directories(root);
    reject_parent_delete_child(root);
    println!("KELD_KEL266_SUBSTITUTIONS_PASSED");
}

fn reject_completion_object_mismatches(root: &Path) {
    let trust = provision(root, "completion-objects");
    drop(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
            .expect("committed completion-object control"),
    );
    let version = trust.installation.update_root.join("versions/1.0.0");
    let marker = version.join(".complete");
    let original = fs::read(&marker).expect("original completion marker");
    let complete = crate::records::decode_complete(&original).expect("canonical marker");
    let actual_length = fs::metadata(version.join("content.tar"))
        .expect("actual archive metadata")
        .len();
    assert_eq!(complete.content_size, actual_length);
    assert!(actual_length > 1);
    for wrong_size in [actual_length - 1, actual_length + 1] {
        let changed = crate::records::encode_complete(&complete.artifact, wrong_size)
            .expect("canonical marker with independently wrong archive length");
        fs::write(&marker, changed).expect("change only canonical completion size");
        let refused =
            load_windows_baseline(&trust).expect_err("marker length must match retained archive");
        assert!(matches!(
            refused,
            crate::UpdateError::Baseline {
                step: "completion size",
                ..
            }
        ));
        fs::write(&marker, &original).expect("restore exact completion marker");
        drop(load_windows_baseline(&trust).expect("restored completion size"));
    }
    for name in ["content.tar", "tree"] {
        let target = version.join(name);
        let saved = root.join(format!("saved-completion-{name}"));
        assert!(!saved.exists());
        fs::rename(&target, &saved).expect("retain exact original object outside admitted version");
        if name == "content.tar" {
            drop(
                crate::windows_fs::create_directory_relative(&support::directory(&version), name)
                    .expect("directory substituted for regular archive"),
            );
        } else {
            fs::write(&target, b"not a directory").expect("regular file substituted for tree");
        }
        assert!(
            load_windows_baseline(&trust).is_err(),
            "wrong actual type for {name}"
        );
        if name == "content.tar" {
            fs::remove_dir(&target).expect("remove exact empty substitution");
        } else {
            fs::remove_file(&target).expect("remove exact regular substitution");
        }
        fs::rename(&saved, &target).expect("restore exact protected object");
        drop(load_windows_baseline(&trust).expect("restored object-type positive control"));

        fs::rename(&target, &saved).expect("retain original before reparse substitution");
        if name == "content.tar" {
            std::os::windows::fs::symlink_file(&saved, &target).expect("archive reparse fixture");
        } else {
            std::os::windows::fs::symlink_dir(&saved, &target).expect("tree reparse fixture");
        }
        assert!(
            load_windows_baseline(&trust).is_err(),
            "actual reparse for {name} must refuse"
        );
        if name == "content.tar" {
            fs::remove_file(&target).expect("remove exact archive symlink");
        } else {
            fs::remove_dir(&target).expect("remove exact tree symlink");
        }
        fs::rename(&saved, &target).expect("restore exact original after reparse");
        drop(load_windows_baseline(&trust).expect("restored reparse positive control"));
    }
}

fn reject_missing_metadata(root: &Path) {
    let trust = provision(root, "missing-metadata");
    drop(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
            .expect("committed missing-record control"),
    );
    for (index, relative) in [
        "updates/current",
        "updates/last-known-good",
        "updates/version-floor",
        "updates/versions/1.0.0/.complete",
        "updates/versions/1.0.0",
    ]
    .into_iter()
    .enumerate()
    {
        let target = trust.installation.install_root.join(relative);
        let saved = root.join(format!("saved-metadata-{index}"));
        assert!(
            !saved.exists(),
            "fresh recovery location outside admitted roots"
        );
        fs::rename(&target, &saved).expect("remove exactly one admitted object");
        assert!(
            load_windows_baseline(&trust).is_err(),
            "missing {relative} must refuse"
        );
        fs::rename(&saved, &target).expect("restore exact protected object");
        drop(load_windows_baseline(&trust).expect("restored metadata positive control"));
    }
    let lock = trust.installation.update_root.join("bootstrap.lock");
    let saved = root.join("saved-bootstrap-lock");
    assert!(!saved.exists());
    fs::rename(&lock, &saved).expect("committed bootstrap lock is optional");
    drop(load_windows_baseline(&trust).expect("valid committed metadata without bootstrap lock"));
    fs::rename(&saved, &lock).expect("restore optional lock");
    fs::write(&lock, b"corrupt").expect("corrupt existing optional lock");
    assert!(
        load_windows_baseline(&trust).is_err(),
        "present corrupt lock is not ignored"
    );
    fs::write(&lock, b"").expect("restore exact empty bootstrap marker");
    drop(load_windows_baseline(&trust).expect("restored lock positive control"));
}

fn reject_unknown_state(root: &Path) {
    let trust = provision(root, "unknown-state");
    drop(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
            .expect("committed unknown-state control"),
    );
    for name in ["journal", "previous-known-good", "unknown-state"] {
        let extra = trust.installation.update_root.join(name);
        assert!(!extra.exists());
        fs::write(&extra, b"future state").expect("one unsupported update state object");
        assert!(
            load_windows_baseline(&trust).is_err(),
            "unknown update state {name}"
        );
        fs::remove_file(&extra).expect("remove exact test-created unknown state");
        drop(load_windows_baseline(&trust).expect("restored update census"));
    }
    let versions = trust.installation.update_root.join("versions");
    let final_version =
        crate::windows_fs::create_directory_relative(&support::directory(&versions), "2.0.0")
            .expect("unsupported final-version fixture");
    drop(final_version);
    assert!(
        load_windows_baseline(&trust).is_err(),
        "another final version needs future activation admission"
    );
    fs::remove_dir(versions.join("2.0.0")).expect("remove exact empty final-version fixture");
    drop(load_windows_baseline(&trust).expect("restored final-version census"));
}

fn admit_only_named_diagnostic_directories(root: &Path) {
    let trust = provision(root, "diagnostic-state");
    drop(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
            .expect("committed diagnostic control"),
    );
    let versions = trust.installation.update_root.join("versions");
    for name in [
        "incomplete-".to_owned(),
        format!("incomplete-{}", "a".repeat(63)),
        format!("incomplete-{}", "A".repeat(64)),
        format!("incomplete-{}", "a".repeat(65)),
    ] {
        let directory =
            crate::windows_fs::create_directory_relative(&support::directory(&versions), &name)
                .expect("malformed diagnostic name fixture");
        drop(directory);
        assert!(
            load_windows_baseline(&trust).is_err(),
            "malformed diagnostic name {name}"
        );
        fs::remove_dir(versions.join(&name)).expect("remove exact malformed-name fixture");
        drop(load_windows_baseline(&trust).expect("restored diagnostic census"));
    }
    let name = format!("incomplete-{}", "a".repeat(64));
    let directory =
        crate::windows_fs::create_directory_relative(&support::directory(&versions), &name)
            .expect("private diagnostic directory");
    drop(directory);
    let stage = versions.join(&name);
    fs::write(stage.join("current"), b"untrusted diagnostic contents").expect("diagnostic bytes");
    drop(
        load_windows_baseline(&trust).expect("named diagnostic directory never supplies metadata"),
    );
    fs::remove_file(stage.join("current")).expect("remove exact diagnostic payload");
    fs::remove_dir(&stage).expect("remove empty private diagnostic");
    fs::write(&stage, b"not a directory").expect("valid diagnostic spelling with wrong type");
    assert!(
        load_windows_baseline(&trust).is_err(),
        "diagnostic file must refuse"
    );
    fs::remove_file(&stage).expect("remove exact wrong-type fixture");
    std::os::windows::fs::symlink_dir(root, &stage)
        .expect("native diagnostic reparse fixture requires symlink privilege");
    assert!(
        load_windows_baseline(&trust).is_err(),
        "diagnostic reparse must refuse without following"
    );
    fs::remove_dir(&stage).expect("remove only diagnostic symlink");
    assert!(root.is_dir(), "reparse target is preserved");
    drop(load_windows_baseline(&trust).expect("restored diagnostic positive control"));
}

fn reject_parent_delete_child(root: &Path) {
    // The leaf and its records remain correct. Only an intermediate parent's
    // effective DELETE_CHILD changes. This must fail on every fresh load.
    let parent = support::directory(root);
    let ancestor_path = root.join("parent-delete-child");
    let ancestor = crate::windows_fs::create_directory_relative(&parent, "parent-delete-child")
        .expect("private intermediate fixture");
    keld_guard::seal_windows_machine_directory(&mut dacl_handle(&ancestor_path))
        .expect("protected ancestor");
    drop(ancestor);
    let trust = provision(&ancestor_path, "install");
    drop(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
            .expect("valid protected chain"),
    );
    let empty = crate::windows_fs::create_directory_relative(
        &support::directory(&ancestor_path),
        "empty-protected-child",
    )
    .expect("empty child for matched ordinary-user parent-right control");
    keld_guard::seal_windows_machine_directory(&mut dacl_handle(
        &ancestor_path.join("empty-protected-child"),
    ))
    .expect("child itself grants ordinary callers no delete");
    drop(empty);
    set_dacl(
        &ancestor_path,
        "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)(A;;0x40;;;BU)",
    );
    assert!(
        load_windows_baseline(&trust).is_err(),
        "parent DELETE_CHILD cannot hide behind a protected leaf"
    );
    // Leave this intentionally vulnerable isolated parent for the separate ordinary
    // process's matched deletion control. It is never admitted as an installation.
}
