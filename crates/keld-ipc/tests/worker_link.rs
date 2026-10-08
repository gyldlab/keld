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
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use keld_ipc::frame::FLAG_RAW;
use keld_ipc::link::{read_frame, write_frame};
use keld_ipc::{
    APP_LINK_IO_DEADLINE, AppLinkDeadlines, BootstrapAdmission, BootstrapListener,
    BootstrapRejection, BootstrapRejectionObserver, CallError, ChannelId, CorrelationId,
    ECHO_CHANNEL, FrameHeader, FrameKind, IpcError, LIFECYCLE_CHANNEL, write_call_error,
};

/// #418 arm-B load: a 10,000-EVENT burst, then 1,000 EVENTs at 100 per second.
const ARM_B_BURST: u32 = 10_000;
const ARM_B_PACED: u32 = 1_000;
const ARM_B_PACE: Duration = Duration::from_millis(10);
/// Each arm-B EVENT is one 64-byte frame: a 16-byte header and 48 payload bytes.
const ARM_B_PAYLOAD_LEN: usize = 48;
/// Kill switch for one role process; never a synchronization point.
const ROLE_KILL_SWITCH: Duration = Duration::from_secs(90);

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
            .stdin(Stdio::piped())
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

    /// Waits for the role to exit by itself (kill switch: `ROLE_KILL_SWITCH`).
    fn finish(mut self) -> RoleOutput {
        let mut child = self.child.take().expect("role child present");
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll role exit") {
                break status;
            }
            if started.elapsed() > ROLE_KILL_SWITCH {
                let _ = child.kill();
                let status = child.wait().expect("reap role after kill switch");
                let output = self.collect(status);
                panic!("role exceeded the kill switch; {}", output.diagnostics());
            }
            thread::park_timeout(Duration::from_millis(10));
        };
        self.collect(status)
    }

    /// Writes one line to the role's stdin: the host's observable "go" for a
    /// role that must outlive a host-side observation.
    fn release(&mut self) {
        let child = self.child.as_mut().expect("role child present");
        let stdin = child.stdin.as_mut().expect("piped role stdin");
        stdin.write_all(b"go\n").expect("release the role");
        stdin.flush().expect("flush role stdin");
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
    let mut paced_from = None;
    for seq in 0..ARM_B_BURST + ARM_B_PACED {
        if seq >= ARM_B_BURST {
            // Load generation only (spec §7): the paced tail keeps the role
            // parked for about ten seconds. No assertion depends on it. Each
            // EVENT is due on an absolute 100/s schedule, so a sleep that
            // overshoots (hosted macOS runners coalesce timers) shortens the
            // next one instead of stretching the park.
            let start = *paced_from.get_or_insert_with(Instant::now);
            let due = start + ARM_B_PACE * (seq - ARM_B_BURST);
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                thread::sleep(wait);
            }
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

/// Reads the role's next frame and requires it to be a CALL.
fn read_call(stream: &mut UnixStream) -> (FrameHeader, Vec<u8>) {
    let (header, payload) = read_frame(stream).expect("read the role's CALL");
    assert_eq!(
        header.kind,
        FrameKind::Call,
        "expected a role CALL, got {header:?} {:?}",
        String::from_utf8_lossy(&payload)
    );
    (header, payload)
}

/// Reads the role's next CALL and requires its payload text.
fn read_call_named(stream: &mut UnixStream, name: &str) -> FrameHeader {
    let (header, payload) = read_call(stream);
    assert_eq!(String::from_utf8_lossy(&payload), name, "{header:?}");
    header
}

/// Reads the role's next EVENT on the lifecycle channel and requires its text.
fn read_role_event(stream: &mut UnixStream, name: &str) {
    let (header, payload) = read_frame(stream).expect("read the role's EVENT");
    assert_eq!(header.kind, FrameKind::Event, "{header:?}");
    assert_eq!(header.channel, LIFECYCLE_CHANNEL, "{header:?}");
    assert_eq!(String::from_utf8_lossy(&payload), name, "{header:?}");
}

fn host_write(
    stream: &mut UnixStream,
    kind: FrameKind,
    channel: ChannelId,
    corr: CorrelationId,
    payload: &[u8],
) {
    write_frame(stream, kind, 0, channel, corr, payload).expect("host write");
}

fn host_reply(stream: &mut UnixStream, call: FrameHeader, payload: &[u8]) {
    host_write(stream, FrameKind::Reply, call.channel, call.corr, payload);
}

/// An EVENT payload: `seq` as `u32` LE, then the bytes `(seq + i) & 0xff`.
fn seq_payload(seq: u32, len: usize) -> Vec<u8> {
    let mut payload: Vec<u8> = (0..len)
        .map(|i| u8::try_from((seq as usize + i) & 0xff).expect("masked to a byte"))
        .collect();
    payload[..4].copy_from_slice(&seq.to_le_bytes());
    payload
}

fn host_event(stream: &mut UnixStream, seq: u32, len: usize) {
    host_write(
        stream,
        FrameKind::Event,
        LIFECYCLE_CHANNEL,
        CorrelationId(0),
        &seq_payload(seq, len),
    );
}

/// Lets the host wait for the role across quiet phases; writes keep
/// `APP_LINK_IO_DEADLINE`, so criterion 1's writer contract is unchanged.
fn long_reads(stream: &UnixStream) {
    stream
        .set_app_link_read_deadline(Some(Duration::from_mins(1)))
        .expect("host read deadline");
}

/// Reads until the link is lost; returns the frames read first. Link loss is
/// EOF or a reset, which `read_frame` reports as `KELD-IPC-001`.
fn read_until_link_loss(stream: &mut UnixStream) -> Vec<(FrameHeader, Vec<u8>)> {
    let mut frames = Vec::new();
    loop {
        match read_frame(stream) {
            Ok(frame) => frames.push(frame),
            Err(error) => {
                assert!(
                    error.to_string().starts_with("KELD-IPC-001"),
                    "the host observes link loss, not {error}"
                );
                return frames;
            }
        }
    }
}

fn expect_report(output: &RoleOutput, expected: &[(&str, &str)]) {
    let report = output.report();
    for (key, value) in expected {
        assert_eq!(
            report.get(*key).map(String::as_str),
            Some(*value),
            "{key}: {}",
            output.diagnostics()
        );
    }
}

/// Criteria 1, 2 and 3 under the #418 arm-B load against the real writer.
///
/// 1: the host writes every frame and no write returns `KELD-IPC-006` (this
/// test landed failing first, asserting that `KELD-IPC-006`, against today's
/// main-thread client). 2: the call returns the host bytes synchronously, not
/// a thenable, before any listener ran. 3: every EVENT reaches its listener
/// after wake, in order, with no gap or duplicate.
#[test]
fn criterion1_link_drains_during_park() {
    let (mut stream, role) = start("arm-b");
    let call = read_call_named(&mut stream, "arm-b");
    assert_eq!(call.channel, ECHO_CHANNEL);
    let outcome = write_arm_b_load(&mut stream, LIFECYCLE_CHANNEL, call.corr, b"arm-b-reply");
    let output = role.finish();
    assert!(
        outcome.error.is_none(),
        "criterion 1: the writer failed after {} frames: {:?}; {}",
        outcome.frames_written,
        outcome.error.map(|e| e.to_string()),
        output.diagnostics()
    );
    assert_eq!(outcome.frames_written, ARM_B_BURST + ARM_B_PACED + 1);
    assert_arm_b_role(&output);
}

/// The role half of one arm-B run (criteria 2 and 3).
fn assert_arm_b_role(output: &RoleOutput) {
    let report = output.report();
    let diagnostics = output.diagnostics();
    let expected_events = (ARM_B_BURST + ARM_B_PACED).to_string();
    for (key, value) in [
        ("reply", "arm-b-reply"),
        ("thenable", "false"),
        ("listeners-before-return", "0"),
        ("arm-b-events", expected_events.as_str()),
        ("arm-b-in-order", "true"),
        ("arm-b-gaps", "0"),
        ("arm-b-dups", "0"),
        ("done", "true"),
    ] {
        assert_eq!(
            report.get(key).map(String::as_str),
            Some(value),
            "{key}: {diagnostics}"
        );
    }
    assert!(!report.contains_key("call-code"), "{diagnostics}");
    assert!(output.status.success(), "{diagnostics}");
}

/// Criterion 4 (#419 E3): a fact written during the park is visible to a
/// synchronous getter when `callBlocking` returns; its listener runs only
/// after the caller's continuation; a timer armed before the park did not run.
#[test]
fn criterion4_facts_apply_before_return_and_listeners_run_after_continuation() {
    let (mut stream, role) = start("wake-rule");
    let call = read_call_named(&mut stream, "wake");
    for seq in 0..5 {
        host_event(&mut stream, seq, 8);
    }
    host_reply(&mut stream, call, b"wake-reply");
    let output = role.finish();
    expect_report(
        &output,
        &[
            (
                "log",
                "applier:0,applier:1,applier:2,applier:3,applier:4,return,continuation,\
                 listener:0,listener:1,listener:2,listener:3,listener:4",
            ),
            ("getter-after-return", "4"),
            ("timer-callbacks-at-return", "0"),
            ("done", "true"),
        ],
    );
}

/// Criterion 5: a host close without an `ERR` throws `KELD-IPC-022`, returns no
/// value, and the records that preceded the close are still delivered in order.
#[test]
fn criterion5_close_without_err_throws_022_and_keeps_prior_records() {
    let (mut stream, role) = start("close-wake");
    read_call_named(&mut stream, "close");
    for seq in 0..3 {
        host_event(&mut stream, seq, 8);
    }
    stream
        .shutdown(Shutdown::Both)
        .expect("host closes the link");
    drop(stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-022"),
            ("call-returned", "false"),
            ("listeners-at-throw", "0"),
            ("close-events", "3"),
            ("close-in-order", "true"),
            ("later-code", "KELD-IPC-022"),
            ("done", "true"),
        ],
    );
}

/// Criterion 8 harness: one Worker fault while main is parked with a 30 s deadline.
fn worker_fault(scenario: &str, liveness_branch: &str, exit_handler: &str) {
    let (mut stream, role) = start(scenario);
    long_reads(&stream);
    read_call(&mut stream);
    let frames = read_until_link_loss(&mut stream);
    assert!(frames.is_empty(), "no frame follows the fault: {frames:?}");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-025"),
            ("call-returned", "false"),
            ("liveness-branch", liveness_branch),
            ("exit-handler", exit_handler),
            ("later-code", "KELD-IPC-025"),
            ("done", "true"),
        ],
    );
}

