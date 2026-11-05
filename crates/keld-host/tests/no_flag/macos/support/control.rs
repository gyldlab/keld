use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write as _;
use std::net::TcpListener;
use std::net::TcpStream;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::process::Child;
use std::process::Output;
use std::thread;
use std::time::Duration;
use std::time::Instant;

/// A fixture listener whose peers are accepted by polling a NONBLOCKING
/// socket. Every single-peer no-flag macOS fixture accept (observer TCP,
/// control and recovery-generation Unix sockets) goes through
/// [`try_accept_before_observing`]: a completed client `connect` does not mean
/// the server side is already dequeued, so a single immediate `accept` may
/// return `WouldBlock` (Darwin errno 35).
pub(crate) trait FixtureListener {
    type Stream;

    /// One native `accept`, with no flag changes.
    fn accept_stream(&self) -> std::io::Result<Self::Stream>;

    /// BSD `accept` inherits the listener's `O_NONBLOCK`; fixture readers
    /// rely on blocking reads with their own read deadline.
    fn normalize_blocking(stream: &Self::Stream) -> std::io::Result<()>;
}

impl FixtureListener for UnixListener {
    type Stream = UnixStream;

    fn accept_stream(&self) -> std::io::Result<UnixStream> {
        self.accept().map(|(stream, _)| stream)
    }

    fn normalize_blocking(stream: &UnixStream) -> std::io::Result<()> {
        stream.set_nonblocking(false)
    }
}

impl FixtureListener for TcpListener {
    type Stream = TcpStream;

    fn accept_stream(&self) -> std::io::Result<TcpStream> {
        self.accept().map(|(stream, _)| stream)
    }

    fn normalize_blocking(stream: &TcpStream) -> std::io::Result<()> {
        stream.set_nonblocking(false)
    }
}

/// Why [`try_accept_before_observing`] produced no peer.
#[derive(Debug)]
pub(crate) enum AcceptFailure {
    /// The original deadline passed before an accept attempt could start.
    Deadline,
    /// `accept` failed with something other than an empty queue.
    Accept(std::io::Error),
    /// The accepted stream could not be returned to blocking mode.
    Normalize(std::io::Error),
}

pub(crate) fn accept_before<L: FixtureListener>(listener: &L, deadline: Instant) -> L::Stream {
    accept_before_observing(listener, deadline, || {})
}

/// Panicking form of [`try_accept_before_observing`] for test bodies.
pub(crate) fn accept_before_observing<L: FixtureListener>(
    listener: &L,
    deadline: Instant,
    on_empty: impl FnMut(),
) -> L::Stream {
    match try_accept_before_observing(listener, deadline, on_empty) {
        Ok(stream) => stream,
        Err(AcceptFailure::Deadline) => panic!("fixture peer was not accepted before its deadline"),
        Err(AcceptFailure::Accept(error)) => panic!("accept fixture peer: {error}"),
        Err(AcceptFailure::Normalize(error)) => {
            panic!("normalize accepted fixture stream: {error}")
        }
    }
}

/// Accept the next queued peer, retrying only an empty queue until the
/// caller's ORIGINAL deadline. The deadline is checked before every native
/// attempt, so no attempt starts after it. A peer dequeued by the last
/// attempt that started before the deadline is accepted even if that one
/// `accept` syscall returns just after it; that window is one syscall wide.
/// `on_empty` observes each real `WouldBlock`; it cannot substitute an
/// accept result.
pub(crate) fn try_accept_before_observing<L: FixtureListener>(
    listener: &L,
    deadline: Instant,
    mut on_empty: impl FnMut(),
) -> Result<L::Stream, AcceptFailure> {
    loop {
        if Instant::now() >= deadline {
            return Err(AcceptFailure::Deadline);
        }
        match listener.accept_stream() {
            Ok(stream) => {
                L::normalize_blocking(&stream).map_err(AcceptFailure::Normalize)?;
                return Ok(stream);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                on_empty();
                thread::yield_now();
            }
            Err(error) => return Err(AcceptFailure::Accept(error)),
        }
    }
}

/// Darwin SDK sys/fcntl.h `O_NONBLOCK`.
pub(crate) const O_NONBLOCK: std::ffi::c_int = 4;

