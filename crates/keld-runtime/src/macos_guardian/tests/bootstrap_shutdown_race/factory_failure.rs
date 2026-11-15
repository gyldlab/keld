//! Actual KGC1 PREPARED followed by a factory failure must never look orderly.
use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use super::{
    FACTORY_ERROR_GATE_ENV, GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, ROLE_ENV, ROLE_TEST,
    VERIFIED_BUN_ENV, receive_held_milestone, verified_real_bun,
};

#[test]
fn native_prepared_factory_fault_cannot_be_hidden_by_concurrent_real_s() {
    let temp = tempfile::tempdir().expect("native factory fault witness");
    let socket = temp.path().join("factory-after-prepared.sock");
    let bun_marker = temp.path().join("forbidden-bun-image");
    let listener = UnixListener::bind(&socket).expect("own verified PREPARED witness");
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
        .env(FACTORY_ERROR_GATE_ENV, &socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let pending = GuardianBootstrap::spawn_supervised(command).expect("real KGC1 Guardian");
    let mut primary = pending
        .register_guarded_primary_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("actual authenticated Guardian retained by host");
    let (mut held, actual_pid) = receive_held_milestone(&listener, "PREPARED");
    assert_eq!(actual_pid, primary.guardian_pid());
    let (sent_tx, sent_rx) = mpsc::sync_channel(1);
    primary.shutdown_sent = Some(sent_tx);
    let shutdown = thread::spawn(move || {
        let result = primary.shutdown();
        (result, primary)
    });
    sent_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("host actually flushed S after genuine PREPARED");
    held.write_all(b"R")
        .expect("release actual failing factory");
    let (result, mut primary) = shutdown.join().expect("host shutdown task joined");
    let status = primary
        .child
        .try_wait()
        .expect("Guardian status")
        .expect("host must reap real Guardian even on startup failure");
    assert!(
        status.success(),
        "test role must report the retained original fault and exit normally: {status:?}"
    );
    let mut stderr = String::new();
    primary
        .child
        .stderr
        .take()
        .expect("owned native child evidence")
        .take(16 * 1024)
        .read_to_string(&mut stderr)
        .expect("bounded actual Guardian stderr");
    assert!(
        stderr.contains("KELD_TEST_FACTORY_ERROR_RETAINED"),
        "real factory failure must propagate despite S/KSA race: {stderr}"
    );
    assert!(!stderr.contains("KELD_TEST_FACTORY_ERROR_SUPPRESSED"));
    assert!(
        result.is_err(),
        "host cannot report success after factory fault"
    );
    assert!(
        !primary.shutdown_attributed,
        "a genuine local factory fault must not forge an orderly KSA"
    );
    assert!(primary.group_pid().is_none(), "no Bun group was enrolled");
    assert!(
        !bun_marker.exists(),
        "faulty startup must not execute Bun JS"
    );
}