/// Criterion 8(a): an uncaught Worker error runs the exit handler, whose
/// `STATE` compare-and-exchange wakes main; main's liveness branch never ran.
#[test]
fn criterion8a_worker_uncaught_error_wakes_through_its_exit_handler() {
    worker_fault("worker-throw", "0", "1");
}

/// Criterion 8(b): `process.exit` in the Worker runs no exit handler; the
/// parked call wakes through main's liveness branch with `KELD-IPC-025`.
#[test]
fn criterion8b_abrupt_worker_exit_wakes_through_the_liveness_branch() {
    worker_fault("worker-exit", "1", "0");
}

/// Criterion 8(c): a wedged Worker loop stops the heartbeat; main throws
/// `KELD-IPC-025` and terminates it, which closes the link at the host.
#[test]
fn criterion8c_wedged_worker_wakes_through_the_liveness_branch() {
    worker_fault("worker-wedge", "1", "0");
}

/// Criterion 9: five concurrent arm-B runs report no `KELD-IPC-025`.
#[test]
fn criterion9_no_false_liveness_failure_over_five_arm_b_runs() {
    let runs: Vec<_> = (0..5)
        .map(|_| {
            thread::spawn(|| {
                let (mut stream, role) = start("arm-b");
                let call = read_call_named(&mut stream, "arm-b");
                let outcome =
                    write_arm_b_load(&mut stream, LIFECYCLE_CHANNEL, call.corr, b"arm-b-reply");
                (outcome, role.finish())
            })
        })
        .collect();
    for run in runs {
        let (outcome, output) = run.join().expect("arm-B run thread");
        assert!(outcome.error.is_none(), "{}", output.diagnostics());
        assert_arm_b_role(&output);
    }
}

