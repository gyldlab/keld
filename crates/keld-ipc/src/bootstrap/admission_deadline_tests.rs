//! KEL-133 spec §3 criterion 6: an absolute generation/admission deadline
//! that byte drip cannot renew, proven against a real listener with a real
//! byte-drip **child process** (spec §7 row 6 and anti-flake rules: drip
//! actors run in child processes and report status/cleanup).
//!
//! The fixture has two independent generations. The first keeps its listener
//! open until two post-clock writes succeed, so scheduler latency cannot erase
//! the drip oracle. The second observes the host immediately before the real
//! handshake, then releases a separately connected child into a drip that
//! cannot complete inside the 100 ms generation deadline. Separating those
//! responsibilities is what makes wake starvation and deadline-renewal
//! mutations independently falsifiable.

#![allow(clippy::expect_used, clippy::panic)] // test module: expect/panic are the assertion oracles

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::link::set_test_read_entry_witness;
use crate::{
    BootstrapAdmissionFor, BootstrapRejection, BootstrapRejectionObserver, ChannelId,
    CorrelationId, FrameHeader, FrameKind, HEADER_LEN, SESSION_TOKEN_LEN, SessionToken,
    parse_app_link,
};

#[cfg(unix)]
use super::TestConsumeGate;
use super::{BootstrapListener as Listener, TestHandshakeWitness};

const ENDPOINT_ENV: &str = "KELD133_DRIP_APP_LINK";
const MODE_ENV: &str = "KELD133_DRIP_MODE";
const CHILD_ENTRY: &str = "bootstrap::admission_deadline_tests::drip_child_entry";
const TOKEN_CHUNK_BYTES: usize = 1;
const TOKEN_INTERVAL: Duration = Duration::from_millis(10);
const GENERATION_DEADLINE: Duration = Duration::from_millis(100);
const ADMISSION_OUTER_BOUND: Duration = Duration::from_millis(300);
const KILL_SWITCH: Duration = Duration::from_secs(10);

#[derive(Default)]
struct CountingObserver(std::sync::atomic::AtomicU32);

impl BootstrapRejectionObserver for CountingObserver {
    fn rejected(&self, _rejection: BootstrapRejection) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

struct BlockingObserver {
    observed: mpsc::SyncSender<BootstrapRejection>,
    release: std::sync::Mutex<mpsc::Receiver<()>>,
}

impl BootstrapRejectionObserver for BlockingObserver {
    fn rejected(&self, rejection: BootstrapRejection) {
        let _ = self.observed.send(rejection);
        let receiver = self
            .release
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = receiver.recv();
    }
}

/// Child guard: terminates a leftover child on test panic so no fixture
/// process outlives the test (house pattern from `windows_host_death_job`).
struct ObservedChild {
    child: Option<Child>,
    stderr_reader: Option<std::thread::JoinHandle<String>>,
}

struct ObservedLines {
    receiver: mpsc::Receiver<String>,
    reader: std::thread::JoinHandle<()>,
}

struct ChildReport {
    status: std::process::ExitStatus,
    transcript: String,
    stderr: String,
}

impl ObservedChild {
    fn wait_bounded(&mut self, bound: Duration) -> std::process::ExitStatus {
        let child = self.child.as_mut().expect("child present");
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().expect("try_wait") {
                return status;
            }
            assert!(
                started.elapsed() < bound,
                "drip child did not exit within {bound:?}"
            );
            // Coarse parked poll: never competes for the child's CPU.
            std::thread::park_timeout(Duration::from_millis(10));
        }
    }

    /// Forwards the child's stdout lines over a channel so the parent can
    /// block on an exact observable (`CONNECTED`) instead of a timer.
    fn stdout_lines(&mut self) -> ObservedLines {
        let stdout = self
            .child
            .as_mut()
            .expect("child present")
            .stdout
            .take()
            .expect("piped stdout");
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        ObservedLines {
            receiver: rx,
            reader,
        }
    }

    fn release_drip(&mut self) {
        let stdin = self
            .child
            .as_mut()
            .expect("child present")
            .stdin
            .as_mut()
            .expect("piped stdin");
        write_drip_release(stdin);
    }

    fn take_stdin(&mut self) -> ChildStdin {
        self.child
            .as_mut()
            .expect("child present")
            .stdin
            .take()
            .expect("piped stdin")
    }

    fn take_stderr(&mut self) -> String {
        self.stderr_reader
            .take()
            .expect("stderr reader present")
            .join()
            .expect("join child stderr reader")
    }
}

