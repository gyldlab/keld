// Responsibility-local Windows native contract tests: deletion.

use super::super::{
    DeletePurpose, EntryKind, open_for_delete, remove_directory_tree, remove_file_relative,
};
use super::support::{junction, retained_directory};
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::AsRawHandle as _;
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_DISPOSITION_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileDispositionInfo, SetFileInformationByHandle,
};

#[test]
fn file_deletion_is_relative_exact_and_refuses_other_kinds() {
    let fixture = tempfile::tempdir().expect("fixture");
    let parent = retained_directory(fixture.path());
    let absent = remove_file_relative(&parent, "absent").expect_err("absent name");
    assert_eq!(absent.kind(), std::io::ErrorKind::NotFound, "{absent}");
    let file = fixture.path().join("pending-file");
    std::fs::write(&file, b"record").expect("fixture file");
    let mut permissions = std::fs::metadata(&file).expect("metadata").permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&file, permissions).expect("read-only fixture");
    remove_file_relative(&parent, "pending-file").expect("delete a read-only file");
    assert!(!file.exists());
    std::fs::create_dir(fixture.path().join("pending-dir")).expect("fixture directory");
    assert!(remove_file_relative(&parent, "pending-dir").is_err());
    assert!(fixture.path().join("pending-dir").is_dir());
    assert!(
        remove_file_relative(&parent, "a/b").is_err(),
        "one component only"
    );
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&outside).expect("outside directory");
    junction(&fixture.path().join("pending-link"), &outside);
    assert!(remove_file_relative(&parent, "pending-link").is_err());
    assert!(std::fs::symlink_metadata(fixture.path().join("pending-link")).is_ok());
    std::fs::remove_dir(fixture.path().join("pending-link")).expect("remove junction");
}

#[test]
fn tree_removal_deletes_nested_read_only_and_multi_buffer_entries() {
    let fixture = tempfile::tempdir().expect("fixture");
    let tree = fixture.path().join("retired-tree");
    let mut nested = tree.clone();
    for depth in 0..5 {
        nested = nested.join(format!("level-{depth}"));
        std::fs::create_dir_all(&nested).expect("nested directory");
        std::fs::write(nested.join("payload"), b"payload").expect("nested file");
    }
    let readonly = tree.join("level-0").join("payload");
    let mut permissions = std::fs::metadata(&readonly)
        .expect("metadata")
        .permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&readonly, permissions).expect("read-only fixture");
    // Long names push the listing past one 64 KiB enumeration buffer.
    for index in 0..700 {
        std::fs::write(tree.join(format!("{index:04}-{}", "x".repeat(96))), b"")
            .expect("wide directory entry");
    }
    let parent = retained_directory(fixture.path());
    remove_directory_tree(&parent, "retired-tree").expect("remove tree through handles");
    assert!(!tree.exists());
    let absent = remove_directory_tree(&parent, "retired-tree").expect_err("absent tree");
    assert_eq!(absent.kind(), std::io::ErrorKind::NotFound, "{absent}");
}

#[test]
fn tree_removal_deletes_links_and_never_their_targets() {
    let fixture = tempfile::tempdir().expect("fixture");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&outside).expect("outside directory");
    std::fs::write(outside.join("sentinel"), b"unchanged").expect("outside sentinel");
    let tree = fixture.path().join("retired-tree");
    std::fs::create_dir_all(tree.join("nested")).expect("tree");
    junction(&tree.join("link"), &outside);
    junction(&tree.join("nested").join("link"), &outside);
    junction(&fixture.path().join("retired-link"), &outside);
    // Positive control: the links really resolve to the outside object.
    assert_eq!(
        std::fs::read(tree.join("link").join("sentinel")).expect("through link"),
        b"unchanged"
    );
    let parent = retained_directory(fixture.path());
    let link = open_for_delete(&parent, "retired-link", DeletePurpose::Remove)
        .expect("open the link itself");
    assert_eq!(link.kind(), EntryKind::Link);
    drop(link);
    remove_directory_tree(&parent, "retired-tree").expect("remove tree with links");
    assert!(
        remove_directory_tree(&parent, "retired-link").is_err(),
        "a link root is not a directory tree"
    );
    assert!(!tree.exists());
    assert!(std::fs::symlink_metadata(fixture.path().join("retired-link")).is_ok());
    assert_eq!(
        std::fs::read(outside.join("sentinel")).expect("outside sentinel"),
        b"unchanged"
    );
    assert_eq!(
        std::fs::read_dir(&outside)
            .expect("outside listing")
            .count(),
        1
    );
    std::fs::remove_dir(fixture.path().join("retired-link")).expect("remove junction");
}

