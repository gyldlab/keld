//! `HostGuardian`'s legacy fatal owner must not discard a group birth witness
//! when the kernel denies a native signal. A native error is not group absence.
use std::io;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::process_group::{
    install_test_group_signal_counter, require_attested_group_absence, terminate_registered_group,
};
use super::super::{GUARDIAN_SHUTDOWN_DEADLINE, GuardianCleanupCauses, HostGuardian, RuntimeError};
use super::test_support::{NativeChildOwner, await_held_zombie_group, test_group_identity};

fn make_owned_group_and_guardian(
    guardian_program: &str,
    alter_birth: bool,
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
    let enrollment = if alter_birth {
        guardian.alter_group_birth_for_test()
    } else {
        original_birth
    };
    (group, guardian, enrollment)
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

fn assert_fatal_has_native_cleanup_error(error: &RuntimeError, expected_group: u32) {
    match error {
        RuntimeError::GuardianExited {
            group_pid,
            cleanup_error: Some(error),
            ..
        } => {
            assert_eq!(*group_pid, expected_group);
            assert!(
                error
                    .to_string()
                    .contains("process-group leader PID was reused"),
                "must report the actual native group birth mismatch: {error}"
            );
            let causes = error
                .get_ref()
                .and_then(|source| source.downcast_ref::<GuardianCleanupCauses>())
                .expect("legacy GuardianExited preserves original typed cleanup cause");
            assert_eq!(causes.failures().len(), 1, "one native signal refusal");
            assert!(
                matches!(&causes.failures()[0], RuntimeError::Lifecycle { phase, source }
                    if *phase == "macOS guardian registered group identity changed"
                        && source.kind() == io::ErrorKind::PermissionDenied),
                "original native birth mismatch remains a typed lifecycle permission denial"
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
        make_owned_group_and_guardian("read ignored || exit 17", true);
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
    assert_fatal_has_native_cleanup_error(&error, expected.pid);
    assert_fatal_has_native_cleanup_error(&error.clone(), expected.pid);
    assert_retained_after_native_failure(&guardian, &mut group, expected);
}

#[test]
fn legacy_wait_fatal_retains_enrollment_after_refused_native_signal() {
    let (mut group, mut guardian, expected) =
        make_owned_group_and_guardian("read ignored || exit 17", true);
    kill_known_guardian(&guardian);
    let error = guardian
        .wait_fatal()
        .expect_err("unexpected Guardian exit must be fatal and birth-checked");
    assert_fatal_has_native_cleanup_error(&error, expected.pid);
    assert_fatal_has_native_cleanup_error(&error.clone(), expected.pid);
    assert_retained_after_native_failure(&guardian, &mut group, expected);
}

#[test]
fn legacy_failed_child_inspection_retains_native_birth_for_safe_custody() {
    let (mut group, mut guardian, expected) =
        make_owned_group_and_guardian("read ignored || exit 0", true);
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
        make_owned_group_and_guardian("read ignored || exit 0", true);
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
    let (mut group, mut guardian, expected) =
        make_owned_group_and_guardian("exec /bin/sleep 30", true);
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

#[test]
fn legacy_zero_exit_never_releases_birth_before_held_group_members_are_reaped() {
    // Real kernel birth and TWO test-owned members of the same group. The
    // product Guardian owns neither Child wait, so its successful SIGKILL
    // must not be mistaken for a complete process-group absence witness.
    let (mut leader, mut guardian, birth) =
        make_owned_group_and_guardian("read ignored || exit 0", false);
    let group_pid = leader.id();
    let group_raw = i32::try_from(group_pid).expect("owned positive group fits c_int");
    assert_ne!(group_raw, getpgrp().as_raw());
    let mut member = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(group_raw)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own second exact group member"),
    );
    let member_pid = Pid::from_raw(i32::try_from(member.id()).expect("member PID fits"));
    assert_eq!(
        getpgid(Some(member_pid)),
        Ok(Pid::from_raw(group_raw)),
        "both owned native children share the authenticated group"
    );
    // This process-wide counter observes every product killpg boundary for
    // this PGID, including legacy raw-group cleanup from another thread.
    let signals = install_test_group_signal_counter(group_pid);
    let error = guardian
        .shutdown_until(Instant::now() + Duration::from_secs(1))
        .expect_err("SIGKILL delivery before exact reaps cannot prove clean shutdown");
    assert!(
        matches!(&error, RuntimeError::Lifecycle { phase, source }
            if *phase == "macOS guardian generation clear group absence"
                && source.kind() == io::ErrorKind::TimedOut),
        "original native absence failure must remain typed: {error:?}"
    );
    assert_eq!(signals.count(), 1, "one attested group signal only");
    assert_eq!(
        guardian.group_identity_for_test(),
        Some(birth),
        "original group birth is retained while its two members are unreaped"
    );
    // The separate native parent deliberately withholds both Child::waits.
    // Darwin reports EPERM for a zombie-only group, never dual ESRCH.
    await_held_zombie_group(Pid::from_raw(group_raw));
    let mut fresh_member = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(group_raw)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("join the still-enrolled group after its first SIGKILL"),
    );
    assert_eq!(
        getpgid(Some(Pid::from_raw(
            i32::try_from(fresh_member.id()).expect("fresh member PID fits")
        ))),
        Ok(Pid::from_raw(group_raw)),
        "late joiner is a distinct authentic member of the retained group"
    );
    let second = guardian.shutdown();
    assert!(
        matches!(second, Err(RuntimeError::Lifecycle { source, .. })
            if source.kind() == io::ErrorKind::NotConnected),
        "terminal HostGuardian must not re-enter shutdown after failed absence"
    );
    assert_eq!(signals.count(), 1, "terminal retry must not signal again");

    // SIGKILL delivery is asynchronous. Send SIGTERM to the exact late
    // joiner and inspect its wait status; a product group SIGKILL must win
    // this race and make the oracle RED instead of being masked by our own
    // SIGKILL. The signal counter remains an additional independent guard.
    let fresh_member_pid =
        Pid::from_raw(i32::try_from(fresh_member.id()).expect("fresh member PID fits"));
    kill(fresh_member_pid, Some(Signal::SIGTERM))
        .expect("send SIGTERM only to the exact test-owned late joiner");
    assert_eq!(
        fresh_member.wait().expect("reap exact new member").signal(),
        Some(Signal::SIGTERM as i32),
        "the late joiner must exit from test-owned SIGTERM, not product SIGKILL"
    );
    assert_eq!(member.wait().expect("reap exact member").signal(), Some(9));
    assert_eq!(leader.wait().expect("reap exact leader").signal(), Some(9));
    let group = Pid::from_raw(group_raw);
    assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-group_raw), None), Err(Errno::ESRCH));
    require_attested_group_absence(birth, Instant::now() + Duration::from_secs(1))
        .expect("read-only absence owner accepts dual ESRCH after both exact reaps");
    assert_eq!(
        guardian.group_identity_for_test(),
        Some(birth),
        "terminal fatal custody is sticky even after a later clean kernel observation"
    );
    drop(guardian);
    assert_eq!(
        signals.count(),
        1,
        "Guardian Drop must not signal the retained PGID again"
    );
}

#[test]
fn group_signal_counter_observes_raw_cleanup_from_another_thread() {
    let mut leader = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn a test-owned process-group leader"),
    );
    let group = leader.id();
    let group_pid = Pid::from_raw(i32::try_from(group).expect("positive process-group PID"));
    assert_eq!(
        getpgid(Some(group_pid)),
        Ok(group_pid),
        "the child is the leader of its own process group"
    );

    let signals = install_test_group_signal_counter(group);
    std::thread::spawn(move || terminate_registered_group(group))
        .join()
        .expect("raw product group cleanup thread must join")
        .expect("raw product group cleanup must signal the exact test-owned group");

    assert_eq!(
        signals.count(),
        1,
        "the process-wide counter must observe a killpg attempt from another thread"
    );
    assert_eq!(
        leader
            .wait()
            .expect("reap the exact test-owned group leader")
            .signal(),
        Some(9),
        "the kernel exit status confirms the product SIGKILL"
    );
}
