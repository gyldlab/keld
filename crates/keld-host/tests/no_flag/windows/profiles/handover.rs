//! The profile handover's cooperative quit, its forced end at the quit deadline D, and
//! a handover whose UI tail fails (KEL-53 §7 row "8, 20 (profile handover)", cells h11,
//! h12 and the tail half of h9; KEL-270 T4d S6c2a).
//!
//! The debug-only `KELD_TEST_WINDOWS_HANDOVER=1` seam requests a handover right after
//! the first session's `Ready` and asks the loop for one more session, the shape the
//! S6c2e coordinator and the S6c2f restore use. The host ends its UI through the
//! existing `Quit` command, writes `LastWindowClosed` itself once the loop has
//! returned, waits up to D for the application's `Quit` and otherwise ends the session
//! under the handover cause. Each cell observes the application's control stream, the
//! host's own evidence line (its monotonic clock), the profile record at write-free
//! points, the native window census and the next session or launch.

use super::sessions::{
    LifecycleObservation, ObservedSession, SeamedHost, assert_first_exited_before_second,
    engine_constructions, read_lifecycle_record,
};
use crate::support::control::{accept_ready_generation, try_read_control_line};
use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::process::{
    DIAGNOSTIC_QUERY_KILL_SWITCH, PIPE_DRAIN_KILL_SWITCH, bounded_output, open_process_for_wait,
    pipe_text_by, process_times, process_tree_report, read_pipe_on_thread, wait_for_process_signal,
};
use crate::support::product::ProductFixture;
use crate::support::profile_run::{ProfileTestRoot, assert_profile_test_root_feature};
use crate::ui_end_kill_switch;
use serde_json::Value;
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;

/// The debug-only handover trigger (the shape of `KELD_TEST_WINDOWS_SECOND_SESSION`).
const HANDOVER_SEAM: &str = "KELD_TEST_WINDOWS_HANDOVER";
/// The keld-wv fault fixture that loses the host's `BrowserProcessExited` under a short
/// post-exit grace (h9's seam-injected barrier grace expiry).
const BARRIER_GRACE_EXPIRY_SEAM: &str = "KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY";
/// The reference application's h12 mode: it reports `LastWindowClosed` and ignores it.
const IGNORE_LAST_WINDOW_CLOSED: &str = "KELD_T1B_IGNORE_LAST_WINDOW_CLOSED";
/// The host's evidence line for one seam-driven handover (its monotonic clock).
const HOST_EVIDENCE_PREFIX: &str = "KELD_TEST_WINDOWS_HANDOVER ";
/// The quit deadline D as KEL-53 §4 *Margin and deadlines* records it (S6c2a's
/// measurement is below its 10 s floor, so D is the floor).
const QUIT_DEADLINE: Duration = Duration::from_secs(10);
/// How late after D h12's forced end may come, on the host's clock (`forced_after_us`)
/// and on the test's clock (`LastWindowClosed` read to `LINK_EOF` read). Measured
/// overshoot past D over the 44 h12 runs of S6c2a and its review (2026-10-14, RAMANI,
/// the S6c2a debug host): host clock idle max 14.9 ms, under full-core load (16
/// busy-loop burners, 10 runs) max 39.2 ms; test clock idle max 15 ms, load max
/// 115 ms. Each slack is sixteen times its clock's maximum rounded up to a whole
/// second (0.63 s -> 1 s; 1.84 s -> 2 s), wide because only ten samples ran under
/// load, and each stays below the 5 s shift of the review's late-deadline mutant
/// (`r-m6`), which each bound fails on its own.
const FORCED_END_HOST_SLACK: Duration = Duration::from_secs(1);
const FORCED_END_TEST_SLACK: Duration = Duration::from_secs(2);

/// Kill switch over each wait of a handover cell, never a synchronization wait. The
/// longest wait is a UI end (`ui_end_kill_switch`, which also covers the window
/// close, the record writes and the control line); the forced end adds D.
fn handover_kill_switch() -> Duration {
    ui_end_kill_switch() + QUIT_DEADLINE
}

