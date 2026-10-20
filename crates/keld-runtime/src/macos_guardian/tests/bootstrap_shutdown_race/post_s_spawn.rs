//! Native first-PREPARED/pre-exec child must never acquire REGISTERED after S.
//! The OS child exists and its Guardian reporter holds a real pre-exec gate;
//! the authenticated host sends S before permitting the SPAWNED request.
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
    GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, POST_S_SPAWN_DENIAL_ENV, ROLE_ENV, ROLE_TEST,
    VERIFIED_BUN_ENV, verified_real_bun,
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
fn post_s_guardian_command(preexec: &Path, marker: &Path) -> Command {
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
        .env(POST_S_SPAWN_DENIAL_ENV, "1")
        .env("KELD_TEST_GUARDIAN_PREEXEC_GATE", preexec)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    command
}

#[test]
fn native_host_s_refuses_kgc1_spawned_without_late_registered_or_bun_exec() {
    let temp = tempfile::tempdir().expect("owned after-S registration witness");
    let preexec_socket = temp.path().join("preexec.sock");
    let no_bun_marker = temp.path().join("forbidden-bun-exec");
    let listener = UnixListener::bind(&preexec_socket).expect("owned pre-exec reporter socket");
    listener
        .set_nonblocking(true)
        .expect("bounded reporter accept");
    let command = post_s_guardian_command(&preexec_socket, &no_bun_marker);
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
                received_idle: None,
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
    preexec
        .write_all(b"R")
        .expect("permit the actual Guardian reporter to send SPAWNED after S");
    assert_eq!(
        spawned_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE),
        Ok(u32::try_from(group_pid).expect("positive original child PID")),
        "host must independently decode and verify real SPAWNED after S"
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
    let (outcome, mut primary) = shutdown.join().expect("host shutdown task joined");
    let post_s_registered = post_s_registered
        .join()
        .expect("registration observer thread joined");
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
    let guardian_status = primary
        .child
        .try_wait()
        .expect("reaped Guardian result")
        .expect("the real authenticated Guardian must be reaped");
    assert!(
        guardian_status.success(),
        "test role must observe terminal native rejection: {guardian_status:?}"
    );
    // An in-flight denied native spawn is an explicitly fatal startup cut.
    // The actual Guardian may still accept S and send its genuine KSA before
    // Supervisor reports the failed native spawn. Even in that case the
    // host must NOT turn the failed registration into clean shutdown.
    let error = outcome.expect_err("post-S SPAWNED refusal cannot report clean KSA");
    assert!(
        error.to_string().contains("SPAWNED refused after host S"),
        "actual host registration denial must survive shutdown cleanup: {error}"
    );
}
