//! Cross-platform host argument boundary.
#![allow(clippy::expect_used)] // integration-test process/output assertions

use std::process::Command;

/// Launches the host with one non-Unicode argument and asserts the registered
/// refusal: exit 2, `KELD-CLI-051` naming argv position 1, no panic, and no
/// boot selection (`KELD-CORE-034`) or unknown-argument (`KELD-CLI-044`) path.
#[cfg(any(unix, all(windows, not(feature = "windows-qualified-launch"))))]
fn assert_non_unicode_argv_refused_before_boot(arg: std::ffi::OsString) {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg(arg)
        .output()
        .expect("launch host with a test-owned non-Unicode argument");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("fixed error uses UTF-8");
    assert!(
        stderr.contains("KELD-CLI-051: process argument 1 is not valid Unicode."),
        "{stderr}"
    );
    assert!(!stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(!stderr.contains("panicked at"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

#[cfg(all(windows, not(feature = "windows-qualified-launch")))]
#[test]
fn malformed_windows_utf16_argv_is_cli_051_not_a_panic() {
    use std::os::windows::ffi::OsStringExt as _;

    // An unpaired surrogate is representable in the Windows native argv but
    // cannot be converted by std::env::args(). The host must refuse this
    // unsupported CLI argument before any boot or WebView2 authority exists.
    assert_non_unicode_argv_refused_before_boot(std::ffi::OsString::from_wide(&[0xd800]));
}

#[cfg(unix)]
#[test]
fn non_utf8_unix_argv_is_cli_051_not_a_panic() {
    use std::os::unix::ffi::OsStringExt as _;

    // Unix argv is arbitrary bytes; std::env::args() panics on 0xFF.
    assert_non_unicode_argv_refused_before_boot(std::ffi::OsString::from_vec(vec![0xff]));
}

#[cfg(not(all(windows, feature = "windows-qualified-launch")))]
#[test]
fn unknown_argument_is_cli_044_before_boot_selection() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg("--not-a-keld-host-flag")
        .output()
        .expect("run host with unknown argument");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("host error is UTF-8");
    assert!(stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
    assert!(!stderr.contains("keld.boot.json"), "{stderr}");
}

#[cfg(all(windows, feature = "windows-qualified-launch"))]
#[test]
fn qualified_host_rejects_invalid_utf16_before_any_boot_or_role() {
    use std::os::windows::ffi::OsStringExt as _;
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg(std::ffi::OsString::from_wide(&[0xd800]))
        .output()
        .expect("run qualified Host with an unpaired UTF-16 surrogate");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("fixed refusal is UTF-8");
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

/// The original KEL-270 initial-install argument is reserved even when the
/// feature has not been compiled. It must not fall through into the ordinary
/// application boot, developer-role routing or WebView initialization.
#[cfg(all(windows, not(feature = "windows-initial-install")))]
#[test]
fn original_initial_install_flag_without_feature_refuses_before_boot() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg("--keld-install-initial-v3")
        .arg(r"C:\untrusted\nonexistent-request.json")
        .output()
        .expect("run original reserved Windows initial-install argument");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("fixed refusal is UTF-8");
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

/// The original Host role is deliberately NOT enabled as an installer. An
/// exact syntactically valid role still refuses before reading its locator or
/// creating anything, even when the private role feature is compiled.
#[cfg(all(windows, feature = "windows-initial-install"))]
#[test]
fn original_exact_initial_install_role_refuses_uncommissioned_authority() {
    let fixture = tempfile::tempdir().expect("private Windows refusal directory");
    let request = fixture.path().join("missing-untrusted-request.json");
    assert!(!request.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg("--keld-install-initial-v3")
        .arg(&request)
        .output()
        .expect("run original private role without approved catalogue");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("fixed refusal is UTF-8");
    assert!(stderr.contains("KELD-HOST-INITIAL-002"), "{stderr}");
    assert!(!stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
    assert!(
        !request.exists(),
        "refusal must not create or open a missing untrusted request"
    );
    assert_eq!(
        std::fs::read_dir(fixture.path())
            .expect("inspect test-owned refusal directory")
            .count(),
        0,
        "original role refusal must not publish any artifact"
    );
}

#[cfg(all(windows, feature = "windows-initial-install"))]
#[test]
fn malformed_original_initial_install_locator_is_never_authority() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg("--keld-install-initial-v3")
        .arg("relative-untrusted-request.json")
        .output()
        .expect("run original role with a relative caller-controlled locator");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("fixed refusal is UTF-8");
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-HOST-INITIAL-002"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

#[cfg(all(windows, feature = "windows-qualified-launch"))]
#[test]
fn qualified_windows_role_rejects_diagnostic_arguments_without_boot() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg("--hello")
        .output()
        .expect("run qualified Windows Host with an unrecognized diagnostic role");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("fixed refusal is UTF-8");
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

#[cfg(all(windows, feature = "windows-qualified-launch"))]
#[test]
fn qualified_windows_role_refuses_developer_lease_before_boot() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .env("KELD_DEV_LEASE", "1")
        .output()
        .expect("run qualified Windows Host with an unauthorized development lease");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("fixed refusal is UTF-8");
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}