/// The `ci` profile's per-test terminate for this binary (`period` x
/// `terminate-after` of its `[[profile.ci.overrides]]` block in
/// `.config/nextest.toml`), read from the file the gate runs with.
fn nextest_ci_terminate() -> Duration {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.config/nextest.toml");
    let config = std::fs::read_to_string(path).expect("read .config/nextest.toml");
    let block = config
        .split("[[profile.ci.overrides]]")
        .find(|block| block.contains("filter = \"binary(no_flag_windows)\""))
        .expect("no_flag_windows ci override block");
    let line = block
        .lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .find_map(|line| line.trim().strip_prefix("slow-timeout = "))
        .expect("no_flag_windows override sets slow-timeout");
    let period = slow_timeout_field(line, "period")
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix("s\""))
        .and_then(|seconds| seconds.parse::<u64>().ok())
        .expect("slow-timeout period in whole seconds");
    let terminate_after = slow_timeout_field(line, "terminate-after")
        .parse::<u32>()
        .expect("whole-number terminate-after");
    Duration::from_secs(period) * terminate_after
}

/// The raw value of `name` in a one-line `slow-timeout = { ... }` table.
fn slow_timeout_field<'a>(line: &'a str, name: &str) -> &'a str {
    let start = line.find(name).expect("slow-timeout field") + name.len();
    line[start..]
        .trim_start()
        .strip_prefix('=')
        .expect("slow-timeout field assignment")
        .split([',', '}'])
        .next()
        .expect("slow-timeout field value")
        .trim()
}

/// Every kill switch in this binary is shorter than nextest's per-test terminate, so
/// no single wait at its bound outlasts the test on its own (a test whose
/// sequential waits all run to their bounds can still reach it; see the
/// `.config/nextest.toml` block). The handover switch is the longest:
/// `WEBVIEW2_TEARDOWN_BOUND` (UI end plus the post-release scavenge probe) plus the
/// product deadline plus D.
#[test]
fn kill_switches_stay_below_the_nextest_terminate() {
    let terminate = nextest_ci_terminate();
    for (name, switch) in [
        ("startup", crate::startup_kill_switch()),
        ("ui_end", ui_end_kill_switch()),
        (
            "stalled_navigation_end",
            crate::stalled_navigation_end_kill_switch(),
        ),
        ("handover", handover_kill_switch()),
    ] {
        assert!(
            switch < terminate,
            "{name} kill switch {switch:?} reaches the nextest terminate {terminate:?}"
        );
    }
    assert!(
        ui_end_kill_switch()
            >= keld_wv::webview2::WEBVIEW2_UI_END_BOUND + keld_wv::webview2::WEBVIEW2_UI_END_BOUND,
        "the host-exit switch covers the scavenge probe's release after the UI end"
    );
}

/// The run-owned installation, profile root and control listener one cell uses.
struct HandoverCell {
    fixture: ProductFixture,
    root: ProfileTestRoot,
    installed: InstalledApp,
    control: TcpListener,
}

impl HandoverCell {
    fn prepare() -> Self {
        let fixture = ProductFixture::new();
        let root = ProfileTestRoot::new();
        let installed = InstalledApp::install(
            &fixture.project,
            &fixture_env(SIGNED_HOST_A_P1),
            &fixture_env(INSTALLER_ENV),
        );
        assert_profile_test_root_feature(installed.host(), root.path());
        let control = TcpListener::bind(("127.0.0.1", 0)).expect("bind handover control");
        Self {
            fixture,
            root,
            installed,
            control,
        }
    }

    fn finish(self) {
        drop(self.installed);
        self.root.remove();
        drop(self.fixture);
    }
}

/// Independent native observation of the host's main window every ~100 ms, recorded as
/// `(ms since start, main window handle)` transitions (h8: record only).
struct WindowWatch {
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<Vec<(u64, u64)>>>,
}

