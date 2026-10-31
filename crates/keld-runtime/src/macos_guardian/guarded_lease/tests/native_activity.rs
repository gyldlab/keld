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

/// One owner for the injected peer-gone errno pairs: the probe error alone
/// must never pass a live original group.
fn live_group_refuses_injected_errno(errno: i32, name: &str) {
    let root = tempfile::tempdir().expect("private native injected-errno refusal fixture");
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
            |_| Err(std::io::Error::from_raw_os_error(errno)),
            Instant::now() + Duration::from_millis(80),
        );
    }));
    assert!(
        refusal.is_err(),
        "{name} alone must not pass a live OS group"
    );
}

/// After the exact original group receives SIGKILL, the injected errno is accepted
/// only through the independent kernel group census.
fn killed_group_accepts_injected_errno_via_census(errno: i32) {
    let group = NativeGroup::start();
    let original = observe_group_identity(group.leader.id()).expect("original native group birth");
    terminate_attested_group(original).expect("kill exactly the original native group");
    group.assert_stopped_with_probe_until(
        |_| Err(std::io::Error::from_raw_os_error(errno)),
        Instant::now() + Duration::from_secs(3),
    );
}

#[test]
fn native_einval_cannot_claim_stopped_while_original_group_executes() {
    live_group_refuses_injected_errno(nix::libc::EINVAL, "EINVAL");
}

#[test]
fn native_einval_after_attested_sigkill_has_independent_group_census() {
    killed_group_accepts_injected_errno_via_census(nix::libc::EINVAL);
}

// ENOTCONN is the nonce WRITE failing after the listener dies mid-send: XNU
// `uipc_send` checks SS_ISCONNECTED before SS_CANTSENDMORE (a read after peer
// death returns EOF instead, which is already handled).
#[test]
fn native_enotconn_cannot_claim_stopped_while_original_group_executes() {
    live_group_refuses_injected_errno(nix::libc::ENOTCONN, "ENOTCONN");
}

#[test]
fn native_enotconn_after_attested_sigkill_has_independent_group_census() {
    killed_group_accepts_injected_errno_via_census(nix::libc::ENOTCONN);
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
