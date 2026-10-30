//! macOS kernel-entry proof for the original peer-FIN shutdown test.
//!
//! Test-only Mach inspection freezes this process's *own* reader thread at
//! its BSD socket receive trap. The peer's sole writer remains open through the
//! original two-second release verdict. No production FFI or tracing privilege.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
#![allow(clippy::expect_used, clippy::panic)] // Test assertions and child custody.

use std::io::{ErrorKind, Read as _, Write as _};
use std::os::fd::AsRawFd as _;
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use keld_ipc::IpcError;
use keld_ipc::link::{AppLinkDeadlines, read_frame};

const STEP_DEADLINE: Duration = Duration::from_secs(2);
const CHILD_WATCHDOG: Duration = Duration::from_secs(8);
const CHILD_SELECTOR: &str = "original_shutdown_releases_kernel_blocked_reader";
const CHILD_MODE: &str = "KELD_KIPC_BLOCKED_READ_KERNEL_CHILD";
#[path = "support/macos_read_witness.rs"]
mod mach_reader;

fn prove_original_peer_fin(mode: &str) {
    let (mut reader, writer) = UnixStream::pair().expect("real Unix socket pair");
    let reader_fd = reader.as_raw_fd();
    assert_eq!(reader.read_timeout().expect("blocking reader"), None);
    assert_eq!(writer.write_timeout().expect("blocking writer"), None);
    let (port_tx, port_rx) = mpsc::sync_channel(1);
    let (done_tx, done_rx) = mpsc::channel();
    let (guard_tx, guard_rx) = mpsc::channel::<()>();
    let guard = mode == "guard";
    let worker = thread::spawn(move || {
        // The observer creates its own right and transfers custody once.
        let port = mach_reader::owned_current_reader_port();
        port_tx
            .send(port)
            .expect("transfer reader right to controller");
        if guard {
            let _ = guard_rx.recv_timeout(CHILD_WATCHDOG); // Never inside native read.
        }
        let result = read_frame(&mut reader);
        let _ = done_tx.send((Instant::now(), result));
    });
    let startup_deadline = Instant::now()
        .checked_add(STEP_DEADLINE)
        .expect("original reader startup clock");
    let port = port_rx
        .recv_timeout(startup_deadline.saturating_duration_since(Instant::now()))
        .expect("reader Mach right within original 2s");
    println!("KIPC_KERNEL_PHASE=reader-registered");
    // The guard is an independent negative control: a single Mach snapshot
    // must refuse its userspace-held reader without waiting for a timeout.
    let mut frozen = if guard {
        mach_reader::FrozenReader::suspend(&port).and_then(|stopped| {
            if stopped.is_native_blocked_read(reader_fd)? {
                Ok(Some(stopped))
            } else {
                Ok(None)
            }
        })
    } else {
        mach_reader::freeze_inside_kernel_read(&port, reader_fd, startup_deadline)
    };
    let genuinely_blocked = frozen.as_ref().is_ok_and(Option::is_some);
    let premature = done_rx.try_recv();
    println!("KIPC_KERNEL_PHASE=mach-checked");

    // One absolute clock begins BEFORE the shutdown syscall. The OS shutdown
    // and reader's publication share the same two seconds, with no renewal.
    // The opposite endpoint's Write half supplies peer FIN while the sole
    // writer stays owned through the already-snapshotted result.
    let shutdown_started = Instant::now();
    let result_deadline = shutdown_started
        .checked_add(STEP_DEADLINE)
        .expect("fixed original shutdown-result clock");
    let shutdown = writer.shutdown_app_link();
    let shutdown_returned = Instant::now();
    println!("KIPC_KERNEL_PHASE=shutdown-returned");
    if let Ok(Some(ref mut paused)) = frozen {
        paused.resume().expect("resume the exact native reader");
    }
    if guard {
        guard_tx.send(()).expect("release negative pre-read hold");
    }
    let result = done_rx.recv_timeout(result_deadline.saturating_duration_since(Instant::now()));
    println!("KIPC_KERNEL_PHASE=result-captured");
    drop(writer); // Cleanup close cannot create a passing result retroactively.
    worker
        .join()
        .expect("reader worker joins after peer-close cleanup");
    // The frozen-reader borrow keeps this Mach send right owned until
    // inspection and cleanup finish; its Drop then releases the right.

    assert!(matches!(premature, Err(TryRecvError::Empty)));
    shutdown.expect("native peer Write then Read");
    assert!(
        shutdown_returned < result_deadline,
        "native shutdown did not return within the original two seconds"
    );
    let (published_at, result) = result.expect("reader result within original 2s");
    assert!(
        published_at < result_deadline,
        "native reader result exceeded the original two-second clock"
    );
    let err = result.expect_err("peer FIN must not yield a complete frame");
    assert!(
        matches!(err, IpcError::Io(ref e) if matches!(
            e.kind(),
            ErrorKind::UnexpectedEof
                | ErrorKind::ConnectionReset
                | ErrorKind::BrokenPipe
                | ErrorKind::ConnectionAborted
        )),
        "native peer FIN must surface as I/O close: {err}"
    );
    assert!(err.to_string().contains("KELD-IPC-001"), "{err}");
    if let Err(error) = frozen.as_ref() {
        panic!("native Mach inspection failed code={error}");
    }
    if guard {
        assert!(
            !genuinely_blocked,
            "userspace guard passed kernel-read proof"
        );
        println!("KIPC_KERNEL_GUARD_REFUSED");
    } else {
        assert!(
            genuinely_blocked,
            "reader was never frozen inside BSD recvfrom"
        );
        println!("KIPC_KERNEL_READER_PROVED");
    }

    assert_next_socket_roundtrip();
}

