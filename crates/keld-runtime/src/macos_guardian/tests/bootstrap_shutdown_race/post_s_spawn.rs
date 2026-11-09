//! Native first-PREPARED/pre-exec child must never acquire REGISTERED after S.
//! The OS child exists and its Guardian reporter holds a real pre-exec gate;
//! the authenticated host sends S before permitting the SPAWNED request.
//! The refused, never-executed spawn is then an orderly cancellation: the
//! host retires the attested group and the Guardian reaps its exact child.
use std::io::Write as _;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use crate::macos_guardian::generation_host::GenerationRegistrationFence;
use crate::macos_guardian::process_group::{
    GroupIdentity, terminate_attested_group, validate_guardian_child_group,
};

use super::super::test_support::await_spawned_gate;
use super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, POST_DENIAL_BY_QUIT, POST_DENIAL_WITHOUT_S,
    POST_S_SPAWN_DENIAL_ENV, ROLE_ENV, ROLE_TEST, VERIFIED_BUN_ENV, verified_real_bun,
};

struct AttestedTestGroup(GroupIdentity);

impl Drop for AttestedTestGroup {
    fn drop(&mut self) {
        // Only after the product-cleanup assertions. The original process
        // birth makes this fallback unable to signal an unrelated stale PGID.
        let _ = terminate_attested_group(self.0);
    }
}

// Fixture owns the genuine Guardian command and already-verified real Bun
// binary. It does not observe or assert a product authorization outcome.
fn post_s_guardian_command(preexec: &Path, marker: &Path, case: &str) -> Command {
    let bun = verified_real_bun();
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "exec 3>&1; exec 1>&2; exec \"$@\"",
            "guardian-test-proxy",
        ])
        .arg(std::env::current_exe().expect("actual native unit executable"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, marker)
        .env(VERIFIED_BUN_ENV, bun)
        .env(POST_S_SPAWN_DENIAL_ENV, case)
        .env("KELD_TEST_GUARDIAN_PREEXEC_GATE", preexec)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    command
}

/// Which host decision wins the race with the held Guardian reporter.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HostDenial {
    /// The host's actual orderly S is flushed (denying recovery under the
    /// same lock) before the reporter sends SPAWNED.
    OrderlyS,
    /// Negative control: the host only denies recovery. No S is written
    /// until the Guardian has already exited.
    WithoutS,
    /// Negative control: the host's accepted Quit (Q) is flushed and gets a
    /// genuine KQA. The same acknowledged flag is set, but only an orderly S
    /// may retire a refused spawn as a clean cancellation.
    AcceptedQuit,
}

#[test]
fn native_host_s_refuses_kgc1_spawned_without_late_registered_or_bun_exec() {
    let (outcome, guardian_status) = run_post_denial_spawn(HostDenial::OrderlyS);
    // GH-824: lease loss before Bound sends S at any startup stage. A spawn
    // already in flight is refused and retired, then reported as an orderly
    // cancellation with the Guardian's genuine KSA, not a fatal host error.
    assert!(
        guardian_status.success(),
        "Guardian must retire the refused spawn and acknowledge S: {guardian_status:?}"
    );
    let status = outcome.expect("post-S SPAWNED refusal is an orderly cancellation");
    assert!(status.success(), "{status:?}");
}

#[test]
fn native_host_denial_without_s_keeps_the_spawned_refusal_fatal() {
    let (outcome, guardian_status) = run_post_denial_spawn(HostDenial::WithoutS);
    // The role exits 0 only after asserting its own run returned the typed
    // host-closed registration error; the host keeps its refusal as fatal.
    assert!(
        guardian_status.success(),
        "role must observe the Guardian's terminal refusal: {guardian_status:?}"
    );
    let error = outcome.expect_err("a refusal without acknowledged S cannot be clean");
    assert!(
        error.to_string().contains("SPAWNED refused after host S"),
        "actual host registration denial must survive shutdown cleanup: {error}"
    );
}

#[test]
fn native_host_accepted_quit_keeps_the_spawned_refusal_fatal() {
    let (outcome, guardian_status) = run_post_denial_spawn(HostDenial::AcceptedQuit);
    // The role exits 0 only after asserting its own run kept the typed
    // host-closed registration error. With that exit the host's decision
    // rests on its own reconcile alone, which must require KSA, not KQA.
    assert!(
        guardian_status.success(),
        "role must observe the Guardian's terminal refusal under Q: {guardian_status:?}"
    );
    let error = outcome.expect_err("a refusal retired under an accepted Quit cannot be clean");
    assert!(
        error.to_string().contains("SPAWNED refused after host S"),
        "actual host registration denial must survive the Quit shutdown: {error}"
    );
}