/// Independent census of a live descriptor's file status flags (`F_GETFL`).
pub(crate) fn file_status_flags(descriptor: &impl AsRawFd) -> std::ffi::c_int {
    // Darwin SDK sys/fcntl.h: F_GETFL=3, takes no variadic argument.
    const F_GETFL: std::ffi::c_int = 3;
    // SAFETY: the borrow keeps the descriptor live for this call. F_GETFL
    // only reads descriptor status; no pointer, ownership transfer, lifetime
    // extension or status mutation occurs in this independent census.
    let flags = unsafe { fcntl(descriptor.as_raw_fd(), F_GETFL) };
    assert!(
        flags >= 0,
        "file status flag census: {}",
        std::io::Error::last_os_error()
    );
    flags
}

#[test]
fn accept_before_waits_for_a_peer_that_connects_after_real_would_block() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("own native listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking fixture accept mode");
    let address = listener.local_addr().expect("native listener address");
    let (release, released) = std::sync::mpsc::channel::<()>();
    // The peer connects only after the helper has observed a real empty
    // queue. If the helper panics first, dropping `release` ends the peer.
    let peer = thread::spawn(move || {
        if released.recv().is_err() {
            return;
        }
        let mut stream = TcpStream::connect(address).expect("connect native peer");
        stream.write_all(b"W").expect("native peer byte");
    });
    let mut empty_polls = 0_u32;
    let mut release = Some(release);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        accept_before_observing(&listener, Instant::now() + super::EVENT_DEADLINE, || {
            empty_polls += 1;
            if let Some(release) = release.take() {
                release.send(()).expect("release native peer");
            }
        })
    }));
    drop(release);
    peer.join().expect("native peer joins");
    let mut accepted = outcome.expect("WouldBlock before the peer is not an accept failure");
    assert!(empty_polls >= 1, "the helper never saw a real empty queue");
    // The listener is nonblocking and Darwin accept inherits that flag; the
    // owner must hand back a blocking stream.
    assert_eq!(
        file_status_flags(&accepted) & O_NONBLOCK,
        0,
        "accepted TCP fixture stream stayed nonblocking"
    );
    let mut byte = [0_u8; 1];
    accepted
        .set_read_timeout(Some(super::EVENT_DEADLINE))
        .expect("bounded accepted read");
    accepted
        .read_exact(&mut byte)
        .expect("accepted native peer byte");
    assert_eq!(byte, *b"W");
}

#[test]
fn accept_before_returns_a_blocking_unix_stream_from_a_nonblocking_listener() {
    let root = tempfile::tempdir().expect("unix accept root");
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).expect("own unix fixture listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking fixture accept mode");
    let _peer = UnixStream::connect(&path).expect("queued unix peer");
    let accepted = accept_before(&listener, Instant::now() + super::EVENT_DEADLINE);
    // Darwin accept inherits the listener's O_NONBLOCK; the owner must clear
    // it, or the next control-line read fails with WouldBlock.
    assert_eq!(
        file_status_flags(&accepted) & O_NONBLOCK,
        0,
        "accepted Unix fixture stream stayed nonblocking"
    );
}

#[test]
fn accept_before_refuses_a_queued_peer_after_the_original_deadline() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("own native listener");
    let deadline = Instant::now();
    let peer = TcpStream::connect(listener.local_addr().expect("native listener address"))
        .expect("queued native peer");
    // A blocking listener with a completed connect has a dequeuable peer, so
    // only the deadline can refuse it.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        accept_before(&listener, deadline)
    }));
    let panic = outcome.expect_err("a peer accepted after the deadline is not timely");
    assert_eq!(
        panic.downcast_ref::<&str>().copied(),
        Some("fixture peer was not accepted before its deadline")
    );
    let late = listener
        .accept()
        .expect("the queued peer was left unaccepted");
    assert_eq!(
        late.1,
        peer.local_addr().expect("queued native peer address")
    );
}

pub(crate) fn read_control_line(reader: &mut BufReader<UnixStream>) -> String {
    let mut line = String::new();
    let read = reader.read_line(&mut line).expect("read control line");
    assert_ne!(read, 0, "control EOF before observation");
    line.trim_end().to_owned()
}

