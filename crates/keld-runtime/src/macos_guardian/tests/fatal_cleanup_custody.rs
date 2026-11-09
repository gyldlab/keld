//! Fatal cleanup must not discard the original birth-attested group witness
//! when the native signal is refused. A failed signal is not a cleared group.
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardedPrimary, JoinHandle, RecoveryState,
    RegisteredPrimaryGeneration, RuntimeError, lifecycle_error, lock_generation,
};
use super::test_support::{NativeChildOwner, test_group_identity};

mod retry_authority;

fn original_group_with_unusable_enrollment() -> (NativeChildOwner, RegisteredPrimaryGeneration) {
    let owned = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own independent native group child"),
    );
    let pid = owned.id();
    let raw = i32::try_from(pid).expect("group PID fits c_int");
    assert_ne!(raw, getpgrp().as_raw(), "test runner must not be signaled");
    assert_eq!(
        getpgid(Some(Pid::from_raw(raw))).expect("group leader remains live"),
        Pid::from_raw(raw)
    );
    // Simulate a reused/mismatching enrollment. The actual live group stays
    // test-owned and MUST NOT receive a numeric PGID signal from the product.
    let wrong_birth = test_group_identity(pid).altered_birth_for_test();
    (
        owned,
        RegisteredPrimaryGeneration::authenticated(1, wrong_birth),
    )
}

fn primary_with_dead_guardian(
    generation: RegisteredPrimaryGeneration,
    status: i32,
    shutdown_attributed: bool,
) -> GuardedPrimary {
    let mut guardian = Command::new("/bin/sh")
        .args(["-c", &format!("exit {status}")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("own terminating guardian stand-in");
    let writer = guardian.stdin.take().expect("host liveness writer");
    let ack = guardian.stdout.take().expect("host acknowledgment reader");
    let (_updates_tx, updates_rx) = mpsc::channel();
    GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::new(Mutex::new(Some(generation))),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed,
        shutdown_failed: false,
        orderly_acknowledged: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    }
}

fn assert_failed_cleanup_retains_original_owner(
    primary: &GuardedPrimary,
    registered: RegisteredPrimaryGeneration,
    child: &mut NativeChildOwner,
) {
    let current = lock_generation(&primary.current_generation);
    let actual = current
        .as_ref()
        .expect("native failure must retain original birth-attested enrollment");
    assert_eq!(actual.attempt, registered.attempt);
    assert_eq!(actual.group_pid, registered.group_pid);
    assert_eq!(actual.group_identity, registered.group_identity);
    drop(current);
    assert!(
        child
            .try_wait()
            .expect("inspect test-owned native child")
            .is_none(),
        "mismatching birth never authorizes signaling unrelated live group"
    );
    let raw = i32::try_from(child.id()).expect("child PID fits c_int");
    assert_eq!(
        getpgid(Some(Pid::from_raw(raw))).expect("independent native group still exists"),
        Pid::from_raw(raw)
    );
}

#[test]
fn guardian_exit_failed_failsafe_retains_original_group_birth() {
    let (mut owned, registered) = original_group_with_unusable_enrollment();
    let mut primary = primary_with_dead_guardian(registered, 17, false);
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let error = loop {
        if let Err(error) = primary.poll_fatal() {
            break error;
        }
        assert!(
            Instant::now() < deadline,
            "guardian exit observation deadline"
        );
        std::thread::yield_now();
    };
    assert!(
        matches!(
            error,
            RuntimeError::GuardianExited {
                group_pid,
                exit_code: Some(17),
                cleanup_error: Some(_),
            } if group_pid == registered.group_pid
        ),
        "guardian exit must disclose native cleanup failure: {error}"
    );
    assert_failed_cleanup_retains_original_owner(&primary, registered, &mut owned);
    assert!(primary.liveness_writer.is_none());
    // The exact Child owner rescues the fixture AFTER the test's native oracle.
}

#[test]
fn failed_shutdown_native_signal_keeps_group_birth_and_terminal_error() {
    let (mut owned, registered) = original_group_with_unusable_enrollment();
    let mut primary = primary_with_dead_guardian(registered, 23, true);
    let first = primary
        .shutdown()
        .expect_err("nonzero guardian and failed native signal");
    assert!(
        first
            .to_string()
            .contains("process-group leader PID was reused"),
        "native birth mismatch must remain visible: {first}"
    );
    assert!(
        primary.shutdown_failed,
        "terminal shutdown cannot become success"
    );
    assert_failed_cleanup_retains_original_owner(&primary, registered, &mut owned);
    let second = primary.shutdown();
    assert!(
        matches!(second, Err(RuntimeError::Lifecycle { ref source, .. })
            if source.kind() == std::io::ErrorKind::NotConnected),
        "second shutdown cannot erase original error: {second:?}"
    );
}

#[test]
fn failed_worker_failsafe_cannot_consume_original_group_birth() {
    let (mut owned, registered) = original_group_with_unusable_enrollment();
    let mut guardian = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("own live guardian stand-in");
    let writer = guardian.stdin.take().expect("sole liveness writer");
    let ack = guardian.stdout.take().expect("acknowledgment reader");
    let (_updates_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::new(Mutex::new(Some(registered))),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: Some(std::thread::spawn(|| {
            Err(lifecycle_error(
                "test fatal worker outcome",
                std::io::Error::other("injected generation host failure"),
            ))
        })),
        shutdown_attributed: false,
        shutdown_failed: false,
        orderly_acknowledged: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    };
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    while !primary.worker.as_ref().is_some_and(JoinHandle::is_finished) {
        assert!(
            Instant::now() < deadline,
            "fatal worker completion deadline"
        );
        std::thread::yield_now();
    }
    assert!(
        primary
            .child
            .try_wait()
            .expect("inspect live guardian")
            .is_none(),
        "test requires worker failure while the Guardian still runs"
    );
    let error = primary
        .poll_fatal()
        .expect_err("unattributed worker failure");
    assert!(
        error.to_string().contains("test fatal worker outcome"),
        "original worker failure must be reported: {error}"
    );
    assert_failed_cleanup_retains_original_owner(&primary, registered, &mut owned);
    // Close and reap the owned /bin/cat independently of the failed group.
    primary.liveness_writer.take();
    assert!(primary.child.wait().expect("reap owned guardian").success());
}
