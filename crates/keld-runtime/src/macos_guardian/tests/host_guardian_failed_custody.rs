//! `HostGuardian`'s legacy fatal owner must not discard a group birth witness
//! when the kernel denies a native signal. A native error is not group absence.
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::Instant;

use nix::sys::signal::{Signal, kill};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::{GUARDIAN_SHUTDOWN_DEADLINE, HostGuardian, RuntimeError};
use super::test_support::{NativeChildOwner, test_group_identity};

fn make_owned_group_and_guardian(
    guardian_program: &str,
) -> (
    NativeChildOwner,
    HostGuardian,
    super::super::process_group::GroupIdentity,
) {
    let group = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own isolated real group leader"),
    );
    let group_pid = group.id();
    let group_raw = i32::try_from(group_pid).expect("positive group PID fits c_int");
    assert_ne!(group_raw, getpgrp().as_raw());
    assert_eq!(
        getpgid(Some(Pid::from_raw(group_raw))).expect("real group leader exists"),
        Pid::from_raw(group_raw)
    );
    let original_birth = test_group_identity(group_pid);
    let mut child = Command::new("/bin/sh")
        .args(["-c", guardian_program])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("own initially live Guardian");
    let writer = child.stdin.take().expect("sole host liveness writer");
    let mut guardian = HostGuardian::from_registered_parts(child, writer, None, original_birth)
        .expect("register actual independent Guardian with genuine kernel birth");
    let altered_birth = guardian.alter_group_birth_for_test();
    (group, guardian, altered_birth)
}

fn assert_retained_after_native_failure(
    guardian: &HostGuardian,
    owned_group: &mut NativeChildOwner,
    original_enrollment: super::super::process_group::GroupIdentity,
) {
    assert_eq!(
        guardian.group_identity_for_test(),
        Some(original_enrollment),
        "failure must retain the exact original enrolled kernel birth"
    );
    assert_eq!(guardian.group_pid(), Some(original_enrollment.pid));
    assert!(
        owned_group
            .try_wait()
            .expect("observe still-owned native group")
            .is_none(),
        "wrong-birth enrollment must NOT authorize SIGKILL to an unrelated group"
    );
    let pid = Pid::from_raw(i32::try_from(owned_group.id()).expect("group pid fits"));
    assert_eq!(
        getpgid(Some(pid)).expect("native group leader must remain"),
        pid
    );
    // Owned Child Drop kills/waits only after these independent kernel facts.
}

fn kill_known_guardian(guardian: &HostGuardian) {
    let pid = Pid::from_raw(i32::try_from(guardian.guardian_pid()).expect("guardian PID fits"));
    kill(pid, Signal::SIGKILL).expect("stop the exact owned live Guardian child");
}

fn assert_fatal_has_native_cleanup_error(error: RuntimeError, expected_group: u32) {
    match error {
        RuntimeError::GuardianExited {
            group_pid,
            cleanup_error: Some(error),
            ..
        } => {
            assert_eq!(group_pid, expected_group);
            assert!(
                error
                    .to_string()
                    .contains("process-group leader PID was reused"),
                "must report the actual native group birth mismatch: {error}"
            );
        }
        other => {
            panic!("expected attributed fatal exit with native cleanup failure, got {other:?}")
        }
    }
}

#[test]
fn legacy_poll_fatal_retains_enrollment_after_refused_native_signal() {
    let (mut group, mut guardian, expected) =
        make_owned_group_and_guardian("read ignored || exit 17");
    kill_known_guardian(&guardian);
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let error = loop {
        if let Err(error) = guardian.poll_fatal() {
            break error;
        }
        assert!(
            Instant::now() < deadline,
            "Guardian death observation deadline"
        );
        std::thread::yield_now();
    };
    assert_fatal_has_native_cleanup_error(error, expected.pid);
    assert_retained_after_native_failure(&guardian, &mut group, expected);
}

#[test]
fn legacy_wait_fatal_retains_enrollment_after_refused_native_signal() {
    let (mut group, mut guardian, expected) =
        make_owned_group_and_guardian("read ignored || exit 17");
    kill_known_guardian(&guardian);
    let error = guardian
        .wait_fatal()
        .expect_err("unexpected Guardian exit must be fatal and birth-checked");
    assert_fatal_has_native_cleanup_error(error, expected.pid);
    assert_retained_after_native_failure(&guardian, &mut group, expected);
}

#[test]
fn legacy_failed_child_inspection_retains_native_birth_for_safe_custody() {
    let (mut group, mut guardian, expected) =
        make_owned_group_and_guardian("read ignored || exit 0");
    let raw_guardian = i32::try_from(guardian.guardian_pid()).expect("positive guardian PID");
    kill_known_guardian(&guardian);
    // Independently consume the exact owned Guardian's OS status without
    // setting std::process::Child's cached status; poll_fatal must observe a
    // real ECHILD inspection error and route through fail_observation.
    let guardian_pid = rustix::process::Pid::from_raw(raw_guardian).expect("guardian PID nonzero");
    rustix::process::waitpid(Some(guardian_pid), rustix::process::WaitOptions::empty())
        .expect("reap exact test-owned Guardian externally")
        .expect("native Guardian exit status available");
    let error = guardian
        .poll_fatal()
        .expect_err("already externally reaped Guardian must fail process inspection");
    assert!(
        error.to_string().contains("process-handle inspection"),
        "preserve the original inspection failure: {error}"
    );
    assert!(
        error
            .to_string()
            .contains("process-group leader PID was reused"),
        "native group cleanup refusal remains independently visible: {error}"
    );
    assert_retained_after_native_failure(&guardian, &mut group, expected);
}

#[test]
fn legacy_clean_exit_cannot_miscount_failed_group_cleanup_as_shutdown() {
    let (mut group, mut guardian, expected) =
        make_owned_group_and_guardian("read ignored || exit 0");
    let error = guardian
        .shutdown()
        .expect_err("successful Guardian EOF still requires birth-attested group cleanup");
    assert!(
        error
            .to_string()
            .contains("process-group leader PID was reused"),
        "must expose native refusal, not a clean shutdown: {error}"
    );
    assert_retained_after_native_failure(&guardian, &mut group, expected);
    let again = guardian.shutdown();
    assert!(
        matches!(again, Err(RuntimeError::Lifecycle { ref source, .. })
            if source.kind() == std::io::ErrorKind::NotConnected),
        "a prior failed shutdown cannot turn into a clean reused terminal status: {again:?}"
    );
}

#[test]
fn legacy_shutdown_timeout_retains_failed_group_cleanup_witness() {
    let (mut group, mut guardian, expected) = make_owned_group_and_guardian("exec /bin/sleep 30");
    let error = guardian
        .shutdown_until(Instant::now())
        .expect_err("owned Guardian must hit immediate existing deadline");
    assert!(
        error.to_string().contains("shutdown deadline"),
        "original deadline must remain fatal: {error}"
    );
    assert!(
        error
            .to_string()
            .contains("process-group leader PID was reused"),
        "native cleanup failure must not be swallowed: {error}"
    );
    assert_retained_after_native_failure(&guardian, &mut group, expected);
}