impl WindowWatch {
    fn start(pid: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let started = Instant::now();
        let join = thread::spawn(move || {
            let mut transitions = Vec::new();
            let mut last = None;
            while !flag.load(Ordering::Acquire) {
                let handle = host_main_window(pid);
                if last != Some(handle) {
                    transitions.push((elapsed_ms(started), handle));
                    last = Some(handle);
                }
                thread::park_timeout(Duration::from_millis(100));
            }
            transitions
        });
        Self {
            stop,
            join: Some(join),
        }
    }

    fn finish(mut self) -> Vec<(u64, u64)> {
        self.stop.store(true, Ordering::Release);
        self.join
            .take()
            .expect("window watcher thread")
            .join()
            .expect("window watcher did not panic")
    }
}

impl Drop for WindowWatch {
    /// Stops the observer when a failed assertion unwinds past `finish`.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// The first window's disappearance to the second window's appearance, in ms, when
/// both transitions were observed.
fn window_gap_ms(transitions: &[(u64, u64)]) -> Option<u64> {
    let mut seen = false;
    let mut gone_at = None;
    for (at, handle) in transitions {
        match (seen, gone_at, *handle) {
            (false, _, handle) if handle != 0 => seen = true,
            (true, None, 0) => gone_at = Some(*at),
            (true, Some(gone), handle) if handle != 0 => return Some(at.saturating_sub(gone)),
            _ => {}
        }
    }
    None
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).expect("elapsed ms fits u64")
}

/// Harness self-test for the cells' diagnostics (PR #722 review finding P3): a helper
/// process that stalls is killed at its kill switch and reported, a prompt one is read
/// in full, and a pipe whose write end outlives the reader's kill switch yields a
/// marker instead of hanging the report.
#[test]
fn handover_diagnostics_are_bounded() {
    let started = Instant::now();
    let stalled = bounded_output(
        Command::new("powershell.exe").args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 120",
        ]),
        Instant::now() + Duration::from_secs(2),
    );
    let stalled_for = started.elapsed();
    assert!(
        matches!(&stalled, Err(error) if error.contains("kill switch")),
        "a stalled helper is reported: {:?}",
        stalled.as_ref().map(|output| output.stdout.clone())
    );
    assert!(
        stalled_for < DIAGNOSTIC_QUERY_KILL_SWITCH,
        "the stalled helper ended at its kill switch, not its own end: {stalled_for:?}"
    );

    let prompt = bounded_output(
        Command::new("powershell.exe").args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "'ready'",
        ]),
        Instant::now() + DIAGNOSTIC_QUERY_KILL_SWITCH,
    )
    .expect("a prompt helper");
    assert!(prompt.status.success(), "{}", prompt.stderr);
    assert_eq!(prompt.stdout.trim(), "ready");

    let (pipe, write_end) = std::io::pipe().expect("anonymous pipe");
    let text = read_pipe_on_thread(pipe);
    assert_eq!(
        pipe_text_by(&text, Instant::now() + Duration::from_millis(200)),
        "<pipe still open at its kill switch>"
    );
    drop(write_end);
    assert_eq!(
        pipe_text_by(&text, Instant::now() + PIPE_DRAIN_KILL_SWITCH),
        ""
    );
}

/// The host's main window handle as the OS reports it now, `0` when it has none.
fn host_main_window(pid: u32) -> u64 {
    host_main_window_with_title(pid).0
}

/// The host's main window handle and title as the OS reports them now.
fn host_main_window_with_title(pid: u32) -> (u64, String) {
    let output = bounded_output(
        Command::new("powershell.exe").args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "$p=Get-Process -Id {pid} -ErrorAction SilentlyContinue; if ($p) {{ [string]([uint64]$p.MainWindowHandle.ToInt64()) + '|' + $p.MainWindowTitle }} else {{ '0|' }}"
            ),
        ]),
        Instant::now() + DIAGNOSTIC_QUERY_KILL_SWITCH,
    )
    .unwrap_or_else(|error| panic!("query host main window: {error}"));
    let text = output.stdout.trim();
    let (handle, title) = text.split_once('|').unwrap_or(("0", ""));
    (handle.parse().unwrap_or(0), title.to_owned())
}

