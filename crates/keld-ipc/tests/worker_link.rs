//! GH-528 T1 harness: the Bun role's link against the real host writer.
//!
//! Spec: `docs/specs/gh527-worker-owned-blocking-call-transport.md` §3, §7. This
//! test is the host half. It mints a real `KELD_APP_LINK` with
//! [`BootstrapListener`], spawns `packages/@keld/kipc/test/worker-link-role.ts`
//! under Bun as the role, authenticates it, and drives the link with the real
//! [`keld_ipc::link::write_frame`] writer and its `APP_LINK_IO_DEADLINE` send
//! timeout. The role prints `KELD_WL key=value` observations; both halves are
//! asserted here.
//!
//! Anti-flake (spec §7): the only sleep is the arm-B 100 EVENT/s pacing, which is
//! load generation. Every wait is on an observable (a frame, a role line, a
//! process exit) with a kill-switch bound, and no assertion is a duration.
//!
//! macOS only: T1 is the first proof (spec §6). T5 qualifies Linux and Windows.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are the assertion oracles

use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use keld_ipc::link::{read_frame, write_frame};
use keld_ipc::{
    APP_LINK_IO_DEADLINE, AppLinkDeadlines, BootstrapAdmission, BootstrapListener,
    BootstrapRejection, BootstrapRejectionObserver, ChannelId, CorrelationId, ECHO_CHANNEL,
    FrameHeader, FrameKind, IpcError, LIFECYCLE_CHANNEL,
};

/// #418 arm-B load: a 10,000-EVENT burst, then 1,000 EVENTs at 100 per second.
const ARM_B_BURST: u32 = 10_000;
const ARM_B_PACED: u32 = 1_000;
const ARM_B_PACE: Duration = Duration::from_millis(10);
/// Each arm-B EVENT is one 64-byte frame: a 16-byte header and 48 payload bytes.
const ARM_B_PAYLOAD_LEN: usize = 48;

struct IgnoreRejections;

impl BootstrapRejectionObserver for IgnoreRejections {
    fn rejected(&self, _rejection: BootstrapRejection) {}
}

fn role_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/@keld/kipc/test/worker-link-role.ts")
}

/// One spawned Bun role process and its line-oriented stdout.
struct Role {
    child: Option<Child>,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
    stdout: Option<thread::JoinHandle<()>>,
    stderr: Option<thread::JoinHandle<String>>,
}

struct RoleOutput {
    status: ExitStatus,
    lines: Vec<String>,
    stderr: String,
}

impl RoleOutput {
    fn report(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for line in &self.lines {
            if let Some(rest) = line.strip_prefix("KELD_WL ")
                && let Some((key, value)) = rest.split_once('=')
            {
                out.insert(key.to_owned(), value.to_owned());
            }
        }
        out
    }

    fn diagnostics(&self) -> String {
        format!(
            "status={:?}\nstdout:\n{}\nstderr:\n{}",
            self.status.code(),
            self.lines.join("\n"),
            self.stderr
        )
    }
}

