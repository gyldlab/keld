// Responsibility-local Windows native contract tests: creation.

use super::super::{
    create_directory_relative, create_file_relative_exclusive_with_profile,
    duplicate_activation_lease_for_keeper, publish_new,
};
use super::support::{
    assert_rename_stayed_in_retained_parent, rename_fixture, substitute_parent_spelling,
};
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::os::windows::fs::OpenOptionsExt as _;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

#[test]
fn relative_creation_protects_and_retains_new_directory_objects() {
    let fixture = tempfile::tempdir().expect("fixture");
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(fixture.path())
        .expect("retain fixture parent");
    let child = create_directory_relative(&parent, "private").expect("create child");
    keld_guard::validate_windows_owner_private_directory(&child)
        .expect("actual private descriptor");
    let nested = create_directory_relative(&child, "nested").expect("create relative grandchild");
    assert!(fixture.path().join("private/nested").is_dir());
    assert!(create_directory_relative(&parent, "private").is_err());
    File::create(fixture.path().join("existing-file")).expect("collision file");
    assert!(create_directory_relative(&parent, "existing-file").is_err());
    for name in [
        "",
        "a/b",
        "a\\b",
        "..",
        "NUL",
        "alias~1",
        "decomposed-e\u{301}",
    ] {
        assert!(
            create_directory_relative(&parent, name).is_err(),
            "{name:?}"
        );
    }
    assert!(!fixture.path().join("a").exists());
    assert!(std::fs::rename(fixture.path().join("private"), fixture.path().join("moved")).is_err());
    drop(nested);
    drop(child);
    std::fs::rename(fixture.path().join("private"), fixture.path().join("moved"))
        .expect("released handles permit rename");
    drop(parent);
}

#[test]
fn exclusive_profiled_file_blocks_readers_until_its_handle_closes() {
    let fixture = tempfile::tempdir().expect("exclusive file fixture");
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(fixture.path())
        .expect("retain fixture parent");
    let file = create_file_relative_exclusive_with_profile(
        &parent,
        "activation.lock",
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    )
    .expect("create with exact profile and no sharing");
    keld_guard::validate_windows_owner_private_file(&file).expect("exact file profile");
    assert!(File::open(fixture.path().join("activation.lock")).is_err());
    drop(file);
    File::open(fixture.path().join("activation.lock"))
        .expect("readers can open the persistent lease file after release");
}

#[test]
fn sibling_publication_never_replaces_or_copies() {
    let fixture = tempfile::tempdir().expect("publication fixture");
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(fixture.path())
        .expect("retain publication parent");
    std::fs::write(fixture.path().join("pending"), b"verified bytes").expect("source");
    std::fs::write(fixture.path().join("existing"), b"existing sentinel").expect("target");
    assert!(publish_new(&parent, "pending", "existing").is_err());
    assert_eq!(
        std::fs::read(fixture.path().join("existing")).expect("sentinel"),
        b"existing sentinel"
    );
    assert_eq!(
        std::fs::read(fixture.path().join("pending")).expect("source preserved"),
        b"verified bytes"
    );
    for destination in ["../outside", "a/b", "a\\b", "pending", "PENDING"] {
        assert!(
            publish_new(&parent, "pending", destination).is_err(),
            "{destination}"
        );
    }
    publish_new(&parent, "pending", "published").expect("absent sibling publication");
    assert!(!fixture.path().join("pending").exists());
    assert_eq!(
        std::fs::read(fixture.path().join("published")).expect("published bytes"),
        b"verified bytes"
    );
    drop(parent);
}

#[test]
fn reduced_activation_lease_handle_retains_share_zero_without_write_access() {
    let fixture = tempfile::tempdir().expect("lease fixture");
    let path = fixture.path().join("activation.lock");
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(&path)
        .expect("create exact share-zero lease");
    let keeper_handle = duplicate_activation_lease_for_keeper(&lease)
        .expect("duplicate read-attributes/synchronize rights");
    drop(lease);

    let mut keeper_file = File::from(keeper_handle);
    assert_eq!(
        keeper_file
            .write_all(b"forbidden")
            .expect_err("keeper handle has no write access")
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert!(
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&path)
            .is_err(),
        "reduced duplicate retains the original share-zero open"
    );
    drop(keeper_file);
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .expect("writer exclusion ends when the final duplicate closes");
}