/// Criterion 10 harness: the ring fills, the Worker fails closed with 026.
fn overflow(scenario: &str, frames: u32, retained: &str) {
    let (mut stream, role) = start(scenario);
    long_reads(&stream);
    read_call_named(&mut stream, "overflow");
    for seq in 0..frames {
        // Writes after the Worker closed may fail; the host stops there.
        if write_frame(
            &mut stream,
            FrameKind::Event,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            &arm_b_payload(seq),
        )
        .is_err()
        {
            break;
        }
    }
    read_until_link_loss(&mut stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-026"),
            ("call-returned", "false"),
            ("overflow-events", retained),
            ("overflow-in-order", "true"),
            ("overflow-gaps", "0"),
            ("overflow-dups", "0"),
            ("done", "true"),
        ],
    );
}

/// Criterion 10: one frame past the byte bound (1,024 records of 64 bytes in
/// a 64 KiB ring) closes the link with 026 and keeps every retained record.
#[test]
fn criterion10_byte_bound_overflow_fails_closed_with_026() {
    overflow("overflow-bytes", 1_100, "1024");
}

/// Criterion 10: one frame past the record bound (`ringRecords = 8`).
#[test]
fn criterion10_record_bound_overflow_fails_closed_with_026() {
    overflow("overflow-records", 20, "8");
}

/// Criterion 11: a second `open` in the realm is `KELD-IPC-005` before any
/// connect; a second connect to the consumed locator is refused by the OS;
/// the first link keeps working.
#[test]
fn criterion11_second_open_and_second_connect_are_refused() {
    let (mut stream, role) = start("second-link");
    let call = read_call_named(&mut stream, "still-up");
    host_reply(&mut stream, call, b"first-link-up");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("second-open", "KELD-IPC-005"),
            ("second-connect", "refused:ENOENT"),
            ("first-link", "first-link-up"),
            ("done", "true"),
        ],
    );
}

/// Criterion 12: a REPLY for another live id never satisfies the blocking
/// call, and a late REPLY for an abandoned id is discarded, never appended.
#[test]
fn criterion12_reply_is_selected_by_correlation_and_late_reply_is_discarded() {
    let (mut stream, role) = start("correlation");
    long_reads(&stream);
    let other = read_call_named(&mut stream, "other");
    let mine = read_call_named(&mut stream, "mine");
    host_reply(&mut stream, other, b"for-other");
    host_reply(&mut stream, mine, b"for-mine");
    let expired = read_call_named(&mut stream, "expires");
    // Sent only after "expires" threw KELD-IPC-006 at its deadline.
    let fresh = read_call_named(&mut stream, "fresh");
    host_reply(&mut stream, expired, b"late");
    host_reply(&mut stream, fresh, b"fresh-reply");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("blocking", "for-mine"),
            ("other", "for-other"),
            ("expired-code", "KELD-IPC-006"),
            ("fresh", "fresh-reply"),
            ("late-appended", "0"),
            ("done", "true"),
        ],
    );
}

/// Criterion 12: a state applier's `callBlocking` during the step-1 drain is
/// `KELD-IPC-005` before any write, with the reply slot unchanged; the
/// applier's throw then closes the link and the outer call throws 022.
#[test]
fn criterion12_applier_blocking_call_is_refused_before_any_write() {
    let (mut stream, role) = start("inner-blocking-call");
    long_reads(&stream);
    let outer = read_call_named(&mut stream, "outer");
    host_event(&mut stream, 0, 8);
    host_reply(&mut stream, outer, b"outer-reply");
    let frames = read_until_link_loss(&mut stream);
    assert!(
        frames
            .iter()
            .all(|(header, _)| header.kind != FrameKind::Call),
        "the inner call wrote no CALL: {frames:?}"
    );
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("blocking-word-at-drain", "0"),
            ("inner-code", "KELD-IPC-005"),
            ("inner-returned", "false"),
            ("slot-unchanged", "true"),
            ("outer-code", "KELD-IPC-022"),
            ("outer-cause-005", "true"),
            ("outer-returned", "false"),
            ("done", "true"),
        ],
    );
}

