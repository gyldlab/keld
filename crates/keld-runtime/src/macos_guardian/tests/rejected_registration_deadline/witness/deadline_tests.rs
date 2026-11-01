//! Real private Unix registration-refusal clocks, including accepted peers
//! and LF bytes received after the caller's original absolute deadline.
use super::{WitnessReadStage, await_denial_witness, await_denial_witness_with_hook};
use std::any::Any;
use std::io::Write as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn owned_native_peer() -> (tempfile::TempDir, UnixListener, UnixStream) {
    let root = tempfile::tempdir().expect("own native Guardian refusal socket path");
    let socket = root.path().join("refused-registration.sock");
    let listener = UnixListener::bind(&socket).expect("bind actual native Unix listener");
    listener
        .set_nonblocking(true)
        .expect("bound native listener's original acceptance clock");
    let peer = UnixStream::connect(&socket).expect("connect independently owned real peer");
    (root, listener, peer)
}

fn panic_message(result: Result<String, Box<dyn Any + Send>>) -> String {
    let source = result.expect_err("native late refusal must fail closed");
    if let Some(message) = source.downcast_ref::<String>() {
        return message.clone();
    }
    source
        .downcast_ref::<&str>()
        .expect("source assertion carries a literal reason")
        .to_string()
}

#[test]
fn real_accepted_refusal_peer_cannot_cross_original_post_accept_deadline() {
    let (_root, listener, mut peer) = owned_native_peer();
    peer.write_all(b"DENIED 123\n")
        .expect("queue real complete Guardian refusal line");
    let deadline = Instant::now() + Duration::from_millis(250);
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (on_time_tx, on_time_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        catch_unwind(AssertUnwindSafe(|| {
            await_denial_witness_with_hook(&listener, deadline, |stage| match stage {
                WitnessReadStage::AfterNativeAccept => {
                    let _ = accepted_tx.send(Instant::now());
                    let _ = release_rx.recv_timeout(Duration::from_secs(1));
                }
                WitnessReadStage::AcceptedWithinDeadline => {
                    let _ = on_time_tx.send(());
                }
                _ => {}
            })
        }))
    });
    let observed = accepted_rx.recv_timeout(Duration::from_millis(200));
    while Instant::now() < deadline {
        thread::yield_now();
    }
    let _ = release_tx.send(());
    drop(peer);
    let result = reader.join().expect("join original refusal socket reader");

    assert!(
        observed.is_ok_and(|at| at < deadline),
        "actual native accept must finish before original clock is crossed"
    );
    assert!(
        !matches!(on_time_rx.try_recv(), Ok(())),
        "a late accepted peer cannot reach the on-time authenticated stage"
    );
    assert!(
        panic_message(result).contains("accepted after original deadline"),
        "the post-native-accept fence, not a later byte check, must reject this phase"
    );
}

#[test]
fn real_late_native_lf_after_pre_read_check_cannot_complete_refusal_record() {
    const PREFIX: &[u8] = b"DENIED 123";
    let (_root, listener, mut peer) = owned_native_peer();
    peer.write_all(PREFIX)
        .expect("write real bounded native refusal prefix");
    let deadline = Instant::now() + Duration::from_millis(250);
    let (pre_tx, pre_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (native_tx, native_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut paused = false;
        catch_unwind(AssertUnwindSafe(|| {
            await_denial_witness_with_hook(&listener, deadline, |stage| match stage {
                WitnessReadStage::BeforeNativeRead(n) if n == PREFIX.len() && !paused => {
                    paused = true;
                    let _ = pre_tx.send(Instant::now());
                    let _ = release_rx.recv_timeout(Duration::from_secs(1));
                }
                WitnessReadStage::AfterNativeByte(b'\n') => {
                    let _ = native_tx.send(Instant::now());
                }
                _ => {}
            })
        }))
    });
    let prechecked = pre_rx.recv_timeout(Duration::from_millis(200));
    while Instant::now() < deadline {
        thread::yield_now();
    }
    let sent = peer.write_all(b"\n");
    let _ = release_tx.send(());
    let observed_lf = native_rx.recv_timeout(Duration::from_secs(1));
    drop(peer);
    let result = reader
        .join()
        .expect("join only actual native refusal reader");

    assert!(
        sent.is_ok(),
        "real controller must deliver final LF after cutoff"
    );
    assert!(
        prechecked.is_ok_and(|at| at < deadline),
        "the entire prior refusal prefix passed an in-deadline native pre-read check"
    );
    assert!(
        observed_lf.is_ok_and(|at| at >= deadline),
        "actual native LF must be consumed after the original clock"
    );
    assert!(
        panic_message(result).contains("byte arrived after original deadline"),
        "post-native-byte clock must reject late LF before final frame decode"
    );
}

#[test]
fn genuine_timely_private_denial_witness_still_accepts_exact_frame() {
    let (_root, listener, mut peer) = owned_native_peer();
    peer.write_all(b"DENIED 123\n")
        .expect("write original exact native refusal record");
    let record = await_denial_witness(&listener, Instant::now() + Duration::from_secs(1));
    assert_eq!(record, "DENIED 123\n");
}

#[test]
fn partial_real_native_refusal_eof_cannot_be_a_complete_record() {
    let (_root, listener, mut peer) = owned_native_peer();
    peer.write_all(b"DENIED 123")
        .expect("write native partial refusal record");
    peer.shutdown(std::net::Shutdown::Write)
        .expect("send real peer EOF without newline");
    let result = catch_unwind(AssertUnwindSafe(|| {
        await_denial_witness(&listener, Instant::now() + Duration::from_millis(250))
    }));
    assert!(
        panic_message(result).contains("truncated independent denial witness"),
        "native partial EOF must never authorize the expected refusal record"
    );
}
