//! Spec kel133 §3 criteria 7–8 over real sockets: a started frame expires
//! at the earlier of the enclosing session/call deadline and the stall
//! limit (both orderings), an idle session terminates at its separately
//! declared deadline with a bounded poll cadence, and shutdown stays
//! promptly cancellable. Timeout assertions are lower bounds plus generous
//! upper kill-switch bounds — never sleeps for synchronization.

use std::sync::atomic::AtomicU32;
use std::sync::mpsc;
use std::thread;

use super::{
    AbsoluteDeadline, AppLinkDeadlines, AtomicBool, ChannelId, CorrelationId, Duration,
    FrameHeader, FrameKind, Instant, Ordering, Read, read_frame_interruptible_with_limits,
    read_validated_frame_interruptible_until, read_validated_frame_with_stall,
};
use crate::receive::ReceivePolicy;
use std::io::{self, Write as _};

#[cfg(unix)]
fn connected_pair() -> (
    std::os::unix::net::UnixStream,
    std::os::unix::net::UnixStream,
) {
    std::os::unix::net::UnixStream::pair().expect("unix pair")
}

/// The windows-latest acceptance row (spec kel133 §7 row 8) proves the
/// shipped named-pipe reader clock, not loopback TCP. The reader is the
/// server end, as in production.
#[cfg(windows)]
fn connected_pair() -> (
    crate::bootstrap::WindowsNamedPipeBootstrapStream,
    crate::bootstrap::WindowsNamedPipeBootstrapStream,
) {
    crate::bootstrap::connected_named_pipe_pair().expect("named-pipe pair")
}

fn hello_header_prefix() -> [u8; 4] {
    let bytes = FrameHeader {
        kind: FrameKind::Hello,
        flags: 0,
        channel: ChannelId(0),
        corr: CorrelationId(0),
        len: 32,
    }
    .encode();
    [bytes[0], bytes[1], bytes[2], bytes[3]]
}

/// AC7 ordering A: the enclosing session/call deadline is earlier than
/// `first_byte + APP_LINK_IO_DEADLINE`; the frame expires at the session
/// deadline with exact `KELD-IPC-006`. Ignoring the enclosing deadline
/// would run to the 5 s stall limit and fail the upper bound.
#[test]
fn started_frame_expires_at_the_earlier_enclosing_deadline() {
    let (mut reader, mut writer) = connected_pair();
    reader
        .set_app_link_read_deadline(Some(Duration::from_millis(20)))
        .expect("read poll");
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let peer = thread::spawn(move || {
        writer
            .write_all(&hello_header_prefix())
            .expect("write partial header");
        writer.flush().expect("flush");
        let _ = release_rx.recv(); // hold the socket open; released on exit
    });

    let stop = AtomicBool::new(false);
    let policy = ReceivePolicy::server_pre_auth_hello();
    let session_deadline = Duration::from_millis(200);
    let started = Instant::now();
    let err = read_validated_frame_interruptible_until(
        &mut reader,
        &policy,
        &stop,
        AbsoluteDeadline::at(started + session_deadline),
    )
    .expect_err("a stalled frame must expire at the session deadline");
    let elapsed = started.elapsed();

    assert!(err.to_string().starts_with("KELD-IPC-006"), "{err}");
    assert!(
        elapsed >= session_deadline,
        "expired before the absolute session deadline: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "the earlier session deadline must govern, not the 5 s stall limit: {elapsed:?}"
    );
    drop(release_tx);
    peer.join().expect("peer thread");
}

