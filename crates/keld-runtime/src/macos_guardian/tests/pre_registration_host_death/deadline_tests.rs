//! Native accepted-event deadline regression; no shipping Guardian changes.
use super::receive_pre_registration_event_with_accept_hook;
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
