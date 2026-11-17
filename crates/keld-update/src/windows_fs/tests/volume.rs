// Responsibility-local Windows native contract tests: volume.

use super::super::{require_volume_root_handle, validate_volume_locator, volume_guid_root};
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt as _;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

#[test]
fn guid_root_is_exact_and_excludes_the_directory_suffix() {
    let expected = "\\\\?\\Volume{01234567-89ab-CDEF-0123-456789abcdef}\\\0";
    for suffix in ["", "protected\\versions"] {
        let input = format!("\\\\?\\Volume{{01234567-89ab-CDEF-0123-456789abcdef}}\\{suffix}");
        assert_eq!(
            volume_guid_root(&input.encode_utf16().collect::<Vec<_>>())
                .expect("valid GUID root")
                .as_slice(),
            expected.encode_utf16().collect::<Vec<_>>()
        );
    }
}

#[test]
fn guid_root_rejects_fallback_names_and_malformed_identifiers() {
    for input in [
        "",
        "C:\\protected",
        "\\\\server\\share\\protected",
        "\\Device\\HarddiskVolume1\\protected",
        "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}",
        "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdeg}\\",
        "\\\\?\\Volume{01234567889ab-cdef-0123-456789abcdef}\\",
        "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\bad\0suffix",
    ] {
        assert!(
            volume_guid_root(&input.encode_utf16().collect::<Vec<_>>()).is_err(),
            "{input:?}"
        );
    }
}

#[test]
fn trusted_volume_locator_is_one_root_not_a_path() {
    let root = r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\";
    validate_volume_locator(root).expect("literal root");
    assert!(validate_volume_locator(&format!("{root}child")).is_err());
    assert!(validate_volume_locator(r"C:\").is_err());
}

#[test]
fn a_same_volume_subdirectory_is_not_a_drive_anchor() {
    let fixture = tempfile::tempdir().expect("anchor fixture");
    let directory = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(fixture.path())
        .expect("directory handle");
    assert!(require_volume_root_handle(&directory).is_err());
    let root_path: std::path::PathBuf = fixture.path().components().take(2).collect();
    let root = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(root_path)
        .expect("actual drive root");
    require_volume_root_handle(&root).expect("actual volume root positive control");
}

/// Reopening a bare DOS path must never resolve to a different file than the
/// retained verbatim handle. The ordinary sibling remains a positive control.
#[test]
fn fixed_local_dos_path_rejects_verbatim_trailing_dot_component() {
    let fixture = tempfile::tempdir().expect("private fixed-local fixture");
    let normal = fixture.path().join("receipt");
    std::fs::write(&normal, b"ordinary-file").expect("normal sibling");
    let verbatim = fixture
        .path()
        .canonicalize()
        .expect("Win32 verbatim directory")
        .join("receipt.");
    let special = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&verbatim)
        .expect("create literal trailing-dot NTFS filename");
    let admitted = super::super::fixed_local_dos_path(&special);
    assert!(
        admitted.is_err(),
        "a normalized DOS path would reopen the sibling instead of the retained file: {admitted:?}"
    );
    let spaced = fixture
        .path()
        .canonicalize()
        .expect("verbatim directory")
        .join("spaced ");
    let space_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&spaced)
        .expect("create trailing-space NTFS filename");
    assert!(
        super::super::fixed_local_dos_path(&space_file).is_err(),
        "a trailing-space component must fail closed"
    );

    let odd_parent = fixture
        .path()
        .canonicalize()
        .expect("verbatim directory")
        .join("parent.");
    std::fs::create_dir(&odd_parent).expect("create verbatim trailing-dot directory");
    let child = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(odd_parent.join("child"))
        .expect("create child under unusual parent");
    assert!(
        super::super::fixed_local_dos_path(&child).is_err(),
        "a nonfinal trailing-dot directory component must fail closed"
    );

    let ordinary = OpenOptions::new()
        .read(true)
        .open(&normal)
        .expect("open ordinary sibling");
    let stable = super::super::fixed_local_dos_path(&ordinary)
        .expect("ordinary fixed-local path still allowed");
    assert_eq!(
        std::fs::read(stable).expect("reopen ordinary retained file"),
        b"ordinary-file"
    );
}
