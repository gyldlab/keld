//! Fail-closed pre-registration liveness admission: malformed, absent and non-pipe writers.
use std::io;
use std::os::unix::process::ExitStatusExt as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use nix::fcntl::{FcntlArg, OFlag, fcntl};

use super::{liveness_pipe, long_running_command, run_with_registration};

#[test]
fn pre_child_liveness_bytes_fail_and_revoke_prepared_resources() {
    let (reader, mut writer) = liveness_pipe();
    std::io::Write::write_all(&mut writer, &[1_u8]).expect("write invalid liveness byte");
    let spawned = Arc::new(AtomicBool::new(false));
    let spawned_in_callback = Arc::clone(&spawned);
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_in_callback = Arc::clone(&revoked);
    let error = run_with_registration(
        long_running_command(),
        reader,
        io::sink(),
        move |_| {
            spawned_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
        move || {
            revoked_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .expect_err("liveness bytes must fail closed");
    assert!(
        error.to_string().contains("carried data instead of EOF"),
        "{error}"
    );
    assert!(!spawned.load(Ordering::SeqCst));
    assert!(revoked.load(Ordering::SeqCst));
}

#[test]
fn liveness_without_a_writer_fails_before_spawn() {
    let (reader, writer) = liveness_pipe();
    drop(writer);
    let spawned = Arc::new(AtomicBool::new(false));
    let spawned_in_callback = Arc::clone(&spawned);
    let error = run_with_registration(
        long_running_command(),
        reader,
        io::sink(),
        move |_| {
            spawned_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
        || Ok(()),
    )
    .expect_err("a forged bootstrap without a host writer must fail");
    assert!(error.to_string().contains("no live writer"), "{error}");
    assert!(!spawned.load(Ordering::SeqCst));
}

#[test]
fn non_pipe_liveness_fails_before_spawn() {
    let file = tempfile::tempfile().expect("temporary regular file");
    let spawned = Arc::new(AtomicBool::new(false));
    let spawned_in_callback = Arc::clone(&spawned);
    let error = run_with_registration(
        long_running_command(),
        file,
        io::sink(),
        move |_| {
            spawned_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
        || Ok(()),
    )
    .expect_err("a forged regular-file bootstrap must fail");
    assert!(error.to_string().contains("is not a pipe"), "{error}");
    assert!(!spawned.load(Ordering::SeqCst));
}

#[test]
fn initially_nonblocking_liveness_is_normalized_to_wait_for_eof() {
    let (reader, writer) = liveness_pipe();
    let flags = fcntl(&reader, FcntlArg::F_GETFL).expect("read liveness flags");
    fcntl(
        &reader,
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .expect("make liveness reader initially nonblocking");
    let (spawned_tx, spawned_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = run_with_registration(
            long_running_command(),
            reader,
            io::sink(),
            move |_| {
                spawned_tx.send(()).map_err(io::Error::other)?;
                Ok(())
            },
            || Ok(()),
        );
        done_tx.send(result).expect("report guardian result");
    });
    spawned_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("guardian child registered");
    assert!(
        matches!(done_rx.try_recv(), Err(TryRecvError::Empty)),
        "guardian must still own the child while a live writer exists"
    );

    drop(writer);
    let report = done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("guardian finishes after EOF")
        .expect("guardian EOF cleanup succeeds");
    assert_eq!(report.leader_status().signal(), Some(9));
    worker.join().expect("guardian worker joins");
}