/// AC7 ordering B: the stall limit is earlier than the enclosing
/// deadline; the frame expires at `first_byte + stall` with exact
/// `KELD-IPC-006`. Resetting the stall clock per read would push expiry
/// toward the 10 s enclosing deadline and fail the upper bound.
#[test]
fn started_frame_expires_at_the_earlier_stall_limit() {
    let (mut reader, mut writer) = connected_pair();
    reader
        .set_app_link_read_deadline(Some(Duration::from_millis(20)))
        .expect("read poll");
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let peer = thread::spawn(move || {
        writer
            .write_all(&hello_header_prefix())
            .expect("write partial header");
        writer.flush().expect("flush");
        let _ = release_rx.recv();
    });

    let stop = AtomicBool::new(false);
    let policy = ReceivePolicy::server_pre_auth_hello();
    let stall = Duration::from_millis(150);
    let started = Instant::now();
    let err = read_frame_interruptible_with_limits(
        &mut reader,
        &stop,
        Some(started + Duration::from_secs(10)),
        stall,
        Some(&policy),
    )
    .expect_err("a stalled frame must expire at the stall limit");
    let elapsed = started.elapsed();

    assert!(err.to_string().starts_with("KELD-IPC-006"), "{err}");
    assert!(
        elapsed >= stall,
        "expired before the started-frame stall limit: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "the earlier stall limit must govern, not the 10 s enclosing deadline: {elapsed:?}"
    );
    drop(release_tx);
    peer.join().expect("peer thread");
}

struct CountingReader<S> {
    inner: S,
    reads: std::sync::Arc<AtomicU32>,
}

impl<S: Read> Read for CountingReader<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.inner.read(buf)
    }
}

/// AC8: an idle session with no frame byte crosses multiple bounded
/// reader polls without starting a frame clock, then terminates at its
/// separately declared session deadline with exact `KELD-IPC-006`. The
/// read-attempt counter is the cadence observable: a tight `WouldBlock`
/// retry loop produces orders of magnitude more attempts and fails the
/// upper bound; a blocking read that never rearms produces too few and
/// fails the lower bound (or hangs into the kill switch).
#[test]
fn idle_session_terminates_at_its_declared_deadline_with_bounded_cadence() {
    let (reader, _writer_keepalive) = connected_pair();
    reader
        .set_app_link_read_deadline(Some(Duration::from_millis(50)))
        .expect("read poll");
    let reads = std::sync::Arc::new(AtomicU32::new(0));
    let mut counting = CountingReader {
        inner: reader,
        reads: std::sync::Arc::clone(&reads),
    };

    let stop = AtomicBool::new(false);
    let policy = ReceivePolicy::echo_receiver();
    let session_deadline = Duration::from_millis(400);
    let started = Instant::now();
    let err = read_validated_frame_interruptible_until(
        &mut counting,
        &policy,
        &stop,
        AbsoluteDeadline::at(started + session_deadline),
    )
    .expect_err("an idle session must terminate at its declared deadline");
    let elapsed = started.elapsed();
    let attempts = reads.load(Ordering::Relaxed);

    assert!(err.to_string().starts_with("KELD-IPC-006"), "{err}");
    assert!(
        elapsed >= session_deadline,
        "terminated before the declared session deadline: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "idle termination must not wait for the stall limit: {elapsed:?}"
    );
    // ~8 polls at 50 ms over 400 ms; generous slack for scheduler jitter,
    // hard bound against busy-spin (a tight retry loop yields thousands).
    assert!(
        (2..=40).contains(&attempts),
        "poll cadence out of bounds: {attempts} read attempts in {elapsed:?}"
    );
}

