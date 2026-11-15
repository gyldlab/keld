// Responsibility-local Windows native contract tests: retained parent.

use super::super::create_directory_relative;
use super::support::{clear_mount_point_on_open_directory, set_mount_point_on_open_directory};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::os::windows::fs::OpenOptionsExt as _;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
};

#[test]
fn relative_creation_uses_the_acquired_parent_after_name_substitution() {
    let fixture = tempfile::tempdir().expect("fixture");
    let original = fixture.path().join("original");
    let moved = fixture.path().join("moved");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&original).expect("original parent");
    std::fs::create_dir(&outside).expect("outside directory");
    std::fs::write(outside.join("sentinel"), b"unchanged").expect("outside sentinel");
    // Deliberately omit the extraction owner's rename protection: this
    // tests the native RootDirectory relationship independently of pins.
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("retain renameable parent");
    std::fs::rename(&original, &moved).expect("rename succeeds with delete sharing");
    let junction = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:KELD_PRIMITIVE_LINK -Target $env:KELD_PRIMITIVE_TARGET | Out-Null",
        ])
        .env("KELD_PRIMITIVE_LINK", &original)
        .env("KELD_PRIMITIVE_TARGET", &outside)
        .output()
        .expect("junction fixture command");
    assert!(
        junction.status.success(),
        "junction setup failed: {}",
        String::from_utf8_lossy(&junction.stderr)
    );
    // Prove the old spelling now actually resolves to the outside object.
    assert_eq!(
        std::fs::read(original.join("sentinel")).expect("redirected sentinel"),
        b"unchanged"
    );
    let child = create_directory_relative(&parent, "child").expect("create on retained object");
    assert!(moved.join("child").is_dir());
    assert!(!outside.join("child").exists());
    assert!(!original.join("child").exists());
    assert_eq!(
        std::fs::read(outside.join("sentinel")).expect("outside after create"),
        b"unchanged"
    );
    // Positive control for the unsafe alternative: an ambient writer using
    // the replaced spelling really escapes to the outside fixture directory.
    std::fs::create_dir(original.join("ambient-control")).expect("ambient write control");
    assert!(outside.join("ambient-control").is_dir());
    assert!(!moved.join("ambient-control").exists());
    drop(child);
    drop(parent);
    std::fs::remove_dir(&original).expect("remove junction itself before fixture cleanup");
}

#[test]
fn retained_parent_live_reparse_mutation_cannot_redirect_relative_creates() {
    use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};

    let fixture = tempfile::tempdir().expect("fixture");
    let original = fixture.path().join("retained-empty");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&original).expect("empty parent");
    std::fs::create_dir(&outside).expect("outside target");
    std::fs::write(outside.join("sentinel"), b"outside-unchanged").expect("outside sentinel");
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("retain parent without delete sharing");
    // Same-user adversarial handle: no pathname replacement or rename.
    // The first handle's write sharing permits this independent acquisition.
    let hostile = OpenOptions::new()
        .access_mode(FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&original)
        .expect("independent write/attribute handle");
    set_mount_point_on_open_directory(&hostile, &outside);
    assert_eq!(
        std::fs::read(original.join("sentinel")).expect("active junction positive control"),
        b"outside-unchanged"
    );

    let created = create_directory_relative(&parent, "native-child");
    let file_created = if created.is_ok() {
        let retained =
            cap_std::fs::Dir::from_std_file(parent.try_clone().expect("duplicate retained handle"));
        let mut options = cap_std::fs::OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        match retained.open_with("cap-child", &options) {
            Ok(mut file) => {
                file.write_all(b"retained-file")
                    .expect("write retained child");
                true
            }
            Err(_) => false,
        }
    } else {
        false
    };
    let outside_native = outside.join("native-child").exists();
    let outside_file = outside.join("cap-child").exists();
    let sentinel = std::fs::read(outside.join("sentinel")).expect("outside sentinel after probes");
    // Remove only this same object's reparse data so its actual contents
    // become observable again, then drop handles before TempDir cleanup.
    clear_mount_point_on_open_directory(&hostile);
    assert_eq!(original.join("native-child").is_dir(), created.is_ok());
    if file_created {
        assert_eq!(
            std::fs::read(original.join("cap-child"))
                .expect("retained file after clearing reparse"),
            b"retained-file"
        );
    }
    drop(created);
    drop(hostile);
    drop(parent);
    assert!(
        !outside_native,
        "relative native mkdir escaped through live parent reparse data"
    );
    assert!(
        !outside_file,
        "capability file create escaped through live parent reparse data"
    );
    assert_eq!(sentinel, b"outside-unchanged");
}