/// Criterion 13: a missing or invalid deadline is `KELD-IPC-005` before any
/// frame; an unanswered call throws `KELD-IPC-006` and the link stays up.
#[test]
fn criterion13_every_call_needs_a_finite_deadline() {
    let (mut stream, role) = start("deadlines");
    long_reads(&stream);
    // The first CALL the host reads is the valid one: the five refused calls wrote nothing.
    read_call_named(&mut stream, "silent");
    let after = read_call_named(&mut stream, "after");
    host_reply(&mut stream, after, b"after-reply");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("refused", "5"),
            ("silent-code", "KELD-IPC-006"),
            ("silent-returned", "false"),
            ("after", "after-reply"),
            ("done", "true"),
        ],
    );
}

/// Criteria 14 and 21 harness: one inbound frame the selected policy rejects.
/// The link closes before any append; the parked call throws 022 whose detail
/// names the KELD-IPC-005 cause; no listener, waiter or applier sees it.
fn inbound_violation(write_bad: impl FnOnce(&mut UnixStream, FrameHeader)) {
    let (mut stream, role) = start("inbound-violation");
    long_reads(&stream);
    let call = read_call_named(&mut stream, "violation");
    write_bad(&mut stream, call);
    read_until_link_loss(&mut stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-022"),
            ("call-cause-005", "true"),
            ("call-returned", "false"),
            ("w-recs", "0"),
            ("reply-ready", "0"),
            ("listeners", "0"),
            ("appliers", "0"),
            ("done", "true"),
        ],
    );
}

/// Criterion 14: a malformed inbound frame (`FLAG_RAW` on a structured EVENT).
#[test]
fn criterion14_malformed_inbound_frame_wakes_with_022() {
    inbound_violation(|stream, _call| {
        write_frame(
            stream,
            FrameKind::Event,
            FLAG_RAW,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            &[0],
        )
        .expect("host write");
    });
}

/// Criterion 21: a REPLY whose id is not in the pending-CALL map.
#[test]
fn criterion21_unsolicited_reply_closes_before_append() {
    inbound_violation(|stream, _call| {
        host_write(
            stream,
            FrameKind::Reply,
            ECHO_CHANNEL,
            CorrelationId(999),
            b"x",
        );
    });
}

/// Criterion 21: an EVENT on a channel the receive table does not declare.
#[test]
fn criterion21_undeclared_event_channel_closes() {
    inbound_violation(|stream, _call| {
        host_write(
            stream,
            FrameKind::Event,
            ChannelId(2),
            CorrelationId(0),
            b"x",
        );
    });
}

/// Criterion 21: an EVENT with a nonzero correlation id on a declared channel.
#[test]
fn criterion21_correlated_event_closes() {
    inbound_violation(|stream, _call| {
        host_write(
            stream,
            FrameKind::Event,
            LIFECYCLE_CHANNEL,
            CorrelationId(4),
            b"x",
        );
    });
}

/// Criterion 21: a REPLY whose id equals `BLOCKING` but whose channel differs
/// from the pending CALL's: validated before the claim, so `REPLY_READY` stays 0.
#[test]
fn criterion21_reply_on_the_wrong_channel_closes_without_a_claim() {
    inbound_violation(|stream, call| {
        host_write(stream, FrameKind::Reply, LIFECYCLE_CHANNEL, call.corr, b"x");
    });
}

/// Criterion 14: main's liveness failure and the Worker's own close race to
/// end the link. Exactly one code is recorded and it never changes.
#[test]
fn criterion14_liveness_and_close_race_records_one_code() {
    let (mut stream, role) = start("liveness-close-race");
    long_reads(&stream);
    read_call_named(&mut stream, "race");
    read_until_link_loss(&mut stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-returned", "false"),
            ("code-matches-state", "true"),
            ("later-matches", "true"),
            ("done", "true"),
        ],
    );
    let report = output.report();
    let recorded = report.get("state-at-throw").map(String::as_str);
    assert!(
        matches!(recorded, Some("22" | "25")),
        "{}",
        output.diagnostics()
    );
    assert_eq!(recorded, report.get("state-later").map(String::as_str));
}

/// Criterion 18: with the ring full (1,024 records in 64 KiB), a blocking REPLY
/// of exactly `replyBytes` still returns, with no `KELD-IPC-026`.
#[test]
fn criterion18_blocking_reply_uses_its_slot_when_the_ring_is_full() {
    let (mut stream, role) = start("reply-slot");
    let call = read_call_named(&mut stream, "full-ring");
    for seq in 0..1024 {
        host_write(
            &mut stream,
            FrameKind::Event,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            &arm_b_payload(seq),
        );
    }
    let reply: Vec<u8> = (0..4096_u32)
        .map(|i| u8::try_from(i & 0xff).expect("masked to a byte"))
        .collect();
    host_reply(&mut stream, call, &reply);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("reply-len", "4096"),
            ("reply-pattern", "true"),
            ("slot-events", "1024"),
            ("slot-in-order", "true"),
            ("done", "true"),
        ],
    );
}

