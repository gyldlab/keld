//! The host recovery-denial lock encloses its real pipe S write/flush.
//! A native Guardian can attempt first PREPARE in this exact hostile window;
//! the worker must not observe denial without the corresponding S queued.
use std::io::Write as _;
use std::os::unix::net::UnixListener;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use crate::macos_guardian::generation_host::GenerationRegistrationFence;

use super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, POST_PROBE_GATE_ENV, ROLE_ENV, ROLE_TEST,
    VERIFIED_BUN_ENV, receive_held_milestone, verified_real_bun,
};

fn spawn_held_native_guardian(
    socket: &std::path::Path,
    bun_marker: &std::path::Path,
    bun: &std::path::Path,
) -> GuardianBootstrap {
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "exec 3>&1; exec 1>&2; exec \"$@\"",
            "guardian-test-proxy",
        ])
        .arg(std::env::current_exe().expect("native test process"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, bun_marker)
        .env(VERIFIED_BUN_ENV, bun)
        .env(POST_PROBE_GATE_ENV, socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    GuardianBootstrap::spawn_supervised(command).expect("real KGC1 Guardian")
}

fn release_failed_pre_s_observation(
    mut held: std::os::unix::net::UnixStream,
    gate_released: bool,
    denial_observed: bool,
    s_released: bool,
    allow_s_tx: &mpsc::SyncSender<()>,
) {
    // Never release an un-denied Guardian into Bun startup. On early failure
    // close the held native gate; after denial, permit PREPARE so S can drain.
    if !gate_released && denial_observed {
        let _ = held.write_all(b"R");
    }
    drop(held);
    if !s_released {
        let _ = allow_s_tx.try_send(());
    }
}

fn assert_native_denial_shutdown(
    result: Result<std::process::ExitStatus, super::super::RuntimeError>,
    mut primary: crate::macos_guardian::GuardedPrimary,
    bun_marker: &std::path::Path,
    denial_visible_before_s: bool,
) {
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

#[test]
fn native_host_denial_is_not_visible_before_real_s_is_queued() {
    let temp = tempfile::tempdir().expect("native host denial witness");
    let socket = temp.path().join("post-probe.sock");
    let bun_marker = temp.path().join("forbidden-bun-image");
    let listener = UnixListener::bind(&socket).expect("own post-probe gate");
    listener.set_nonblocking(true).expect("bounded accept");
    let bun = verified_real_bun();
    let pending = spawn_held_native_guardian(&socket, &bun_marker, &bun);
    let (prepare_tx, prepare_rx) = mpsc::sync_channel(1);
    let (idle_tx, idle_rx) = mpsc::sync_channel(1);
    let arm_idle = Arc::new(AtomicBool::new(false));
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
                received_idle: Some((Arc::clone(&arm_idle), idle_tx)),
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
    let mut gate_released = false;
    let mut denial_observed = false;
    let mut s_released = false;
    let observation = catch_unwind(AssertUnwindSafe(|| {
        denied_rx
            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("real host denied initial recovery before writing S");
        denial_observed = true;
        // Force an actual idle read while shutdown owns the recovery mutex.
        arm_idle.store(true, Ordering::Release);
        idle_rx
            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("actual KGC1 idle poll while recovery mutex is held");
        let denial_visible_before_s = recovery.try_lock().is_ok();
        assert!(!bun_marker.exists());
        held.write_all(b"R")
            .expect("permit Guardian to attempt PREPARE");
        gate_released = true;
        prepare_rx
            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("actual host decoded a full first KGC1 PREPARE before S");
        allow_s_tx
            .send(())
            .expect("release real host S while lock held");
        s_released = true;
        sent_rx
            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("native host S must be flushed");
        denial_visible_before_s
    }));
    if observation.is_err() {
        // An intentional RED must still release both native gates and join
        // the sole retained Child/worker owner before replaying the ORIGINAL
        // assertion failure. Never leave an orphan on failure-first tests.
        release_failed_pre_s_observation(
            held,
            gate_released,
            denial_observed,
            s_released,
            &allow_s_tx,
        );
    }
    let joined = shutdown.join();
    let denial_visible_before_s = match observation {
        Ok(value) => value,
        Err(original) => {
            if let Ok((_, mut primary)) = joined {
                let reaped = primary.child.try_wait().ok().flatten().is_some();
                eprintln!("KELD_TEST_DENIAL_PANIC_CHILD_REAPED {reaped}");
            }
            resume_unwind(original);
        }
    };
    let (result, primary) = joined.expect("host shutdown task");
    assert_native_denial_shutdown(result, primary, &bun_marker, denial_visible_before_s);
}