fn write_drip_release(stdin: &mut ChildStdin) {
    stdin.write_all(b"GO\n").expect("release drip child");
    stdin.flush().expect("flush drip release");
}

/// Blocks until a line starting with `prefix` arrives; `kill_switch` bounds
/// the wait and is never used to synchronize.
fn await_line(rx: &mpsc::Receiver<String>, prefix: &str, kill_switch: Duration) -> Vec<String> {
    let started = Instant::now();
    let mut seen = Vec::new();
    loop {
        let remaining = kill_switch
            .checked_sub(started.elapsed())
            .unwrap_or_else(|| panic!("child never printed {prefix:?}; saw {seen:?}"));
        let line = rx
            .recv_timeout(remaining)
            .unwrap_or_else(|_| panic!("child never printed {prefix:?}; saw {seen:?}"));
        let hit = line.starts_with(prefix);
        seen.push(line);
        if hit {
            return seen;
        }
    }
}

fn finish_child(
    mut child: ObservedChild,
    lines: ObservedLines,
    mut transcript: Vec<String>,
) -> ChildReport {
    let status = child.wait_bounded(KILL_SWITCH);
    lines.reader.join().expect("join child stdout reader");
    transcript.extend(lines.receiver);
    ChildReport {
        status,
        transcript: transcript.join("\n"),
        stderr: child.take_stderr(),
    }
}

fn assert_drip_report(report: &ChildReport, require_two_writes: bool) {
    assert!(
        report.status.success(),
        "drip child failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        report.status,
        report.transcript,
        report.stderr
    );
    assert!(
        report.stderr.is_empty(),
        "drip child wrote unexpected stderr:\n{}",
        report.stderr
    );
    if require_two_writes {
        assert!(
            report.transcript.matches("CHUNK ").count() >= 2,
            "child never completed two post-clock writes: {}",
            report.transcript
        );
    }
    assert!(
        report
            .transcript
            .lines()
            .any(|line| line.starts_with("DISCONNECTED ") || line == "CLOSED"),
        "drip child never reported terminal cleanup: {}",
        report.transcript
    );
}

impl Drop for ObservedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take()
            && child.try_wait().is_ok_and(|status| status.is_none())
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

fn spawn_child(app_link: &str, mode: &str) -> ObservedChild {
    let exe = std::env::current_exe().expect("test binary path");
    let mut child = Command::new(exe)
        .args(["--exact", CHILD_ENTRY, "--ignored", "--nocapture"])
        .env(ENDPOINT_ENV, app_link)
        .env(MODE_ENV, mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn drip child");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let stderr_reader = std::thread::spawn(move || {
        let mut output = String::new();
        stderr
            .read_to_string(&mut output)
            .expect("read child stderr");
        output
    });
    ObservedChild {
        child: Some(child),
        stderr_reader: Some(stderr_reader),
    }
}

fn prove_successful_post_clock_drip() {
    let listener = Listener::bind().expect("bind drip-witness listener");
    let app_link = listener.app_link();
    let (endpoint, _token) = parse_app_link(&app_link).expect("parse witness app link");
    let endpoint = endpoint.to_owned();
    let mut child = spawn_child(&app_link, "drip-witness");
    let lines = child.stdout_lines();
    let mut transcript = await_line(&lines.receiver, "READY", KILL_SWITCH);

    // This clock belongs only to the successful-write oracle. Admission is
    // deliberately deferred, so even the 300 ms wake-starvation control cannot
    // close the real transport before two post-clock writes complete.
    let drip_started = Instant::now();
    child.release_drip();
    transcript.extend(await_line(&lines.receiver, "CHUNK 1", KILL_SWITCH));
    assert!(
        drip_started.elapsed() < KILL_SWITCH,
        "post-clock drip witness exceeded its kill switch"
    );

    drop(listener);
    let report = finish_child(child, lines, transcript);
    assert_drip_report(&report, true);
    assert_locator_dead(&endpoint, "completed witness generation");
}