/// Criterion 18: a blocking REPLY of `replyBytes + 1` payload bytes fails closed.
#[test]
fn criterion18_oversize_blocking_reply_fails_closed_with_026() {
    let (mut stream, role) = start("reply-slot-oversize");
    long_reads(&stream);
    let call = read_call_named(&mut stream, "oversize");
    host_reply(&mut stream, call, &[7; 4097]);
    read_until_link_loss(&mut stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-026"),
            ("call-returned", "false"),
            ("done", "true"),
        ],
    );
}

/// Criterion 18 harness: the Worker stalls between its claim and `REPLY_READY`
/// past the deadline; once main's deadline compare-and-exchange has failed, the
/// call returns the real reply and never also throws `KELD-IPC-006`. A hook holds
/// main's deadline branch until the Worker has claimed, so the result does not
/// depend on the host round trip beating the 200 ms deadline.
fn claim_stall(reply_delay: Duration) {
    let (mut stream, role) = start("claim-stall");
    long_reads(&stream);
    let call = read_call_named(&mut stream, "stall");
    // Artificial host delay (load shaping): the late-round-trip arm proves
    // that no assertion depends on the deadline outrunning the reply.
    thread::sleep(reply_delay);
    host_reply(&mut stream, call, b"stalled-reply");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-returned", "true"),
            ("call-value", "stalled-reply"),
            ("deadline-cas-failed", "1"),
            ("watchdog-advances-below-limit", "true"),
            ("done", "true"),
        ],
    );
}

/// Criterion 18: the claimed reply wins over the deadline.
#[test]
fn criterion18_claimed_reply_wins_over_the_deadline() {
    claim_stall(Duration::ZERO);
}

/// Criterion 18 under a late round trip: the host answers 2 s after the CALL,
/// ten times the 200 ms deadline, and the outcome is unchanged.
#[test]
fn criterion18_claimed_reply_wins_when_the_round_trip_is_late() {
    claim_stall(Duration::from_secs(2));
}

/// Criterion 19 harness: the second EVENT is appended while the dispatch task
/// for the first is running, and the host sends nothing more.
fn stranded(scenario: &str, uncaught: &str) {
    let (mut stream, role) = start(scenario);
    long_reads(&stream);
    host_event(&mut stream, 0, 8);
    read_role_event(&mut stream, "send-second");
    host_event(&mut stream, 1, 8);
    let probe = read_call_named(&mut stream, "still-up");
    host_reply(&mut stream, probe, b"up");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("stranded-events", "2"),
            ("stranded-in-order", "true"),
            ("idle-calls-positive", "true"),
            ("idle-violations", "0"),
            ("uncaught", uncaught),
            ("link-up", "up"),
            ("done", "true"),
        ],
    );
}

/// Criterion 19: no ring record is left without a requested dispatch task.
#[test]
fn criterion19_no_record_is_stranded_in_the_ring() {
    stranded("stranded", "0");
}

/// Criterion 19: a throwing listener does not end the link or strand the next
/// record; its error is reported once, as an uncaught error in a later task.
#[test]
fn criterion19_throwing_listener_is_isolated() {
    stranded("stranded-throwing", "1");
}

/// Criterion 20: an EVENT, an asynchronous CALL and a blocking CALL queued
/// before the Worker handles any reach the host in send order, and no
/// outbound frame advances `W_BYTES` or `W_RECS`.
#[test]
fn criterion20_outbound_frames_keep_send_order() {
    let (mut stream, role) = start("outbound-order");
    long_reads(&stream);
    read_role_event(&mut stream, "e1");
    let c1 = read_call_named(&mut stream, "c1");
    let c2 = read_call_named(&mut stream, "c2");
    host_reply(&mut stream, c2, b"r2");
    read_role_event(&mut stream, "reply-c1");
    host_reply(&mut stream, c1, b"r1");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("c2", "r2"),
            ("w-bytes-delta", "0"),
            ("w-recs-delta", "0"),
            ("c1", "r1"),
            ("done", "true"),
        ],
    );
}

/// Criterion 21: a REPLY for a pending id, an EVENT on a declared channel and a
/// host echo CALL are admitted; a PING is echoed with its channel and
/// correlation id and never enters the ring.
#[test]
fn criterion21_admitted_frames_and_ping_echo() {
    let (mut stream, role) = start("inbound-admitted");
    long_reads(&stream);
    let pending = read_call_named(&mut stream, "pending");
    host_reply(&mut stream, pending, b"pending-reply");
    host_event(&mut stream, 7, 8);
    host_write(
        &mut stream,
        FrameKind::Call,
        ECHO_CHANNEL,
        CorrelationId(77),
        b"host-echo",
    );
    host_write(
        &mut stream,
        FrameKind::Ping,
        ChannelId(42),
        CorrelationId(9),
        &[],
    );
    let mut ping = None;
    let mut answer = None;
    while ping.is_none() || answer.is_none() {
        let (header, payload) = read_frame(&mut stream).expect("read the role's answers");
        match header.kind {
            FrameKind::Ping => ping = Some((header, payload)),
            FrameKind::Reply => answer = Some((header, payload)),
            other => panic!("unexpected role frame {other:?}"),
        }
    }
    let (ping, ping_payload) = ping.expect("PING echo");
    assert_eq!((ping.channel, ping.corr), (ChannelId(42), CorrelationId(9)));
    assert!(ping_payload.is_empty());
    let (answer, answer_payload) = answer.expect("echo CALL answer");
    assert_eq!(
        (answer.channel, answer.corr),
        (ECHO_CHANNEL, CorrelationId(77))
    );
    assert_eq!(answer_payload, b"answer:host-echo");
    let finish = read_call_named(&mut stream, "finish");
    host_reply(&mut stream, finish, b"finish-reply");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("pending", "pending-reply"),
            ("event-seq", "7"),
            ("handled", "host-echo"),
            ("finish", "finish-reply"),
            ("w-recs", "3"),
            ("done", "true"),
        ],
    );
}