/// Reads one control line under the handover kill switch. A failed read reports the
/// host's process tree first (the browser's liveness); the unwinding `SeamedHost` then
/// terminates the host and prints its stdout and stderr.
fn read_handover_line(reader: &mut BufReader<TcpStream>, host_pid: u32, label: &str) -> String {
    try_read_control_line(reader, Instant::now() + handover_kill_switch())
        .unwrap_or_else(|error| panic!("{label}: {error}; {}", process_tree_report(host_pid)))
}

/// The one `KELD_TEST_WINDOWS_HANDOVER` line the seam-driven host prints.
fn host_handover_evidence(stdout: &str) -> Value {
    let lines: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix(HOST_EVIDENCE_PREFIX))
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "exactly one host handover evidence line\nstdout:\n{stdout}"
    );
    serde_json::from_str(lines[0].trim_end()).expect("host handover evidence JSON")
}

struct HandoverRun {
    host_pid: u32,
    bun_pids: [u32; 2],
    /// Taken only while the host waits D (h12); the reference application's session
    /// ends too fast for an unambiguous census, so h11 records the window transitions
    /// and asserts no census. h12's census covers h11's host path too: up to the
    /// `LastWindowClosed` write (3a) the two cells run the identical host code, and
    /// they differ only in the application's mode after it.
    window_census_at_last_window_closed: Option<u64>,
    control_ms_from_last_window_closed_to_link_eof: u64,
    quit_reply: bool,
    transitions: Vec<(u64, u64)>,
    evidence: Value,
    stdout: String,
}

/// One seam-driven handover followed by a second session the controller quits: the
/// shared body of h11 and h12, which differ only in the application's mode.
fn run_handover_then_second_session(
    cell: &HandoverCell,
    ignore_last_window_closed: bool,
) -> HandoverRun {
    let mut seams = vec![(HANDOVER_SEAM, "1")];
    if ignore_last_window_closed {
        seams.push((IGNORE_LAST_WINDOW_CLOSED, "1"));
    }
    let mut seamed = SeamedHost::launch(&cell.installed, cell.root.path(), &cell.control, &seams);
    let host_pid = seamed.pid;
    let watch = WindowWatch::start(host_pid);

    // Session 1 to Ready. The seam requests the handover right after Ready, so the
    // window is closing from here on and the record is not read until the host waits.
    let (mut reader, _writer, bun_1, link_1) =
        accept_ready_generation(&cell.control, &mut seamed.host);
    let bun_1_handle = open_process_for_wait(bun_1, true);

    // 3a: the UI loop returned and the host told the application; the window is gone.
    assert_eq!(
        read_handover_line(&mut reader, host_pid, "await LastWindowClosed"),
        "LAST_WINDOW_CLOSED"
    );
    let last_window_closed_at = Instant::now();
    let mut window_census_at_last_window_closed = None;
    if ignore_last_window_closed {
        // The host waits D from here (10 s, write-free), so this census and record
        // read cannot race the next session: no window of the host exists once the
        // application is told, and the loop released the profile before the write.
        let (census, window_title) = host_main_window_with_title(host_pid);
        assert_eq!(
            census, 0,
            "the application is told LastWindowClosed only after the window closed (host main window title: {window_title:?})"
        );
        assert_eq!(
            read_lifecycle_record(cell.root.path()),
            LifecycleObservation::idle(),
            "the profile is idle before the application is told"
        );
        window_census_at_last_window_closed = Some(census);
    }
    // With the reference application the Quit reply, the link close and the
    // application's exit follow within milliseconds, and a reaped peer's reset makes
    // Windows discard control lines not yet read, so nothing may delay the reads
    // below; the next session's window can also appear within hundreds of
    // milliseconds, so the ordering is asserted by h12 and only recorded here (h8).

    // 3b / 3c as the application's mode decides.
    let next = read_handover_line(&mut reader, host_pid, "await the session end");
    let quit_reply = if ignore_last_window_closed {
        assert_eq!(
            next, "LINK_EOF",
            "an application that ignores LastWindowClosed is ended without a Quit reply"
        );
        false
    } else {
        assert_eq!(
            next, "QUIT_REPLY",
            "the host answers the application's Quit"
        );
        assert_eq!(
            read_handover_line(&mut reader, host_pid, "await the link close"),
            "LINK_EOF"
        );
        true
    };
    let control_ms_from_last_window_closed_to_link_eof = elapsed_ms(last_window_closed_at);
    if ignore_last_window_closed {
        let control = Duration::from_millis(control_ms_from_last_window_closed_to_link_eof);
        assert!(
            control >= QUIT_DEADLINE.saturating_sub(Duration::from_secs(1)),
            "the forced end waited for D: {control_ms_from_last_window_closed_to_link_eof} ms"
        );
        assert!(
            control <= QUIT_DEADLINE + FORCED_END_TEST_SLACK,
            "the forced end came at D, not later than D + {FORCED_END_TEST_SLACK:?} on the test's clock: {control_ms_from_last_window_closed_to_link_eof} ms"
        );
    }

    let (bun_2, transitions, evidence, stdout) =
        second_session_then_exit(cell, seamed, watch, bun_1, &bun_1_handle, &link_1);
    HandoverRun {
        host_pid,
        bun_pids: [bun_1, bun_2],
        window_census_at_last_window_closed,
        control_ms_from_last_window_closed_to_link_eof,
        quit_reply,
        transitions,
        evidence,
        stdout,
    }
}