fn prove_rejection_retry_cannot_renew_generation() {
    let listener = std::sync::Arc::new(Listener::bind().expect("bind retry listener"));
    let app_link = listener.app_link();
    let (endpoint, _token) = parse_app_link(&app_link).expect("parse retry app link");
    let endpoint = endpoint.to_owned();
    let cancellation = listener.cancellation();
    let (observed_tx, observed_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let observer = BlockingObserver {
        observed: observed_tx,
        release: std::sync::Mutex::new(release_rx),
    };
    let generation_deadline = Instant::now() + Duration::from_millis(500);
    let worker_listener = std::sync::Arc::clone(&listener);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let result = worker_listener.accept_authenticated_until(generation_deadline, &observer);
        let _ = result_tx.send(result);
    });

    let mut hostile = raw_connect(&endpoint).expect("connect retry probe");
    hostile
        .write_all(&[0_u8; HEADER_LEN])
        .and_then(|()| hostile.flush())
        .expect("send malformed retry probe");
    assert!(
        matches!(
            observed_rx.recv_timeout(KILL_SWITCH),
            Ok(BootstrapRejection::Header)
        ),
        "malformed peer never reached the rejection boundary"
    );
    while Instant::now() < generation_deadline {
        std::thread::yield_now();
    }
    let released_at = Instant::now();
    release_tx.send(()).expect("release rejected peer after D");
    let result = match result_rx.recv_timeout(Duration::from_millis(200)) {
        Ok(result) => result.expect("retry admission"),
        Err(error) => {
            cancellation
                .cancel()
                .expect("cancel renewed retry admission");
            drop(hostile);
            worker.join().expect("join cancelled retry admission");
            panic!("rejection renewed the generation deadline: {error}");
        }
    };
    assert!(
        matches!(result, BootstrapAdmissionFor::DeadlineElapsed),
        "expired rejection retried with the wrong terminal result"
    );
    assert!(
        released_at.elapsed() < Duration::from_millis(200),
        "expired rejection did not terminate promptly"
    );
    drop(hostile);
    worker.join().expect("join retry admission");
    assert_locator_dead(&endpoint, "expired retry generation");
}

fn prove_fresh_generation(
    observer: &CountingObserver,
    expired_endpoint: &str,
    expired_token: SessionToken,
) {
    let fresh = Listener::bind().expect("bind fresh generation");
    let fresh_link = fresh.app_link();
    let (fresh_endpoint, fresh_token) =
        parse_app_link(&fresh_link).expect("parse fresh generation app link");
    assert!(
        fresh_endpoint != expired_endpoint,
        "fresh generation must mint a different endpoint"
    );
    assert!(
        fresh_token != expired_token,
        "fresh generation must mint a different redacted token"
    );
    let mut prompt = spawn_child(&fresh_link, "prompt");
    let lines = prompt.stdout_lines();
    let transcript = await_line(&lines.receiver, "CONNECTED", KILL_SWITCH);
    let fresh_started = Instant::now();
    let admitted = fresh
        .accept_authenticated_until(fresh_started + Duration::from_secs(5), observer)
        .expect("fresh listener");
    assert!(
        matches!(admitted, BootstrapAdmissionFor::Authenticated(_)),
        "a prompt peer must authenticate on the fresh generation"
    );
    let report = finish_child(prompt, lines, transcript);
    assert!(
        report.status.success()
            && report.stderr.is_empty()
            && report.transcript.contains("AUTHENTICATED"),
        "fresh prompt child failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        report.status,
        report.transcript,
        report.stderr
    );
}

