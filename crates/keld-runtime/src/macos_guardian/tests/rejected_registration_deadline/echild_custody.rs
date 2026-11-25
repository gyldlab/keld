//! An externally reaped Child no longer grants numeric PID signal authority.
use std::process::Command;

use crate::RuntimeError;

use super::super::super::process_group::{
    reap_pending_guardian, trap_pending_guardian_kill_for_test,
};

#[test]
fn externally_reaped_pending_guardian_never_signals_stale_pid() {
    let mut child = Command::new("/usr/bin/true")
        .spawn()
        .expect("spawn exactly owned harmless Guardian stand-in");
    let raw_pid = i32::try_from(child.id()).expect("positive child PID");
    let pid = rustix::process::Pid::from_raw(raw_pid).expect("nonzero child PID");
    let waited = rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::empty())
        .expect("externally reap exactly owned Child")
        .expect("owned Child produced an exit status");
    assert!(waited.1.exited(), "reap saw original child termination");

    let inspection = child
        .try_wait()
        .expect_err("Child has no cached exit status after external waitpid");
    assert_eq!(inspection.raw_os_error(), Some(nix::libc::ECHILD));

    // A regression must fail before sending SIGKILL to a numeric PID whose
    // kernel owner may have changed. The trap is local to this test thread.
    let _kill_trap = trap_pending_guardian_kill_for_test();
    let error = reap_pending_guardian(&mut child).expect_err("ECHILD fails closed");
    match error {
        RuntimeError::Lifecycle { phase, source } => {
            assert_eq!(
                phase,
                "macOS guardian rejected-registration process inspection"
            );
            assert_eq!(source.raw_os_error(), Some(nix::libc::ECHILD));
        }
        other => panic!("unexpected failure after ECHILD: {other}"),
    }
}