/// A second owned socket pair must remain usable after the terminal shutdown.
fn assert_next_socket_roundtrip() {
    let (mut new_reader, mut new_writer) = UnixStream::pair().expect("fresh pair");
    new_writer.write_all(b"K").expect("fresh write");
    let mut byte = [0u8; 1];
    new_reader.read_exact(&mut byte).expect("fresh read");
    assert_eq!(byte, *b"K");
}

struct ChildOwner(Option<Child>);

impl Drop for ChildOwner {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn run_child(mode: &str, marker: &str) {
    let deadline = Instant::now()
        .checked_add(CHILD_WATCHDOG)
        .expect("finite child kill switch");
    let mut owner = ChildOwner(Some(
        Command::new(std::env::current_exe().expect("current test binary"))
            .args(["--exact", CHILD_SELECTOR, "--nocapture"])
            .env(CHILD_MODE, mode)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn original read fixture child"),
    ));
    let mut reason = "natural";
    loop {
        match owner.0.as_mut().expect("owned child").try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                thread::park_timeout(Duration::from_millis(5));
            }
            Ok(None) => {
                reason = "watchdog";
                break;
            }
            Err(_) => {
                reason = "observe-error";
                break;
            }
        }
    }
    if reason != "natural" {
        let _ = owner.0.as_mut().expect("owned child").kill();
    }
    let output = owner
        .0
        .take()
        .expect("owned child")
        .wait_with_output()
        .expect("reap and capture child");
    let captured = String::from_utf8_lossy(&output.stdout);
    let phase = [
        "result-captured",
        "shutdown-returned",
        "mach-checked",
        "reader-registered",
    ]
    .into_iter()
    .find(|phase| captured.contains(&format!("KIPC_KERNEL_PHASE={phase}")))
    .unwrap_or("unobserved");
    assert_eq!(
        reason, "natural",
        "emergency reap is a test failure: mode={mode} phase={phase}"
    );
    assert!(
        output.status.success(),
        "macOS kernel witness child failed: mode={mode} phase={phase} code={:?} signal={:?} stdout_bytes={} stderr_bytes={}",
        output.status.code(),
        output.status.signal(),
        output.stdout.len(),
        output.stderr.len()
    );
    assert_eq!(captured.matches(marker).count(), 1, "child oracle missing");
    assert_eq!(
        captured.matches("KIPC_KERNEL_CHILD_ENTERED").count(),
        1,
        "one real child selector must execute"
    );
}

#[test]
fn original_shutdown_releases_kernel_blocked_reader() {
    match std::env::var(CHILD_MODE) {
        // Same exact test selector runs under the parent-supplied mode.
        // There is no ignored/disabled fixture test in normal discovery.
        Ok(mode) => {
            assert!(matches!(mode.as_str(), "native" | "guard"));
            println!("KIPC_KERNEL_CHILD_ENTERED");
            prove_original_peer_fin(&mode);
        }
        Err(std::env::VarError::NotPresent) => {
            run_child("native", "KIPC_KERNEL_READER_PROVED");
            run_child("guard", "KIPC_KERNEL_GUARD_REFUSED");
        }
        Err(error) => panic!("invalid child selector mode: {error}"),
    }
}
