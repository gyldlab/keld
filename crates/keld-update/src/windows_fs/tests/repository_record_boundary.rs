// Native Windows negative controls for the T82 repository journal grant boundary.
//
// D64 phase replacement and D68 pointer retirement are not commissioned. These
// tests pin the narrower operations that WindowsFS currently permits; they do
// not mint a repository writer, perform retirement or qualify D90's census.

use super::super::{RecordSlot, publish_new, replace_record_slot};
use super::support::retained_directory;
use std::fs;
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS};

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
        publish_new(&parent, &prepared, "absent-publication-control.json")
            .expect("the same retained parent and generated source can publish absent-only");
        assert_eq!(
            fs::read(&control).expect("positive native rename produced its target"),
            b"control record"
        );
        assert!(!sibling.exists(), "positive rename consumed its source");

        fs::write(&prior, b"original durable record").expect("existing protected target fixture");
        fs::write(&sibling, b"new pending record").expect("pending publication fixture");

        let refused = publish_new(&parent, &prepared, &target)
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
