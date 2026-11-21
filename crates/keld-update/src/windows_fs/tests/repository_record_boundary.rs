// Native Windows negative controls for the T82 repository journal grant boundary.
//
// D64 phase replacement and D68 pointer retirement are not commissioned. These
// tests pin the narrower operations that WindowsFS currently permits; they do
// not mint a repository writer, perform retirement or qualify D90's census.

use super::super::{RecordSlot, publish_new};
use super::support::retained_directory;
use std::fs;

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

/// Failure-first mutation: changing publish_new's native replace flag from
/// false to true overwrites the independently planted D64 or D68 bytes and
/// fails these assertions. Source-only until real Windows Cargo is qualified.
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
        fs::write(&prior, b"original durable record").expect("existing protected target fixture");
        fs::write(&sibling, b"new pending record").expect("pending publication fixture");

        let refused = publish_new(&parent, &prepared, &target);
        assert!(
            refused.is_err(),
            "existing {namespace}/{target} must refuse absent-target publication: {refused:?}"
        );
        assert_eq!(
            fs::read(&prior).expect("existing target survives refusal"),
            b"original durable record"
        );
        assert_eq!(
            fs::read(&sibling).expect("unpublished sibling remains diagnostic"),
            b"new pending record"
        );
    }
}
