//! Negative native containment boundary: SIGKILL of both independent cleanup
//! owners after Bun exec leaves no process alive to observe host EOF or signal
//! Bun. The fixture owns/rescues every known member; no success is claimed.
use std::io::Write as _;
use std::os::unix::net::UnixListener;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::Stdio;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::{Pid, getpgid};

use super::{
    BUN_ENV, CUT_ENV, MARKER_ENV, RetainedCut, observe_group_identity, process_role, read_marker,
    receive_event, validate_guardian_child_group, verified_real_bun,
};

#[test]
fn simultaneous_host_guardian_sigkill_after_bun_exec_requires_external_os_custody() {
    let bun = verified_real_bun();
    let temp = tempfile::tempdir().expect("private simultaneous-death fixture");
    let socket = temp.path().join("held-ack.sock");
    let marker = temp.path().join("real-bun-and-descendant.pid");
    let listener = UnixListener::bind(&socket).expect("owned KGR1 event listener");
    listener.set_nonblocking(true).expect("bounded listener");
    let mut command = process_role("host");
    command
        .env(CUT_ENV, &socket)
        .env(MARKER_ENV, &marker)
        .env(BUN_ENV, bun)
        .process_group(0)
        .stderr(Stdio::inherit());
    let host = command.spawn().expect("real host for independent Guardian");
    let host_group = i32::try_from(host.id()).expect("native host group");
    let mut owners = RetainedCut {
        host: Some(host),
        host_group,
        host_birth: None,
        group: None,
        group_birth: None,
        descendant: None,
    };
    owners.host_birth = Some(
        observe_group_identity(u32::try_from(host_group).expect("positive host PID"))
            .expect("capture original host-group kernel birth"),
    );
    assert_ne!(Pid::from_raw(host_group), nix::unistd::getpgrp());
    assert_eq!(
        getpgid(Some(Pid::from_raw(host_group))),
        Ok(Pid::from_raw(host_group)),
        "actual test host must lead its isolated group"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut held, receipt) = receive_event(&listener, deadline);
    let parts: Vec<&str> = receipt.split_whitespace().collect();
    assert_eq!(parts.len(), 4, "{receipt}");
    assert_eq!(parts[0], "RETAINED");
    let group: u32 = parts[1].parse().expect("true group");
    let guardian: u32 = parts[2].parse().expect("true Guardian");
    let host_pid: u32 = parts[3].parse().expect("true host");
    assert_eq!(owners.host.as_ref().expect("host").id(), host_pid);
    let raw_group = i32::try_from(group).expect("Bun group fits");
    let raw_guardian = i32::try_from(guardian).expect("Guardian PID fits");
    owners.group = Some(raw_group);
    owners.group_birth = Some(
        validate_guardian_child_group(group, guardian)
            .expect("real Bun is the retained Guardian's direct OS child"),
    );
    assert_eq!(
        getpgid(Some(Pid::from_raw(raw_guardian))),
        Ok(Pid::from_raw(host_group)),
        "host and Guardian must share the test-only SIGKILL group"
    );
    held.write_all(b"R").expect("allow authentic KGR1 A");
    let (bun_pid, descendant) = read_marker(&marker, deadline);
    owners.descendant = Some(descendant);
    assert_eq!(
        bun_pid, raw_group,
        "real Bun exec in isolated process group"
    );
    assert_eq!(
        getpgid(Some(Pid::from_raw(descendant))),
        Ok(Pid::from_raw(raw_group)),
        "test observes actual live native descendant"
    );

    // This single native SIGKILL targets both cleanup owners atomically; it
    // does not kill the separately enrolled Bun group. A host-kill-only test
    // would leave the Guardian able to terminate Bun and give false confidence.
    killpg(Pid::from_raw(host_group), Signal::SIGKILL)
        .expect("terminate the exact test-owned host plus Guardian group");
    // Preserve the owned Child in the cleanup guard across a failed wait.
    let host = owners.host.as_mut().expect("sole host child owner");
    assert_eq!(host.wait().expect("reap host").signal(), Some(9));
    owners.host_birth = None;
    let _ = owners.host.take(); // original host is now reaped

    // This is an intentionally RED product-security boundary. Observe both
    // real Bun and descendant still live BEFORE the fixture's Drop rescues
    // them. It must not be counted as a passing containment guarantee.
    let bun_alive = kill(Pid::from_raw(raw_group), None).is_ok();
    let descendant_alive = kill(Pid::from_raw(descendant), None).is_ok();
    let descendant_still_enrolled =
        getpgid(Some(Pid::from_raw(descendant))) == Ok(Pid::from_raw(raw_group));
    assert!(
        bun_alive && descendant_alive && descendant_still_enrolled,
        "without a surviving external OS custodian, the raw-stage Guardian cannot guarantee cleanup after simultaneous owner SIGKILL"
    );
    // The negative product oracle precedes this test-only rescue. Check the
    // original birth and actual kernel absence rather than letting a fallible
    // Drop silently leak a live Bun/descendant after reporting test success.
    let birth = owners
        .group_birth
        .expect("the original enrolled Bun group remains in test custody");
    super::terminate_attested_group(birth)
        .expect("test-only dual-death group rescue must retire the original birth");
    super::group_gone(raw_group, Some(descendant), raw_guardian);
    owners.group_birth = None;
    owners.group = None;
    owners.descendant = None;
}
