//! Original KGC1 pre-generation S/KSA shutdown must never accept host EOF
//! after its bounded post-acknowledgment deadline. Both witnesses are actual
//! kernel pipe/socket I/O, not fabricated `HostLiveness` enum outcomes.
use super::{acknowledge_pre_generation_orderly_shutdown_with_hook, set_liveness_nonblocking};
use crate::RuntimeError;
use crate::macos_guardian::SUPERVISED_ORDERLY_ACK;
use nix::unistd::pipe;
use std::fs::File;
use std::io::{self, Read as _};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn native_host_eof_observed_before_cutoff_cannot_acknowledge_after_original_deadline() {
    let (read_fd, write_fd) = pipe().expect("own actual host liveness pipe");
    let mut reader = File::from(read_fd);
    set_liveness_nonblocking(&reader, true).expect("real bounded host EOF reader");
    let (mut ack_writer, mut ack_peer) = UnixStream::pair().expect("real Guardian KSA socket pair");
    ack_peer
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("bound independent native acknowledgment receipt");
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        acknowledge_pre_generation_orderly_shutdown_with_hook(
            &mut reader,
            &mut ack_writer,
            Duration::from_millis(250),
            |deadline| {
                let _ = seen_tx.send((Instant::now(), deadline));
                let _ = release_rx.recv_timeout(Duration::from_secs(1));
            },
        )
    });

    // Only after the actual Guardian has written its KSA may Host close the
    // exact liveness writer. The callback runs only AFTER native pipe EOF.
    let mut received_ack = [0_u8; 3];
    let ack_result = ack_peer.read_exact(&mut received_ack);
    drop(write_fd);
    let observed = seen_rx.recv_timeout(Duration::from_millis(200));
    if let Ok((_, deadline)) = observed {
        while Instant::now() < deadline {
            thread::yield_now();
        }
    }
    let _ = release_tx.send(());
    let result = worker
        .join()
        .expect("join owned native EOF observer thread");

    assert!(
        ack_result.is_ok(),
        "native Guardian must write its real KSA"
    );
    assert_eq!(received_ack, SUPERVISED_ORDERLY_ACK);
    assert!(
        observed.is_ok_and(|(at, deadline)| at < deadline),
        "real liveness pipe EOF must be read before the original deadline"
    );
    assert!(
        matches!(
            result,
            Err(RuntimeError::Lifecycle {
                phase: "macOS guardian pre-generation host EOF deadline",
                source,
            }) if source.kind() == io::ErrorKind::TimedOut
        ),
        "late native EOF must preserve the exact typed lifecycle timeout, not another error or a clean KSA transition"
    );
}

#[test]
fn real_native_host_eof_within_original_post_ack_budget_is_still_orderly() {
    let (read_fd, write_fd) = pipe().expect("own native host liveness pipe");
    let mut reader = File::from(read_fd);
    set_liveness_nonblocking(&reader, true).expect("native nonblocking host EOF");
    let (mut ack_writer, mut ack_peer) = UnixStream::pair().expect("real native KSA pair");
    ack_peer
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("bounded acknowledgment read");
    drop(write_fd);
    acknowledge_pre_generation_orderly_shutdown_with_hook(
        &mut reader,
        &mut ack_writer,
        Duration::from_secs(1),
        |_| {},
    )
    .expect("actual native host EOF within original budget is orderly");
    let mut ack = [0_u8; 3];
    ack_peer.read_exact(&mut ack).expect("consume real KSA");
    assert_eq!(ack, SUPERVISED_ORDERLY_ACK);
}