/// The named criterion-6 test: independently anchored successful drip, then a
/// real 100 ms admission whose connected-peer handshake stage is observed
/// before the paced child is released. Terminal state, joined child, dead
/// locator, and next fresh-generation success follow in that order.
#[test]
fn a_byte_drip_child_cannot_renew_a_100ms_generation_deadline() {
    prove_successful_post_clock_drip();
    prove_rejection_retry_cannot_renew_generation();

    let listener = Listener::bind().expect("bind deadline listener");
    let app_link = listener.app_link();
    let (endpoint, token) = parse_app_link(&app_link).expect("parse deadline app link");
    let endpoint = endpoint.to_owned();
    let observer = CountingObserver::default();
    let mut child = spawn_child(&app_link, "deadline-drip");
    let lines = child.stdout_lines();
    let transcript = await_line(&lines.receiver, "READY", KILL_SWITCH);

    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    listener.install_handshake_witness(TestHandshakeWitness {
        entered: entered_tx,
    });
    let (read_entered_tx, read_entered_rx) = mpsc::channel();
    set_test_read_entry_witness(Some(read_entered_tx));
    let mut drip_release = child.take_stdin();
    let coordinator = std::thread::spawn(move || {
        let entry = entered_rx
            .recv_timeout(KILL_SWITCH)
            .expect("listener never entered the connected-peer handshake");
        let read_entered_at = read_entered_rx
            .recv_timeout(KILL_SWITCH)
            .expect("listener never entered the first handshake read");
        write_drip_release(&mut drip_release);
        (entry, read_entered_at)
    });

    let started = Instant::now();
    let generation_deadline = started + GENERATION_DEADLINE;
    let admission = listener
        .accept_authenticated_until(generation_deadline, &observer)
        .expect("host-side listener must not fail");
    set_test_read_entry_witness(None);
    let elapsed = started.elapsed();
    let deadline_elapsed = matches!(admission, BootstrapAdmissionFor::DeadlineElapsed);

    // Probe before a second admission call or explicit listener drop, so
    // neither can mask a first-expiry cleanup defect.
    assert_locator_dead(&endpoint, "expired deadline generation");

    // Terminal state: the expired generation answers immediately. Unix
    // reports DeadlineElapsed again; Windows refuses as already consumed.
    let again_started = Instant::now();
    let again = listener.accept_authenticated_until(generation_deadline, &observer);
    #[cfg(unix)]
    assert!(
        matches!(again, Ok(BootstrapAdmissionFor::DeadlineElapsed)),
        "expired Unix generation returned the wrong terminal result: {again:?}"
    );
    #[cfg(windows)]
    assert!(
        matches!(again, Err(ref error) if error.kind() == std::io::ErrorKind::NotConnected),
        "consumed Windows generation returned the wrong terminal result: {again:?}"
    );
    assert!(
        again_started.elapsed() < Duration::from_millis(200),
        "an expired generation must answer immediately: {:?}",
        again_started.elapsed()
    );

    // Drop a mutated late-authentication stream before joining its child; the
    // ordinary deadline path carries no stream, so this is only cleanup.
    drop(admission);
    let (entry, read_entered_at) = coordinator.join().expect("join drip coordinator");
    let report = finish_child(child, lines, transcript);
    assert_eq!(
        entry.generation_deadline,
        Some(generation_deadline),
        "handshake witness reported a different generation deadline"
    );
    assert!(
        entry.entered_at < generation_deadline,
        "handshake began only after the generation deadline"
    );
    assert!(
        read_entered_at < generation_deadline,
        "deadline generation entered its first read only after expiry"
    );
    assert_drip_report(&report, true);

    // These are separate from the child transcript oracle above. Correct code
    // returns at 100 ms. The full 16-byte header is immediate, proving a host
    // read on Windows as well as Unix. The 32-byte token follows one byte every
    // 10 ms. Every operation is far shorter than the deadline, but a per-read
    // renewal completes no earlier than 32 * 10 = 320 ms and must violate the
    // strict 300 ms outer bound even after late authentication is downgraded.
    assert!(
        deadline_elapsed,
        "drip must not authenticate and must not keep admission open"
    );
    assert!(
        elapsed >= GENERATION_DEADLINE,
        "admission ended before its absolute deadline: {elapsed:?}"
    );
    assert!(
        elapsed < ADMISSION_OUTER_BOUND,
        "byte drip renewed the generation deadline: {elapsed:?}"
    );
    assert_eq!(
        entry.peer_deadline, generation_deadline,
        "connected peer did not inherit the exact generation deadline"
    );
    assert_eq!(
        observer.0.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "generation expiry is not a peer rejection"
    );

    drop(listener);
    prove_fresh_generation(&observer, &endpoint, token);
}

#[cfg(unix)]
#[test]
fn unix_generation_expiry_at_final_auth_boundary_is_terminal() {
    let listener = std::sync::Arc::new(Listener::bind().expect("bind final-boundary listener"));
    let app_link = listener.app_link();
    let (endpoint, _token) = parse_app_link(&app_link).expect("parse final-boundary link");
    let endpoint = endpoint.to_owned();
    let mut child = spawn_child(&app_link, "prompt");
    let lines = child.stdout_lines();
    let transcript = await_line(&lines.receiver, "CONNECTED", KILL_SWITCH);
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    listener.install_before_consume_gate(TestConsumeGate {
        entered: entered_tx,
        release: release_rx,
    });

    let deadline = Instant::now() + Duration::from_millis(500);
    let worker_listener = std::sync::Arc::clone(&listener);
    let worker = std::thread::spawn(move || {
        worker_listener.accept_authenticated_until(deadline, &CountingObserver::default())
    });
    entered_rx
        .recv_timeout(KILL_SWITCH)
        .expect("server never reached the final authentication boundary");
    while Instant::now() < deadline {
        std::thread::yield_now();
    }
    release_tx
        .send(())
        .expect("release final authentication boundary");
    let admission = worker
        .join()
        .expect("join final-boundary worker")
        .expect("final-boundary admission");
    assert!(
        matches!(admission, BootstrapAdmissionFor::DeadlineElapsed),
        "late Unix authentication must be terminal deadline expiry"
    );
    assert_locator_dead(&endpoint, "final-boundary generation");

    let report = finish_child(child, lines, transcript);
    assert!(
        report.status.success()
            && report.stderr.is_empty()
            && report.transcript.contains("AUTHENTICATED"),
        "prompt child failed at final boundary: {:?}\nstdout:\n{}\nstderr:\n{}",
        report.status,
        report.transcript,
        report.stderr
    );
}