pub(crate) fn wait_child_output(child: Child, timeout: Duration) -> Output {
    wait_child_output_observing(child, timeout, || {})
}

pub(crate) fn wait_child_output_observing(
    child: Child,
    timeout: Duration,
    mut observe: impl FnMut(),
) -> Output {
    // Install custody before pipe setup or callbacks can fail. Rust's Child
    // alone does not kill/reap on Drop.
    let mut child = ReapOnDrop(child);
    let mut stdout = PipeCapture::new(child.0.stdout.take());
    let mut stderr = PipeCapture::new(child.0.stderr.take());
    let deadline = Instant::now() + timeout;
    let mut status = None;
    loop {
        // Bound work per stream so continuous output cannot starve observation
        // or the deadline. No reader thread can outlive this wait on unwind.
        stdout.read_once();
        stderr.read_once();
        if status.is_none() {
            status = child.0.try_wait().expect("inspect child exit");
        }
        if let Some(status) = status
            && stdout.pipe.is_none()
            && stderr.pipe.is_none()
        {
            return Output {
                status,
                stdout: stdout.bytes,
                stderr: stderr.bytes,
            };
        }
        if status.is_none() {
            observe();
        }
        assert!(
            Instant::now() < deadline,
            "child exceeded exit/output deadline (status={status:?}, stdout_bytes={}, stderr_bytes={}, stderr_markers={:?})",
            stdout.bytes.len(),
            stderr.bytes.len(),
            redacted_stderr_markers(&stderr.bytes),
        );
        thread::yield_now();
    }
}

/// Reveal only fixed marker names on failure. Raw host stderr may contain
/// temporary file paths or runtime bootstrap details; never print its body.
fn redacted_stderr_markers(stderr: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .take(24)
        .map(|line| {
            line.split_whitespace()
                .next()
                .map(|word| word.trim_end_matches(':'))
                .filter(|word| {
                    // Keep only a known marker or an existing typed code shape,
                    // never its following path, bootstrap data or error body.
                    let typed_code = [
                        "KELD-CORE-",
                        "KELD-WV-",
                        "KELD-RUNTIME-",
                        "KELD-IPC-",
                        "KELD-GUARD",
                        "KELD-COMPAT-",
                        "KELD-NATIVE-",
                        "KELD-CLI-",
                        "KELD-UPDATE-",
                    ]
                    .into_iter()
                    .any(|prefix| {
                        word.strip_prefix(prefix).is_some_and(|digits| {
                            digits.len() == 3 && digits.bytes().all(|byte| byte.is_ascii_digit())
                        })
                    });
                    typed_code
                        || matches!(
                            *word,
                            "KELD_KEL142_BIND"
                                | "KELD_KEL140_PUBLIC_APP_COMPONENT"
                                | "KELD_KEL140_PUBLIC_QUIT_REQUEST"
                                | "KELD_KEL140_PUBLIC_FS_RETIRED"
                        )
                })
                .unwrap_or("[redacted]")
                .to_owned()
        })
        .collect()
}

#[test]
fn bounded_child_timeout_stderr_diagnostics_do_not_echo_untrusted_fields() {
    let markers = redacted_stderr_markers(
        b"KELD_KEL140_PUBLIC_QUIT_REQUEST calls=0 secret=value\nuntrusted /tmp/secret-material\nKELD_KEL142_BIND nonce=private\nKELD_KEL_BROKEN/path should-not-print\nKELD_KELSECRET_ABC123PRIVATE\nKELD-CORE-037: /private/secret bootstrap=hidden\nKELD-GUARD005: secret parser input\nKELD-CORE-037PRIVATE must-not-print\nKELD-SECRET-123 must-not-print\n",
    );
    assert_eq!(
        markers,
        [
            "KELD_KEL140_PUBLIC_QUIT_REQUEST",
            "[redacted]",
            "KELD_KEL142_BIND",
            "[redacted]",
            "[redacted]",
            "KELD-CORE-037",
            "KELD-GUARD005",
            "[redacted]",
            "[redacted]"
        ]
    );
}

/// Owns a spawned child so every exit path, including unwinding, kills and
/// reaps it.
pub(crate) struct ReapOnDrop(pub(crate) Child);

impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        // Child caches an observed exit, so these do not signal a reused PID.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct PipeCapture<P> {
    pipe: Option<P>,
    bytes: Vec<u8>,
    limit: Option<usize>,
}

unsafe extern "C" {
    fn fcntl(fd: std::ffi::c_int, command: std::ffi::c_int, ...) -> std::ffi::c_int;
}

impl<P: Read + AsRawFd> PipeCapture<P> {
    fn new(pipe: Option<P>) -> Self {
        if let Some(pipe) = &pipe {
            // Darwin SDK sys/fcntl.h: retain every existing status flag while
            // enabling nonblocking reads for bounded fixture cleanup.
            const F_SETFL: std::ffi::c_int = 4;
            let flags = file_status_flags(pipe);
            // SAFETY: `pipe` owns this live descriptor for the entire borrow.
            // F_SETFL takes one integer: its current status flags (from the
            // census above) plus Darwin O_NONBLOCK. No pointers, ownership
            // transfer or lifetime extension.
            let result = unsafe { fcntl(pipe.as_raw_fd(), F_SETFL, flags | O_NONBLOCK) };
            assert_eq!(
                result,
                0,
                "nonblocking capture: {}",
                std::io::Error::last_os_error()
            );
        }
        Self {
            pipe,
            bytes: Vec::new(),
            limit: None,
        }
    }

    fn with_limit(pipe: Option<P>, limit: usize) -> Self {
        let mut capture = Self::new(pipe);
        capture.limit = Some(limit);
        capture
    }

    fn read_once(&mut self) {
        let Some(pipe) = &mut self.pipe else { return };
        let mut buffer = [0; 8192];
        match pipe.read(&mut buffer) {
            Ok(0) => {
                self.pipe.take();
            }
            Ok(count) => {
                assert!(
                    self.limit
                        .is_none_or(|limit| count <= limit.saturating_sub(self.bytes.len())),
                    "live child output exceeded capture byte limit"
                );
                self.bytes.extend_from_slice(&buffer[..count]);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => panic!("drain child pipe: {error}"),
        }
    }
}

/// Immediate live draining mode of the existing `PipeCapture` owner. It owns
/// only stdout/stderr, never a Child, process identity, waiter or cleanup grant.
type LiveOutputResult = Result<(Vec<u8>, Vec<u8>), &'static str>;

pub(crate) struct LiveChildOutput {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<LiveOutputResult>>,
}

impl LiveChildOutput {
    pub(crate) fn start(child: &mut Child, deadline: Instant) -> Self {
        const MAX_STREAM_BYTES: usize = 1024 * 1024;
        let mut stdout = PipeCapture::with_limit(child.stdout.take(), MAX_STREAM_BYTES);
        let mut stderr = PipeCapture::with_limit(child.stderr.take(), MAX_STREAM_BYTES);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader_stop = std::sync::Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                loop {
                    if reader_stop.load(std::sync::atomic::Ordering::Relaxed) {
                        // One final nonblocking pass retains an already-buffered
                        // startup code when fixture unwind beats this reader's next
                        // turn. Cancellation remains failure even if EOF is seen.
                        stderr.read_once();
                        stdout.read_once();
                        return Err("live output capture cancelled before EOF");
                    }
                    if Instant::now() >= deadline {
                        return Err("live child output deadline exceeded");
                    }
                    stdout.read_once();
                    stderr.read_once();
                    if stdout.pipe.is_none() && stderr.pipe.is_none() {
                        return Ok(());
                    }
                    std::thread::yield_now();
                }
            }));
            match outcome {
                Ok(Ok(())) => Ok((stdout.bytes, stderr.bytes)),
                Ok(Err(reason)) => {
                    report_incomplete_live_output(reason, stdout.bytes.len(), &stderr.bytes);
                    Err(reason)
                }
                Err(panic) => {
                    report_incomplete_live_output(
                        "reader-failed",
                        stdout.bytes.len(),
                        &stderr.bytes,
                    );
                    std::panic::resume_unwind(panic)
                }
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }

    pub(crate) fn finish(mut self) -> (Vec<u8>, Vec<u8>) {
        self.worker
            .take()
            .expect("live output reader owner")
            .join()
            .expect("live child output reader failed (read or byte limit)")
            .expect("live child output did not complete before its deadline")
    }
}

