//! Post-installer ordinary-user access, independent of SYSTEM qualification.

use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, WRITE_DAC, WRITE_OWNER,
};

use super::support::{self, trust_for};
use crate::windows_baseline::load_windows_baseline;

#[test]
#[ignore = "requires actual SYSTEM-created fixture and a separate ordinary-user process"]
fn ordinary_user_reads_but_cannot_mutate_committed_baseline() {
    support::assert_ordinary_token();
    assert!(
        keld_guard::require_windows_system_token().is_err(),
        "ordinary-user proof cannot run as SYSTEM"
    );
    let root = support::suite_root();
    assert!(
        root.join("system-finished.txt").is_file(),
        "operator SYSTEM suite finished"
    );
    let trust = trust_for(&root.join("success"));
    let loaded = load_windows_baseline(&trust).expect("ordinary user can load protected identity");
    assert_eq!(loaded.version_floor(), "1.0.0");
    drop(loaded); // All loader pins are gone before any denial assertion.
    super::alias::run(&root);
    let install = &trust.installation.install_root;
    let update = &trust.installation.update_root;
    let version = update.join("versions").join("1.0.0");
    for path in [
        install.clone(),
        update.clone(),
        version.clone(),
        version.join("tree"),
        install.join("install-provenance"),
        update.join("activation.lock"),
        update.join("version-floor"),
        update.join("current"),
        update.join("last-known-good"),
        version.join("content.tar"),
        version.join(".complete"),
        version.join("tree/nest/one"),
    ] {
        for right in [WRITE_DAC, WRITE_OWNER, DELETE] {
            let opened = OpenOptions::new()
                .access_mode(right)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path);
            let error = opened
                .expect_err("ordinary caller cannot acquire mutation authority after pins close");
            assert_eq!(
                error.raw_os_error(),
                Some(5),
                "actual access denial, path={path:?} right={right:#x}"
            );
        }
    }
    assert_activation_lock_is_readable(update);
    let content = version.join("tree/nest/one");
    assert_eq!(fs::read(&content).expect("ordinary payload read"), [b'!']);
    assert_eq!(
        OpenOptions::new()
            .write(true)
            .open(&content)
            .expect_err("payload write denied")
            .raw_os_error(),
        Some(5)
    );
    assert_eq!(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(update.join("ordinary-new"))
            .expect_err("state creation denied")
            .raw_os_error(),
        Some(5)
    );
    assert_eq!(
        fs::rename(install, root.join("replaced-success"))
            .expect_err("persistent parent policy prevents replacement")
            .raw_os_error(),
        Some(5)
    );
    // Matched writable control establishes that the caller can actually exercise
    // these APIs; OS denial is not an absent path or a permanently failing helper.
    let control = tempfile::tempdir().expect("ordinary-user writable control");
    let original = control.path().join("original");
    fs::write(&original, b"ordinary").expect("positive write control");
    let owner = OpenOptions::new()
        .access_mode(WRITE_DAC)
        .open(&original)
        .expect("positive owner WRITE_DAC control");
    drop(owner);
    fs::rename(&original, control.path().join("renamed")).expect("positive rename control");
    let unsafe_parent = root.join("parent-delete-child");
    let inadmissible = trust_for(&unsafe_parent.join("install"));
    assert!(
        load_windows_baseline(&inadmissible).is_err(),
        "mutable ancestry refuses to load"
    );
    let child = unsafe_parent.join("empty-protected-child");
    assert!(
        child.is_dir(),
        "matched control exists before ordinary-user delete"
    );
    fs::remove_dir(&child).expect("parent DELETE_CHILD permits deleting the empty protected child");
    assert!(
        !child.exists(),
        "OS effect independently demonstrates why the ancestor rule matters"
    );
    println!("KELD_KEL266_ORDINARY_DENIAL_FINISHED={}", root.display());
}

fn assert_activation_lock_is_readable(update: &Path) {
    assert!(
        fs::read(update.join("activation.lock"))
            .expect("ordinary user reads the activation lock")
            .is_empty()
    );
}
