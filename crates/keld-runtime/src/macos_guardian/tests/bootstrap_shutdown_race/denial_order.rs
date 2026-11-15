//! The host recovery-denial lock encloses its real pipe S write/flush.
//! A native Guardian can attempt first PREPARE in this exact hostile window;
//! the worker must not observe denial without the corresponding S queued.
use std::io::Write as _;
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use crate::macos_guardian::generation_host::GenerationRegistrationFence;

use super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, POST_PROBE_GATE_ENV, ROLE_ENV, ROLE_TEST,
    VERIFIED_BUN_ENV, receive_held_milestone, verified_real_bun,
};

#[test]
fn native_host_denial_is_not_visible_before_real_s_is_queued() {
    let temp = tempfile::tempdir().expect("native host denial witness");
    let socket = temp.path().join("post-probe.sock");
    let bun_marker = temp.path().join("forbidden-bun-image");
    let listener = UnixListener::bind(&socket).expect("own post-probe gate");
    listener.set_nonblocking(true).expect("bounded accept");
    let bun = verified_real_bun();
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "exec 3>&1; exec 1>&2; exec \"$@\"",
            "guardian-test-proxy",
        ])
        .arg(std::env::current_exe().expect("native test process"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, &bun_marker)
        .env(VERIFIED_BUN_ENV, &bun)
        .env(POST_PROBE_GATE_ENV, &socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let pending = GuardianBootstrap::spawn_supervised(command).expect("real KGC1 Guardian");
    let (prepare_tx, prepare_rx) = mpsc::sync_channel(1);
    let (never_registered_tx, _never_registered_rx) = mpsc::sync_channel(1);
    let (_never_release_tx, never_release_rx) = mpsc::channel();
    let mut primary = pending
        .register_guarded_primary_until_with_registration_fence(
            Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
            GenerationRegistrationFence {
                attempt: 1,
                entered: never_registered_tx,
                release: never_release_rx,
                received_prepare: Some(prepare_tx),
                received_spawned: None,
            },
        )
        .expect("host retained actual Guardian");
    let (mut held, real_pid) = receive_held_milestone(&listener, "PROBED");
    assert_eq!(real_pid, primary.guardian_pid());
    let recovery = Arc::clone(&primary.recovery_decision);
    let (denied_tx, denied_rx) = mpsc::sync_channel(1);
    let (allow_s_tx, allow_s_rx) = mpsc::sync_channel(1);
    let (sent_tx, sent_rx) = mpsc::sync_channel(1);
    primary.shutdown_denial_fence = Some((denied_tx, allow_s_rx));
    primary.shutdown_sent = Some(sent_tx);
    let shutdown = thread::spawn(move || {
        let result = primary.shutdown();
        (result, primary)
    });
    denied_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("real host denied initial recovery before writing S");
    // This is the deterministically falsifiable, actual shipping owner lock.
    // Before the fix the worker could acquire it, observe deny and close KGC1
    // before the sole Guardian liveness owner had sent S.
    let denial_visible_before_s = recovery.try_lock().is_ok();
    assert!(!bun_marker.exists());
    held.write_all(b"R")
        .expect("permit Guardian to attempt PREPARE");
    prepare_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("actual host decoded a full first KGC1 PREPARE before S");
    allow_s_tx
        .send(())
        .expect("release real host S while lock held");
    sent_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("native host S must be flushed");
    let (result, mut primary) = shutdown.join().expect("host shutdown task");
    assert!(
        !denial_visible_before_s,
        "initial recovery denial became visible before the host queued S"
    );
    let status = result.expect("ordered S/KSA cannot fail before first generation");
    assert!(status.success(), "actual Guardian orderly exit: {status:?}");
    assert!(primary.group_pid().is_none(), "no enrolled Bun group");
    assert!(
        primary
            .child
            .try_wait()
            .expect("real Guardian status")
            .is_some(),
        "host reaped actual Guardian"
    );
    assert!(!bun_marker.exists(), "no JS marker after host deny+S");
}
