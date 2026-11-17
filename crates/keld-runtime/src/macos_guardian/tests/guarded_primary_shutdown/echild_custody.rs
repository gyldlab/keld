//! An externally reaped Guardian cannot authorize a numeric process kill.
use std::cell::Cell;
use std::io;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use super::super::super::guarded_primary::trap_shutdown_error_kill_for_test;
use super::super::super::process_group::{
    GroupIdentity, TestGroupSignalPoint, install_test_group_signal_hook, observe_group_identity,
};
use super::super::super::{
    GuardedPrimary, RecoveryState, RegisteredPrimaryGeneration, RuntimeError, lock_generation,
};
use super::super::test_support::{
    NativeChildOwner, await_held_zombie_group, reap_owned_child_with_deadline,
};

#[test]
fn externally_reaped_guardian_shutdown_never_kills_stale_pid() {
    let mut child = Command::new("/usr/bin/true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn harmless owned Guardian stand-in");
    let writer = child.stdin.take().expect("owned liveness writer");
    let ack = child.stdout.take().expect("owned acknowledgment reader");
    let raw = i32::try_from(child.id()).expect("positive owned PID");
    let pid = rustix::process::Pid::from_raw(raw).expect("valid test-owned PID");
    let (_, status) = rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::empty())
        .expect("externally reap exactly owned child")
        .expect("wait produced the child's exit status");
    assert!(status.exited(), "original owned child terminated");
    let inspection = child
        .try_wait()
        .expect_err("no cached status after external waitpid");
    assert_eq!(inspection.raw_os_error(), Some(nix::libc::ECHILD));

    let (_, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::new(Mutex::new(None)),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: true,
        shutdown_failed: false,
        orderly_acknowledged: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    };
    // Without the guard this panics before Child::kill reaches the kernel.
    let _kill_trap = trap_shutdown_error_kill_for_test();
    let error = primary
        .shutdown()
        .expect_err("externally reaped child is fatal");
    match error {
        RuntimeError::Lifecycle { phase, source } => {
            assert_eq!(phase, "macOS guardian generation shutdown wait");
            assert_eq!(source.raw_os_error(), Some(nix::libc::ECHILD));
        }
        other => panic!("unexpected Guardian shutdown error: {other}"),
    }
    assert!(
        primary.shutdown_failed,
        "failed terminal state remains sticky"
    );
    assert!(primary.liveness_writer.is_none(), "EOF writer was released");
    let retry = primary
        .shutdown()
        .expect_err("failed shutdown stays failed");
    assert!(
        matches!(retry, RuntimeError::Lifecycle { source, .. }
            if source.kind() == io::ErrorKind::NotConnected),
        "the terminal error cannot be retried as a success"
    );
}

// Prerequisite: both members belong to one fresh test-owned group, and the
// retained leader birth is verified by a second independent kernel read.
fn enrolled_test_owned_group() -> (NativeChildOwner, NativeChildOwner, GroupIdentity) {
    let leader = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("test-owned group leader"),
    );
    let group_pid = leader.id();
    let raw_group = i32::try_from(group_pid).expect("group PID fits i32");
    let group = Pid::from_raw(raw_group);
    assert_ne!(getpgid(None).expect("test harness group"), group);
    assert_eq!(getpgid(Some(group)).expect("live own leader"), group);
    let member = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(raw_group)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("test-owned same-group member"),
    );
    let member_pid = i32::try_from(member.id()).expect("member PID fits i32");
    assert_eq!(
        getpgid(Some(Pid::from_raw(member_pid))).expect("real same-group member"),
        group
    );
    let identity = super::super::test_support::test_group_identity(group_pid);
    assert_eq!(
        observe_group_identity(group_pid).expect("unchanged native leader birth"),
        identity,
        "registration must retain the exact kernel-observed leader birth"
    );
    (leader, member, identity)
}

