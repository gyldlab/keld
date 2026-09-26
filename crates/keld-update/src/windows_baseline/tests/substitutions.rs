//! SYSTEM fixture mutations isolate the loader and initial-seed proof predicates.

use std::fs;
use std::path::Path;

use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::wrappers::SetSecurityInfo;
use windows_permissions::{LocalBox, SecurityDescriptor};

use super::support::{self, baseline, dacl_handle, provision};
use crate::windows_baseline::{initialize_windows_baseline, load_windows_baseline};

fn set_dacl(path: &Path, text: &str) {
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
        let loaded = load_windows_baseline(&trust)
            .expect("metadata loader does not claim active package selection");
        assert!(
            super::super::load::validate_initial_seed(&loaded.roots).is_err(),
            "initial seed predicate: {label}"
        );
        drop(loaded);
        fs::write(&target, original).expect("restore initial seed");
        let loaded = load_windows_baseline(&trust).expect("protected metadata");
        super::super::load::validate_initial_seed(&loaded.roots)
            .expect("restored seed positive control");
    }

    reject_parent_delete_child(root);
    println!("KELD_KEL266_SUBSTITUTIONS_PASSED");
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
