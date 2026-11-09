//! Cross-platform host argument boundary.
#![allow(clippy::expect_used)] // integration-test process/output assertions

use std::process::Command;

/// Launches the host with one non-Unicode argument and asserts the registered
/// refusal: exit 2, `KELD-CLI-051` naming argv position 1, no panic, and no
/// boot selection (`KELD-CORE-034`) or unknown-argument (`KELD-CLI-044`) path.
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

#[cfg(windows)]
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

/// Runs `keld-host <args>` and returns its exit code and UTF-8 stderr.
fn run_host(args: &[&str]) -> (Option<i32>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .args(args)
        .output()
        .expect("run host with test-owned arguments");
    let stderr = String::from_utf8(output.stderr).expect("fixed host refusal is UTF-8");
    (output.status.code(), stderr)
}

// `keld_core::host_hello_requested` owns one order-free hello position rule
// on every OS. Each argv below ends in a token the hello parser must refuse,
// so a selected hello command exits 2 before any webview or window exists.

#[test]
fn title_before_hello_selects_the_hello_command() {
    let (code, stderr) = run_host(&["--title", "Acme", "--hello", "--not-a-keld-host-flag"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("KELD-CLI-044: unknown hello flag `--not-a-keld-host-flag`"),
        "`--title X --hello` must reach the hello parser: {stderr}"
    );
    assert!(!stderr.contains("unknown host argument"), "{stderr}");
}

#[test]
fn hello_after_unknown_argument_is_refused_before_any_window() {
    let (code, stderr) = run_host(&["--not-a-keld-host-flag", "--hello"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("KELD-CLI-044: unknown hello flag `--not-a-keld-host-flag`"),
        "{stderr}"
    );
    assert!(!stderr.contains("KELD-WV-"), "{stderr}");
}

#[test]
fn hello_in_title_value_position_is_refused_before_any_window() {
    let (code, stderr) = run_host(&["--title", "--hello"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("KELD-CLI-044: unknown hello flag `--hello`"),
        "{stderr}"
    );
    assert!(!stderr.contains("KELD-WV-"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn hello_as_program_path_does_not_select_the_hello_command() {
    use std::os::unix::process::CommandExt as _;

    // argv0 is the program path, never a command: with argv0 `--hello`, the
    // trailing unknown flag must reach the ordinary host refusal, not the
    // hello parser.
    let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
        .arg0("--hello")
        .arg("--not-a-keld-host-flag")
        .output()
        .expect("run host with argv0 set to --hello");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("fixed host refusal is UTF-8");
    assert!(
        stderr.contains("KELD-CLI-044: unknown host argument"),
        "{stderr}"
    );
    assert!(!stderr.contains("unknown hello flag"), "{stderr}");
}

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
