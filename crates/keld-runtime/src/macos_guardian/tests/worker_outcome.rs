// Unexpected generation-worker completion is not an attributed shutdown.
// The host must fail closed and retire its still-registered native process group.
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Instant;

use nix::unistd::Pid;

use super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardedPrimary, JoinHandle, RecoveryState,
    RegisteredPrimaryGeneration, lock_generation, lock_recovery,
};
use super::test_support::NativeChildOwner;

#[test]
fn premature_successful_generation_worker_exit_reaps_unattributed_group() {
    let mut group = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("isolated native Bun-group stand-in"),
    );
    let group_pid = group.id();
    let raw_pid = i32::try_from(group_pid).expect("PID fits c_int");
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(raw_pid)))
            .expect("test group leader is live")
            .as_raw(),
        raw_pid,
        "must never signal the test runner process group"
    );

    let mut guardian = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("persistent guardian stand-in");
    let writer = guardian.stdin.take().expect("sole host liveness writer");
    let ack = guardian.stdout.take().expect("owned acknowledgment reader");
    let current_generation = Arc::new(Mutex::new(Some(RegisteredPrimaryGeneration {
        attempt: 1,
        group_pid,
        group_identity: Some(super::test_support::test_group_identity(group_pid)),
    })));
    let (_updates_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::clone(&current_generation),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: Some(thread::spawn(|| Ok(()))),
        shutdown_attributed: false,
        shutdown_failed: false,
        shutdown_sent: None,
        #[cfg(test)]
        shutdown_denial_fence: None,
    };
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    while !primary.worker.as_ref().is_some_and(JoinHandle::is_finished) {
        assert!(Instant::now() < deadline, "worker completion deadline");
        thread::yield_now();
    }
    assert!(
        primary
            .child
            .try_wait()
            .expect("inspect live guardian")
            .is_none(),
        "test must observe successful worker exit while guardian remains live"
    );

    let result = primary.poll_fatal();
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let mut group_status = group.try_wait().expect("inspect live registered group");
    while group_status.is_none() && Instant::now() < deadline {
        thread::yield_now();
        group_status = group.try_wait().expect("observe native group retirement");
    }
    let group_retired = group_status.is_some();
    if !group_retired {
        // This fixture has one native group member and retains its Child.
        group.kill().expect("kill exact unreaped native test child");
        group.wait().expect("reap exact native test child");
    }
    primary.shutdown_attributed = true; // Avoid stand-in Q/S ACK I/O.
    let guardian_shutdown = primary.shutdown();
    assert!(
        guardian_shutdown.is_ok(),
        "guardian cleanup: {guardian_shutdown:?}"
    );

    let error = result.expect_err("unattributed worker exit must be fatal");
    assert!(
        error.to_string().contains("acknowledged shutdown"),
        "missing typed premature-worker failure: {error}"
    );
    assert!(group_retired, "worker success left the native group alive");
    assert!(
        lock_generation(&current_generation).is_none(),
        "host retained stale registered group authority"
    );
}

// Even an acknowledged Quit does not authorize success if the control
// worker/guardian left an enrolled process group on the host side.
#[test]
fn zero_guardian_exit_with_successful_worker_reaps_registered_group() {
    let mut group = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("native owned group fixture"),
    );
    let group_pid = group.id();
    let raw_pid = i32::try_from(group_pid).expect("group PID fits c_int");
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(raw_pid)))
            .expect("native group registered")
            .as_raw(),
        raw_pid,
        "test must not signal the harness process group"
    );

    let mut guardian = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("owned zero-exit guardian");
    let writer = guardian.stdin.take().expect("liveness writer");
    let ack = guardian.stdout.take().expect("ack reader");
    let current_generation = Arc::new(Mutex::new(Some(RegisteredPrimaryGeneration {
        attempt: 1,
        group_pid,
        group_identity: Some(super::test_support::test_group_identity(group_pid)),
    })));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).shutdown_acknowledged = true;
    let (_updates_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::clone(&current_generation),
        recovery_decision: recovery,
        updates_rx,
        worker: Some(thread::spawn(|| Ok(()))),
        shutdown_attributed: true,
        shutdown_failed: false,
        shutdown_sent: None,
        #[cfg(test)]
        shutdown_denial_fence: None,
    };

    let result = primary.shutdown();
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let mut group_status = group.try_wait().expect("inspect native child");
    while group_status.is_none() && Instant::now() < deadline {
        thread::yield_now();
        group_status = group.try_wait().expect("observe group cleanup");
    }
    let group_retired = group_status.is_some();
    if !group_retired {
        group.kill().expect("kill exact unreaped native test child");
        group.wait().expect("reap exact native test child");
    }
    let error = result.expect_err("registered group cannot survive successful shutdown");
    assert!(
        error.to_string().contains("registered group"),
        "missing orphan-group lifecycle failure: {error}"
    );
    assert!(group_retired, "zero guardian exit leaked a native group");
    assert!(
        lock_generation(&current_generation).is_none(),
        "host retained a stale generation after guardian exit"
    );
}

// A child may be reaped outside std::process::Child, making try_wait
// return ECHILD. Inspection failure must not strand the enrolled group.
#[test]
fn poll_fatal_inspection_error_reaps_registered_group() {
    let mut group = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("isolated native process group"),
    );
    let group_pid = group.id();
    let raw_group = i32::try_from(group_pid).expect("group id fits c_int");
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(raw_group)))
            .expect("owned group remains alive")
            .as_raw(),
        raw_group,
        "never target the test runner group"
    );

    let mut guardian = Command::new("/usr/bin/true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("owned guardian stand-in");
    let writer = guardian.stdin.take().expect("liveness writer");
    let ack = guardian.stdout.take().expect("ack reader");
    // Deliberately consume the OS child status without updating Child's cache.
    let raw_guardian = i32::try_from(guardian.id()).expect("guardian id fits c_int");
    let guardian_pid = rustix::process::Pid::from_raw(raw_guardian).expect("positive guardian id");
    rustix::process::waitpid(Some(guardian_pid), rustix::process::WaitOptions::empty())
        .expect("reap guardian externally")
        .expect("wait produced exited guardian");

    let current_generation = Arc::new(Mutex::new(Some(RegisteredPrimaryGeneration {
        attempt: 1,
        group_pid,
        group_identity: Some(super::test_support::test_group_identity(group_pid)),
    })));
    let (_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::clone(&current_generation),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: false,
        shutdown_failed: false,
        shutdown_sent: None,
        #[cfg(test)]
        shutdown_denial_fence: None,
    };
    let result = primary.poll_fatal();
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let mut group_status = group.try_wait().expect("inspect registered group");
    while group_status.is_none() && Instant::now() < deadline {
        thread::yield_now();
        group_status = group.try_wait().expect("observe group retirement");
    }
    let group_retired = group_status.is_some();
    if !group_retired {
        // A red-phase regression must never leave its own group alive.
        group.kill().expect("kill exact unreaped native test child");
        group.wait().expect("reap exact native test child");
    }
    // The guardian was already externally reaped, so avoid Drop's Q/S I/O.
    primary.liveness_writer.take();
    let error = result.expect_err("failed child inspection must be fatal");
    assert!(
        error.to_string().contains("generation process inspection"),
        "unexpected inspection error: {error}"
    );
    assert!(group_retired, "inspection failure stranded a native group");
    assert!(
        lock_generation(&current_generation).is_none(),
        "failed guardian inspection retained stale group authority"
    );
}