/// The loop proceeds after the handover: a second full session in the same process,
/// ended by the controller as h5 ends its sessions, then the host's orderly exit and
/// its evidence (the Bun reap order, the released profile, two engine constructions
/// and the host's handover line).
fn second_session_then_exit(
    cell: &HandoverCell,
    mut seamed: SeamedHost,
    watch: WindowWatch,
    bun_1: u32,
    bun_1_handle: &OwnedHandle,
    link_1: &str,
) -> (u32, Vec<(u64, u64)>, Value, String) {
    let host_pid = seamed.pid;
    let second =
        ObservedSession::observe(&cell.control, &mut seamed.host, host_pid, cell.root.path());
    seamed.assert_same_live_process();
    assert_ne!(
        bun_1, second.bun_pid,
        "the second session starts a new Bun primary"
    );
    assert_ne!(
        link_1,
        second.link.as_str(),
        "the second session mints fresh app-link authority"
    );
    assert_eq!(
        wait_for_process_signal(bun_1_handle, 0),
        WAIT_OBJECT_0,
        "the first Bun primary is reaped before the second session's generation binds"
    );
    assert_first_exited_before_second(
        process_times(bun_1_handle.as_raw_handle().cast()),
        process_times(second.bun.as_raw_handle().cast()),
    );
    let bun_2 = second.bun_pid;
    let bun_2_handle = second.quit("second session");
    let (status, stdout, stderr) = seamed.finish_by(Instant::now() + handover_kill_switch());
    let transitions = watch.finish();
    assert!(
        status.success(),
        "host exited with {status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        wait_for_process_signal(&bun_2_handle, 0),
        WAIT_OBJECT_0,
        "the second Bun primary is reaped by the host exit"
    );
    assert_eq!(
        read_lifecycle_record(cell.root.path()),
        LifecycleObservation::idle(),
        "the profile is released after the second session"
    );
    let (prepared_folders, environments) = engine_constructions(&stdout);
    assert_eq!(
        (prepared_folders.len(), environments),
        (2, 2),
        "two engine constructions in one process\nstdout:\n{stdout}"
    );
    assert_eq!(
        prepared_folders[0], prepared_folders[1],
        "both engines prepared the same persistent profile folder"
    );
    let evidence = host_handover_evidence(&stdout);
    assert_eq!(
        evidence["deadline_us"].as_u64(),
        Some(u64::try_from(QUIT_DEADLINE.as_micros()).expect("D fits u64")),
        "the host's quit deadline is D as the spec records it: {evidence}"
    );
    (bun_2, transitions, evidence, stdout)
}