impl Role {
    fn spawn(scenario: &str, app_link: &str) -> Self {
        let mut child = Command::new("bun")
            .arg(role_script())
            .arg(scenario)
            .env("KELD_APP_LINK", app_link)
            .env("KELD_KIPC_TEST_HOOKS", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bun must be on PATH (same contract as keld-cli bun_echo)");
        let stdout = child.stdout.take().expect("piped role stdout");
        let mut stderr = child.stderr.take().expect("piped role stderr");
        let (tx, lines) = mpsc::channel();
        let stdout = thread::spawn(move || {
            let mut text = String::new();
            let mut stdout = stdout;
            let mut buf = [0_u8; 4096];
            loop {
                match stdout.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        text.push_str(&String::from_utf8_lossy(&buf[..n]));
                        while let Some(end) = text.find('\n') {
                            let line: String = text.drain(..=end).collect();
                            if tx.send(line.trim_end().to_owned()).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
            if !text.is_empty() {
                let _ = tx.send(text);
            }
        });
        let stderr = thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        });
        Self {
            child: Some(child),
            lines,
            seen: Vec::new(),
            stdout: Some(stdout),
            stderr: Some(stderr),
        }
    }

    /// Kills the role and returns what it printed. Used once the host has
    /// already observed the outcome under test.
    fn kill(mut self) -> RoleOutput {
        let mut child = self.child.take().expect("role child present");
        let _ = child.kill();
        let status = child.wait().expect("reap killed role");
        self.collect(status)
    }

    fn collect(&mut self, status: ExitStatus) -> RoleOutput {
        if let Some(stdout) = self.stdout.take() {
            stdout.join().expect("join role stdout reader");
        }
        while let Ok(line) = self.lines.try_recv() {
            self.seen.push(line);
        }
        let stderr = self
            .stderr
            .take()
            .map(|handle| handle.join().expect("join role stderr reader"))
            .unwrap_or_default();
        RoleOutput {
            status,
            lines: std::mem::take(&mut self.seen),
            stderr,
        }
    }
}

impl Drop for Role {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Binds a fresh bootstrap listener, spawns the role, and authenticates it.
fn start(scenario: &str) -> (UnixStream, Role) {
    let listener = BootstrapListener::bind().expect("bind bootstrap listener");
    let role = Role::spawn(scenario, &listener.app_link());
    let deadline = Instant::now() + Duration::from_secs(20);
    let stream = match listener
        .accept_authenticated_until(deadline, &IgnoreRejections)
        .expect("accept role")
    {
        BootstrapAdmission::Authenticated(stream) => stream,
        BootstrapAdmission::Cancelled | BootstrapAdmission::DeadlineElapsed => {
            let output = role.kill();
            panic!("role did not authenticate: {}", output.diagnostics());
        }
    };
    stream
        .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
        .expect("host app-link deadlines");
    (stream, role)
}

fn arm_b_payload(seq: u32) -> [u8; ARM_B_PAYLOAD_LEN] {
    let mut payload = [0xA5_u8; ARM_B_PAYLOAD_LEN];
    payload[..4].copy_from_slice(&seq.to_le_bytes());
    payload
}

/// What the host writer observed while writing a frame sequence.
struct WriteOutcome {
    frames_written: u32,
    error: Option<IpcError>,
}

/// Writes the arm-B EVENT load on `channel`, then `reply` for `corr`, through
/// the real writer. Stops at the first writer error.
fn write_arm_b_load(
    stream: &mut UnixStream,
    channel: ChannelId,
    corr: CorrelationId,
    reply: &[u8],
) -> WriteOutcome {
    let mut frames_written = 0;
    for seq in 0..ARM_B_BURST + ARM_B_PACED {
        if seq >= ARM_B_BURST {
            // Load generation only (spec §7): the paced tail keeps the role
            // parked for about ten seconds. No assertion depends on it.
            thread::sleep(ARM_B_PACE);
        }
        if let Err(error) = write_frame(
            stream,
            FrameKind::Event,
            0,
            channel,
            CorrelationId(0),
            &arm_b_payload(seq),
        ) {
            return WriteOutcome {
                frames_written,
                error: Some(error),
            };
        }
        frames_written += 1;
    }
    if let Err(error) = write_frame(stream, FrameKind::Reply, 0, ECHO_CHANNEL, corr, reply) {
        return WriteOutcome {
            frames_written,
            error: Some(error),
        };
    }
    frames_written += 1;
    WriteOutcome {
        frames_written,
        error: None,
    }
}

fn read_call(stream: &mut UnixStream) -> FrameHeader {
    let (header, _payload) = read_frame(stream).expect("read the role's blocking CALL");
    assert_eq!(header.kind, FrameKind::Call, "first role frame is its CALL");
    header
}

/// Criterion 1 (failing first): a role parked in a blocking call under the
/// arm-B load must keep draining its link. Today's main-thread client cannot:
/// the expected status asserted here is the host writer's `KELD-IPC-006`
/// once the platform send space is full.
#[test]
fn criterion1_link_drains_during_park() {
    let (mut stream, role) = start("criterion1");
    let call = read_call(&mut stream);
    assert_eq!(call.channel, ECHO_CHANNEL);
    let outcome = write_arm_b_load(&mut stream, LIFECYCLE_CHANNEL, call.corr, b"arm-b-reply");
    let output = role.kill();
    let error = outcome.error.unwrap_or_else(|| {
        panic!(
            "failing-first: the main-thread client unexpectedly drained the link; {}",
            output.diagnostics()
        )
    });
    assert!(
        error.to_string().starts_with("KELD-IPC-006"),
        "failing-first status is the writer deadline, got {error}"
    );
    assert!(
        outcome.frames_written < ARM_B_BURST,
        "the writer stalled inside the burst: {} frames",
        outcome.frames_written
    );
    println!(
        "criterion1 failing-first: host wrote {} frames ({} bytes) before {error}",
        outcome.frames_written,
        u64::from(outcome.frames_written) * 64
    );
    assert_eq!(
        output.report().get("parked").map(String::as_str),
        Some("true")
    );
}