/// Criterion 22: `MAX_ABANDONED_CALLS` expiries leave the link up and late
/// replies for them are discarded; the expiry that crosses the cap still
/// throws `KELD-IPC-006`, then every other pending and later call throws 027.
#[test]
fn criterion22_abandoned_set_is_bounded() {
    const MAX_ABANDONED_CALLS: usize = 256;
    let (mut stream, role) = start("abandoned-cap");
    long_reads(&stream);
    let abandoned: Vec<FrameHeader> = (0..MAX_ABANDONED_CALLS)
        .map(|_| read_call(&mut stream).0)
        .collect();
    read_role_event(&mut stream, "send-late-replies");
    for call in &abandoned[..4] {
        host_reply(&mut stream, *call, b"late");
    }
    let probe = read_call_named(&mut stream, "probe");
    host_reply(&mut stream, probe, b"probe-reply");
    for i in 0..4 {
        read_call_named(&mut stream, &format!("refill-{i}"));
    }
    read_call_named(&mut stream, "long");
    read_call_named(&mut stream, "crossing");
    let after = read_until_link_loss(&mut stream);
    assert!(
        after.is_empty(),
        "no CALL is written after the cap: {after:?}"
    );
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("first-006", "256"),
            ("probe", "probe-reply"),
            ("late-appended", "0"),
            ("refill-006", "4"),
            ("crossing-code", "KELD-IPC-006"),
            ("long-code", "KELD-IPC-027"),
            ("later-code", "KELD-IPC-027"),
            ("done", "true"),
        ],
    );
}

/// Criterion 25: a 4 MiB ring (a power of two) opens and carries a call.
#[test]
fn criterion25_four_mib_ring_opens() {
    let (mut stream, role) = start("ring-4mib");
    let call = read_call_named(&mut stream, "four-mib");
    host_reply(&mut stream, call, b"four");
    let output = role.finish();
    expect_report(&output, &[("reply", "four"), ("done", "true")]);
}

/// Criterion 25: byte counters start at `2^32 - 64`; 46-byte records straddle
/// the counter wrap intact and in order, and the blocking reply's `REPLY_AT`
/// lands past the wrap: every fact before it is applied before return, none after.
#[test]
fn criterion25_records_straddle_the_counter_wrap_in_order() {
    let (mut stream, role) = start("counter-wrap");
    let call = read_call_named(&mut stream, "wrap");
    for seq in 0..4 {
        host_event(&mut stream, seq, 30);
    }
    host_reply(&mut stream, call, b"wrapped");
    for seq in 4..8 {
        host_event(&mut stream, seq, 30);
    }
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("applied-at-return", "0,1,2,3"),
            ("reply-at-wrapped", "true"),
            ("wrap-events", "8"),
            ("wrap-in-order", "true"),
            ("payloads-intact", "true"),
            ("applied-final", "0,1,2,3,4,5,6,7"),
            ("done", "true"),
        ],
    );
}

/// Criterion 26 harness: the Worker claims the reply and skips the publish while
/// its heartbeat keeps running; main's post-claim bound throws `KELD-IPC-025`
/// before the watchdog counts `2 * window / interval` heartbeat advances. The
/// claim-first hook makes the claim precede the deadline at any round-trip speed.
fn claim_skip_publish(reply_delay: Duration) {
    let (mut stream, role) = start("claim-skip-publish");
    long_reads(&stream);
    let call = read_call_named(&mut stream, "skip");
    // Artificial host delay (load shaping), as in `claim_stall`.
    thread::sleep(reply_delay);
    host_reply(&mut stream, call, b"never-published");
    read_until_link_loss(&mut stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-025"),
            ("call-returned", "false"),
            ("state", "25"),
            ("post-claim-branch", "1"),
            ("watchdog-advances-below-limit", "true"),
            ("done", "true"),
        ],
    );
}

/// Criterion 26: a claim without a publish is bounded.
#[test]
fn criterion26_claim_without_publish_is_bounded() {
    claim_skip_publish(Duration::ZERO);
}

/// Criterion 26 under a late round trip (2 s host delay, deadline 300 ms).
#[test]
fn criterion26_claim_without_publish_is_bounded_when_the_round_trip_is_late() {
    claim_skip_publish(Duration::from_secs(2));
}