impl Drop for LiveChildOutput {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(Ok((stdout, stderr))) => {
                    // EOF without the owning finish/Child status is still an
                    // unconsumed fixture failure, never product-success proof.
                    report_incomplete_live_output("unconsumed-eof", stdout.len(), &stderr);
                }
                Ok(Err(_)) => {} // Reader retained its bounded failure summary.
                Err(_) => eprintln!("KELD_HARNESS_CAPTURE_INCOMPLETE reason=reader-panicked"),
            }
        }
    }
}

fn report_incomplete_live_output(reason: &'static str, stdout_bytes: usize, stderr: &[u8]) {
    // Source bytes are untrusted diagnostics, not authenticated cause or native
    // evidence. At most24 known markers/codes and two counts reach the log.
    eprintln!(
        "KELD_HARNESS_CAPTURE_INCOMPLETE reason={reason} stdout_bytes={stdout_bytes} stderr_bytes={} stderr_markers={:?}",
        stderr.len(),
        redacted_stderr_markers(stderr),
    );
}

pub(crate) fn parse_pid(field: Option<&str>, line: &str) -> u32 {
    field
        .unwrap_or_else(|| panic!("missing pid: {line}"))
        .parse()
        .unwrap_or_else(|error| panic!("invalid pid ({error}): {line}"))
}

// Real subprocess fixtures exercise the same ShippingLaunchCleanup owner used
// by FirstProofLaunch. No app Ready/log marker is an output-capture oracle.
fn live_output_fixture(
    marker: &std::path::Path,
    stdout_bytes: usize,
    stderr_bytes: usize,
) -> (
    crate::support::dev_cycle::ShippingLaunchCleanup,
    std::process::ChildStdin,
) {
    const WRITER: &str = r"
import os, pathlib, sys
for fd, count, byte in [(1, int(sys.argv[2]), b'o'), (2, int(sys.argv[3]), b'e')]:
    remaining = byte * count
    while remaining:
        remaining = remaining[os.write(fd, remaining):]
marker = pathlib.Path(sys.argv[1])
pending = marker.with_name(marker.name + '.pending')
pending.write_bytes(b'after-both-writes')
pending.replace(marker)
sys.stdin.buffer.read(1)
sys.exit(23)
";
    let child = std::process::Command::new("/usr/bin/python3")
        .args(["-u", "-c", WRITER])
        .arg(marker)
        .arg(stdout_bytes.to_string())
        .arg(stderr_bytes.to_string())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("actual live output writer");
    let mut owner = crate::support::dev_cycle::ShippingLaunchCleanup::new(child);
    let lease = owner
        .cli
        .as_mut()
        .expect("sole Child owner")
        .stdin
        .take()
        .expect("fixture release lease");
    (owner, lease)
}

#[test]
fn live_output_drains_both_pipe_buffers_before_wait_and_preserves_exact_cap_and_status() {
    let root = tempfile::tempdir().expect("owned live output fixture");
    let marker = root.path().join("both-writes-finished");
    // Literal approved value: a mutation of the implementation's ceiling must
    // not silently change fixture volume or this independent oracle.
    let (mut owner, lease) = live_output_fixture(&marker, 1_048_576, 1_048_576);
    let deadline = Instant::now() + crate::support::EVENT_DEADLINE;
    owner.capture_output_before(deadline);
    // More than a kernel pipe buffer must drain while the caller still owns
    // the lease and has not invoked any Child/output wait.
    while !marker.exists() {
        assert!(
            Instant::now() < deadline,
            "writer blocked before independent file handoff"
        );
        assert!(
            owner
                .cli
                .as_mut()
                .expect("retained Child")
                .try_wait()
                .expect("writer state")
                .is_none(),
            "writer exited before the handoff"
        );
        thread::yield_now();
    }
    assert_eq!(
        std::fs::read(&marker).expect("actual writer file"),
        b"after-both-writes"
    );
    assert!(
        owner
            .cli
            .as_mut()
            .expect("retained Child")
            .try_wait()
            .expect("writer held by lease")
            .is_none()
    );
    drop(lease);
    let output = owner.wait(deadline.saturating_duration_since(Instant::now()));
    assert_eq!(
        output.status.code(),
        Some(23),
        "preserve the actual Child status"
    );
    assert_eq!(output.stdout.len(), 1_048_576);
    assert!(
        output.stdout.iter().all(|byte| *byte == b'o'),
        "stdout bytes changed"
    );
    assert_eq!(output.stderr.len(), 1_048_576);
    assert!(
        output.stderr.iter().all(|byte| *byte == b'e'),
        "stderr bytes changed"
    );
}

