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

/// GH-1118: a private host role invoked outside its argv contract is `KELD-CORE-044`,
/// never the no-flag session code `KELD-CORE-037` and never a code another crate owns.
#[cfg(target_os = "macos")]
mod private_roles_macos {
    use std::process::{Command, Stdio};

    #[test]
    fn malformed_guardian_argv_is_core_044_before_any_guardian_work() {
        let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
            .arg(keld_runtime::macos_guardian::SUPERVISED_GUARDIAN_ARG)
            .arg("only-one-argument")
            .stdin(Stdio::null())
            .output()
            .expect("run host with a malformed private guardian argv");
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr).expect("host error is UTF-8");
        assert!(stderr.contains("KELD-CORE-044"), "{stderr}");
        assert!(
            stderr.contains("guardian invocation is malformed"),
            "{stderr}"
        );
        assert!(!stderr.contains("KELD-CORE-037"), "{stderr}");

        // Negative control: a well-shaped argv is not misuse, so the misuse code is
        // absent and the two other conditions surface under their own codes. The
        // guardian's primary failure (its liveness validation refuses a non-pipe
        // stdin; boot_admission.rs owns that oracle) is the kept no-flag session code,
        // and a test binary never sits in a `.keld/dev/<nonce>` stage, so the stage
        // refusal is deterministic.
        let temp = tempfile::tempdir().expect("shaped-argv fixture");
        let shaped = Command::new(env!("CARGO_BIN_EXE_keld-host"))
            .arg(keld_runtime::macos_guardian::SUPERVISED_GUARDIAN_ARG)
            .arg(temp.path())
            .arg("entry.ts")
            .arg("1")
            .arg("1")
            .env_remove("KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION")
            .stdin(Stdio::null())
            .output()
            .expect("run host with a well-shaped private guardian argv");
        assert!(
            !shaped.status.success(),
            "shaped argv without a host became success"
        );
        let shaped_stderr = String::from_utf8(shaped.stderr).expect("host error is UTF-8");
        assert!(!shaped_stderr.contains("KELD-CORE-044"), "{shaped_stderr}");
        assert!(
            shaped_stderr.contains("KELD-CORE-037: supervised Bun guardian failed"),
            "{shaped_stderr}"
        );
        assert!(
            shaped_stderr.contains(
                "KELD-CORE-045: guardian is not executing from a validated private dev stage"
            ),
            "{shaped_stderr}"
        );
    }

    /// The KEL-135 fixture roles exist only in a debug host built with
    /// `profile-test-hooks`; a bare host refuses the same argv as an unknown argument.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    #[test]
    fn malformed_profile_fixture_argv_is_core_044_not_wv_009() {
        for role in [
            "--keld-profile-identity-fixture-v1",
            "--keld-profile-presence-fixture-v1",
            "--keld-profile-purge-fixture-v1",
            "--keld-profile-webview-fixture-v1",
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
                .arg(role)
                .arg("unexpected-extra-argument")
                .stdin(Stdio::null())
                .output()
                .expect("run host with a malformed fixture argv");
            assert_eq!(output.status.code(), Some(2), "{role}");
            let stderr = String::from_utf8(output.stderr).expect("host error is UTF-8");
            assert!(stderr.contains("KELD-CORE-044"), "{role}: {stderr}");
            assert!(stderr.contains("fixture invocation"), "{role}: {stderr}");
            // Negative controls: the profile-selection owner's code and the no-flag
            // session code both stay out of an argv refusal.
            assert!(!stderr.contains("KELD-WV-009"), "{role}: {stderr}");
            assert!(!stderr.contains("KELD-CORE-037"), "{role}: {stderr}");
        }
    }
}

/// GH-1118: the Windows private roles report argv misuse as `KELD-CORE-044`; the
/// cleanup sentinel's own runtime failures are `KELD-CORE-046`, never `KELD-CORE-037`.
#[cfg(windows)]
mod private_roles_windows {
    use std::process::{Command, Stdio};

    #[test]
    fn malformed_cleanup_sentinel_argv_is_core_044() {
        let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
            .arg("--keld-windows-dev-stage-cleanup-v1")
            .arg("only-one-argument")
            .stdin(Stdio::null())
            .output()
            .expect("run host with a malformed private sentinel argv");
        assert_eq!(output.status.code(), Some(1));
        // The sentinel reports on stdout: its stderr is the inherited Job handle.
        let stdout = String::from_utf8(output.stdout).expect("sentinel report is UTF-8");
        // The sentinel's own report leads; nested causes may carry other crates' codes.
        assert!(stdout.starts_with("KELD-CORE-044: "), "{stdout}");
        assert!(
            stdout.contains("cleanup invocation is malformed"),
            "{stdout}"
        );
        assert!(!stdout.contains("KELD-CORE-037"), "{stdout}");
        assert!(!stdout.contains("KELD-CORE-046"), "{stdout}");

        // Negative control: a well-shaped argv is not misuse. With a pipe where the
        // inherited Job handle belongs, the sentinel fails its own Job admission,
        // which is the sentinel's runtime code and not the argv code.
        let shaped = Command::new(env!("CARGO_BIN_EXE_keld-host"))
            .arg("--keld-windows-dev-stage-cleanup-v1")
            .arg(std::env::temp_dir())
            .arg(std::process::id().to_string())
            .stdin(Stdio::null())
            .output()
            .expect("run host with a well-shaped private sentinel argv");
        assert_eq!(shaped.status.code(), Some(1));
        let shaped_stdout = String::from_utf8(shaped.stdout).expect("sentinel report is UTF-8");
        // One sentinel, one leading code: the keld-core stage validate/delete steps and the
        // host's own Job/readiness/release steps all report KELD-CORE-046.
        assert!(
            shaped_stdout.starts_with("KELD-CORE-046: "),
            "{shaped_stdout}"
        );
        assert!(
            shaped_stdout.contains("cleanup Job admission failed"),
            "{shaped_stdout}"
        );
        assert!(!shaped_stdout.contains("KELD-CORE-044"), "{shaped_stdout}");
        assert!(!shaped_stdout.contains("KELD-CORE-037"), "{shaped_stdout}");
    }

    /// Without `kel135-profile-test-root` the probe role is unavailable; with it and no
    /// selected root the invocation is malformed. Both are misuse of a private role.
    #[test]
    fn profile_test_root_probe_misuse_is_core_044() {
        let output = Command::new(env!("CARGO_BIN_EXE_keld-host"))
            .arg("--keld-profile-test-root-probe-v1")
            .env_remove("KELD_PROFILE_TEST_ROOT")
            .stdin(Stdio::null())
            .output()
            .expect("run host probe without a selected root");
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8(output.stderr).expect("host error is UTF-8");
        assert!(stderr.contains("KELD-CORE-044"), "{stderr}");
        assert!(stderr.contains("profile test-root probe"), "{stderr}");
        assert!(!stderr.contains("KELD-CORE-037"), "{stderr}");
    }
}
