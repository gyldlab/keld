//! Real macOS local-clone shutdown: a live peer must not supply cleanup EOF.

// Cohesion review (GH-824): keep the OS reader, controller, child watchdog and
// post-shutdown verdict together while this is one macOS terminal scenario.
// Revisit this placement if another independent shutdown scenario is added.

#![allow(clippy::expect_used, clippy::panic)] // Test assertions and child ownership.

use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use super::{AppLinkDeadlines, HEADER_LEN, IpcError, read_frame};

const CHILD_SELECTOR: &str = "link::macos_shutdown_tests::local_clone_shutdown_native_child";
const CHILD_ENV: &str = "KELD_KIPC_MACOS_LOCAL_SHUTDOWN_CHILD";
const CHILD_MARKER: &str = "KIPC_LOCAL_SHUTDOWN_CHILD_ENTERED";
const OBSERVE_LIMIT: Duration = Duration::from_secs(2);
// An outer kill switch protects the test runner if kernel I/O or join never returns.
// It is not the two-second acceptance clock for either native-read or shutdown.
const CHILD_WATCHDOG: Duration = Duration::from_secs(8);

/// Reports only after an actual native `UnixStream` read has consumed a byte.
/// This proves native receive progress, not that the subsequent recv is already
/// blocked inside the kernel when the controller initiates shutdown.
struct NativeFirstRead {
    socket: UnixStream,
    first: Option<SyncSender<Instant>>,
}

impl Read for NativeFirstRead {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.socket.read(bytes)?;
        if count > 0
            && let Some(sender) = self.first.take()
        {
            let _ = sender.send(Instant::now());
        }
        Ok(count)
    }
}

/// The peer retains the only remaining write half until both OS observations
/// are captured. Otherwise dropping it would create an unrelated, passing EOF.
/// Suppress `SHUT_RD` alone to falsify local wake; the peer FIN still witnesses
/// `SHUT_WR`. Suppress `SHUT_WR` alone to falsify peer FIN independently.
fn prove_local_shutdown() {
    assert_eq!(HEADER_LEN, 16, "one byte must be an incomplete KIPC header");
    let (controller, mut peer) = UnixStream::pair().expect("real Unix socket pair");
    let (native_tx, native_rx) = mpsc::sync_channel(1);
    let (done_tx, done_rx) = mpsc::channel();
    let mut reader = NativeFirstRead {
        socket: controller.try_clone().expect("same-socket local reader"),
        first: Some(native_tx),
    };

    peer.write_all(&[0x4b]).expect("one incomplete header byte");
    let worker = thread::spawn(move || {
        let result = read_frame(&mut reader).map(|_| ());
        let _ = done_tx.send((Instant::now(), result));
    });

    let native_read = native_rx.recv_timeout(OBSERVE_LIMIT);
    let premature = done_rx.try_recv();
    let shutdown_started = Instant::now();
    let shutdown = controller.shutdown_app_link();
    // Preserve the original two-second result deadline from shutdown return.
    // Snapshot it BEFORE either socket owner is dropped.
    let released = done_rx.recv_timeout(OBSERVE_LIMIT);
    peer.set_nonblocking(true)
        .expect("nonblocking peer-FIN probe");
    let mut byte = [0_u8; 1];
    let peer_fin = peer.read(&mut byte);

    // Terminal cleanup is deliberately after the two immutable observations.
    // Closing this sole peer writer must never turn a failed shutdown into green.
    drop(peer);
    drop(controller);
    let joined = worker.join();

    let native_read = native_read.expect("native read did not consume the prefix within 2s");
    assert!(
        native_read < shutdown_started,
        "native recv must precede shutdown"
    );
    assert!(
        matches!(&premature, Err(TryRecvError::Empty)),
        "reader returned before shutdown: {premature:?}"
    );
    shutdown.expect("both real socket shutdown halves");
    joined.expect("native reader worker panicked");
    assert_eq!(
        peer_fin.expect("peer FIN probe"),
        0,
        "peer must receive FIN"
    );
    let (returned_at, result) =
        released.expect("same-socket shutdown did not release the reader within 2s");
    assert!(
        returned_at >= shutdown_started,
        "native read must exit after shutdown begins"
    );
    let err = result.expect_err("shutdown must not complete a partial frame");
    assert!(
        matches!(&err, IpcError::Io(source) if matches!(
            source.kind(),
            ErrorKind::UnexpectedEof
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::BrokenPipe
        )),
        "reader must observe a real I/O close: {err}"
    );
    assert!(err.to_string().contains("KELD-IPC-001"), "{err}");

    let (mut healthy_reader, mut healthy_writer) =
        UnixStream::pair().expect("fresh socket pair after teardown");
    healthy_writer.write_all(b"X").expect("fresh peer write");
    let mut healthy = [0_u8; 1];
    healthy_reader
        .read_exact(&mut healthy)
        .expect("fresh peer read");
    assert_eq!(healthy, *b"X");
}

/// The parent reaps its child even if an assertion panics while observing it.
struct ChildOwner(Option<Child>);

impl Drop for ChildOwner {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn shutdown_local_clone_releases_native_reader_and_sends_peer_fin() {
    let deadline = Instant::now()
        .checked_add(CHILD_WATCHDOG)
        .expect("finite child owner deadline");
    let mut child = ChildOwner(Some(
        Command::new(std::env::current_exe().expect("current test binary"))
            .args(["--exact", CHILD_SELECTOR, "--ignored", "--nocapture"])
            .env(CHILD_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn isolated macOS shutdown probe"),
    ));

    let mut reason = "natural";
    loop {
        match child.0.as_mut().expect("owned child").try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    reason = "watchdog";
                    break;
                }
                // Throttle exit polling; try_wait, not elapsed time, observes completion.
                thread::park_timeout(remaining.min(Duration::from_millis(5)));
            }
            Err(_) => {
                reason = "observe-error";
                break;
            }
        }
    }
    if reason != "natural" {
        let _ = child.0.as_mut().expect("owned child").kill();
    }
    let output = child
        .0
        .take()
        .expect("owned child")
        .wait_with_output()
        .expect("reap and collect macOS shutdown probe");
    assert_eq!(
        reason,
        "natural",
        "native shutdown child exceeded terminal bound: code={:?} signal={:?}",
        output.status.code(),
        output.status.signal()
    );
    assert!(
        output.status.success(),
        "native shutdown child failed: code={:?} signal={:?}; stderr={}",
        output.status.code(),
        output.status.signal(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .matches(CHILD_MARKER)
            .count(),
        1,
        "the selected child test must actually execute once"
    );
}

#[test]
#[ignore = "invoked by shutdown_local_clone_releases_native_reader_and_sends_peer_fin"]
fn local_clone_shutdown_native_child() {
    assert_eq!(
        std::env::var(CHILD_ENV).as_deref(),
        Ok("1"),
        "the native child must have an owning parent"
    );
    println!("{CHILD_MARKER}");
    prove_local_shutdown();
}