#[test]
fn live_output_refuses_literal_limit_plus_one_on_each_stream() {
    for (stdout_bytes, stderr_bytes) in [(1_048_577, 0), (0, 1_048_577)] {
        let root = tempfile::tempdir().expect("owned live overflow fixture");
        let (mut owner, lease) = live_output_fixture(
            &root.path().join("writer-finished"),
            stdout_bytes,
            stderr_bytes,
        );
        let deadline = Instant::now() + crate::support::EVENT_DEADLINE;
        owner.capture_output_before(deadline);
        drop(lease);
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            owner.wait(deadline.saturating_duration_since(Instant::now()))
        }));
        let Err(panic) = failure else {
            panic!("1 MiB + 1 must never be successful or silently truncated");
        };
        let text = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("named capture failure");
        assert!(
            text.contains("live child output reader failed"),
            "unexpected failure: {text}"
        );
        drop(owner); // sole cleanup owner reaps on every failure path
    }
}

#[test]
fn live_output_drop_stops_and_joins_reader_while_original_child_remains_live() {
    let child = std::process::Command::new("/bin/cat")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("actual open-pipe child");
    let mut owner = crate::support::dev_cycle::ShippingLaunchCleanup::new(child);
    let started = Instant::now();
    let capture = LiveChildOutput::start(
        owner.cli.as_mut().expect("sole Child owner"),
        started + crate::support::EVENT_DEADLINE,
    );
    let reader_lifetime = std::sync::Arc::clone(&capture.stop);
    drop(capture);
    assert!(
        started.elapsed() < crate::support::PROCESS_DEADLINE,
        "cancellation waited for the original child/reader deadline"
    );
    assert_eq!(
        std::sync::Arc::strong_count(&reader_lifetime),
        1,
        "reader still owns a live stop token after synchronous Drop"
    );
    assert!(
        owner
            .cli
            .as_mut()
            .expect("Child was not transferred to the reader")
            .try_wait()
            .expect("actual Child state")
            .is_none()
    );
    drop(owner); // the reader owns no process cleanup authority
}

#[test]
fn live_output_uses_original_expired_deadline_without_waiting_for_child_eof() {
    let child = std::process::Command::new("/bin/cat")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("actual non-EOF child");
    let mut owner = crate::support::dev_cycle::ShippingLaunchCleanup::new(child);
    let started = Instant::now();
    let capture = LiveChildOutput::start(owner.cli.as_mut().expect("sole Child owner"), started);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| capture.finish()));
    let panic = failure.expect_err("expired original deadline must not become a fresh timeout");
    assert!(
        started.elapsed() < crate::support::PROCESS_DEADLINE,
        "capture replaced the already-expired original deadline"
    );
    let text = panic
        .downcast_ref::<String>()
        .expect("capture deadline failure");
    assert!(
        text.contains("live child output deadline exceeded"),
        "{text}"
    );
    assert!(
        owner
            .cli
            .as_mut()
            .expect("retained Child")
            .try_wait()
            .expect("actual open-pipe Child state")
            .is_none()
    );
    drop(owner);
}

