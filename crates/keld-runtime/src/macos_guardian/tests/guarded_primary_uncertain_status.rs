//! Never treat an unobserved Child status as proof that interrupting the
//! worker is safe, even after the original five-second timeout.
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use super::super::{GuardedPrimary, RecoveryState, lock_recovery};

#[test]
fn externally_reaped_guardian_does_not_authorize_control_cancellation() {
    let mut child = Command::new("/usr/bin/true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("test owns exactly one native child");
    let writer = child.stdin.take().expect("host's liveness writer");
    let ack = child.stdout.take().expect("host's ACK reader");
    // Deliberately consume the original child wait status outside Child.
    // Child::try_wait then fails with ECHILD, without an authoritative
    // ExitStatus in the GuardedPrimary owner.
    let pid =
        rustix::process::Pid::from_raw(i32::try_from(child.id()).expect("native PID fits i32"))
            .expect("positive child PID");
    rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::empty())
        .expect("external native reap")
        .expect("true child has terminal status");

    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).shutdown_acknowledged = true;
    let (_, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::new(Mutex::new(None)),
        recovery_decision: Arc::clone(&recovery),
        updates_rx,
        worker: Some(thread::spawn(move || {
            let _ = release_rx.recv();
            Ok(())
        })),
        shutdown_attributed: true,
        shutdown_failed: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    };
    let shutdown = thread::spawn(move || {
        let result = primary.shutdown();
        done_tx.send(result).expect("retain original owner result");
    });
    assert!(
        done_rx
            .recv_timeout(super::super::GUARDIAN_SHUTDOWN_DEADLINE)
            .is_err(),
        "worker's test-owned release is still withheld"
    );
    release_tx.send(()).expect("release exact owned worker");
    let result = done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("released worker joins");
    shutdown.join().expect("shutdown owner joined");
    assert!(result.is_err(), "ECHILD cannot yield shutdown success");
    assert!(
        lock_recovery(&recovery)
            .guardian_reaped_shutdown_at
            .is_none(),
        "an unobservable Child status minted false authority to abort KGC1"
    );
}
