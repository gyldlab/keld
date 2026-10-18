//! Native accepted-event deadline regression; no shipping Guardian changes.
use super::{
    receive_pre_registration_event_with_accept_hook, receive_pre_registration_event_with_read_hooks,
};
use std::io::Write as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn panic_message(value: &(dyn std::any::Any + Send)) -> &str {
    value
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| value.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic")
}

#[test]
fn late_newline_on_accepted_native_event_cannot_extend_caller_deadline() {
    let root = tempfile::tempdir().expect("private event deadline fixture");
    let path = root.path().join("event.sock");
    let listener = UnixListener::bind(&path).expect("own native event listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking event accept");
    let mut peer = UnixStream::connect(&path).expect("real Unix event peer");
    peer.write_all(b"PAUSED 123 456")
        .expect("partial native event with no newline");

    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let deadline = Instant::now() + Duration::from_millis(200); // test-only, not product timeout
    let reader = thread::spawn(move || {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            receive_pre_registration_event_with_accept_hook(&listener, deadline, || {
                accepted_tx.send(()).expect("real accept receipt");
                release_rx
                    .recv()
                    .expect("release test-owned accepted stream");
            })
        }));
        let result = match outcome {
            Ok((_, event)) => format!("late event was accepted: {event}"),
            Err(panic) => panic_message(panic.as_ref()).to_owned(),
        };
        let _ = finished_tx.send(result);
    });

    let accepted = accepted_rx.recv_timeout(Duration::from_secs(2));
    // Complete the *already accepted* connection only after the original
    // caller deadline; release the owned reader on either test outcome.
    if accepted.is_ok() {
        while Instant::now() < deadline {
            thread::yield_now();
        }
        let _ = peer.write_all(b"\n");
    }
    let _ = release_tx.send(());
    drop(peer);
    let completed = finished_rx.recv_timeout(Duration::from_secs(2));
    reader
        .join()
        .expect("owned reader joins after peer close/release");
    accepted.expect("test observed real listener.accept before the deadline cut");
    let result = completed.expect("bounded native event reader completed");
    assert!(
        result.contains("control event exceeded original deadline"),
        "late newline must be rejected by the original caller deadline, not accepted or misreported: {result}"
    );
}

#[test]
fn native_late_lf_after_successful_precheck_requires_post_read_fence() {
    const PREFIX: &[u8] = b"PAUSED 123 456";

    let root = tempfile::tempdir().expect("private native late-LF fixture");
    let path = root.path().join("event.sock");
    let listener = UnixListener::bind(&path).expect("own native event listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking event accept");

    let mut peer = UnixStream::connect(&path).expect("native event peer");
    peer.write_all(PREFIX)
        .expect("native partial frame before original deadline");

    let (prechecked_tx, prechecked_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let (native_lf_tx, native_lf_rx) = mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let deadline = Instant::now() + Duration::from_millis(200); // original test-only budget
    let reader = thread::spawn(move || {
        let mut paused = false;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            receive_pre_registration_event_with_read_hooks(
                &listener,
                deadline,
                || {},
                |bytes_received| {
                    if bytes_received == PREFIX.len() && !paused {
                        paused = true;
                        // All prefix bytes came from the actual UnixStream, and
                        // the original pre-read deadline check just succeeded.
                        prechecked_tx
                            .send(Instant::now())
                            .expect("publish successful precheck after native prefix");
                        release_rx.recv().expect("release native LF read");
                    }
                },
                |byte| {
                    if byte == b'\n' {
                        native_lf_tx
                            .send(Instant::now())
                            .expect("publish actual LF byte read before post-read check");
                    }
                },
            )
        }));
        let result = match outcome {
            Ok((_, event)) => format!("late event was accepted: {event}"),
            Err(panic) => panic_message(panic.as_ref()).to_owned(),
        };
        let _ = finished_tx.send(result);
    });

    let prechecked = prechecked_rx.recv_timeout(Duration::from_secs(2));
    // The reader is now suspended between its successful deadline precheck
    // and native read. Deliver LF only after the original deadline has passed.
    let late_write = if prechecked.is_ok() {
        while Instant::now() < deadline {
            thread::yield_now();
        }
        peer.write_all(b"\n")
    } else {
        Ok(())
    };
    let _ = release_tx.send(());
    let completed = finished_rx.recv_timeout(Duration::from_secs(2));
    drop(peer); // Rescue even if the staged read or its assertion failed.
    reader.join().expect("owned native event reader joins");

    let prechecked_at = prechecked.expect("reader consumed prefix before deadline");
    assert!(
        prechecked_at < deadline,
        "must witness successful pre-read check within original 200ms"
    );
    late_write.expect("late LF delivered through native UnixStream");
    let native_lf_at = native_lf_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("reader actually consumed the late LF before its post-read fence");
    assert!(
        native_lf_at >= deadline,
        "the actual LF read must follow the original caller deadline"
    );
    let result = completed.expect("reader terminated under the original deadline fence");
    assert!(
        result.contains("control event exceeded original deadline"),
        "post-read fence must reject an observed late native LF: {result}"
    );
}
