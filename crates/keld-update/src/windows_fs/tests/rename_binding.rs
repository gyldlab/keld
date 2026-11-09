// Responsibility-local Windows native contract tests: rename destination binding.
//
// GH-896 failure-first oracle. The retained parent's pathname is substituted only
// AFTER a pathname-based rename would have resolved its paths, through the
// `cfg(test)` seam in `rename_relative`. An implementation that resolved or rebuilt
// a source or destination pathname before that point (for example
// `GetFinalPathNameByHandleW` plus `MoveFileExW`) follows the planted junction into
// `outside`; only a rename bound to the retained parent HANDLE lands in `moved`.
//
// The seam precedes the source open because NTFS refuses (ERROR_ACCESS_DENIED) to
// rename a directory while any child is open, so the parent cannot be substituted
// once the source handle exists.

use super::super::{
    RecordSlot, before_rename_binding_pending, publish_new, replace_record_slot,
    set_before_rename_binding,
};
use super::support::{
    assert_rename_stayed_in_retained_parent, rename_fixture, substitute_parent_spelling,
};
use std::path::Path;

/// Arms the seam to move the parent and plant the junction mid-rename.
fn substitute_before_rename_binding(original: &Path, moved: &Path, outside: &Path, source: &str) {
    let (original, moved, outside, source) = (
        original.to_path_buf(),
        moved.to_path_buf(),
        outside.to_path_buf(),
        source.to_owned(),
    );
    set_before_rename_binding(move || {
        substitute_parent_spelling(&original, &moved, &outside, &source);
    });
}

#[test]
fn publication_stays_bound_to_retained_parent_when_its_path_changes_mid_rename() {
    let (_fixture, parent, original, moved, outside) =
        rename_fixture("prepared", "published", None);
    substitute_before_rename_binding(&original, &moved, &outside, "prepared");
    let result = publish_new(&parent, "prepared", "published");
    assert!(
        !before_rename_binding_pending(),
        "the mid-rename parent substitution must have run"
    );
    result.expect("publish into the retained parent despite the mid-rename junction");
    assert_rename_stayed_in_retained_parent(&moved, &outside, "prepared", "published");
    drop(parent);
    std::fs::remove_dir(original).expect("remove junction, not its target");
}

#[test]
fn typed_replacement_stays_bound_to_retained_parent_when_its_path_changes_mid_rename() {
    let pending = format!("pending-{}", "a".repeat(64));
    let (_fixture, parent, original, moved, outside) =
        rename_fixture(&pending, "current", Some(b"prior current"));
    substitute_before_rename_binding(&original, &moved, &outside, &pending);
    let result = replace_record_slot(&parent, &pending, RecordSlot::Current);
    assert!(
        !before_rename_binding_pending(),
        "the mid-rename parent substitution must have run"
    );
    result.expect("replace the retained record despite the mid-rename junction");
    assert_rename_stayed_in_retained_parent(&moved, &outside, &pending, "current");
    drop(parent);
    std::fs::remove_dir(original).expect("remove junction, not its target");
}
