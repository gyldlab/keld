//! Failure-first real Unix socket oracles for one original retained-ACK cut clock.
//! No simulated event is used: the listener accepts an actual kernel peer and
//! the exact LF is read by the code whose post-read fence the test protects.
use super::{receive_event, receive_event_with_hooks};
use std::io::Write as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn native_late_lf_after_timely_precheck_cannot_complete_retained_ack_receipt() {
    const PREFIX: &[u8] = b"RETAINED 123 456";
    let root = tempfile::tempdir().expect("own private native event socket");
    let socket = root.path().join("retained-ack.sock");
    let listener = UnixListener::bind(&socket).expect("own real native event listener");
    listener
        .set_nonblocking(true)
        .expect("bounded native accept");
    let mut peer = UnixStream::connect(&socket).expect("own real event peer");
    peer.write_all(PREFIX)
        .expect("send real receipt bytes before terminal LF");

    let deadline = Instant::now() + Duration::from_millis(250);
    let (prechecked_tx, prechecked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (read_tx, read_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut gated = false;
        catch_unwind(AssertUnwindSafe(|| {
            receive_event_with_hooks(
                &listener,
                deadline,
                |bytes| {
                    if bytes == PREFIX.len() && !gated {
                        gated = true;
                        let _ = prechecked_tx.send(Instant::now());
                        let _ = release_rx.recv_timeout(Duration::from_secs(1));
                    }
                },
                |byte| {
                    if byte == b'\n' {
                        let _ = read_tx.send(Instant::now());
                    }
                },
            )
        }))
    });

    // The exact reader crossed its native prefix and its pre-read hook while
    // the caller's original deadline was still valid, then remains owned and
    // blocked until the controller *actually* writes the delayed LF.
    let prechecked = prechecked_rx.recv_timeout(Duration::from_millis(200));
    while Instant::now() < deadline {
        thread::yield_now();
    }
    let sent = peer.write_all(b"\n");
    let _ = release_tx.send(());
    let native_lf = read_rx.recv_timeout(Duration::from_secs(1));
    drop(peer);
    let result = reader.join().expect("join the exact native read owner");

    assert!(sent.is_ok(), "real socket must carry the exact late LF");
    assert!(
        prechecked.is_ok_and(|at| at < deadline),
        "original native bytes/pre-read checkpoint must precede the deadline"
    );
    assert!(
        native_lf.is_ok_and(|at| at >= deadline),
        "actual native LF must be consumed AFTER the original deadline"
    );
    assert!(
        result.is_err(),
        "post-read fence must reject actual late native LF; old helper accepted a complete event"
    );
}

#[test]
fn expired_caller_deadline_cannot_accept_already_queued_native_receipt() {
    let root = tempfile::tempdir().expect("own queued event socket");
    let socket = root.path().join("preexpired.sock");
    let listener = UnixListener::bind(&socket).expect("real native listener");
    listener
        .set_nonblocking(true)
        .expect("real nonblocking accept");
    let mut peer = UnixStream::connect(&socket).expect("queue owned native peer");
    peer.write_all(b"RETAINED 123 456\n")
        .expect("complete kernel-queued receipt");
    let past = Instant::now()
        .checked_sub(Duration::from_millis(1))
        .expect("monotonic native test instant must admit one-millisecond subtraction");
    let result = catch_unwind(AssertUnwindSafe(|| receive_event(&listener, past)));
    drop(peer);
    assert!(
        result.is_err(),
        "an expired original clock may not accept a queued native record"
    );
}

#[test]
fn timely_native_receipt_still_accepted_without_changing_protocol_bytes() {
    let root = tempfile::tempdir().expect("own valid native event socket");
    let socket = root.path().join("timely.sock");
    let listener = UnixListener::bind(&socket).expect("real valid listener");
    listener
        .set_nonblocking(true)
        .expect("bounded native accept");
    let mut peer = UnixStream::connect(&socket).expect("real valid peer");
    peer.write_all(b"RETAINED 123 456\n")
        .expect("send native valid receipt");
    let (accepted, receipt) = receive_event(&listener, Instant::now() + Duration::from_secs(1));
    assert_eq!(receipt, "RETAINED 123 456");
    drop(accepted);
    drop(peer);
}