fn run_post_denial_spawn(
    denial: HostDenial,
) -> (
    Result<std::process::ExitStatus, crate::RuntimeError>,
    std::process::ExitStatus,
) {
    let temp = tempfile::tempdir().expect("owned after-S registration witness");
    let preexec_socket = temp.path().join("preexec.sock");
    let no_bun_marker = temp.path().join("forbidden-bun-exec");
    let listener = UnixListener::bind(&preexec_socket).expect("owned pre-exec reporter socket");
    listener
        .set_nonblocking(true)
        .expect("bounded reporter accept");
    let case = match denial {
        HostDenial::OrderlyS => "orderly-s",
        HostDenial::WithoutS => POST_DENIAL_WITHOUT_S,
        HostDenial::AcceptedQuit => POST_DENIAL_BY_QUIT,
    };
    let command = post_s_guardian_command(&preexec_socket, &no_bun_marker, case);
    let pending = GuardianBootstrap::spawn_supervised(command).expect("real KGC1 Guardian role");
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let (spawned_tx, spawned_rx) = mpsc::sync_channel(1);
    let mut primary = pending
        .register_guarded_primary_until_with_registration_fence(
            Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
            GenerationRegistrationFence {
                attempt: 1,
                entered: entered_tx,
                release: release_rx,
                received_prepare: None,
                received_spawned: Some(spawned_tx),
            },
        )
        .expect("host retained authentic Guardian control owner");
    let (mut preexec, group_pid, guardian_pid) = await_spawned_gate(&listener);
    assert_eq!(guardian_pid, primary.guardian_pid());
    let witness = AttestedTestGroup(
        validate_guardian_child_group(
            u32::try_from(group_pid).expect("positive native Bun group PID"),
            guardian_pid,
        )
        .expect("held pre-exec group has actual Guardian parent and birth"),
    );
    assert!(
        !no_bun_marker.exists(),
        "pre-exec group cannot run Bun before host retention"
    );
    // The host registration fence is *after* birth validation but *before*
    // CONTROL_REGISTERED. If it is reached, release it so cleanup remains
    // bounded and the test can independently expose this real authorization.
    let post_s_registered = thread::spawn(move || {
        let observed = entered_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE).ok();
        if observed.is_some() {
            let _ = release_tx.send(());
        }
        observed
    });
    let (outcome, mut primary) = match denial {
        HostDenial::OrderlyS => {
            let (sent_tx, sent_rx) = mpsc::sync_channel(1);
            primary.shutdown_sent = Some(sent_tx);
            let shutdown = thread::spawn(move || {
                let result = primary.shutdown();
                (result, primary)
            });
            // Host's real liveness control is already flushed, not just scheduled.
            sent_rx
                .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
                .expect("actual host wrote S before reporter was released");
            release_held_reporter(&mut preexec, &spawned_rx, group_pid);
            shutdown.join().expect("host shutdown task joined")
        }
        HostDenial::AcceptedQuit => {
            let (sent_tx, sent_rx) = mpsc::sync_channel(1);
            primary.shutdown_sent = Some(sent_tx);
            let quit = thread::spawn(move || {
                let accepted = primary.accept_shutdown();
                (accepted, primary)
            });
            // The real Q byte is flushed before SPAWNED, so the Guardian's
            // liveness loop reads it (and sends KQA) before seeing Failed.
            sent_rx
                .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
                .expect("actual host wrote Q before reporter was released");
            release_held_reporter(&mut preexec, &spawned_rx, group_pid);
            let (accepted, mut primary) = quit.join().expect("host Quit task joined");
            accepted.expect("the real Guardian acknowledged the accepted Quit with KQA");
            primary.shutdown_sent = None;
            (primary.shutdown(), primary)
        }
        HostDenial::WithoutS => {
            primary.deny_recovery();
            release_held_reporter(&mut preexec, &spawned_rx, group_pid);
            await_guardian_exit(&mut primary);
            (primary.shutdown(), primary)
        }
    };
    let post_s_registered = post_s_registered
        .join()
        .expect("registration observer thread joined");
    assert_refused_spawn_retired(&witness, post_s_registered, &no_bun_marker);
    assert!(
        primary.group_pid().is_none(),
        "no refused group may stay registered with the host"
    );
    let guardian_status = primary
        .child
        .try_wait()
        .expect("reaped Guardian result")
        .expect("the real authenticated Guardian must be reaped");
    (outcome, guardian_status)
}

// Only after the Guardian has finished on its own may the host write S, so
// the Guardian provably decided without one. Bounded; no timing sleep.
fn await_guardian_exit(primary: &mut crate::macos_guardian::GuardedPrimary) {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    while primary
        .child
        .try_wait()
        .expect("inspect real Guardian")
        .is_none()
    {
        assert!(
            Instant::now() < deadline,
            "Guardian did not finish after the host closed control"
        );
        thread::yield_now();
    }
}

fn assert_refused_spawn_retired(
    witness: &AttestedTestGroup,
    post_s_registered: Option<u32>,
    no_bun_marker: &Path,
) {
    let group = i32::try_from(witness.0.pid).expect("positive kernel group identity");
    let original_group_gone = matches!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(-group), None),
        Err(nix::errno::Errno::ESRCH)
    );
    assert_eq!(
        post_s_registered, None,
        "host reached native REGISTERED after the already-flushed S; group {group}"
    );
    assert!(
        !no_bun_marker.exists(),
        "real Bun JS executed after the host's orderly shutdown byte"
    );
    assert!(
        original_group_gone,
        "native pre-exec child group must retire before fallback cleanup"
    );
}

fn release_held_reporter(
    preexec: &mut std::os::unix::net::UnixStream,
    spawned_rx: &mpsc::Receiver<u32>,
    group_pid: i32,
) {
    preexec
        .write_all(b"R")
        .expect("permit the actual Guardian reporter to send SPAWNED after denial");
    assert_eq!(
        spawned_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE),
        Ok(u32::try_from(group_pid).expect("positive original child PID")),
        "host must independently decode and verify real SPAWNED after denial"
    );
}