/// Guards against regressions to stored or recomputed paths. The race between a
/// path resolution and its deletion is not reproducible deterministically.
#[test]
fn tree_removal_acts_on_the_retained_parent_not_a_stored_path() {
    let fixture = tempfile::tempdir().expect("fixture");
    let original = fixture.path().join("versions");
    let moved = fixture.path().join("moved");
    let impostor = fixture.path().join("impostor");
    std::fs::create_dir_all(original.join("retired-tree")).expect("retained tree");
    std::fs::write(original.join("retired-tree").join("payload"), b"retired")
        .expect("retired payload");
    std::fs::create_dir_all(impostor.join("retired-tree")).expect("impostor tree");
    std::fs::write(impostor.join("retired-tree").join("keep"), b"keep").expect("keep");
    let parent = retained_directory(&original);
    std::fs::rename(&original, &moved).expect("rename retained parent away");
    junction(&original, &impostor);
    // Positive control: the old spelling now resolves to the impostor.
    assert!(original.join("retired-tree").join("keep").is_file());
    remove_directory_tree(&parent, "retired-tree").expect("remove through retained parent");
    assert!(!moved.join("retired-tree").exists());
    assert_eq!(
        std::fs::read(impostor.join("retired-tree").join("keep")).expect("impostor kept"),
        b"keep"
    );
    drop(parent);
    std::fs::remove_dir(&original).expect("remove junction itself before cleanup");
}

#[test]
fn deletion_refuses_an_entry_held_open_without_delete_sharing() {
    let fixture = tempfile::tempdir().expect("fixture");
    let path = fixture.path().join("pending-held");
    std::fs::write(&path, b"held").expect("fixture file");
    let holder = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .expect("hold without delete sharing");
    let parent = retained_directory(fixture.path());
    let error = remove_file_relative(&parent, "pending-held").expect_err("sharing conflict");
    assert_eq!(error.raw_os_error(), Some(32), "{error}");
    drop(holder);
    assert!(
        path.is_file(),
        "a refused deletion leaves the entry in place"
    );
    remove_file_relative(&parent, "pending-held").expect("delete after release");
    assert!(!path.exists());
}

#[test]
fn an_empty_relative_name_never_reopens_the_parent() {
    let fixture = tempfile::tempdir().expect("fixture");
    let parent = retained_directory(fixture.path());
    let error = super::super::open_units_for_delete(&parent, Vec::new(), DeletePurpose::Remove)
        .err()
        .expect("an empty name refuses");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{error}");
    assert!(fixture.path().is_dir());
}

#[test]
fn an_entry_already_pending_deletion_is_not_found() {
    let fixture = tempfile::tempdir().expect("fixture");
    let path = fixture.path().join("pending-doomed");
    std::fs::write(&path, b"doomed").expect("fixture file");
    let doomed = OpenOptions::new()
        .access_mode(DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(&path)
        .expect("open with delete access");
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    let size = u32::try_from(size_of::<FILE_DISPOSITION_INFO>()).expect("small structure");
    // SAFETY: `doomed` owns a live handle with DELETE access; the initialized
    // FILE_DISPOSITION_INFO and its exact size stay valid for this synchronous call.
    let marked = unsafe {
        SetFileInformationByHandle(
            doomed.as_raw_handle().cast(),
            FileDispositionInfo,
            (&raw const disposition).cast(),
            size,
        )
    };
    assert_ne!(marked, 0, "{}", std::io::Error::last_os_error());
    let parent = retained_directory(fixture.path());
    let error = open_for_delete(&parent, "pending-doomed", DeletePurpose::Remove)
        .err()
        .expect("a delete-pending name refuses");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "{error}");
    drop(doomed);
    assert!(!path.exists());
}

/// The *same retained parent object* can gain a mount-point after opening.
/// Neither relative deletion path may follow that live reparse to outside data.
#[test]
fn live_parent_reparse_cannot_redirect_relative_file_or_tree_deletion() {
    use super::support::{clear_mount_point_on_open_directory, set_mount_point_on_open_directory};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_WRITE_ATTRIBUTES,
        FILE_WRITE_DATA,
    };

    let fixture = tempfile::tempdir().expect("live parent-reparse deletion fixture");
    let original = fixture.path().join("retained-empty");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&original).expect("empty parent, eligible for a mount-point");
    std::fs::create_dir_all(outside.join("retired-tree")).expect("outside tree");
    std::fs::write(outside.join("victim"), b"keep-file").expect("outside file");
    std::fs::write(outside.join("retired-tree").join("victim"), b"keep-tree")
        .expect("outside nested file");
    std::fs::write(outside.join("ambient-control"), b"control").expect("outside control");

    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("pin the real parent before its mutation");
    let hostile = OpenOptions::new()
        .access_mode(FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("independent writable handle to that same directory");
    set_mount_point_on_open_directory(&hostile, &outside);
    assert_eq!(
        std::fs::read(original.join("victim")).expect("live reparse is effective"),
        b"keep-file"
    );
    // Positive control: an ambient delete really reaches the outside target.
    std::fs::remove_file(original.join("ambient-control"))
        .expect("ambient deletion follows the adversarial mount-point");
    assert!(!outside.join("ambient-control").exists());

    let file_result = remove_file_relative(&parent, "victim");
    let tree_result = remove_directory_tree(&parent, "retired-tree");
    let outside_file = std::fs::read(outside.join("victim"));
    let outside_tree = std::fs::read(outside.join("retired-tree").join("victim"));
    clear_mount_point_on_open_directory(&hostile);
    drop(hostile);
    drop(parent);

    assert!(
        file_result.is_err(),
        "empty retained parent has no file: {file_result:?}"
    );
    assert!(
        tree_result.is_err(),
        "empty retained parent has no tree: {tree_result:?}"
    );
    assert_eq!(outside_file.expect("outside file survives"), b"keep-file");
    assert_eq!(outside_tree.expect("outside tree survives"), b"keep-tree");
}
