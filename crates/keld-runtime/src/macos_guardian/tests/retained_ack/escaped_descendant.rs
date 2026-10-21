//! Separate negative OS boundary: Bun can deliberately spawn a process in a
//! new session/group. A Guardian's verified original killpg cannot terminate
//! this new group without acquiring an independent kernel custody mechanism.
use std::io::Write as _;
use std::os::unix::net::UnixListener;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::RuntimeError;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use super::super::super::process_group::{
    GroupIdentity, observe_group_identity, terminate_attested_group,
};
use super::{
    BUN_ENV, CUT_ENV, ESCAPE_ENV, MARKER_ENV, RetainedCut, group_gone, process_role, read_marker,
    receive_event, validate_guardian_child_group, verified_real_bun,
};

struct EscapedTestGroup {
    marker: PathBuf,
    observed: Option<GroupIdentity>,
}

impl Drop for EscapedTestGroup {
    fn drop(&mut self) {
        // Only the birth actually observed while the original escaped child
        // was alive can authorize rescue. A PID from an old private marker
        // is not custody: re-attesting it now could capture an outsider that
        // reused the numeric group ID after a failed fixture.
        if let Some(group) = self.observed.take() {
            let _ = terminate_attested_group(group);
        } else if self.marker.exists() {
            // Drop must remain infallible even if stderr has been closed
            // during a native fixture's panic/unwind cleanup.
            let _ = writeln!(
                std::io::stderr().lock(),
                "escaped-child test cleanup lacks retained birth proof; refusing marker PID signaling"
            );
        }
    }
}

/// A marker PID is never cleanup authority on its own: retain the kernel
/// birth only after the detached group is proved Bun's actual live child.
fn original_detached_child_birth(
    bun_pid: i32,
    detached_pid: i32,
) -> Result<GroupIdentity, RuntimeError> {
    validate_guardian_child_group(
        u32::try_from(detached_pid).expect("positive detached PID"),
        u32::try_from(bun_pid).expect("positive original Bun PID"),
    )
}

fn detached_session_group(detached_pid: i32) -> Pid {
    let group = getpgid(Some(Pid::from_raw(detached_pid)))
        .expect("detached real Bun descendant still live");
    assert_eq!(
        group,
        Pid::from_raw(detached_pid),
        "Bun created a new session/group rather than an ordinary descendant"
    );
    group
}

#[test]
fn detached_bun_descendant_outlives_ordinary_guardian_group_reap() {
    let bun = verified_real_bun();
    let temp = tempfile::tempdir().expect("private detached-group fixture");
    let socket = temp.path().join("private-ack.sock");
    let marker = temp.path().join("actual-detached-child.pid");
    let mut escaped = EscapedTestGroup {
        marker: marker.clone(),
        observed: None,
    };
    let listener = UnixListener::bind(&socket).expect("owned control events");
    listener
        .set_nonblocking(true)
        .expect("bound event observation");
    let mut command = process_role("host");
    command
        .env(CUT_ENV, &socket)
        .env(MARKER_ENV, &marker)
        .env(BUN_ENV, bun)
        .env(ESCAPE_ENV, "1")
        .process_group(0)
        .stderr(Stdio::inherit());
    let host = command.spawn().expect("real host for native Guardian");
    let host_group = i32::try_from(host.id()).expect("test host PGID");
    let mut cut = RetainedCut {
        host: Some(host),
        host_group,
        host_birth: None,
        group: None,
        group_birth: None,
        descendant: None, // Escaped descendant has a distinct owned cleanup guard.
    };
    cut.host_birth = Some(
        observe_group_identity(u32::try_from(host_group).expect("positive host PID"))
            .expect("capture exact originally owned host-group birth"),
    );
    assert_ne!(Pid::from_raw(host_group), nix::unistd::getpgrp());
    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut gate, receipt) = receive_event(&listener, deadline);
    let parts: Vec<&str> = receipt.split_whitespace().collect();
    assert_eq!(parts.len(), 4, "{receipt}");
    assert_eq!(parts[0], "RETAINED");
    let group: u32 = parts[1].parse().expect("registered Bun group");
    let guardian: u32 = parts[2].parse().expect("actual Guardian PID");
    let host_pid: u32 = parts[3].parse().expect("actual host PID");
    assert_eq!(cut.host.as_ref().expect("owned host").id(), host_pid);
    let raw_group = i32::try_from(group).expect("Bun group ID");
    let raw_guardian = i32::try_from(guardian).expect("Guardian PID");
    cut.group = Some(raw_group);
    cut.group_birth = Some(
        validate_guardian_child_group(group, guardian)
            .expect("retain original Bun leader birth before releasing the ACK"),
    );
    gate.write_all(b"R")
        .expect("release authenticated host-retention ACK");
    let (bun_pid, detached_pid) = read_marker(&marker, deadline);
    assert_eq!(bun_pid, raw_group, "Bun leads its own registered group");
    assert_ne!(detached_pid, raw_group, "actual detached child identity");
    let independent_session = detached_session_group(detached_pid);
    // A marker and self-PGID do not establish who spawned this child. Require
    // the detached group's live kernel parent to be the observed original Bun
    // before arming any rescue signal with its original birth identity.
    escaped.observed = Some(
        original_detached_child_birth(bun_pid, detached_pid)
            .expect("detached leader must still be the original Bun's direct child"),
    );
    let host = cut.host.as_mut().expect("own actual host Child");
    host.kill()
        .expect("SIGKILL real host while Guardian is live");
    let (_completed, witness) = receive_event(&listener, Instant::now() + Duration::from_secs(10));
    assert_eq!(
        witness,
        format!("GUARDIAN_DONE {guardian} OK"),
        "the real Guardian still correctly reaps its registered Bun group"
    );
    group_gone(raw_group, None, raw_guardian);
    assert_eq!(
        cut.host
            .as_mut()
            .expect("original owned test host")
            .wait()
            .expect("reap host only after its Guardian/group retired")
            .signal(),
        Some(9)
    );
    // The product must not claim this still-live escaped group was contained.
    // Inspect BEFORE the independent, birth-attested test-only rescue.
    assert!(
        kill(Pid::from_raw(detached_pid), None).is_ok()
            && getpgid(Some(Pid::from_raw(detached_pid))) == Ok(independent_session),
        "a process that left Bun's PGID cannot be accounted for by the original killpg"
    );
    let escaped_birth = escaped
        .observed
        .expect("the original detached-child birth remains in native test custody");
    terminate_attested_group(escaped_birth)
        .expect("test-only detached group retirement must not silently fail");
    group_gone(detached_pid, None, raw_guardian);
    escaped.observed = None; // independent native absence proved; Drop disarmed
    cut.host_birth = None;
    let _ = cut.host.take(); // original host already reaped, after all observations
    cut.group = None;
    cut.group_birth = None;
}

#[cfg(test)]
#[path = "escaped_descendant/cleanup_tests.rs"]
mod cleanup_tests;
