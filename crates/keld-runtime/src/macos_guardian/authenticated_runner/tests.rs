//! Real KGR1 terminal failure must report denied native group cleanup without
//! waiting indefinitely for a still-executing, independently test-owned Child.
use super::run_live_group;
use crate::macos_guardian::process_group::{observe_group_identity, terminate_attested_group};
use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::sys::wait::{WaitPidFlag, waitpid as native_waitpid};
use nix::unistd::{Pid, getpgid, getpgrp, pipe};
use std::fs::File;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

// Own the actual spawned Child from the first fallible fixture assertion
// until it is transferred to run_live_group. A setup panic must never leave
// the native /bin/sleep leader executing without an owned direct-child wait.
struct PendingDirectChild(Option<Child>);

impl PendingDirectChild {
    fn take(&mut self) -> Child {
        self.0.take().expect("transfer exact native child once")
    }
}

impl Drop for PendingDirectChild {
    fn drop(&mut self) {
        let Some(child) = self.0.as_mut() else {
            return;
        };
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

#[test]
fn denied_attested_kgr1_group_signal_never_blocks_on_live_direct_child_wait() {
    let mut child = PendingDirectChild(Some(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own original native KGR1-style group leader"),
    ));
    let pid = child.0.as_ref().expect("original child owner").id();
    let raw = i32::try_from(pid).expect("original native group PID");
    assert_ne!(
        Pid::from_raw(raw),
        getpgrp(),
        "never use the test runner's PGID"
    );
    let original = observe_group_identity(pid).expect("retain live original kernel birth");
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Ok(Pid::from_raw(raw)));

    let (reader, writer) = pipe().expect("owned true host-liveness pipe");
    drop(writer); // Actual host EOF is what starts terminal group cleanup.
    let stale_birth = original.altered_birth_for_test();
    let (tx, rx) = mpsc::channel();
    let child = child.take();
    let worker = std::thread::spawn(move || {
        let result = run_live_group(child, stale_birth, File::from(reader), || Ok(()));
        let _ = tx.send(result);
    });

    // Sample the product result and actual direct-child disappearance BEFORE
    // independent rescue. Wrong-birth group authority MUST stay denied, but
    // the retained unreaped Child may still be killed as its original owner.
    // A mutant which only returns Err and drops that live Child is also RED.
    // The test subsequently rescues with its TRUE retained kernel birth even
    // in an old unbounded-wait or live-child-drop variant.
    let early = rx.recv_timeout(Duration::from_millis(400));
    let gone_before_rescue = kill(Pid::from_raw(raw), None) == Err(Errno::ESRCH)
        && getpgid(Some(Pid::from_raw(raw))) == Err(Errno::ESRCH);
    // This read is BEFORE the test-only rescue; a successful product return
    // must already have reaped its actual Child. A mutant that merely drops
    // the live child returns StillAlive; one that signals but does not reap
    // returns a waitable zombie. Never let test cleanup create a false green.
    // If the 400ms negative timeout fired, the product still owns a live
    // Child on the worker thread. Never race its waitpid or steal its OS
    // exit status. Independent ECHILD proof is safe only AFTER an actual
    // product completion receipt was observed.
    let reaped_before_rescue = early
        .as_ref()
        .ok()
        .map(|_| native_waitpid(Pid::from_raw(raw), Some(WaitPidFlag::WNOHANG)));
    let rescue = terminate_attested_group(original);
    worker
        .join()
        .expect("terminal error worker joins after owned rescue");
    if !matches!(reaped_before_rescue, Some(Err(Errno::ECHILD))) {
        // An intentionally broken mutant must not leave its original child
        // zombie behind after the negative verdict has been sampled.
        let _ = native_waitpid(Pid::from_raw(raw), None);
    }
    rescue.expect("original test-owned birth must permit native cleanup");
    assert!(
        early.is_ok(),
        "failed attested group cleanup may not block indefinitely in Child::wait"
    );
    assert!(
        matches!(reaped_before_rescue, Some(Err(Errno::ECHILD))),
        "the Guardian must reap its exact original direct Child before test rescue, not leave zombie custody to a later test-only wait: {reaped_before_rescue:?}"
    );
    assert!(
        gone_before_rescue,
        "a failed group signal must not abandon an executing original direct Child when exact retained Child::kill is available"
    );
    let error = early
        .expect("captured bounded result")
        .expect_err("refused group signal cannot be reported as clean KGR1 success");
    assert!(
        error
            .to_string()
            .contains("process-group leader PID was reused"),
        "retain the original native birth-mismatch failure: {error}"
    );
}