/// An empty retained parent refuses the outside-only source before publication.
/// This is an early-open negative, NOT a destination `RootDirectory` oracle; the
/// nonempty reparented positive test below independently exercises both sides.
#[test]
fn live_reparse_parent_cannot_redirect_absent_publication() {
    use super::support::{clear_mount_point_on_open_directory, set_mount_point_on_open_directory};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_WRITE,
        FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
    };
    let fixture = tempfile::tempdir().expect("live publisher parent fixture");
    let original = fixture.path().join("retained-empty");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&original).expect("empty retained parent");
    std::fs::create_dir(&outside).expect("outside");
    std::fs::write(outside.join("prepared"), b"outside source").expect("outside source");
    std::fs::write(outside.join("ambient"), b"ambient control").expect("outside control");
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("pin parent");
    let hostile = OpenOptions::new()
        .access_mode(FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("hostile descriptor");
    set_mount_point_on_open_directory(&hostile, &outside);
    assert_eq!(
        std::fs::read(original.join("prepared")).expect("reparse effective"),
        b"outside source"
    );
    std::fs::rename(original.join("ambient"), original.join("ambient-moved"))
        .expect("ambient rename reaches outside");
    assert!(outside.join("ambient-moved").is_file());

    let result = super::super::publish_new(&parent, "prepared", "published");
    let outside_source = std::fs::read(outside.join("prepared"));
    let outside_target = std::fs::read(outside.join("published"));
    clear_mount_point_on_open_directory(&hostile);
    drop(hostile);
    drop(parent);

    assert!(
        result.is_err(),
        "outside source was wrongly admitted: {result:?}"
    );
    assert_eq!(
        outside_source.expect("outside source retained"),
        b"outside source"
    );
    assert!(
        outside_target.is_err(),
        "outside destination must stay absent"
    );
}

/// An absent retained pending record refuses before typed destination replacement.
/// The real-source positive case below tests destination `RootDirectory` handling.
#[test]
fn live_reparse_parent_cannot_redirect_typed_record_replacement() {
    use super::super::{RecordSlot, replace_record_slot};
    use super::support::{clear_mount_point_on_open_directory, set_mount_point_on_open_directory};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_WRITE,
        FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
    };

    let fixture = tempfile::tempdir().expect("live record parent fixture");
    let original = fixture.path().join("retained-empty");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&original).expect("empty retained parent");
    std::fs::create_dir(&outside).expect("outside");
    let pending = format!("pending-{}", "a".repeat(64));
    std::fs::write(outside.join(&pending), b"outside pending").expect("outside prepared record");
    std::fs::write(outside.join("current"), b"outside current").expect("outside pointer");
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("retain actual empty update parent");
    let hostile = OpenOptions::new()
        .access_mode(FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("independent writable handle");
    set_mount_point_on_open_directory(&hostile, &outside);
    assert_eq!(
        std::fs::read(original.join("current")).expect("live mount-point redirects paths"),
        b"outside current"
    );

    let result = replace_record_slot(&parent, &pending, RecordSlot::Current);
    let outside_pending = std::fs::read(outside.join(&pending));
    let outside_current = std::fs::read(outside.join("current"));
    clear_mount_point_on_open_directory(&hostile);
    drop(hostile);
    drop(parent);

    assert!(
        result.is_err(),
        "outside preparation wrongly admitted: {result:?}"
    );
    assert_eq!(
        outside_pending.expect("outside preparation stays"),
        b"outside pending"
    );
    assert_eq!(
        outside_current.expect("outside pointer stays"),
        b"outside current"
    );
}

/// Opens the real source BEFORE the pathname is redirected. The original
/// directory can be renamed because this deliberate primitive fixture permits
/// delete sharing; production's stronger retained pins do not permit the move.
fn replaced_spelling_with_pinned_source(
    source: &str,
    destination: &str,
    prior_destination: Option<&[u8]>,
) -> (
    tempfile::TempDir,
    File,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (fixture, parent, original, moved, outside) =
        rename_fixture(source, destination, prior_destination);
    substitute_parent_spelling(&original, &moved, &outside, source);
    (fixture, parent, original, moved, outside)
}

#[test]
fn publication_uses_pinned_source_and_destination_after_parent_path_substitution() {
    let (_fixture, parent, original, moved, outside) =
        replaced_spelling_with_pinned_source("prepared", "published", None);
    publish_new(&parent, "prepared", "published")
        .expect("publish real source into retained parent despite hostile pathname");
    assert_rename_stayed_in_retained_parent(&moved, &outside, "prepared", "published");
    drop(parent);
    std::fs::remove_dir(original).expect("remove junction, not its target");
}

#[test]
fn typed_replacement_uses_pinned_source_and_destination_after_parent_path_substitution() {
    use super::super::{RecordSlot, replace_record_slot};

    let pending = format!("pending-{}", "a".repeat(64));
    let (_fixture, parent, original, moved, outside) =
        replaced_spelling_with_pinned_source(&pending, "current", Some(b"prior current"));
    replace_record_slot(&parent, &pending, RecordSlot::Current)
        .expect("replace typed record under retained parent despite hostile pathname");
    assert_rename_stayed_in_retained_parent(&moved, &outside, &pending, "current");
    drop(parent);
    std::fs::remove_dir(original).expect("remove junction, not its target");
}
