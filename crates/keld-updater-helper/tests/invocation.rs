//! The built helper's refusals, observed as a process: exit status and the typed code
//! on its redirected stderr (KEL-53 §7 "17 (argument shape)" and "17 (helper launch and
//! self-anchor)").
#![allow(clippy::expect_used)] // extra test crate: expect is an assertion oracle
#![allow(clippy::disallowed_methods)] // test-only: Command::output runs the built helper, the product under test

use std::process::{Command, Output};

const HELPER: &str = env!("CARGO_BIN_EXE_keld-updater-helper");

fn run(args: &[&str]) -> (i32, String) {
    let Output {
        status,
        stdout,
        stderr,
    } = Command::new(HELPER)
        .args(args)
        .output()
        .expect("start the built helper");
    assert!(stdout.is_empty(), "the helper writes only to stderr");
    (
        status.code().expect("the helper exits with a status"),
        String::from_utf8(stderr).expect("UTF-8 stderr"),
    )
}

/// The argument check precedes the self-anchor: from the same location, the refused
/// argument returns `KELD-HELPER-001` where an accepted one reaches the image
/// verification and returns `KELD-HELPER-002`. The test binary is unsigned and sits in
/// no installation, so the accepted argument can only end there.
#[cfg(windows)]
#[test]
fn a_refused_argument_is_typed_before_the_self_anchor_runs() {
    let rendezvous =
        r"\\.\pipe\keld-attempt-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let refusals: [&[&str]; 4] = [
        &[],
        &[rendezvous, rendezvous],
        &[
            r"\\server\pipe\keld-attempt-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ],
        &["--recovery-role"],
    ];
    for args in refusals {
        let (code, stderr) = run(args);
        assert_eq!(code, 1, "{args:?}: {stderr}");
        assert!(
            stderr.starts_with("KELD-HELPER-001: "),
            "{args:?}: {stderr}"
        );
        assert!(stderr.ends_with('\n'), "{args:?}: one line: {stderr:?}");
    }

    let (code, stderr) = run(&[rendezvous]);
    assert_eq!(code, 1, "{stderr}");
    assert!(
        stderr.starts_with(
            "KELD-HELPER-002: keld-updater-helper.exe could not verify its own image \
             (WinVerifyTrust rejected the current executable with status 0x800b0100)."
        ),
        "an unsigned helper refuses on its own image verification: {stderr}"
    );
}

#[cfg(not(windows))]
#[test]
fn every_other_platform_refuses_whatever_the_arguments() {
    for args in [&[][..], &["--recovery-role"][..], &["a", "b"][..]] {
        let (code, stderr) = run(args);
        assert_eq!(code, 1, "{args:?}: {stderr}");
        assert!(
            stderr.starts_with(
                "KELD-HELPER-001: keld-updater-helper.exe refused how it was started (it runs \
                 only on Windows)."
            ),
            "{args:?}: {stderr}"
        );
    }
}
