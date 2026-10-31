// Unexpected generation-worker completion is not an attributed shutdown.
// The host must fail closed and retire its still-registered native process group.
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use nix::unistd::Pid;

use super::super::{
    CONTROL_PREPARE, CONTROL_PREPARED, GUARDIAN_SHUTDOWN_DEADLINE, GenerationControlRecord,
    GuardedPrimary, JoinHandle, RecoveryState, RegisteredPrimaryGeneration, RuntimeError,
    lock_generation, lock_recovery, read_generation_control, run_generation_host,
    write_generation_control,
};
use super::test_support::NativeChildOwner;

// The actual Child and generation worker are owned BEFORE the first KGC1
// request. Every failed assertion, I/O error or unwind before PREPARED must
// close the only native peer, reap the exact Child, and join the worker.
// Transfer all three owners only after the authenticated control exchange.
struct PendingGuardianWorker {
    guardian: Option<NativeChildOwner>,
    peer: Option<UnixStream>,
    worker: Option<JoinHandle<Result<(), RuntimeError>>>,
}

impl Drop for PendingGuardianWorker {
    fn drop(&mut self) {
        // Removing the only peer ends the worker's real KGC1 reader even
        // when a handshake assertion panics before the product handoff.
        drop(self.peer.take());
        // The original Child owns its PID. Never rescue via a bare PID/PGID.
        drop(self.guardian.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

// A source-visible panic before PREPARED previously dropped a bare Child and
// detached its live KGC1 worker. Drive the same pre-transfer failure with an
// actual native Child and socket: the pending owner must reap and join them.
#[test]
fn premature_guardian_prepared_failure_reaps_owned_child_and_joins_control_worker() {
    let guardian = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("8")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("retain original native Guardian stand-in"),
    );
    let pid = i32::try_from(guardian.id()).expect("native PID fits i32");
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(pid)))
            .expect("exact test group leader remains live")
            .as_raw(),
        pid,
        "never operate on the harness process group"
    );
    let mut pending = PendingGuardianWorker {
        guardian: Some(guardian),
        peer: None,
        worker: None,
    };
    let (control, peer) = UnixStream::pair().expect("real KGC1 peer");
    pending.peer = Some(peer);
    let current = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    let (updates_tx, _updates_rx) = mpsc::channel();
    let (worker_done_tx, worker_done_rx) = mpsc::sync_channel(1);
    pending.worker = Some(thread::spawn(move || {
        let outcome = run_generation_host(
            super::super::generation_host::GenerationHostAdmission::new(
                u32::try_from(pid).expect("native guardian id"),
                Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
            ),
            control,
            &updates_tx,
            &current,
            &recovery,
            None,
            None,
        );
        let _ = worker_done_tx.send(());
        outcome
    }));
    let cleanup_started = Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _pending = pending;
        panic!("injected failure before first KGC1 PREPARED");
    }));
    assert!(result.is_err(), "negative panic cut did not execute");
    assert!(
        cleanup_started.elapsed() < GUARDIAN_SHUTDOWN_DEADLINE,
        "panic cleanup kept the original KGC1 reader waiting through its five-second deadline"
    );
    worker_done_rx
        .try_recv()
        .expect("the owned control worker finished before the panic was caught");
    assert_eq!(
        nix::sys::wait::waitpid(
            Pid::from_raw(pid),
            Some(nix::sys::wait::WaitPidFlag::WNOHANG)
        ),
        Err(nix::errno::Errno::ECHILD),
        "NativeChildOwner must reap the exact original child before unwinding returns"
    );
}

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
        orderly_acknowledged: false,
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
        orderly_acknowledged: false,
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
        orderly_acknowledged: false,
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

