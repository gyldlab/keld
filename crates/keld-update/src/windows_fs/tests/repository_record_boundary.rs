// Native Windows negative controls for the T82 repository journal grant boundary.
//
// D64 phase replacement and D68 pointer retirement are not commissioned. These
// tests pin the narrower operations that WindowsFS currently permits; they do
// not mint a repository writer, perform retirement or qualify D90's census.

use super::super::{
    RecordSlot, publish_new, publish_new_record_file, replace_record_slot,
    set_after_rename_source_kind,
};
use super::support::{junction, retained_directory};
use std::cell::Cell;
use std::fs::{self, OpenOptions};
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::rc::Rc;
use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_SHARING_VIOLATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_WRITE_ATTRIBUTES,
};

/// The Rust type is exhaustive: adding any slot requires review of this
/// activation-only contract before a new replacement target can compile.
fn activation_slot_contract(slot: RecordSlot) -> &'static str {
    match slot {
        RecordSlot::Journal => "activation-journal",
        RecordSlot::Floor => "version-floor",
        RecordSlot::Current => "current",
        RecordSlot::LastKnownGood => "last-known-good",
        RecordSlot::PreviousKnownGood => "previous-known-good",
    }
}

#[test]
fn existing_fixed_replacement_slots_cannot_name_repository_journals() {
    let d64 = format!("{}.json", "2a".repeat(32));
    let d68 = "pending-operation.json";
    for slot in [
        RecordSlot::Journal,
        RecordSlot::Floor,
        RecordSlot::Current,
        RecordSlot::LastKnownGood,
        RecordSlot::PreviousKnownGood,
    ] {
        assert_eq!(slot.leaf(), activation_slot_contract(slot));
        assert_ne!(slot.leaf(), d64.as_str());
        assert_ne!(slot.leaf(), d68);
    }
}

/// Failure-first mutation: changing `publish_new`'s native replace flag from
/// false to true overwrites the independently planted D64 or D68 bytes and
/// fails these assertions. The successful control uses the same native parent
/// and generated sibling name, so unrelated early IO denial cannot pass the
/// collision assertion. Mutation RED remains unrun until Windows qualifies.
#[test]
fn absent_publication_never_replaces_existing_d64_or_d68_record() {
    for (namespace, target) in [
        ("admissions", format!("{}.json", "2a".repeat(32))),
        ("repository", "pending-operation.json".to_owned()),
    ] {
        let fixture = tempfile::tempdir().expect("isolated repository record fixture");
        let directory = fixture.path().join(namespace);
        fs::create_dir(&directory).expect("operation directory");
        let parent = retained_directory(&directory);
        let prior = directory.join(&target);
        let prepared = format!("pending-{}", "ab".repeat(32));
        let sibling = directory.join(&prepared);
        let control = directory.join("absent-publication-control.json");
        assert!(!control.exists(), "control target starts absent");
        fs::write(&sibling, b"control record").expect("valid pending source fixture");
        publish_new_record_file(&parent, &prepared, "absent-publication-control.json")
            .expect("the same retained parent and generated source can publish absent-only");
        assert_eq!(
            fs::read(&control).expect("positive native rename produced its target"),
            b"control record"
        );
        assert!(!sibling.exists(), "positive rename consumed its source");

        fs::write(&prior, b"original durable record").expect("existing protected target fixture");
        fs::write(&sibling, b"new pending record").expect("pending publication fixture");

        let refused = publish_new_record_file(&parent, &prepared, &target)
            .expect_err("native absent-target rename must refuse an existing destination");
        assert!(
            matches!(
                refused.raw_os_error(),
                Some(code)
                    if code == ERROR_ALREADY_EXISTS.cast_signed()
                        || code == ERROR_FILE_EXISTS.cast_signed()
            ),
            "expected native destination collision for {namespace}/{target}, got {refused:?} (raw OS code {:?})",
            refused.raw_os_error()
        );
        assert_eq!(
            fs::read(&prior).expect("existing target survives refusal"),
            b"original durable record"
        );
        assert_eq!(
            fs::read(&sibling).expect("unpublished sibling remains diagnostic"),
            b"new pending record"
        );
        assert_eq!(
            fs::read(&control).expect("successful control target remains intact"),
            b"control record"
        );
    }
}