#[cfg(unix)]
fn raw_connect(endpoint: &str) -> std::io::Result<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(endpoint)
}

#[cfg(unix)]
fn assert_locator_dead(endpoint: &str, generation: &str) {
    let error = raw_connect(endpoint).expect_err("dead Unix locator must reject connect");
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::NotFound,
        "{generation} locator failed for the wrong reason: {error}"
    );
    let metadata = std::fs::symlink_metadata(endpoint);
    assert!(
        matches!(metadata, Err(ref error) if error.kind() == std::io::ErrorKind::NotFound),
        "{generation} Unix socket path was not removed: {metadata:?}"
    );
}

#[cfg(windows)]
fn raw_connect(endpoint: &str) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(endpoint)
}

#[cfg(windows)]
fn assert_locator_dead(endpoint: &str, generation: &str) {
    let error = raw_connect(endpoint).expect_err("dead Windows locator must reject connect");
    assert_eq!(
        error.raw_os_error(),
        Some(2),
        "{generation} pipe failed for the wrong reason: {error}"
    );
}

fn write_drip_chunk(stream: &mut impl Write, index: usize, chunk: &[u8]) -> bool {
    match stream.write_all(chunk).and_then(|()| stream.flush()) {
        Ok(()) => {
            println!("CHUNK {index}");
            true
        }
        Err(error) => {
            println!("DISCONNECTED {index} {}", error.kind());
            false
        }
    }
}

/// Private subprocess entry point: connects to the listener under
/// `KELD133_DRIP_APP_LINK` and either drips a valid HELLO in 6-byte chunks
/// with an adversarial schedule (`drip`) or sends it whole and completes the
/// handshake (`prompt`). Prints a line protocol for the parent's assertions.
#[test]
#[ignore = "private subprocess entry point"]
fn drip_child_entry() {
    let Ok(app_link) = std::env::var(ENDPOINT_ENV) else {
        return; // not invoked as a child
    };
    let mode = std::env::var(MODE_ENV).unwrap_or_else(|_| "drip".to_owned());
    let (endpoint, token) = parse_app_link(&app_link).expect("child parses app link");

    let mut stream = raw_connect(endpoint).expect("child connects");
    println!("CONNECTED");

    let header = FrameHeader {
        kind: FrameKind::Hello,
        flags: 0,
        channel: ChannelId(0),
        corr: CorrelationId(0),
        len: u32::try_from(SESSION_TOKEN_LEN).expect("token length"),
    }
    .encode();
    let mut frame = header.to_vec();
    frame.extend_from_slice(token.as_bytes());

    match mode.as_str() {
        "drip-witness" | "deadline-drip" => {
            println!("READY");
            std::io::stdout().flush().expect("flush drip readiness");
            let mut go = String::new();
            std::io::stdin()
                .read_line(&mut go)
                .expect("child reads the release line");
            assert_eq!(go.trim(), "GO", "unexpected drip release line");

            if !write_drip_chunk(&mut stream, 0, &frame[..HEADER_LEN]) {
                return;
            }
            let token_chunks = frame[HEADER_LEN..].chunks(TOKEN_CHUNK_BYTES);
            assert_eq!(token_chunks.len(), 32, "token must have 32 chunks");
            for (token_index, chunk) in token_chunks.enumerate() {
                std::thread::sleep(TOKEN_INTERVAL);
                if !write_drip_chunk(&mut stream, token_index + 1, chunk) {
                    return;
                }
            }
            println!("DRIP_COMPLETE");
            // Drain until the host closes; it must never authenticate us.
            let mut sink = [0u8; 64];
            loop {
                match stream.read(&mut sink) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            println!("CLOSED");
        }
        "prompt" => {
            stream.write_all(&frame).expect("prompt hello");
            stream.flush().expect("flush");
            let mut reply = vec![0u8; HEADER_LEN + SESSION_TOKEN_LEN];
            stream.read_exact(&mut reply).expect("server hello reply");
            println!("AUTHENTICATED");
        }
        other => panic!("unknown drip mode {other}"),
    }
}
