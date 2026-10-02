//! Fixture construction and bounded native subprocess observation, not acceptance oracles.

#![allow(unsafe_code)] // Test-only process wait uses the retained child handle.
#![deny(unsafe_op_in_unsafe_fn)]

use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read as _, Seek as _, SeekFrom};
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcessToken, WaitForSingleObject,
};

use crate::tests::{digest_hex, expected_identity, manifest_json, release_json, sign, signing_key};
use crate::windows_baseline::WindowsBaselineTrust;
use crate::{BaselineVerifier, VerifiedBaseline};

pub(super) const ROOT_ENV: &str = "KELD_KEL266_NATIVE_ROOT";
pub(super) const MACHINE_UAC_ROOT_ENV: &str = "KELD_KEL270_MACHINE_UAC_ROOT";
pub(super) const CASE_ENV: &str = "KELD_KEL266_NATIVE_CASE";
pub(super) const CUT_ENV: &str = "KELD_KEL266_NATIVE_CUT";
pub(super) const GOLDEN: &[u8] =
    include_bytes!("../../../../keld-pack/tests/fixtures/windows-v0-content.tar");

pub(super) fn assert_user_principal_token() {
    let sid = windows_permissions::utilities::current_process_sid().expect("actual TokenUser");
    let text = windows_permissions::wrappers::ConvertSidToStringSid(&sid).expect("TokenUser text");
    let text = text.to_string_lossy();
    assert!(
        text.starts_with("S-1-5-21-") || text.starts_with("S-1-12-1-"),
        "ordinary-user proof cannot use a service or SYSTEM identity: {text}"
    );
    println!("KELD_KEL266_USER_PRINCIPAL sid={text}");
}

pub(super) fn assert_ordinary_token() {
    assert_user_principal_token();
    let mut raw = std::ptr::null_mut();
    // SAFETY: the pseudo process handle is used synchronously; the output is a
    // writable HANDLE slot. On success ownership immediately enters RAII.
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw) },
        0
    );
    // SAFETY: successful OpenProcessToken returned one owned, non-null token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 1 };
    let elevation_bytes = u32::try_from(std::mem::size_of::<TOKEN_ELEVATION>())
        .expect("native token layout fits a Win32 byte count");
    let mut returned = 0_u32;
    // SAFETY: token remains live; the initialized output is exactly its advertised
    // size, and the return-length pointer remains valid for the synchronous call.
    assert_ne!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                TokenElevation,
                std::ptr::from_mut(&mut elevation).cast(),
                elevation_bytes,
                &raw mut returned,
            )
        },
        0
    );
    assert_eq!(returned, elevation_bytes);
    assert_eq!(
        elevation.TokenIsElevated, 0,
        "ordinary-user denial proof must be unelevated"
    );
    println!("KELD_KEL266_ORDINARY_TOKEN elevated=0");
}

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
    // Trusted test configuration observes the fixture's selected volume, never the
    // untrusted on-disk provenance whose equality is being tested. Some fixtures have
    // not created the install leaf yet, so bind to its nearest existing ancestor.
    let volume_anchor = install
        .ancestors()
        .find(|candidate| candidate.is_dir())
        .expect("fixture has an existing install ancestor");
    let volume = crate::windows_fs::qualified_volume_root(&directory(volume_anchor))
        .expect("native fixed NTFS fixture volume");
    WindowsBaselineTrust {
        installation: identity,
        owner: crate::InstallOwner::Direct,
        publisher_scope: [0x26; 32],
        volume_guid: volume,
    }
}

pub(super) fn provision(root: &Path, label: &str) -> WindowsBaselineTrust {
    let parent = directory(root);
    let profile = keld_guard::WindowsInstallProtectionProfile::MachineSystem;
    let install =
        crate::windows_fs::create_directory_relative_with_profile(&parent, label, profile)
            .expect("machine-profile install");
    let update =
        crate::windows_fs::create_directory_relative_with_profile(&install, "updates", profile)
            .expect("machine-profile update");
    let _versions =
        crate::windows_fs::create_directory_relative_with_profile(&update, "versions", profile)
            .expect("machine-profile versions");
    trust_for(&root.join(label))
}

pub(super) fn provision_machine_uac(root: &Path, label: &str) -> WindowsBaselineTrust {
    keld_guard::require_windows_system_token().expect("SYSTEM creates isolated UAC fixture");
    let parent = directory(root);
    let profile = keld_guard::WindowsInstallProtectionProfile::MachineUac;
    let install =
        crate::windows_fs::create_directory_relative_with_profile(&parent, label, profile)
            .expect("create exact Administrators-owned fixture install root");
    let update =
        crate::windows_fs::create_directory_relative_with_profile(&install, "updates", profile)
            .expect("create exact Administrators-owned update root");
    crate::windows_fs::create_directory_relative_with_profile(&update, "versions", profile)
        .expect("create exact Administrators-owned versions root");
    let mut trust = trust_for(&root.join(label));
    trust.installation.install_mode = crate::DirectInstallMode::MachineUacDirect;
    trust
}