// The only external reaper in this scenario targets this exact owned Guardian
// PID, never a wildcard waitpid or an unrelated process's status.
fn externally_reaped_owned_guardian() -> (Child, ChildStdin, ChildStdout) {
    let mut guardian = Command::new("/usr/bin/true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("test-owned Guardian stand-in");
    let writer = guardian.stdin.take().expect("sole host writer");
    let ack = guardian.stdout.take().expect("owned acknowledgment reader");
    let raw_guardian = i32::try_from(guardian.id()).expect("Guardian PID fits i32");
    let guardian_pid = rustix::process::Pid::from_raw(raw_guardian).expect("positive Guardian PID");
    assert!(
        rustix::process::waitpid(Some(guardian_pid), rustix::process::WaitOptions::empty())
            .expect("externally reap only the exact Guardian")
            .expect("Guardian produced exit status")
            .1
            .exited(),
        "Guardian itself has exited"
    );
    assert_eq!(
        guardian
            .try_wait()
            .expect_err("std Child cannot observe externally reaped Guardian")
            .raw_os_error(),
        Some(nix::libc::ECHILD)
    );
    (guardian, writer, ack)
}

// ECHILD must not discard enrolled-group custody. Only the *exact* Guardian
// child is externally reaped; the test keeps exclusive wait ownership of both
// native group children. The host's one birth-attested group signal remains
// authorized, but the Guardian's now-stale raw PID never is.
#[test]
fn externally_reaped_guardian_shutdown_retires_enrolled_group_once() {
    let (leader, member, identity) = enrolled_test_owned_group();
    let group_pid = leader.id();
    let raw_group = i32::try_from(group_pid).expect("group PID fits i32");
    let group = Pid::from_raw(raw_group);
    let (guardian, writer, ack) = externally_reaped_owned_guardian();
    let current_generation = Arc::new(Mutex::new(Some(
        RegisteredPrimaryGeneration::authenticated(1, identity),
    )));
    let (_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::clone(&current_generation),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: true,
        shutdown_failed: false,
        orderly_acknowledged: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    };
    let signals = Rc::new(Cell::new(0_u8));
    let counted = Rc::clone(&signals);
    let _signal_hook = install_test_group_signal_hook(
        group_pid,
        Box::new(move |point| {
            assert_eq!(
                point,
                TestGroupSignalPoint::BeforeSignal,
                "no second or denied group signal may gain authority"
            );
            counted.set(counted.get() + 1);
        }),
    );
    // Concurrent exact-child waits model the real Guardian reaper, not a
    // second host signal or permission to abandon an unreaped zombie group.
    let leader_reaper = reap_owned_child_with_deadline(leader);
    let member_reaper = reap_owned_child_with_deadline(member);
    let _stale_pid_trap = trap_shutdown_error_kill_for_test();
    let error = primary
        .shutdown()
        .expect_err("ECHILD remains fatal despite group cleanup");
    match error {
        RuntimeError::Lifecycle { phase, source } => {
            assert_eq!(phase, "macOS guardian generation shutdown wait");
            assert_eq!(source.raw_os_error(), Some(nix::libc::ECHILD));
        }
        other => panic!("unexpected terminal Guardian error: {other}"),
    }
    assert_eq!(
        signals.get(),
        1,
        "one attested group signal, never a second"
    );
    assert!(
        lock_generation(&current_generation).is_none(),
        "retired group released"
    );
    assert!(
        primary.shutdown_failed,
        "fatal shutdown outcome stays sticky"
    );
    assert!(
        primary.liveness_writer.is_none(),
        "host liveness writer released"
    );
    assert!(
        matches!(primary.shutdown(), Err(RuntimeError::Lifecycle { source, .. })
            if source.kind() == io::ErrorKind::NotConnected),
        "a later shutdown may not convert ECHILD into success"
    );
    primary
        .fail_current_generation(1)
        .expect("retired generation cannot authorize another signal");
    assert_eq!(
        signals.get(),
        1,
        "repeat operations never signal the group again"
    );

    // Both test-owned exact children were reaped during the host absence
    // window; the Host never waited on or signalled a stale numeric PID.
    for reaper in [leader_reaper, member_reaper] {
        assert_eq!(
            reaper
                .join()
                .expect("exact owned child reaper completed")
                .signal(),
            Some(nix::libc::SIGKILL)
        );
    }
    assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw_group), None), Err(Errno::ESRCH));
}

// The earlier enrolled ECHILD test reaps both members concurrently and thus
// proves only the successful absence cut. Here the Guardian has already been
// reaped by an exact external waiter, but one group member is deliberately
// held in test-owned zombie custody throughout shipping shutdown().
// Terminal enrollment must survive that failed transaction until a later
// shipping read-only probe sees actual absence; the stale Guardian PID must
// never be killed or waited again, and the original group cannot be signaled
// for a second time.
fn externally_reaped_registered_primary(
    identity: GroupIdentity,
) -> (
    GuardedPrimary,
    Arc<Mutex<Option<RegisteredPrimaryGeneration>>>,
) {
    let (guardian, writer, ack) = externally_reaped_owned_guardian();
    let current_generation = Arc::new(Mutex::new(Some(
        RegisteredPrimaryGeneration::authenticated(1, identity),
    )));
    let (_tx, updates_rx) = mpsc::channel();
    let primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::clone(&current_generation),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: true,
        shutdown_failed: false,
        orderly_acknowledged: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    };
    (primary, current_generation)
}

