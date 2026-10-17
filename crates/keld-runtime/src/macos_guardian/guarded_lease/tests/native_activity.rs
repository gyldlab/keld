//! A closed socket is not independent proof that its native group retired.
use std::os::unix::process::CommandExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::super::super::process_group::{observe_group_identity, terminate_attested_group};
use super::{NativeGroup, UnixListener};

#[test]
fn closed_activity_sockets_cannot_hide_live_native_group() {
    let root = tempfile::tempdir().expect("private native false-green observer");
    let leader = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("owned native group remains actively executing");
    // Install Drop ownership immediately. A failed socket setup still reaps
    // only this unreaped direct Child's private process group.
    let group = NativeGroup { leader, root };
    for role in ["leader", "descendant"] {
        let listener = UnixListener::bind(group.socket(role))
            .expect("bind simulated private observation socket");
        drop(listener);
    }
    // The old socket-only assert_stopped returned success here even though
    // the test-owned original group was still actively running.
    let refusal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        group.assert_stopped_with_probe_until(
            super::fresh_response,
            Instant::now() + Duration::from_millis(80),
        );
    }));
    assert!(
        refusal.is_err(),
        "a closed activity socket must never substitute for native group retirement"
    );
}

#[test]
fn native_einval_cannot_claim_stopped_while_original_group_executes() {
    let root = tempfile::tempdir().expect("private native EINVAL refusal fixture");
    let leader = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("owned original native group is executing");
    let group = NativeGroup { leader, root };
    let refusal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        group.assert_stopped_with_probe_until(
            |_| Err(std::io::Error::from_raw_os_error(nix::libc::EINVAL)),
            Instant::now() + Duration::from_millis(80),
        );
    }));
    assert!(
        refusal.is_err(),
        "EINVAL alone must not pass a live OS group"
    );
}

#[test]
fn native_einval_after_attested_sigkill_has_independent_group_census() {
    let group = NativeGroup::start();
    let original = observe_group_identity(group.leader.id()).expect("original native group birth");
    terminate_attested_group(original).expect("kill exactly the original native group");
    group.assert_stopped_with_probe_until(
        |_| Err(std::io::Error::from_raw_os_error(nix::libc::EINVAL)),
        Instant::now() + Duration::from_secs(3),
    );
}

#[test]
fn failed_native_census_cannot_certify_group_absence() {
    // A killed or otherwise failed ps can emit no bytes. The original test
    // runner retains its own leader Child, so absence of ps output is never
    // evidence of an empty OS process group.
    let observation = std::process::Output {
        status: std::process::ExitStatus::from_raw(nix::libc::SIGKILL),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert!(
        NativeGroup::parse_group_census(&observation).is_err(),
        "a failed kernel census must not turn a live native group into success"
    );
}

#[test]
fn empty_successful_native_census_cannot_certify_group_absence() {
    // The fixture deliberately leaves its original direct leader Child
    // unreaped until Drop. A successful ps observation therefore has at
    // least that live/zombie process-state row; empty output is a bad probe.
    let observation = std::process::Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert!(
        NativeGroup::parse_group_census(&observation).is_err(),
        "a successful but empty OS census cannot prove an owned group stopped"
    );
}
