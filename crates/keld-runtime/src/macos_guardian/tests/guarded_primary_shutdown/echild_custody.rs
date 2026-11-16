//! An externally reaped Guardian cannot authorize a numeric process kill.
use std::io;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};

use super::super::super::guarded_primary::trap_shutdown_error_kill_for_test;
use super::super::super::{GuardedPrimary, RecoveryState, RuntimeError};

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