/// A D64 journal or D68 pointer is a retained repository record, never the
/// generated temporary sibling accepted by the activation replacement API.
///
/// Negative control: removing the generated-source check in
/// `replace_record_slot` permits the native rename to consume one of these
/// records and replace the independently planted activation slot.
#[test]
fn repository_records_cannot_be_activation_replacement_sources() {
    for (namespace, repository_leaf) in [
        ("admissions", format!("{}.json", "2a".repeat(32))),
        ("repository", "pending-operation.json".to_owned()),
    ] {
        let fixture = tempfile::tempdir().expect("isolated repository record fixture");
        let directory = fixture.path().join(namespace);
        fs::create_dir(&directory).expect("isolated retained directory");
        let parent = retained_directory(&directory);
        let repository_record = directory.join(&repository_leaf);
        let activation_record = directory.join(RecordSlot::Journal.leaf());
        fs::write(&repository_record, b"frozen repository evidence")
            .expect("persist original D64 or D68 evidence");
        fs::write(&activation_record, b"unrelated activation state")
            .expect("persist independent target");

        let refused = replace_record_slot(&parent, &repository_leaf, RecordSlot::Journal)
            .expect_err("D64/D68 cannot be a prepared activation replacement sibling");
        assert_eq!(
            refused.kind(),
            std::io::ErrorKind::Unsupported,
            "reject the foreign source type before any native rename"
        );
        assert_eq!(
            fs::read(&repository_record).expect("retained D64/D68 evidence"),
            b"frozen repository evidence"
        );
        assert_eq!(
            fs::read(&activation_record).expect("unmodified activation slot"),
            b"unrelated activation state"
        );
    }
}

/// Independent native OS oracle: the pending name is a real directory or
/// junction. Without the source-kind check, an absent target can be renamed
/// into that protected record slot. The target must remain absent here.
#[test]
fn prepared_repository_record_never_publishes_directory_or_junction_source() {
    for linked in [false, true] {
        let fixture = tempfile::tempdir().expect("isolated native repository fixture");
        let directory = fixture.path().join("repository");
        fs::create_dir(&directory).expect("repository fixture parent");
        let parent = retained_directory(&directory);
        let prepared = format!("pending-{}", "7c".repeat(32));
        let source = directory.join(&prepared);
        let target = directory.join("pending-operation.json");
        let outside = fixture.path().join("outside");
        fs::create_dir(&outside).expect("independent junction target");
        fs::write(outside.join("sentinel"), b"unrelated target data")
            .expect("independent outside data");
        if linked {
            junction(&source, &outside);
            assert_ne!(
                fs::symlink_metadata(&source)
                    .expect("native junction metadata")
                    .file_attributes()
                    & FILE_ATTRIBUTE_REPARSE_POINT,
                0,
                "the negative source must be a real Windows reparse point"
            );
        } else {
            fs::create_dir(&source).expect("native directory source");
            assert!(
                fs::symlink_metadata(&source)
                    .expect("native directory metadata")
                    .is_dir()
            );
        }

        let refused = publish_new_record_file(&parent, &prepared, "pending-operation.json")
            .expect_err("a nonregular source cannot be published as a D68 pointer");
        assert_eq!(refused.kind(), std::io::ErrorKind::Unsupported);
        assert!(source.exists(), "rejected source must remain named");
        assert!(!target.exists(), "no protected pointer may be created");
        assert_eq!(
            fs::read(outside.join("sentinel")).expect("independent data remains"),
            b"unrelated target data"
        );
        if linked {
            let denied = publish_new(&parent, &prepared, "published-version")
                .expect_err("even directory publication must reject a reparse entry");
            assert_eq!(denied.kind(), std::io::ErrorKind::Unsupported);
            assert!(!directory.join("published-version").exists());
        }
        drop(parent);
        if linked {
            fs::remove_dir(&source).expect("remove only the fixture junction");
        }
    }
}

#[test]
fn real_completed_version_directory_still_publishes_under_retained_parent() {
    let fixture = tempfile::tempdir().expect("real version directory fixture");
    let parent = retained_directory(fixture.path());
    let staged = fixture.path().join("incomplete-version");
    fs::create_dir(&staged).expect("create completed stage directory");
    fs::write(staged.join("content"), b"protected package bytes")
        .expect("stage content stays inside the source");

    publish_new(&parent, "incomplete-version", "1.0.0")
        .expect("native directory publication must remain supported");
    assert!(!staged.exists(), "source directory name was consumed");
    assert_eq!(
        fs::read(fixture.path().join("1.0.0").join("content"))
            .expect("published version still contains the original bytes"),
        b"protected package bytes"
    );
}

