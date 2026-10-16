// Responsibility-local Windows native contract tests: creation.

use super::super::{
    create_directory_relative, create_file_relative_exclusive_with_profile,
    duplicate_activation_lease_for_keeper, publish_new,
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