/// h11, cooperative quit: the reference application receives `LastWindowClosed` after
/// the window closed, requests `Quit` within D, the host answers it and the session ends
/// under the lifecycle-Quit cause, never the forced one; the loop then runs the next
/// session. The host's own clock yields one D sample (write to Quit frame). This cell
/// records the window transitions but asserts no window census: h12 asserts it on the
/// identical host path (`HandoverRun::window_census_at_last_window_closed`).
#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1 (a debug host built with kel135-profile-test-root) and KELD_KEL254_INSTALLER_FIXTURE (KEL-254 installed-host operator fixtures)"]
fn kel270_s6c2a_h11_reference_app_quits_within_d_after_handover() {
    let cell = HandoverCell::prepare();
    let run = run_handover_then_second_session(&cell, false);
    assert!(run.quit_reply);
    assert_eq!(
        run.evidence["cause"], "lifecycle-quit",
        "the application's Quit claimed the session: {}",
        run.evidence
    );
    let quit_wait_us = run.evidence["quit_wait_us"]
        .as_u64()
        .unwrap_or_else(|| panic!("the host measured the Quit frame: {}", run.evidence));
    assert!(
        quit_wait_us < run.evidence["deadline_us"].as_u64().expect("deadline"),
        "the Quit arrived inside D: {}",
        run.evidence
    );
    assert!(
        run.evidence["forced_after_us"].is_null(),
        "the forced tail never ran: {}",
        run.evidence
    );
    println!(
        "KELD_KEL270_H11 {}",
        serde_json::json!({
            "host_pid": run.host_pid,
            "bun_pids": run.bun_pids,
            "host": run.evidence,
            "window_census_at_last_window_closed": run.window_census_at_last_window_closed,
            "control_ms_last_window_closed_to_link_eof": run.control_ms_from_last_window_closed_to_link_eof,
            "window_transitions_ms": run.transitions,
            "window_gap_ms": window_gap_ms(&run.transitions),
        })
    );
    drop(run.stdout);
    cell.finish();
}

/// h12, forced end: an application that ignores `LastWindowClosed` is ended at D under
/// the handover cause (no Quit reply, the link closed, the Bun primary reaped) and the
/// loop then proceeds to the next session. Without the deadline the session never
/// ends within the kill switch, which is this cell's negative control.
#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1 (a debug host built with kel135-profile-test-root) and KELD_KEL254_INSTALLER_FIXTURE (KEL-254 installed-host operator fixtures)"]
fn kel270_s6c2a_h12_app_ignoring_last_window_closed_is_ended_at_d() {
    let cell = HandoverCell::prepare();
    let run = run_handover_then_second_session(&cell, true);
    assert!(!run.quit_reply);
    assert_eq!(
        run.evidence["cause"], "forced",
        "the handover cause ended the session: {}",
        run.evidence
    );
    let forced_after_us = run.evidence["forced_after_us"]
        .as_u64()
        .unwrap_or_else(|| panic!("the host recorded the forced end: {}", run.evidence));
    let deadline_us = run.evidence["deadline_us"].as_u64().expect("deadline");
    assert!(
        forced_after_us >= deadline_us,
        "the forced end came no earlier than D on the host's clock: {}",
        run.evidence
    );
    let slack_us = u64::try_from(FORCED_END_HOST_SLACK.as_micros()).expect("slack fits u64");
    assert!(
        forced_after_us < deadline_us + slack_us,
        "the forced end came less than {FORCED_END_HOST_SLACK:?} after D on the host's clock: {}",
        run.evidence
    );
    assert!(
        run.evidence["quit_wait_us"].is_null(),
        "no Quit frame claimed the session: {}",
        run.evidence
    );
    println!(
        "KELD_KEL270_H12 {}",
        serde_json::json!({
            "host_pid": run.host_pid,
            "bun_pids": run.bun_pids,
            "host": run.evidence,
            "window_census_at_last_window_closed": run.window_census_at_last_window_closed,
            "control_ms_last_window_closed_to_link_eof": run.control_ms_from_last_window_closed_to_link_eof,
            "window_transitions_ms": run.transitions,
            "window_gap_ms": window_gap_ms(&run.transitions),
        })
    );
    drop(run.stdout);
    cell.finish();
}