#[test]
fn directory_publication_refuses_a_regular_file_as_a_version_tree() {
    let fixture = tempfile::tempdir().expect("real directory-type refusal fixture");
    let parent = retained_directory(fixture.path());
    let source = fixture.path().join("incomplete-version");
    let target = fixture.path().join("1.0.0");
    fs::write(&source, b"ordinary file not a version tree")
        .expect("create real file where directory is required");

    let error = publish_new(&parent, "incomplete-version", "1.0.0")
        .expect_err("a file cannot enter the protected version-directory namespace");
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    assert!(!target.exists());
    assert_eq!(
        fs::read(source).expect("preserve unrelated file"),
        b"ordinary file not a version tree"
    );
}

/// A source-kind check without write-sharing exclusion would be stale by the
/// time NtSetInformationFile uses the original handle. Mutation: change the
/// source open from Admit to Remove. The concurrent FILE_WRITE_ATTRIBUTES
/// opener succeeds and this test becomes RED at the actual OS sharing result.
#[test]
fn native_record_source_remains_write_exclusive_after_kind_validation() {
    let fixture = tempfile::tempdir().expect("native rename source custody fixture");
    let parent = retained_directory(fixture.path());
    let source = fixture.path().join("pending-verified-record");
    let target = fixture.path().join("published-record");
    fs::write(&source, b"verified record bytes").expect("one real prepared record");
    let observed = Rc::new(Cell::new(false));
    let saw_open = Rc::clone(&observed);
    let attacked_source = source.clone();
    set_after_rename_source_kind(move || {
        let denied = OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(&attacked_source)
            .expect_err("source handle must deny a concurrent metadata writer");
        assert_eq!(
            denied.raw_os_error(),
            Some(ERROR_SHARING_VIOLATION.cast_signed()),
            "the actual OS share check must deny metadata mutation during the kind-to-rename cut"
        );
        saw_open.set(true);
    });
    publish_new_record_file(&parent, "pending-verified-record", "published-record")
        .expect("the original pinned record remains publishable");
    assert!(
        observed.get(),
        "the probe ran while the source handle was retained"
    );
    assert_eq!(
        fs::read(&target).expect("read the published record"),
        b"verified record bytes"
    );
    let _after = OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(&target)
        .expect("the same test-owned file allows attribute writes after rename and handle drop");
}

#[test]
fn replacement_must_not_promote_reparse_source_to_activation_record() {
    let fixture = tempfile::tempdir().expect("isolated native replacement fixture");
    let parent = retained_directory(fixture.path());
    let pending = format!("pending-{}", "48".repeat(32));
    let source = fixture.path().join(&pending);
    let outside = fixture.path().join("outside");
    fs::create_dir(&outside).expect("independent junction target");
    fs::write(outside.join("sentinel"), b"preserve").expect("outside marker");
    junction(&source, &outside);
    assert_ne!(
        fs::symlink_metadata(&source)
            .expect("native junction metadata")
            .file_attributes()
            & FILE_ATTRIBUTE_REPARSE_POINT,
        0
    );
    let target = fixture.path().join(RecordSlot::Journal.leaf());
    assert!(!target.exists());

    let refused = replace_record_slot(&parent, &pending, RecordSlot::Journal)
        .expect_err("replacement cannot install a junction as the activation journal");
    assert_eq!(refused.kind(), std::io::ErrorKind::Unsupported);
    assert!(
        fs::symlink_metadata(&source).is_ok(),
        "junction remains named"
    );
    assert!(!target.exists(), "activation journal remains absent");
    assert_eq!(
        fs::read(outside.join("sentinel")).expect("outside data stays intact"),
        b"preserve"
    );

    drop(parent);
    fs::remove_dir(&source).expect("remove only the fixture junction");
    fs::write(&source, b"real prepared record").expect("regular source control");
    let parent = retained_directory(fixture.path());
    replace_record_slot(&parent, &pending, RecordSlot::Journal)
        .expect("regular pending record can still replace its exact slot");
    assert_eq!(
        fs::read(&target).expect("positive control published the record"),
        b"real prepared record"
    );
}
