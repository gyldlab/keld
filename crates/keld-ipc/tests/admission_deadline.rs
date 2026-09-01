//! KEL-133 spec §3 criterion 6: an absolute generation/admission deadline
//! that byte drip cannot renew, proven against a real listener with a real
//! byte-drip **child process** (spec §7 row 6 and anti-flake rules: drip
//! actors run in child processes and report status/cleanup).
//!
//! The drip child writes a *valid* HELLO frame — only its timing is hostile —
//! so the assertion isolates the clock: eight 25 ms-spaced writes must not
//! stretch a 100 ms admission window toward ~800 ms. The child's pacing
//! sleeps are the hostile stimulus under test, not synchronization.

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are the assertion oracles

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use keld_ipc::{
    BootstrapAdmissionFor, BootstrapRejection, BootstrapRejectionObserver, ChannelId,
    CorrelationId, FrameHeader, FrameKind, HEADER_LEN, SESSION_TOKEN_LEN, parse_app_link,
};

#[cfg(unix)]
use keld_ipc::BootstrapListener as Listener;
#[cfg(windows)]
use keld_ipc::WindowsNamedPipeBootstrapListener as Listener;

const ENDPOINT_ENV: &str = "KELD133_DRIP_APP_LINK";
const MODE_ENV: &str = "KELD133_DRIP_MODE";
const CHILD_ENTRY: &str = "drip_child_entry";

#[derive(Default)]
struct CountingObserver(std::sync::atomic::AtomicU32);

impl BootstrapRejectionObserver for CountingObserver {
    fn rejected(&self, _rejection: BootstrapRejection) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Child guard: terminates a leftover child on test panic so no fixture
/// process outlives the test (house pattern from `windows_host_death_job`).
struct ObservedChild(Option<Child>);

impl ObservedChild {
    fn wait_bounded(&mut self, bound: Duration) -> std::process::ExitStatus {
        let child = self.0.as_mut().expect("child present");
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().expect("try_wait") {
                return status;
            }
            assert!(
                started.elapsed() < bound,
                "drip child did not exit within {bound:?}"
            );
            std::thread::yield_now();
        }
    }
}

impl Drop for ObservedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take()
            && child.try_wait().is_ok_and(|status| status.is_none())
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn_child(app_link: &str, mode: &str) -> ObservedChild {
    let exe = std::env::current_exe().expect("test binary path");
    let child = Command::new(exe)
        .args(["--exact", CHILD_ENTRY, "--ignored", "--nocapture"])
        .env(ENDPOINT_ENV, app_link)
        .env(MODE_ENV, mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn drip child");
    ObservedChild(Some(child))
}

fn child_stdout(child: &mut ObservedChild) -> String {
    let mut out = String::new();
    child
        .0
        .as_mut()
        .expect("child present")
        .stdout
        .take()
        .expect("piped stdout")
        .read_to_string(&mut out)
        .expect("read child stdout");
    out
}

/// The named criterion-6 test: a 100 ms generation deadline, a byte-drip
/// child, an elapsed monotonic bound, terminal state, a joined child, a dead
/// locator, and next fresh-generation success — in that order.
#[test]
fn a_byte_drip_child_cannot_renew_a_100ms_generation_deadline() {
    let listener = Listener::bind().expect("bind bootstrap listener");
    let app_link = listener.app_link();
    let (endpoint, _token) = parse_app_link(&app_link).expect("parse app link");
    let endpoint = endpoint.to_owned();
    let observer = CountingObserver::default();

    let mut child = spawn_child(&app_link, "drip");
    let deadline = Duration::from_millis(100);
    let started = Instant::now();
    let admission = listener
        .accept_authenticated_until(started + deadline, &observer)
        .expect("host-side listener must not fail");
    let elapsed = started.elapsed();

    // Expiry is terminal for the generation, and drip cannot renew the clock:
    // the spec's own bound is that 100 ms must not become ~800 ms.
    assert!(
        matches!(admission, BootstrapAdmissionFor::DeadlineElapsed),
        "drip must not authenticate and must not keep admission open"
    );
    assert!(
        elapsed >= deadline,
        "admission ended before its absolute deadline: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(700),
        "byte drip renewed the generation deadline: {elapsed:?}"
    );

    // Terminal state: the expired generation never authenticates a later
    // peer, and it answers immediately. Unix reports DeadlineElapsed again;
    // the Windows listener is stricter still and refuses as already consumed.
    let again_started = Instant::now();
    if let Ok(BootstrapAdmissionFor::Authenticated(_)) =
        listener.accept_authenticated_until(started + deadline, &observer)
    {
        panic!("an expired generation must never authenticate");
    }
    assert!(
        again_started.elapsed() < Duration::from_millis(200),
        "an expired generation must answer immediately: {:?}",
        again_started.elapsed()
    );

    // Joined child with status and observed drip progress.
    let status = child.wait_bounded(Duration::from_secs(10));
    assert!(status.success(), "drip child must exit cleanly: {status:?}");
    let out = child_stdout(&mut child);
    assert!(out.contains("CONNECTED"), "child never connected: {out}");
    assert!(
        out.matches("CHUNK ").count() >= 2,
        "child never actually dripped: {out}"
    );

    // Dead locator: after the generation ends and the listener drops, the old
    // endpoint admits no new peer.
    drop(listener);
    let reconnect = raw_connect(&endpoint);
    assert!(
        reconnect.is_err(),
        "the expired generation's locator must be dead"
    );

    // Next fresh generation succeeds promptly with a well-behaved child.
    let fresh = Listener::bind().expect("bind fresh generation");
    let fresh_link = fresh.app_link();
    let mut prompt = spawn_child(&fresh_link, "prompt");
    let fresh_started = Instant::now();
    let admitted = fresh
        .accept_authenticated_until(fresh_started + Duration::from_secs(5), &observer)
        .expect("fresh listener");
    assert!(
        matches!(admitted, BootstrapAdmissionFor::Authenticated(_)),
        "a prompt peer must authenticate on the fresh generation"
    );
    let prompt_status = prompt.wait_bounded(Duration::from_secs(10));
    assert!(prompt_status.success(), "prompt child: {prompt_status:?}");
}

#[cfg(unix)]
fn raw_connect(endpoint: &str) -> std::io::Result<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(endpoint)
}

#[cfg(windows)]
fn raw_connect(endpoint: &str) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(endpoint)
}

/// Private subprocess entry point: connects to the listener under
/// `KELD133_DRIP_APP_LINK` and either drips a valid HELLO in 6-byte chunks
/// every 25 ms (`drip`) or sends it whole and completes the handshake
/// (`prompt`). Prints a line-oriented protocol for the parent's assertions.
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
        "drip" => {
            // The hostile stimulus: valid bytes, hostile pacing.
            for (index, chunk) in frame.chunks(6).enumerate() {
                match stream.write_all(chunk).and_then(|()| stream.flush()) {
                    Ok(()) => println!("CHUNK {index}"),
                    Err(err) => {
                        // The host expired the generation and closed us —
                        // exactly the expected outcome.
                        println!("DISCONNECTED {index} {}", err.kind());
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
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
