//! Cross-platform host argument boundary.
#![allow(clippy::expect_used)] // integration-test process/output assertions

use std::io::Read as _;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

/// Report an error after bounded cleanup of the *test-owned* Host child.
/// Never block after a failed kill or claim unsuccessful cleanup succeeded.
fn stop_owned_host_child(child: &mut Child, reason: &str) -> String {
    if let Err(error) = child.kill()
        && child.try_wait().ok().flatten().is_none()
    {
        return format!("{reason}: could not terminate test-owned Host child: {error}");
    }
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return reason.to_owned(),
            Ok(None) if Instant::now() < until => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                return format!("{reason}: test-owned Host child did not terminate after kill");
            }
            Err(error) => return format!("{reason}: cannot verify Host child cleanup: {error}"),
        }
    }
}

/// Read stderr concurrently into an in-memory 8192-byte buffer. Drain even
/// after overflow so the child cannot block on the pipe; on overflow the
/// waiting test kills only its own child. An inherited pipe that stays open
/// after child exit also has a bounded EOF wait and makes the test fail.
/// This is a test process bound, not a production startup deadline.
fn bounded_host_refusal_checked(command: &mut Command) -> Result<(ExitStatus, String), String> {
    const DEADLINE: Duration = Duration::from_secs(10);
    const STDERR_LIMIT: usize = 8_192;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch only the test-owned Host child");
    let mut stderr = child
        .stderr
        .take()
        .expect("test-owned Host child has a captured stderr pipe");
    let overflow = Arc::new(AtomicBool::new(false));
    let reader_overflow = Arc::clone(&overflow);
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut captured = Vec::with_capacity(STDERR_LIMIT);
        let mut chunk = [0_u8; 4_096];
        let result = loop {
            match stderr.read(&mut chunk) {
                Ok(0) => break Ok(captured),
                Ok(n) => {
                    let remaining = STDERR_LIMIT - captured.len();
                    if n > remaining {
                        reader_overflow.store(true, Ordering::Release);
                    }
                    captured.extend_from_slice(&chunk[..n.min(remaining)]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => break Err(error),
            }
        };
        let _ = send.send(result);
    });
    let until = Instant::now() + DEADLINE;
    let status = loop {
        if overflow.load(Ordering::Acquire) {
            return Err(stop_owned_host_child(
                &mut child,
                "Host refusal exceeded 8192 stderr bytes",
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                return Err(stop_owned_host_child(
                    &mut child,
                    &format!("cannot query test-owned Host child: {error}"),
                ));
            }
        }
        if Instant::now() >= until {
            return Err(stop_owned_host_child(
                &mut child,
                "Host refusal exceeded its ten-second kill switch",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let captured = receive
        .recv_timeout(Duration::from_secs(2))
        .expect("Host refusal stderr pipe did not reach EOF within two seconds")
        .expect("read test-owned Host refusal stderr");
    assert!(
        !overflow.load(Ordering::Acquire),
        "Host refusal emitted more than 8192 bytes"
    );
    Ok((
        status,
        String::from_utf8(captured).expect("Host refusal uses UTF-8"),
    ))
}

/// Convert a bounded child-process refusal into an integration-test assertion.
fn bounded_host_refusal(command: &mut Command) -> (ExitStatus, String) {
    bounded_host_refusal_checked(command)
        .expect("the test-owned Host must refuse without timing out or overflowing stderr")
}

/// This is the original, byte-for-byte uncommissioned Host-owner module from
/// product pin 7ff41173. A caller-path read or alternate implementation must
/// trigger source review rather than silently pass the process refusal tests.
/// A hash is a source-shape regression, NOT runtime/file-I/O/signing evidence.
#[test]
fn original_host_refusal_owner_is_pinned_against_hidden_file_reads() {
    use sha2::{Digest as _, Sha256};
    const REFUSAL: &str = "    Err(InitialInstallRefusal::CatalogAuthorityUndefined)";
    const PINNED_SHA256: &str = "f216c294ba21bcf1606b7189296a65610c2a869da1b38636555dfbeeca394235";
    let original = include_bytes!("../src/windows_initial_install.rs");
    let is_pinned = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes)) == PINNED_SHA256;
    assert!(
        is_pinned(original),
        "uncommissioned original Host owner changed; require independent source/security review"
    );
    // Fail-first single-fault falsifier: an ignored request read leaves both
    // the 001/002 process output and the empty directory unchanged.
    let original_text = std::str::from_utf8(original).expect("pinned source is UTF-8");
    let production = original_text
        .split_once("#[cfg(test)]")
        .expect("the original module has an isolated test-only suffix")
        .0;
    assert_eq!(
        production.matches(REFUSAL).count(),
        1,
        "exactly one production refusal is pinned, independent of unit tests"
    );
    let mutant = original_text.replacen(
        REFUSAL,
        "    let _ = std::fs::read(_untrusted_request);\n    Err(InitialInstallRefusal::CatalogAuthorityUndefined)",
        1,
    );
    assert!(
        !is_pinned(mutant.as_bytes()),
        "inserting a caller-request read must invalidate the pinned original owner"
    );
}

/// Launches the host with one non-Unicode argument and asserts the registered
/// refusal: exit 2, `KELD-CLI-051` naming argv position 1, no panic, and no
/// boot selection (`KELD-CORE-034`) or unknown-argument (`KELD-CLI-044`) path.
#[cfg(any(unix, all(windows, not(feature = "windows-qualified-launch"))))]
fn assert_non_unicode_argv_refused_before_boot(arg: std::ffi::OsString) {
    let (status, stderr) =
        bounded_host_refusal(Command::new(env!("CARGO_BIN_EXE_keld-host")).arg(arg));
    assert_eq!(status.code(), Some(2));
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
    let (status, stderr) = bounded_host_refusal(
        Command::new(env!("CARGO_BIN_EXE_keld-host")).arg("--not-a-keld-host-flag"),
    );
    assert_eq!(status.code(), Some(2));
    assert!(stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
    assert!(!stderr.contains("keld.boot.json"), "{stderr}");
}

#[cfg(all(windows, feature = "windows-qualified-launch"))]
#[test]
fn qualified_host_rejects_invalid_utf16_before_any_boot_or_role() {
    use std::os::windows::ffi::OsStringExt as _;
    let (status, stderr) = bounded_host_refusal(
        Command::new(env!("CARGO_BIN_EXE_keld-host")).arg(std::ffi::OsString::from_wide(&[0xd800])),
    );
    assert_eq!(status.code(), Some(2));
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

/// The original KEL-270 initial-install argument is reserved even when the
/// feature has not been compiled. It must not fall through into the ordinary
/// application boot, developer-role routing or `WebView` initialization.
#[cfg(all(windows, not(feature = "windows-initial-install")))]
#[test]
fn original_initial_install_flag_without_feature_refuses_before_boot() {
    let (status, stderr) = bounded_host_refusal(
        Command::new(env!("CARGO_BIN_EXE_keld-host"))
            .arg("--keld-install-initial-v3")
            .arg(r"C:\untrusted\nonexistent-request.json"),
    );
    assert_eq!(status.code(), Some(2));
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

/// The original Host role is deliberately NOT enabled as an installer. An
/// exact syntactically valid role still reports uncommissioned authority and
/// leaves the test-owned request directory empty, even with the feature compiled.
/// This checks refusal and publication, not attempted file opens.
#[cfg(all(windows, feature = "windows-initial-install"))]
#[test]
fn original_exact_initial_install_role_refuses_uncommissioned_authority() {
    let fixture = tempfile::tempdir().expect("private Windows refusal directory");
    let request = fixture.path().join("missing-untrusted-request.json");
    assert!(!request.exists());
    let (status, stderr) = bounded_host_refusal(
        Command::new(env!("CARGO_BIN_EXE_keld-host"))
            .arg("--keld-install-initial-v3")
            .arg(&request),
    );
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("KELD-HOST-INITIAL-002"), "{stderr}");
    assert!(!stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
    assert!(
        !request.exists(),
        "refusal must not create the missing untrusted request"
    );
    assert_eq!(
        std::fs::read_dir(fixture.path())
            .expect("inspect test-owned refusal directory")
            .count(),
        0,
        "original role refusal must leave the test-owned directory empty"
    );
}

#[cfg(all(windows, feature = "windows-initial-install"))]
#[test]
fn malformed_original_initial_install_locator_is_never_authority() {
    let (status, stderr) = bounded_host_refusal(
        Command::new(env!("CARGO_BIN_EXE_keld-host"))
            .arg("--keld-install-initial-v3")
            .arg("relative-untrusted-request.json"),
    );
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-HOST-INITIAL-002"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

#[cfg(all(windows, feature = "windows-qualified-launch"))]
#[test]
fn qualified_windows_role_rejects_diagnostic_arguments_without_boot() {
    let (status, stderr) =
        bounded_host_refusal(Command::new(env!("CARGO_BIN_EXE_keld-host")).arg("--hello"));
    assert_eq!(status.code(), Some(2));
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

#[cfg(all(windows, feature = "windows-qualified-launch"))]
#[test]
fn qualified_windows_role_refuses_developer_lease_before_boot() {
    let (status, stderr) = bounded_host_refusal(
        Command::new(env!("CARGO_BIN_EXE_keld-host")).env("KELD_DEV_LEASE", "1"),
    );
    assert_eq!(status.code(), Some(2));
    assert!(stderr.contains("KELD-HOST-INITIAL-001"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}
