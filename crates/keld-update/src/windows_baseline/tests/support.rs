//! Fixture construction and bounded native subprocess observation, not acceptance oracles.

#![allow(unsafe_code)] // Test-only process wait uses the retained child handle.
#![deny(unsafe_op_in_unsafe_fn)]

use std::fs::{self, File, OpenOptions};
use std::io::Cursor;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::AsRawHandle as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

use crate::tests::{digest_hex, expected_identity, manifest_json, release_json, sign, signing_key};
use crate::windows_baseline::WindowsBaselineTrust;
use crate::{BaselineVerifier, VerifiedBaseline};

pub(super) const ROOT_ENV: &str = "KELD_KEL266_NATIVE_ROOT";
pub(super) const CASE_ENV: &str = "KELD_KEL266_NATIVE_CASE";
pub(super) const CUT_ENV: &str = "KELD_KEL266_NATIVE_CUT";
pub(super) const GOLDEN: &[u8] =
    include_bytes!("../../../../keld-pack/tests/fixtures/windows-v0-content.tar");

pub(super) fn directory(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .expect("open fixture directory without delete sharing")
}

pub(super) fn dacl_handle(path: &Path) -> File {
    OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .expect("open SYSTEM fixture descriptor handle")
}

pub(super) fn suite_root() -> PathBuf {
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("explicit native fixture root"));
    assert_eq!(
        root.parent(),
        Some(Path::new(r"C:\")),
        "fixture is a direct C: child"
    );
    let leaf = root
        .file_name()
        .and_then(|value| value.to_str())
        .expect("UTF-8 fixture leaf");
    assert!(
        leaf.starts_with("Keld266-") && leaf.len() > 16,
        "unique task fixture name"
    );
    root
}

pub(super) fn create_suite() -> PathBuf {
    keld_guard::require_windows_system_token().expect("native initializer must actually be SYSTEM");
    let root = suite_root();
    assert!(
        !root.exists(),
        "never reuse or repair an existing suite root"
    );
    let anchor = directory(Path::new(r"C:\"));
    keld_guard::validate_windows_machine_volume_anchor(&anchor)
        .expect("supported unchanged volume anchor");
    let created = crate::windows_fs::create_directory_relative(
        &anchor,
        root.file_name()
            .and_then(|value| value.to_str())
            .expect("leaf"),
    )
    .expect("create isolated SYSTEM-private test ancestor");
    let mut writable = dacl_handle(&root);
    keld_guard::seal_windows_machine_directory(&mut writable).expect("final machine ancestor");
    drop(created);
    fs::write(root.join("source.tar"), GOLDEN).expect("independently pinned canonical package");
    root
}

pub(super) fn trust_for(install: &Path) -> WindowsBaselineTrust {
    let mut identity = expected_identity();
    identity.install_root = install.to_path_buf();
    identity.update_root = install.join("updates");
    identity.baseline.content_blake3 = *blake3::hash(GOLDEN).as_bytes();
    // Trusted test configuration observes its explicitly selected volume, never the
    // untrusted on-disk provenance whose equality is being tested.
    let volume = crate::windows_fs::qualified_volume_root(&directory(Path::new(r"C:\")))
        .expect("native fixed NTFS fixture volume");
    WindowsBaselineTrust {
        installation: identity,
        publisher_scope: [0x26; 32],
        volume_guid: volume,
    }
}

pub(super) fn provision(root: &Path, label: &str) -> WindowsBaselineTrust {
    let parent = directory(root);
    let install =
        crate::windows_fs::create_directory_relative(&parent, label).expect("private install");
    let update =
        crate::windows_fs::create_directory_relative(&install, "updates").expect("private update");
    let _versions = crate::windows_fs::create_directory_relative(&update, "versions")
        .expect("private versions");
    trust_for(&root.join(label))
}

pub(super) fn baseline(trust: &WindowsBaselineTrust) -> VerifiedBaseline {
    let compressed = zstd::stream::encode_all(Cursor::new(GOLDEN), 0).expect("fixture compression");
    let release = release_json(
        "1.0.0",
        &compressed.len().to_string(),
        &digest_hex(&compressed),
        &GOLDEN.len().to_string(),
        &digest_hex(GOLDEN),
        "",
    );
    let manifest = manifest_json(&release);
    BaselineVerifier::new(
        trust.installation.clone(),
        signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted baseline key")
    .verify_manifest(&manifest, &sign(&manifest))
    .expect("actual detached signature")
    .verify_full(&mut Cursor::new(compressed), &mut Vec::new())
    .expect("authenticated full bytes")
}

pub(super) fn child(selector: &str, root: &Path, case: &str, cut: &str, code: i32) -> String {
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", selector, "--ignored", "--nocapture"])
        .env(ROOT_ENV, root)
        .env(CASE_ENV, case)
        .env(CUT_ENV, cut)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("isolated native test subprocess");
    // SAFETY: the Child retains its process handle for this bounded wait. No
    // borrowed pointer or ownership transfer occurs; timeout is only a kill switch.
    let waited = unsafe { WaitForSingleObject(child.as_raw_handle().cast(), 60_000) };
    if waited != WAIT_OBJECT_0 {
        let _ = child.kill();
        let _ = child.wait();
        panic!("native child did not exit within kill-switch bound: {waited}");
    }
    let output = child
        .wait_with_output()
        .expect("collect completed native child");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(code),
        "child stdout={stdout}; stderr={stderr}"
    );
    stdout.into_owned()
}