/// The ordinary-reader byte-trickle gap (architecture 02 §7) is closed:
/// a peer dripping one byte per 20 ms — always inside the per-receive
/// deadline, which `read_exact` alone would renew forever — hits the
/// frame-wide stall clock on the plain validated read path.
#[test]
fn plain_validated_read_closes_the_byte_trickle_gap() {
    // The per-receive deadline is deliberately LONGER than the stall limit.
    // `IpcError::from` folds `WouldBlock`/`TimedOut` into `Timeout`
    // (lib.rs), so a bare `SO_RCVTIMEO` expiry and a frame-wide stall
    // expiry are the same `KELD-IPC-006` at this level. With a receive
    // deadline shorter than the stall limit, a single missed drip window
    // makes the socket time out first and the test would credit the stall
    // clock for a per-receive expiry (observed on a loaded macOS runner:
    // 006 at 108 ms against a 150 ms stall limit). Ordering them this way
    // makes the stall clock the only thing that can fire, so the assertion
    // below tests the property instead of the scheduler.
    const RECEIVE_DEADLINE: Duration = Duration::from_millis(400);
    let (mut reader, mut writer) = connected_pair();
    reader
        .set_app_link_read_deadline(Some(RECEIVE_DEADLINE))
        .expect("read deadline");
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    // Observable start condition, not a sleep: the drip thread signals
    // after its first byte is written, so the reader never begins with an
    // idle poll that could expire before any byte exists (the plain path
    // treats a pre-first-byte idle expiry as the per-receive deadline).
    let (first_tx, first_rx) = mpsc::channel::<()>();
    let drip = thread::spawn(move || {
        let mut bytes = FrameHeader {
            kind: FrameKind::Hello,
            flags: 0,
            channel: ChannelId(0),
            corr: CorrelationId(0),
            len: 32,
        }
        .encode()
        .to_vec();
        // A complete, valid HELLO (header + 32 token bytes): without the
        // frame-wide stall clock the reader would eventually ADMIT it
        // (~960 ms at 20 ms per byte), so the oracle is admit-vs-006, not
        // elapsed time alone.
        bytes.extend_from_slice(&[0xA5; 32]);
        for (index, byte) in bytes.into_iter().enumerate() {
            if !matches!(stop_rx.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                return; // stopped, or the test side is gone
            }
            if writer
                .write_all(&[byte])
                .and_then(|()| writer.flush())
                .is_err()
            {
                return;
            }
            if index == 0 {
                let _ = first_tx.send(());
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = stop_rx.recv();
    });
    first_rx.recv().expect("first dripped byte is in flight");

    let policy = ReceivePolicy::server_pre_auth_hello();
    let stall = Duration::from_millis(150);
    let started = Instant::now();
    let err = read_validated_frame_with_stall(&mut reader, &policy, stall)
        .expect_err("a dripping frame must expire at the stall clock");
    let elapsed = started.elapsed();

    assert!(err.to_string().starts_with("KELD-IPC-006"), "{err}");
    assert!(
        elapsed >= stall,
        "expired before the frame-wide stall clock: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "drip renewed the plain read path: {elapsed:?}"
    );
    // Plain-path bound: the stall clock is only consulted between partial
    // reads, so expiry lands within one receive deadline of it.
    assert!(
        elapsed < stall + RECEIVE_DEADLINE + Duration::from_millis(400),
        "plain-path bound is stall + one receive deadline: {elapsed:?}"
    );
    drop(stop_tx);
    drip.join().expect("drip thread");
}

/// AC8 shutdown half: an idle wait observes `stop` promptly — it returns
/// `Ok(None)` long before its 10 s session deadline instead of consuming
/// a poll budget.
#[test]
fn idle_session_shutdown_is_promptly_cancellable() {
    let (reader, _writer_keepalive) = connected_pair();
    reader
        .set_app_link_read_deadline(Some(Duration::from_millis(50)))
        .expect("read poll");
    let mut reader = reader;

    let stop = AtomicBool::new(true); // shutdown already requested
    let policy = ReceivePolicy::echo_receiver();
    let started = Instant::now();
    let outcome = read_validated_frame_interruptible_until(
        &mut reader,
        &policy,
        &stop,
        AbsoluteDeadline::at(started + Duration::from_secs(10)),
    )
    .expect("shutdown is not an error");
    let elapsed = started.elapsed();

    assert!(outcome.is_none(), "stop must yield Ok(None)");
    assert!(
        elapsed < Duration::from_secs(1),
        "shutdown must be observed promptly: {elapsed:?}"
    );
}