/// h9, tail half: a handover whose UI end fails (seam-injected barrier grace expiry)
/// ends the host with the tail's error, with no second session and no `LastWindowClosed`
/// to the application; the profile record stays `stopping` under the dead host, and the
/// next launch boots (the current version) and recovers the profile to `Ready`.
#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1 (a debug host built with kel135-profile-test-root) and KELD_KEL254_INSTALLER_FIXTURE (KEL-254 installed-host operator fixtures)"]
fn kel270_s6c2a_h9_failed_handover_tail_exits_with_its_error() {
    let cell = HandoverCell::prepare();
    let mut seamed = SeamedHost::launch(
        &cell.installed,
        cell.root.path(),
        &cell.control,
        &[(HANDOVER_SEAM, "1"), (BARRIER_GRACE_EXPIRY_SEAM, "1")],
    );
    let host_pid = seamed.pid;
    let (mut reader, _writer, bun_1, _link) =
        accept_ready_generation(&cell.control, &mut seamed.host);
    let bun_1_handle = open_process_for_wait(bun_1, true);

    // The handover's UI end faults at the barrier: the host exits with that error.
    let (failed_status, stdout, stderr) = seamed.finish_by(Instant::now() + handover_kill_switch());
    assert!(
        !failed_status.success(),
        "a failed handover tail must end the host with an error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("KELD-WV-009") && stderr.contains("within the post-exit grace"),
        "the host reports the barrier's fault\nstderr:\n{stderr}"
    );
    let (prepared_folders, environments) = engine_constructions(&stdout);
    assert_eq!(
        (prepared_folders.len(), environments),
        (1, 1),
        "no second session follows a failed tail\nstdout:\n{stdout}"
    );
    assert!(
        !stdout.contains(HOST_EVIDENCE_PREFIX),
        "no handover completed\nstdout:\n{stdout}"
    );
    // The application was never told and never answered: its only possible control
    // record after Ready is the link's end of file (or none, when it was reaped first).
    let after_ready = try_read_control_line(&mut reader, Instant::now() + Duration::from_secs(2));
    assert!(
        matches!(after_ready.as_deref(), Ok("LINK_EOF") | Err(_)),
        "the application saw neither LastWindowClosed nor a Quit reply: {after_ready:?}"
    );
    assert_eq!(
        wait_for_process_signal(&bun_1_handle, 0),
        WAIT_OBJECT_0,
        "the Bun primary is reaped with the failed host"
    );
    assert_eq!(
        read_lifecycle_record(cell.root.path()),
        LifecycleObservation {
            phase: "stopping".to_owned(),
            owner_pid: Some(u64::from(host_pid)),
        },
        "idle was never reached: the record stays stopping under the dead host"
    );

    // The next launch, with no seam, boots and recovers the profile to Ready.
    let mut relaunched = SeamedHost::launch(&cell.installed, cell.root.path(), &cell.control, &[]);
    let relaunched_pid = relaunched.pid;
    let session = ObservedSession::observe(
        &cell.control,
        &mut relaunched.host,
        relaunched_pid,
        cell.root.path(),
    );
    let bun_2_handle = session.quit("relaunched session");
    let (status, relaunch_stdout, relaunch_stderr) = relaunched.finish();
    assert!(
        status.success(),
        "relaunched host exited with {status}\nstdout:\n{relaunch_stdout}\nstderr:\n{relaunch_stderr}"
    );
    assert_eq!(wait_for_process_signal(&bun_2_handle, 0), WAIT_OBJECT_0);
    assert_eq!(
        read_lifecycle_record(cell.root.path()),
        LifecycleObservation::idle(),
        "the relaunched session released the recovered profile"
    );
    println!(
        "KELD_KEL270_H9 {}",
        serde_json::json!({
            "failed_host_pid": host_pid,
            "failed_host_status": failed_status.code(),
            "after_ready_control": format!("{after_ready:?}"),
            "relaunched_host_pid": relaunched_pid,
            "failed_host_stderr": stderr.lines().take(4).collect::<Vec<_>>(),
        })
    );
    cell.finish();
}