#[test]
fn live_capture_fixture_unwind_reports_bounded_typed_stderr_for_cancel_and_unconsumed_eof() {
    for eof in [false, true] {
        let root = tempfile::tempdir().expect("owned capture failure diagnostic fixture");
        let child =
            std::process::Command::new(std::env::current_exe().expect("same test executable"))
                .args([
                    "--exact",
                    "support::control::live_capture_failure_fixture_process",
                    "--nocapture",
                ])
                .env(
                    "KELD_LIVE_CAPTURE_DIAGNOSTIC_FIXTURE",
                    root.path().join("after-writes"),
                )
                .env(
                    "KELD_LIVE_CAPTURE_DIAGNOSTIC_EOF",
                    if eof { "1" } else { "0" },
                )
                .env("RUST_BACKTRACE", "0")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("isolated fixture-unwind diagnostic child");
        let output = wait_child_output(child, crate::support::EVENT_DEADLINE);
        assert!(
            output.status.success(),
            "isolated helper failed: {:?}",
            output.status
        );
        assert_eq!(
            std::fs::read(root.path().join("after-writes")).expect("actual writer handoff"),
            b"after-writes"
        );
        assert!(
            output.stdout.len() < 4096 && output.stderr.len() < 4096,
            "unwind printed a stream body instead of bounded diagnostics"
        );
        let stderr = String::from_utf8(output.stderr).expect("isolated diagnostic UTF-8");
        assert!(stderr.contains("KELD_HARNESS_CAPTURE_INCOMPLETE reason="));
        assert!(stderr.contains(if eof {
            "reason=unconsumed-eof"
        } else {
            "reason=live output capture cancelled before EOF"
        }));
        assert!(stderr.contains("stdout_bytes=") && stderr.contains("stderr_bytes="));
        assert!(
            stderr.contains("KELD-CORE-037") && stderr.contains("KELD-GUARD005"),
            "actual captured typed errors were discarded"
        );
        for secret in [
            "SENSITIVE_STDOUT_PAYLOAD",
            "SENSITIVE_STDERR_PAYLOAD",
            "/private/hidden",
            "bootstrap=hidden",
        ] {
            assert!(
                !stderr.contains(secret),
                "raw fixture output escaped its redacted summary"
            );
        }
    }
}

/// Exact-selector child only. The outer real-process regression verifies its
/// independent file handshake and stderr, so zero selected tests cannot pass.
#[test]
fn live_capture_failure_fixture_process() {
    const WRITER: &str = r"
import pathlib, sys
sys.stdout.buffer.write(b'SENSITIVE_STDOUT_PAYLOAD' + b'x' * (1048576 - 24))
sys.stdout.buffer.flush()
sys.stderr.write('KELD-CORE-037: SENSITIVE_STDERR_PAYLOAD /private/hidden bootstrap=hidden\nKELD-GUARD005: SENSITIVE_STDERR_PAYLOAD\n')
sys.stderr.flush()
marker = pathlib.Path(sys.argv[1])
pending = marker.with_name(marker.name + '.pending')
pending.write_bytes(b'after-writes')
pending.replace(marker)
if sys.argv[2] == '0':
    sys.stdin.buffer.read(1)
";
    let Some(marker) = std::env::var_os("KELD_LIVE_CAPTURE_DIAGNOSTIC_FIXTURE") else {
        return;
    };
    let eof = std::env::var("KELD_LIVE_CAPTURE_DIAGNOSTIC_EOF").expect("fixture mode") == "1";
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let child = std::process::Command::new("/usr/bin/python3")
            .args(["-u", "-c", WRITER])
            .arg(&marker)
            .arg(if eof { "1" } else { "0" })
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("actual diagnostic pipe writer");
        let mut owner = crate::support::dev_cycle::ShippingLaunchCleanup::new(child);
        let deadline = Instant::now() + crate::support::EVENT_DEADLINE;
        let capture =
            LiveChildOutput::start(owner.cli.as_mut().expect("sole Child owner"), deadline);
        while !std::path::Path::new(&marker).exists() {
            assert!(
                Instant::now() < deadline,
                "actual writer never completed its pipe handoff"
            );
            thread::yield_now();
        }
        if eof {
            while !capture
                .worker
                .as_ref()
                .expect("reader join owner")
                .is_finished()
            {
                assert!(
                    Instant::now() < deadline,
                    "actual pipe reader never reached EOF"
                );
                thread::yield_now();
            }
        } else {
            assert!(
                owner
                    .cli
                    .as_mut()
                    .expect("sole live Child owner")
                    .try_wait()
                    .expect("actual writer state")
                    .is_none(),
                "cancel fixture must retain open pipes"
            );
        }
        panic!("injected fixture assertion after actual captured stderr");
    }));
    assert!(failed.is_err(), "fixture did not exercise unwind");
}