/// Criterion 26 harness: a throw inside the claim step makes the Worker record
/// 25 itself, so main wakes on `STATE`: its post-claim branch, its liveness
/// branch and its 30 s deadline never decided the outcome (counters, no
/// duration). A late host round trip changes nothing; a Worker stall past the
/// liveness window would, by design, because that is the liveness failure.
fn claim_step_throw(reply_delay: Duration) {
    let (mut stream, role) = start("claim-throw");
    long_reads(&stream);
    let call = read_call_named(&mut stream, "claim-throw");
    // Artificial host delay (load shaping), as in `claim_stall`.
    thread::sleep(reply_delay);
    host_reply(&mut stream, call, b"claimed");
    read_until_link_loss(&mut stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-025"),
            ("call-returned", "false"),
            ("state", "25"),
            ("post-claim-branch", "0"),
            ("deadline-cas-failed", "0"),
            ("liveness-branch", "0"),
            ("done", "true"),
        ],
    );
}

/// Criterion 26: a claim-step throw records 025 from the Worker.
#[test]
fn criterion26_claim_step_throw_records_025() {
    claim_step_throw(Duration::ZERO);
}

/// Criterion 26 throw arm under a late round trip (2 s host delay).
#[test]
fn criterion26_claim_step_throw_records_025_when_the_round_trip_is_late() {
    claim_step_throw(Duration::from_secs(2));
}

/// Criterion 27: asynchronous replies are settled on main by correlation id,
/// after the listener of an earlier EVENT; a late reply after `KELD-IPC-006`
/// changes nothing and leaves the link up.
#[test]
fn criterion27_async_replies_settle_by_correlation_after_listeners() {
    let (mut stream, role) = start("async-dispatch");
    long_reads(&stream);
    let c1 = read_call_named(&mut stream, "c1");
    let c2 = read_call_named(&mut stream, "c2");
    host_event(&mut stream, 0, 8);
    host_reply(&mut stream, c2, b"r2");
    write_call_error(
        &mut stream,
        c1.channel,
        c1.corr,
        &CallError {
            code: "KELD-GUARD001".to_owned(),
            message: "KELD-GUARD001: denied by the test host".to_owned(),
        },
    )
    .expect("host ERR");
    let late = read_call_named(&mut stream, "late");
    read_role_event(&mut stream, "send-late-reply");
    host_reply(&mut stream, late, b"late-reply");
    let probe = read_call_named(&mut stream, "probe");
    host_reply(&mut stream, probe, b"probe-reply");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("log", "event,c2:r2,c1:KELD-GUARD001"),
            ("late-code", "KELD-IPC-006"),
            ("probe", "probe-reply"),
            ("done", "true"),
        ],
    );
}

/// Criterion 27: a host echo CALL written during a park runs its handler only
/// after `callBlocking` returns; the host reads exactly one answer frame with
/// the CALL's channel and correlation id.
#[test]
fn criterion27_host_call_is_answered_after_the_park() {
    let (mut stream, role) = start("host-call-parked");
    long_reads(&stream);
    let parked = read_call_named(&mut stream, "parked");
    host_write(
        &mut stream,
        FrameKind::Call,
        ECHO_CHANNEL,
        CorrelationId(500),
        b"ping",
    );
    host_reply(&mut stream, parked, b"parked-reply");
    let (answer, payload) = read_frame(&mut stream).expect("read the handler's answer");
    assert_eq!(answer.kind, FrameKind::Reply);
    assert_eq!(
        (answer.channel, answer.corr),
        (ECHO_CHANNEL, CorrelationId(500))
    );
    assert_eq!(payload, b"answer:ping");
    let finish = read_call_named(&mut stream, "finish");
    host_reply(&mut stream, finish, b"finish-reply");
    let rest = read_until_link_loss(&mut stream);
    assert!(
        rest.iter()
            .all(|(header, _)| header.corr != CorrelationId(500)),
        "exactly one frame answers the host CALL: {rest:?}"
    );
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("handler-runs-at-return", "0"),
            ("handler-runs-after", "1"),
            ("finish", "finish-reply"),
            ("done", "true"),
        ],
    );
}

/// Criterion 27: an `echoReceiver` with no call handler set makes a host CALL
/// close the link with `KELD-IPC-005`; every pending call rejects with 022.
#[test]
fn criterion27_host_call_without_a_handler_closes_the_link() {
    let (mut stream, role) = start("host-call-no-handler");
    long_reads(&stream);
    read_call_named(&mut stream, "pending");
    host_write(
        &mut stream,
        FrameKind::Call,
        ECHO_CHANNEL,
        CorrelationId(600),
        b"ping",
    );
    let frames = read_until_link_loss(&mut stream);
    assert!(
        frames
            .iter()
            .all(|(header, _)| header.corr != CorrelationId(600)),
        "a missing handler sends no answer: {frames:?}"
    );
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("pending-code", "KELD-IPC-022"),
            ("pending-cause-005", "true"),
            ("later-code", "KELD-IPC-022"),
            ("done", "true"),
        ],
    );
}

/// Review regression (§4.1 edge A8 to A6): the host writes the REPLY and then
/// closes; a test hook holds main between its `REPLY_READY` and `STATE` loads
/// until the Worker has published and recorded the close. The real reply
/// returns and the slot is emptied, never `KELD-IPC-022`.
#[test]
fn review_reply_published_before_the_close_wins() {
    let (mut stream, role) = start("reply-then-close");
    let call = read_call_named(&mut stream, "reply-then-close");
    host_reply(&mut stream, call, b"real-reply");
    stream
        .shutdown(Shutdown::Both)
        .expect("host closes the link");
    drop(stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-returned", "true"),
            ("call-value", "real-reply"),
            ("reply-ready-after", "0"),
            ("done", "true"),
        ],
    );
}