#[test]
fn externally_reaped_guardian_shutdown_keeps_held_member_until_exact_reap() {
    let (leader, held, identity) = enrolled_test_owned_group();
    let group_pid = leader.id();
    let raw_group = i32::try_from(group_pid).expect("group PID fits i32");
    let group = Pid::from_raw(raw_group);
    let (mut primary, current_generation) = externally_reaped_registered_primary(identity);
    let signals = Rc::new(Cell::new(0_u8));
    let counted = Rc::clone(&signals);
    let _signal_hook = install_test_group_signal_hook(
        group_pid,
        Box::new(move |point| {
            assert_eq!(point, TestGroupSignalPoint::BeforeSignal);
            counted.set(counted.get() + 1);
        }),
    );

    // Reap only the exact leader. The second child stays owned and unreaped
    // across shutdown and its native absence deadline.
    let leader_reaper = reap_owned_child_with_deadline(leader);
    let _stale_guardian_pid_trap = trap_shutdown_error_kill_for_test();
    let error = primary
        .shutdown()
        .expect_err("ECHILD plus unreaped group member is terminal and fatal");
    // Multiple RuntimeError causes are currently flattened by
    // collapse_failures. This asserts diagnostic ECHILD preservation, not
    // structured original-cause preservation (a separate owner decision).
    let message = error.to_string();
    assert!(
        message.contains("macOS guardian generation shutdown wait")
            && message.contains(&io::Error::from_raw_os_error(nix::libc::ECHILD).to_string()),
        "the fatal result retains the original ECHILD cause: {message}"
    );
    assert!(
        message.contains("absence remains unproven"),
        "the held original member must prevent absent-group success: {message}"
    );
    assert!(primary.shutdown_failed, "original failure stays sticky");
    assert!(
        primary.liveness_writer.is_none(),
        "sole writer was released"
    );
    assert_eq!(signals.get(), 1, "one attested group signal only");
    assert_eq!(
        leader_reaper
            .join()
            .expect("exact group leader reaper")
            .signal(),
        Some(nix::libc::SIGKILL)
    );
    await_held_zombie_group(group);
    assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw_group), None), Err(Errno::EPERM));
    {
        let retained = lock_generation(&current_generation);
        let original = retained
            .as_ref()
            .expect("held member preserves the registered generation");
        assert_eq!(original.group_identity, Some(identity));
        assert!(
            matches!(
                original.native_signal,
                super::super::super::guarded_primary::NativeSignalAuthority::Delivered { .. }
            ),
            "the accepted signal has spent the generation authority"
        );
    }
    assert!(
        matches!(primary.shutdown(), Err(RuntimeError::Lifecycle { source, .. })
            if source.kind() == io::ErrorKind::NotConnected),
        "a retry cannot mask the ECHILD failure"
    );
    assert_eq!(signals.get(), 1, "failed shutdown never re-signals");

    assert_eq!(
        reap_owned_child_with_deadline(held)
            .join()
            .expect("bounded exact test-owned member reaper")
            .signal(),
        Some(nix::libc::SIGKILL)
    );
    assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw_group), None), Err(Errno::ESRCH));

    // A shipping terminal owner entry point may now release custody with a
    // read-only absence probe. It must still report ECHILD, not success.
    let final_error = primary
        .poll_fatal()
        .expect_err("externally reaped Guardian is never healthy");
    assert!(
        matches!(final_error, RuntimeError::Lifecycle { phase, source }
            if phase == "macOS guardian generation process inspection"
                && source.raw_os_error() == Some(nix::libc::ECHILD)),
        "terminal owner must retain the original typed inspection failure"
    );
    assert!(
        lock_generation(&current_generation).is_none(),
        "shipping terminal owner releases birth only after the exact member was reaped"
    );
    primary
        .fail_current_generation(1)
        .expect("retired group has no new signal authority");
    assert_eq!(signals.get(), 1, "no later signal was emitted");
    assert!(
        matches!(primary.shutdown(), Err(RuntimeError::Lifecycle { source, .. })
            if source.kind() == io::ErrorKind::NotConnected),
        "absence never rewrites the earlier fatal shutdown outcome"
    );
}
