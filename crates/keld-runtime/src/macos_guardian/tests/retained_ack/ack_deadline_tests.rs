//! Native KGR1 withheld-ACK gate; the source clock must bound every real R byte.
use super::await_ack_release_with_hooks;
use std::io::{self, Write as _};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn native_release_byte_read_after_timely_precheck_must_not_authorize_expired_ack() {
    let (mut blocked_guardian, mut controller) =
        UnixStream::pair().expect("two owned native private ACK endpoints");
    blocked_guardian
        .set_nonblocking(true)
        .expect("bound the real test gate reader");
    let deadline = Instant::now() + Duration::from_millis(250);
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (byte_tx, byte_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut gated = false;
        await_ack_release_with_hooks(
            &mut blocked_guardian,
            deadline,
            || {
                if !gated {
                    gated = true;
                    let _ = ready_tx.send(Instant::now());
                    let _ = release_rx.recv_timeout(Duration::from_secs(1));
                }
            },
            |byte| {
                let _ = byte_tx.send((byte, Instant::now()));
            },
        )
    });

    let prechecked = ready_rx.recv_timeout(Duration::from_millis(200));
    while Instant::now() < deadline {
        thread::yield_now();
    }
    let sent = controller.write_all(b"R");
    let _ = release_tx.send(());
    let consumed = byte_rx.recv_timeout(Duration::from_secs(1));
    drop(controller);
    let result = reader
        .join()
        .expect("reap exact test-controlled ACK reader thread");
    assert!(
        sent.is_ok(),
        "the real Unix peer must send R after the original cutoff"
    );
    assert!(
        prechecked.is_ok_and(|at| at < deadline),
        "native ACK reader must reach pre-read fence before the original cutoff"
    );
    assert!(
        consumed.is_ok_and(|(byte, at)| byte == b'R' && at >= deadline),
        "real release R must actually be read AFTER the original cutoff"
    );
    assert!(
        result.is_err_and(|error| error.kind() == io::ErrorKind::TimedOut),
        "a late native release R may not authorize retained host group acknowledgment"
    );
}

#[test]
fn expired_original_ack_clock_rejects_already_queued_native_release() {
    let (mut guardian, mut controller) = UnixStream::pair().expect("own real ACK pair");
    guardian
        .set_nonblocking(true)
        .expect("native release nonblocking");
    controller
        .write_all(b"R")
        .expect("queue real positive native release");
    let deadline = Instant::now()
        .checked_sub(Duration::from_millis(1))
        .expect("monotonic native test instant must admit one-millisecond subtraction");
    let result = await_ack_release_with_hooks(&mut guardian, deadline, || {}, |_| {});
    assert!(
        result.is_err_and(|error| error.kind() == io::ErrorKind::TimedOut),
        "originally expired native ACK must not authorize host retention"
    );
}

#[test]
fn native_release_within_original_budget_remains_valid() {
    let (mut guardian, mut controller) = UnixStream::pair().expect("own native valid ACK");
    guardian
        .set_nonblocking(true)
        .expect("native release nonblocking");
    controller
        .write_all(b"R")
        .expect("send timely real native R");
    await_ack_release_with_hooks(
        &mut guardian,
        Instant::now() + Duration::from_secs(1),
        || {},
        |_| {},
    )
    .expect("valid native ACK remains permitted inside its original budget");
}