/// Review regression (§4.6): a state applier running in a dispatch task, not
/// only in the wake drain, is refused `callBlocking` with `KELD-IPC-005` before
/// any write, so no fact is re-applied; the link stays up.
#[test]
fn review_applier_blocking_call_in_dispatch_is_refused() {
    let (mut stream, role) = start("applier-blocking-in-dispatch");
    long_reads(&stream);
    host_event(&mut stream, 0, 8);
    // The first CALL the host reads is the probe: the refused inner call wrote nothing.
    let probe = read_call_named(&mut stream, "probe");
    host_reply(&mut stream, probe, b"up");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("inner-code", "KELD-IPC-005"),
            ("inner-result", "KELD-IPC-005"),
            ("probe", "up"),
            ("done", "true"),
        ],
    );
}

/// Review regression (§4.4): an ERR whose payload is not a `CallError` closes
/// the link as a session-contract violation; the call throws 022 naming 005.
#[test]
fn review_malformed_err_closes_the_link_with_022() {
    let (mut stream, role) = start("malformed-err");
    long_reads(&stream);
    let call = read_call_named(&mut stream, "err");
    host_write(&mut stream, FrameKind::Err, call.channel, call.corr, &[]);
    read_until_link_loss(&mut stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-022"),
            ("call-cause-005", "true"),
            ("call-returned", "false"),
            ("later-code", "KELD-IPC-022"),
            ("done", "true"),
        ],
    );
}

/// `WorkerLink.close()` records 22 on main: the pending call rejects with
/// `KELD-IPC-022`, later calls and sends throw it, and the CALL posted before
/// the close is still written, in post order, before the host observes link
/// loss. The role stays alive until the host has observed that (stdin "go").
#[test]
fn local_close_rejects_pending_calls_with_022() {
    let (mut stream, mut role) = start("local-close");
    long_reads(&stream);
    let frames = read_until_link_loss(&mut stream);
    role.release();
    assert_eq!(
        frames.len(),
        1,
        "exactly the CALL posted before the close: {frames:?}"
    );
    assert_eq!(frames[0].0.kind, FrameKind::Call);
    assert_eq!(frames[0].1, b"pending");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("pending-code", "KELD-IPC-022"),
            ("pending-returned", "false"),
            ("state", "22"),
            ("later-code", "KELD-IPC-022"),
            ("send-after-close", "KELD-IPC-022"),
            ("done", "true"),
        ],
    );
}

/// §4.6 expiry rule: a `call()` answered while main is parked keeps that reply
/// even when its overdue deadline timer runs before the dispatch task (a hook
/// withholds the dispatch). Failed first, reporting 006, at `17a480bc`.
#[test]
fn expiry_during_park_keeps_a_retained_reply() {
    let (mut stream, role) = start("expiry-during-park");
    long_reads(&stream);
    let early = read_call_named(&mut stream, "early");
    let park = read_call_named(&mut stream, "long-park");
    host_reply(&mut stream, early, b"early-reply");
    // Load shaping, not synchronization: keep the park longer than the async
    // call's 200 ms deadline. The role asserts that precondition itself.
    thread::sleep(Duration::from_millis(600));
    host_reply(&mut stream, park, b"park-reply");
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("park-outlasted-deadline", "true"),
            ("early-returned", "true"),
            ("early-value", "early-reply"),
            ("done", "true"),
        ],
    );
}

/// The claim-first hold is bounded: the Worker exits abruptly before any claim
/// (the host never answers), and the parked call still ends in `KELD-IPC-025`
/// through main's liveness branch instead of hanging inside the hold.
#[test]
fn claim_first_hold_ends_in_025_when_the_worker_dies_before_its_claim() {
    let (mut stream, role) = start("claim-first-worker-dies");
    long_reads(&stream);
    read_call_named(&mut stream, "dies-before-claim");
    read_until_link_loss(&mut stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("call-code", "KELD-IPC-025"),
            ("call-returned", "false"),
            ("claimed", "0"),
            ("liveness-branch", "1"),
            ("done", "true"),
        ],
    );
}

/// §4.6: a `call()` still unanswered when the link ends rejects with the code
/// `STATE` records (022 here), not `KELD-IPC-006`, even when its overdue
/// deadline timer runs before the dispatch task (a hook withholds the dispatch).
#[test]
fn expiry_after_the_link_ended_rejects_with_the_recorded_code() {
    let (mut stream, role) = start("expiry-after-close");
    long_reads(&stream);
    read_call_named(&mut stream, "early");
    read_call_named(&mut stream, "long-park");
    // Load shaping, not synchronization: keep the park past the async call's
    // 200 ms deadline before the host closes without answering either call.
    thread::sleep(Duration::from_millis(600));
    stream
        .shutdown(Shutdown::Both)
        .expect("host closes the link");
    drop(stream);
    let output = role.finish();
    expect_report(
        &output,
        &[
            ("park-code", "KELD-IPC-022"),
            ("early-code", "KELD-IPC-022"),
            ("early-returned", "false"),
            ("done", "true"),
        ],
    );
}