// The control worker remains alive while a native KGC1 peer keeps its
// descriptor open after PREPARE. The real shutdown transaction must not wait
// without limit for that worker once the owned Guardian has been killed.
fn prepared_guardian_worker_fixture() -> (GuardedPrimary, UnixStream) {
    // Independently self-retire if a future regression prevents the fallback
    // from recovering the owning thread and its retained Child.
    let guardian = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("8")
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("owned long-lived guardian stand-in"),
    );
    let mut pending = PendingGuardianWorker {
        guardian: Some(guardian),
        peer: None,
        worker: None,
    };
    let retained = pending.guardian.as_mut().expect("pending original Child");
    let writer = retained.stdin.take().expect("owned guardian liveness pipe");
    let ack = retained.stdout.take().expect("owned guardian ACK pipe");
    let native_guardian = i32::try_from(retained.id()).expect("native Guardian PID fits");
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(native_guardian)))
            .expect("exact native Guardian group leader")
            .as_raw(),
        native_guardian,
        "never target the test harness's process group"
    );
    let (control, peer) = UnixStream::pair().expect("real KGC1 socket pair");
    peer.set_read_timeout(Some(GUARDIAN_SHUTDOWN_DEADLINE))
        .expect("bounded witness receipt");
    pending.peer = Some(peer);
    let current_generation = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).arm();
    let (updates_tx, updates_rx) = mpsc::channel();
    let worker_current = Arc::clone(&current_generation);
    let worker_recovery = Arc::clone(&recovery);
    let guardian_pid = pending.guardian.as_ref().expect("owned Guardian").id();
    pending.worker = Some(thread::spawn(move || {
        run_generation_host(
            super::super::generation_host::GenerationHostAdmission::new(
                guardian_pid,
                Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
            ),
            control,
            &updates_tx,
            &worker_current,
            &worker_recovery,
            None,
            None,
        )
    }));
    let peer = pending.peer.as_mut().expect("pending KGC1 peer");
    write_generation_control(peer, &GenerationControlRecord::new(CONTROL_PREPARE, 1))
        .expect("send first complete KGC1 control frame");
    let prepared = read_generation_control(peer).expect("real generation host PREPARED");
    assert_eq!(prepared.kind, CONTROL_PREPARED);
    assert_eq!(prepared.attempt, 1);
    lock_recovery(&recovery).shutdown_acknowledged = true;
    // No fallible operations follow disarming the pending custody. The
    // shipping GuardedPrimary becomes the only Child and worker owner.
    let peer = pending
        .peer
        .take()
        .expect("transfer KGC1 peer after PREPARED");
    let worker = pending
        .worker
        .take()
        .expect("transfer joined-worker authority");
    let guardian = pending
        .guardian
        .as_mut()
        .expect("transfer Child owner")
        .take();
    let primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation,
        recovery_decision: recovery,
        updates_rx,
        worker: Some(worker),
        shutdown_attributed: true, // Synthetic Guardian cannot emit S/KSA.
        shutdown_failed: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    };
    (primary, peer)
}

#[test]
fn guarded_primary_shutdown_bounds_worker_join_after_guardian_kill() {
    let (mut primary, peer) = prepared_guardian_worker_fixture();
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let shutdown_owner = thread::spawn(move || {
        let result = primary.shutdown();
        result_tx
            .send((result, primary))
            .expect("return original Guardian owner to test parent");
    });

    // An independent retained native socket peer deliberately stays open
    // past the original five-second shutdown budget. Only then release it,
    // so a failed regression still joins and reaps every owned resource.
    let bounded = result_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE + Duration::from_millis(200))
        .ok();
    let met_deadline = bounded.is_some();
    drop(peer);
    let (result, mut primary) = match bounded {
        Some(value) => value,
        None => result_rx
            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("socket release unblocks original owned worker"),
    };
    shutdown_owner
        .join()
        .expect("owned shutdown caller returns");
    let guardian_status = primary
        .child
        .try_wait()
        .expect("observe exact owned Guardian child after teardown");
    assert!(guardian_status.is_some(), "native child was not reaped");
    assert!(primary.worker.is_none(), "generation worker still unjoined");
    assert!(
        result.is_err(),
        "forced Guardian kill cannot be a clean exit"
    );
    assert!(
        met_deadline,
        "Guardian shutdown exceeded its original five-second budget while the real KGC1 control worker was held by a live native socket"
    );
}