pub(super) fn provision_machine_uac_as_administrator(
    root: &Path,
    label: &str,
) -> WindowsBaselineTrust {
    keld_guard::require_windows_non_system_token()
        .expect("explicit-UAC installer must not be SYSTEM");
    keld_guard::require_windows_machine_uac_owner_token()
        .expect("explicit-UAC installer requires an elevated owner-capable Administrators token");
    let parent = directory(root);
    let profile = keld_guard::WindowsInstallProtectionProfile::MachineUac;
    let install =
        crate::windows_fs::create_directory_relative_with_profile(&parent, label, profile)
            .expect("create exact Administrators-owned fixture install root");
    let update =
        crate::windows_fs::create_directory_relative_with_profile(&install, "updates", profile)
            .expect("create exact Administrators-owned update root");
    crate::windows_fs::create_directory_relative_with_profile(&update, "versions", profile)
        .expect("create exact Administrators-owned versions root");
    let mut trust = trust_for(&root.join(label));
    trust.installation.install_mode = crate::DirectInstallMode::MachineUacDirect;
    trust
}

pub(super) fn provision_per_user(root: &Path, label: &str) -> WindowsBaselineTrust {
    let parent = directory(root);
    let profile = keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;
    let install =
        crate::windows_fs::create_directory_relative_with_profile(&parent, label, profile)
            .expect("create exact owner-private per-user install root");
    let update =
        crate::windows_fs::create_directory_relative_with_profile(&install, "updates", profile)
            .expect("create exact owner-private per-user update root");
    crate::windows_fs::create_directory_relative_with_profile(&update, "versions", profile)
        .expect("create exact owner-private per-user versions root");
    let mut trust = trust_for(&root.join(label));
    trust.installation.install_mode = crate::DirectInstallMode::PerUserDirect;
    trust
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
    child_with_timeout(selector, root, case, cut, code, 60_000).0
}

pub(super) fn child_with_timeout(
    selector: &str,
    root: &Path,
    case: &str,
    cut: &str,
    code: i32,
    timeout_ms: u32,
) -> (String, String) {
    let mut stdout_capture = tempfile::tempfile_in(root).expect("retained child stdout capture");
    let mut stderr_capture = tempfile::tempfile_in(root).expect("retained child stderr capture");
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", selector, "--ignored", "--nocapture"])
        .env(ROOT_ENV, root)
        .env(CASE_ENV, case)
        .env(CUT_ENV, cut)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            stdout_capture.try_clone().expect("child stdout handle"),
        ))
        .stderr(Stdio::from(
            stderr_capture.try_clone().expect("child stderr handle"),
        ))
        .spawn()
        .expect("isolated native test subprocess");
    // SAFETY: the Child retains its process handle for this bounded wait. No
    // borrowed pointer or ownership transfer occurs; timeout is only a kill switch.
    let waited = unsafe { WaitForSingleObject(child.as_raw_handle().cast(), timeout_ms) };
    let termination = if waited == WAIT_OBJECT_0 {
        None
    } else {
        Some(child.kill())
    };
    let status = child.wait();
    // The process has been waited/reaped before either retained file is rewound.
    // File-backed capture cannot fill an unread pipe while the parent awaits exit.
    let stdout = read_capture(&mut stdout_capture);
    let stderr = read_capture(&mut stderr_capture);
    if let Some(Err(error)) = termination {
        panic!("failed to terminate native child: {error}; stdout={stdout}; stderr={stderr}");
    }
    let status = status.unwrap_or_else(|error| {
        panic!("failed to reap native child: {error}; stdout={stdout}; stderr={stderr}")
    });
    assert_eq!(
        waited, WAIT_OBJECT_0,
        "native child did not exit within kill-switch bound: {waited}; status={status}; child stdout={stdout}; stderr={stderr}"
    );
    assert_eq!(
        status.code(),
        Some(code),
        "child stdout={stdout}; stderr={stderr}"
    );
    (stdout, stderr)
}

fn read_capture(file: &mut File) -> String {
    file.seek(SeekFrom::Start(0))
        .expect("rewind completed child capture");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .expect("read completed child capture");
    String::from_utf8_lossy(&bytes).into_owned()
}
