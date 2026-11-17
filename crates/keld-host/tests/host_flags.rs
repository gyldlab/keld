//! Cross-platform host argument boundary.
#![allow(clippy::expect_used)] // integration-test process/output assertions

use std::process::Command;

#[cfg(all(windows, feature = "windows-initial-install"))]
#[path = "host_flags/windows_qualified.rs"]
mod windows_qualified;

/// Launches the host with one non-Unicode argument and asserts the registered
/// refusal: exit 2, before boot selection. A qualified Windows distribution
/// rejects the argument at its earlier private-role boundary; dev/unqualified
/// hosts reach the Unicode-specific `KELD-CLI-051` error.
fn assert_non_unicode_argv_refused_before_boot(arg: std::ffi::OsString) {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg(arg)
        .output()
        .expect("launch host with a test-owned non-Unicode argument");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("fixed error uses UTF-8");
    let expected = if cfg!(all(windows, feature = "windows-qualified-launch")) {
        "KELD-HOST-INITIAL-001: qualified Windows Host refuses developer leases"
    } else {
        "KELD-CLI-051: process argument 1 is not valid Unicode."
    };
    assert!(stderr.contains(expected), "{stderr}");
    assert!(!stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(!stderr.contains("panicked at"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

#[cfg(windows)]
#[test]
fn malformed_windows_utf16_argv_is_cli_051_not_a_panic() {
    use std::os::windows::ffi::OsStringExt as _;

    // An unpaired surrogate is representable in the Windows native argv but
    // cannot be converted by std::env::args(). Both standard and qualified
    // hosts must refuse before any boot or WebView2 authority exists.
    assert_non_unicode_argv_refused_before_boot(std::ffi::OsString::from_wide(&[0xd800]));
}

#[cfg(unix)]
#[test]
fn non_utf8_unix_argv_is_cli_051_not_a_panic() {
    use std::os::unix::ffi::OsStringExt as _;

    // Unix argv is arbitrary bytes; std::env::args() panics on 0xFF.
    assert_non_unicode_argv_refused_before_boot(std::ffi::OsString::from_vec(vec![0xff]));
}

#[test]
fn unknown_argument_is_cli_044_before_boot_selection() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg("--not-a-keld-host-flag")
        .output()
        .expect("run host with unknown argument");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("host error is UTF-8");
    let expected = if cfg!(all(windows, feature = "windows-qualified-launch")) {
        "KELD-HOST-INITIAL-001"
    } else {
        "KELD-CLI-044"
    };
    assert!(stderr.contains(expected), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
    assert!(!stderr.contains("keld.boot.json"), "{stderr}");
}
