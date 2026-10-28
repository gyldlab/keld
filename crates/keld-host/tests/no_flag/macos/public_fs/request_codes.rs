//! GH-1095 (`docs/specs/gh1095-request-error-codes.md`) on the real no-flag
//! host, its supervised Bun and the real `WKWebView` page: which code the page
//! gets for its own call before the first generation is admitted, when Bun
//! dies at each request stage (including as its reply leaves), when the page
//! navigates while a call is pending, when the restarted generation has no
//! handler yet, and what one user-driven resend does.
//!
//! The harness is promoted from the GH-1081 experiments (never-merged PR
//! #1082). No production hook: every hold is an ordinary fixture HTTP wait
//! owned by the existing recovery observer, every kill is a real signal, the
//! resend trigger is a real OS pointer click, and every wait is on a journal
//! report, a host or app stderr marker, or a kernel process state.
use super::observer::{Observation, RecoveryCommand, RecoveryPhase};
use super::process_image::process_image_path;
use super::project::{Policy, PublicFsProject, TITLE};
use super::{RecoveryBoot, pid, record_recovery_boot, spawn_shipping_host};
use crate::renderer_bridge::CLICK_SCRIPT;
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::{NativeWindowObserver, native_windows};
use crate::support::process::{
    await_process_gone, await_process_state, kill_pid, parent_process, process_exists,
};
use serde_json::{Value, json};
use std::fs;
use std::io::BufRead as _;
use std::path::PathBuf;
use std::process::{ChildStderr, ChildStdin, Command, Output};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

const NOT_DISPATCHED: &str = "KELD-WV-012";
const OUTCOME_UNKNOWN: &str = "KELD-WV-013";
/// Bun's own code for a CALL that found no handler (S11): asserted by
/// `@keld/api`, passed through by the host unchanged.
const NO_HANDLER: &str = "KELD-API-002";
const NO_HANDLER_DETAIL: &str =
    "KELD-API-002: application channel has no registered handler; no application code ran";
const NO_GENERATION_DETAIL: &str = "no application generation is admitted; the call was not sent";
const LINK_DETAIL: &str = "application link ended before the renderer reply";
const PAGEHIDE_MESSAGE: &str = "KELD-WV-013: renderer document navigated before reply";
/// The host's own marker, printed right after `write_frame` returned `Ok` for
/// a renderer CALL: the dispatch point.
const KIPC_CALL: &str = "KELD_KEL142_KIPC_CALL ";
const RENDERER_ADMIT: &str = "KELD_KEL142_RENDERER_ADMIT ";

/// Where the fixture app registers its Echo handler (spec gh1095 §4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Registration {
    /// At module top, before the first lifecycle call opens the link: no
    /// CALL can find the handler missing.
    BeforeReady,
    /// After `whenReady` resolved, behind a test-owned hold: the window in
    /// which a CALL that reaches Bun gets `KELD-API-002`.
    InsideReady,
}

impl Registration {
    fn as_str(self) -> &'static str {
        match self {
            Self::BeforeReady => "before",
            Self::InsideReady => "inside",
        }
    }
}

/// One launch's fixture inputs. `Scenario::new` is every arm before AT-12d.
struct Scenario<'a> {
    name: &'a str,
    /// The page's scripted steps, one long-poll wait each.
    steps: Value,
    /// The steps of the page the observer serves at `/successor-page`, the
    /// target of a `navigate` step.
    successor_steps: Value,
    registration: Registration,
    /// Admit G1, wait for its Ready, and wait for the page and window before
    /// `start` returns. The pre-admission check keeps G1 held before its first
    /// lifecycle call and finishes the presentation itself.
    admit_first: bool,
}

impl<'a> Scenario<'a> {
    fn new(name: &'a str, steps: Value) -> Self {
        Self {
            name,
            steps,
            successor_steps: json!([]),
            registration: Registration::BeforeReady,
            admit_first: true,
        }
    }
}

/// One staged no-flag host launch and its ordinary observation journal.
struct Launch {
    observer: Observation,
    cleanup: ShippingLaunchCleanup,
    _lease: ChildStdin,
    _stage: keld_cli::boot::DevBootStage,
    bun_image: PathBuf,
    host_pid: u32,
    guardian: u32,
    bun: u32,
    journal: Vec<Value>,
    boots: Vec<RecoveryBoot>,
    steps: usize,
    stderr: StderrTap,
    /// The pre-armed CoreGraphics window observer, until `await_presentation`
    /// consumes it.
    presentation: Option<NativeWindowObserver>,
    name: String,
}

/// Live host stderr: every line is both kept and offered to condition waits,
/// so a test can act on a host or app marker the moment it is written.
struct StderrTap {
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
    reader: Option<thread::JoinHandle<()>>,
}

impl StderrTap {
    fn spawn(pipe: ChildStderr) -> Self {
        let (sender, lines) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut pipe = std::io::BufReader::new(pipe);
            loop {
                let mut line = Vec::new();
                match pipe.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {
                        if sender
                            .send(String::from_utf8_lossy(&line).into_owned())
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        });
        Self {
            lines,
            seen: Vec::new(),
            reader: Some(reader),
        }
    }

    fn count(&self, marker: &str) -> usize {
        self.seen
            .iter()
            .filter(|line| line.contains(marker))
            .count()
    }

    /// Waits until `count` lines contain `marker`; the deadline is a kill switch.
    fn await_count(&mut self, marker: &str, count: usize) {
        let deadline = Instant::now() + EVENT_DEADLINE;
        while self.count(marker) < count {
            let line = self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|_| panic!("host stderr {marker} x{count} before deadline"));
            self.seen.push(line);
        }
    }

    /// Every line, once the host and the processes sharing its stderr exited.
    fn finish(mut self) -> String {
        if let Some(reader) = self.reader.take() {
            reader.join().expect("host stderr reader");
        }
        while let Ok(line) = self.lines.try_recv() {
            self.seen.push(line);
        }
        self.seen.concat()
    }
}

impl Launch {
    fn start(fixture: &PublicFsProject, scenario: &Scenario<'_>) -> Self {
        let observer = Observation::bind_recovery();
        let port = observer.port.to_string();
        let app = include_str!("../../../fixtures/request_codes_app.ts")
            .replace("__PORT__", &port)
            .replace("__REGISTRATION__", scenario.registration.as_str());
        // A reloaded document runs no step of its own: it only reports ready.
        let template = include_str!("../../../fixtures/request_codes_page.html")
            .replace("__PORT__", &port)
            .replace("__RELOAD_STEPS__", "[]");
        observer.serve_page(template.replace("__STEPS__", &scenario.successor_steps.to_string()));
        let page = template.replace("__STEPS__", &scenario.steps.to_string());
        let (stage, bun_image) = fixture.stage_app_body(scenario.name, &app, &page);
        let presentation = NativeWindowObserver::arm_for_title(&fixture.census, TITLE);
        let mut cleanup = spawn_shipping_host(&stage);
        let host = cleanup.cli.as_mut().expect("host custody");
        let host_pid = host.id();
        let lease = host.stdin.take().expect("retain dev lease until host exit");
        let stderr = StderrTap::spawn(host.stderr.take().expect("host stderr pipe"));
        let mut launch = Self {
            observer,
            cleanup,
            _lease: lease,
            _stage: stage,
            bun_image,
            host_pid,
            guardian: 0,
            bun: 0,
            journal: Vec::new(),
            boots: Vec::new(),
            steps: 0,
            stderr,
            presentation: Some(presentation),
            name: scenario.name.to_owned(),
        };
        let boot = launch.await_boot(1);
        if scenario.admit_first {
            launch.admit(boot);
            launch.adopt_ready(1, boot);
            launch.await_presentation();
        }
        launch
    }

    /// The page reported ready and the host owns exactly one on-screen
    /// window with the fixture title.
    fn await_presentation(&mut self) {
        self.await_count("page-ready", 1);
        let windows = self
            .presentation
            .take()
            .expect("the window observer is consumed once")
            .expect_initial(self.host_pid, &self.name);
        assert_eq!(windows.len(), 1, "one host-owned WKWebView window");
    }

    /// Receive one ordinary report. The host-minted possession token in a boot
    /// report is kept only in `boots`, never in the printed journal.
    fn receive(&mut self, deadline: Instant) {
        let report = self
            .observer
            .reports
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|_| {
                panic!(
                    "gh1095 observation before deadline; journal={:?}",
                    self.journal
                )
            });
        if report["phase"] == "recovery-app-boot" {
            let boot = record_recovery_boot(report, &mut self.journal);
            self.boots.push(boot);
        } else {
            eprintln!("KELD_GH1095_OBSERVATION {report}");
            assert_ne!(report["phase"], "error", "fixture error: {report}");
            self.journal.push(report);
        }
    }

    fn await_where(&mut self, label: &str, matches: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + EVENT_DEADLINE;
        loop {
            if let Some(found) = self.journal.iter().find(|report| matches(report)) {
                return found.clone();
            }
            assert!(Instant::now() < deadline, "gh1095 {label} before deadline");
            self.receive(deadline);
        }
    }

    /// The `count`-th (1-based) report of `phase`, in arrival order.
    fn await_count(&mut self, phase: &str, count: usize) -> Value {
        let deadline = Instant::now() + EVENT_DEADLINE;
        loop {
            if let Some(found) = self
                .journal
                .iter()
                .filter(|report| report["phase"] == phase)
                .nth(count - 1)
            {
                return found.clone();
            }
            self.receive(deadline);
        }
    }

    fn await_boot(&mut self, count: usize) -> u32 {
        let deadline = Instant::now() + EVENT_DEADLINE;
        while self.boots.len() < count {
            self.receive(deadline);
        }
        self.boots[count - 1].pid
    }

    fn admit(&self, bun: u32) {
        self.observer
            .send_recovery_command(RecoveryCommand::Admit(bun));
    }

    /// Bind ordinary app data to kernel facts before granting fallback custody.
    fn adopt_ready(&mut self, count: usize, boot: u32) {
        let ready = self.await_count("app-ready", count);
        let bun = pid(&ready, "pid");
        assert_eq!(bun, boot, "Ready came from the admitted generation");
        let guardian = parent_process(bun);
        assert_eq!(parent_process(guardian), self.host_pid, "guardian parent");
        if self.guardian != 0 {
            assert_eq!(guardian, self.guardian, "persistent guardian");
        }
        assert_eq!(
            fs::canonicalize(process_image_path(bun)).expect("kernel Bun image"),
            self.bun_image,
            "admitted generation runs the independently selected Bun image"
        );
        self.guardian = guardian;
        self.bun = bun;
        self.cleanup.bun_group = Some(bun);
    }

    /// Release the page's one long-poll wait without waiting for its report.
    fn send_step(&mut self) {
        self.observer
            .send_recovery_command(RecoveryCommand::Invoke(RecoveryPhase::Before));
        self.steps += 1;
    }

    /// Release the page's one long-poll wait and collect its next step report.
    fn step(&mut self) -> Value {
        self.send_step();
        self.await_count("step", self.steps)
    }

    /// SIGKILL only the admitted Bun; return the successor, booted and held
    /// before its admission (no admitted generation exists until `admit`).
    fn crash_and_hold_successor(&mut self) -> u32 {
        let retired = self.bun;
        kill_pid(retired);
        self.hold_successor(retired)
    }

    /// After `retired`, the admitted Bun, was killed: its successor, booted
    /// and held before its admission, while the host and guardian live on.
    fn hold_successor(&mut self, retired: u32) -> u32 {
        let successor = self.await_boot(self.boots.len() + 1);
        assert_ne!(successor, retired, "a fresh Bun generation");
        await_process_gone(retired);
        assert!(
            process_exists(self.host_pid) && process_exists(self.guardian),
            "Bun loss terminated the host or guardian"
        );
        assert_eq!(parent_process(successor), self.guardian, "successor parent");
        successor
    }

    /// `Registration::InsideReady` only: `bun` holds inside `whenReady` with
    /// no handler registered. Until the test releases it, a CALL that reaches
    /// `bun` finds no handler.
    fn await_registration_hold(&mut self, bun: u32) {
        self.await_where("hold before registration", |report| {
            report["phase"] == "recovery-registration-pending" && report["pid"] == bun
        });
    }

    /// `Registration::InsideReady` only: release `bun`'s hold inside
    /// `whenReady` and wait for its handler to exist.
    fn release_registration(&mut self, bun: u32) {
        self.await_registration_hold(bun);
        self.observer
            .send_recovery_command(RecoveryCommand::RetirePending(bun));
        self.await_where("handler registered", |report| {
            report["phase"] == "handler-registered" && report["pid"] == bun
        });
    }

    /// One real OS pointer click on the page's full-window button, then the
    /// page's own report of what that trusted click did.
    fn click(&mut self, count: usize) -> Value {
        let output = Command::new("/usr/bin/xcrun")
            .args([
                "swift",
                "-e",
                CLICK_SCRIPT,
                &self.host_pid.to_string(),
                TITLE,
            ])
            .output()
            .expect("real CoreGraphics pointer action");
        eprintln!("KELD_GH1095_CLICK {output:?}");
        assert!(
            output.status.success(),
            "OS-visible click failed: {output:?}"
        );
        let clicked = self.await_count("click", count);
        assert_eq!(clicked["trusted"], true, "an OS click, not a script event");
        clicked
    }

    /// Release the current app's Quit and assert product cleanup; return the
    /// host's whole stderr.
    fn quit(mut self) -> String {
        self.observer
            .send_recovery_command(RecoveryCommand::Quit(self.bun));
        let output: Output = wait_child_output(
            self.cleanup.cli.take().expect("host custody"),
            EVENT_DEADLINE,
        );
        assert!(
            output.status.success(),
            "{output:?}; stderr so far: {}",
            self.stderr.seen.concat()
        );
        for process in [self.bun, self.guardian, self.host_pid] {
            await_process_gone(process);
        }
        assert_eq!(native_windows(self.host_pid, TITLE).len(), 0);
        self.cleanup.bun_group = None;
        let stderr = self.stderr.finish();
        eprintln!("KELD_GH1095_HOST_STDERR_LINES {}", stderr.lines().count());
        stderr
    }
}

fn outcome<'a>(report: &'a Value, label: &str) -> &'a Value {
    report["outcomes"]
        .as_array()
        .expect("page outcomes")
        .iter()
        .find(|outcome| outcome["label"] == label)
        .unwrap_or_else(|| panic!("no {label} outcome in {report}"))
}

fn request_nonce(report: &Value, label: &str) -> String {
    report["requests"]
        .as_array()
        .expect("page requests")
        .iter()
        .find(|request| request["label"] == label)
        .and_then(|request| request["nonce"].as_str())
        .unwrap_or_else(|| panic!("no {label} request in {report}"))
        .to_owned()
}

/// The passive bridge result envelopes recorded for one request.
fn terminals<'a>(report: &'a Value, label: &str) -> Vec<&'a Value> {
    let nonce = request_nonce(report, label);
    report["terminals"]
        .as_array()
        .expect("page terminals")
        .iter()
        .filter(|terminal| terminal["nonce"] == nonce.as_str())
        .collect()
}

/// The page saw one rejection whose `err.code` and relayed envelope both carry
/// `code` (and `detail`, when given).
fn rejected_with(
    report: &Value,
    label: &str,
    code: &str,
    detail: Option<&str>,
) -> Result<(), String> {
    let result = outcome(report, label);
    let observed = terminals(report, label);
    let matches = result["ok"] == false
        && result["code"] == code
        && result["error"]
            .as_str()
            .is_some_and(|message| message.starts_with(&format!("{code}: ")))
        && observed.len() == 1
        && observed[0]["ok"] == false
        && observed[0]["code"] == code
        && detail.is_none_or(|detail| observed[0]["detail"] == detail);
    if matches {
        Ok(())
    } else {
        Err(format!("{label}: outcome={result} terminals={observed:?}"))
    }
}

fn succeeded_on(report: &Value, label: &str, bun: u32) -> Result<(), String> {
    let result = outcome(report, label);
    let observed = terminals(report, label);
    // The page decodes the Echo message string as JSON: `{pid, call}`.
    if result["ok"] == true
        && observed.len() == 1
        && observed[0]["ok"] == true
        && result["reply"]["pid"] == bun
    {
        Ok(())
    } else {
        Err(format!("{label}: outcome={result} terminals={observed:?}"))
    }
}

/// The handler census: the labels one Bun generation's handler entered.
fn handled_labels(journal: &[Value], bun: u32) -> Vec<String> {
    journal
        .iter()
        .filter(|report| report["phase"] == "handler" && report["pid"] == bun)
        .map(|report| {
            let message: Value =
                serde_json::from_str(report["message"].as_str().expect("handler message"))
                    .expect("handler message JSON");
            message["label"].as_str().expect("request label").to_owned()
        })
        .collect()
}

fn marker_count(stderr: &str, marker: &str) -> usize {
    stderr.lines().filter(|line| line.contains(marker)).count()
}

/// Every call settled exactly once and the page saw every call settle.
fn assert_settled_once(last: &Value, labels: &[&str]) {
    let requests = last["requests"].as_array().expect("requests").len();
    assert_eq!(requests, labels.len(), "{last}");
    assert_eq!(
        last["outcomes"].as_array().expect("outcomes").len(),
        requests
    );
    for label in labels {
        assert_eq!(terminals(last, label).len(), 1, "one terminal for {label}");
    }
}

fn stop_process(pid: u32) {
    let status = Command::new("/bin/kill")
        .args(["-STOP", &pid.to_string()])
        .status()
        .expect("stop one process");
    assert!(status.success(), "stop {pid}: {status:?}");
    // The kernel state, not the signal send, is the precondition.
    await_process_state(pid, 'T');
}

/// Criterion 1 (AT-12a): a page call while G1 is dead and its successor is
/// booted but not admitted gets `KELD-WV-012`, is never written to kipc, and
/// is never run by either generation, even after the successor serves three
/// more calls. Controls: the same oracle refuses the successor's healthy
/// calls, and the outcome-unknown oracle refuses the gap call.
#[test]
fn gh1095_restart_gap_call_is_not_dispatched_and_never_replayed() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let labels = [
        "g1-baseline",
        "gap",
        "successor-1",
        "successor-2",
        "successor-3",
    ];
    let steps: Vec<Value> = labels
        .iter()
        .map(|label| json!({"do":"invoke","label":label}))
        .collect();
    let mut run = Launch::start(&fixture, &Scenario::new("gh1095-gap", Value::from(steps)));
    let first = run.bun;
    let baseline = run.step();
    let successor = run.crash_and_hold_successor();
    // Test-owned precondition: G1 is gone and G2 has booted but holds before
    // its first lifecycle call, so no generation is admitted.
    let gap = run.step();
    run.admit(successor);
    run.adopt_ready(2, successor);
    let mut last = Value::Null;
    for _ in 0..3 {
        last = run.step();
    }
    let journal = run.journal.clone();
    let stderr = run.quit();
    println!(
        "KELD_GH1095_AT12A first_bun={first} second_bun={successor} gap={} first_handled={:?} successor_handled={:?} renderer_admits={} kipc_calls={}",
        outcome(&gap, "gap"),
        handled_labels(&journal, first),
        handled_labels(&journal, successor),
        marker_count(&stderr, RENDERER_ADMIT),
        marker_count(&stderr, KIPC_CALL),
    );
    succeeded_on(&baseline, "g1-baseline", first).expect("healthy G1 call");
    rejected_with(&gap, "gap", NOT_DISPATCHED, Some(NO_GENERATION_DETAIL))
        .expect("criterion 1: the gap call is definitely not dispatched");
    for label in ["successor-1", "successor-2", "successor-3"] {
        succeeded_on(&last, label, successor).expect("the successor serves");
        assert!(
            rejected_with(&last, label, NOT_DISPATCHED, None).is_err(),
            "control: the not-dispatched oracle refuses a served call"
        );
    }
    assert!(
        rejected_with(&gap, "gap", OUTCOME_UNKNOWN, None).is_err(),
        "control: the gap call is not outcome unknown"
    );
    assert_settled_once(&last, &labels);
    // Census 0 forever for the gap call, and no replay into the successor.
    assert_eq!(handled_labels(&journal, first), ["g1-baseline"]);
    assert_eq!(
        handled_labels(&journal, successor),
        ["successor-1", "successor-2", "successor-3"]
    );
    // Host census: the bridge admitted every call; only the gap call never
    // crossed the dispatch point.
    assert_eq!(marker_count(&stderr, RENDERER_ADMIT), labels.len());
    assert_eq!(marker_count(&stderr, KIPC_CALL), labels.len() - 1);
}

/// Where the test kills G1 for one criterion-3 arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AfterWrite {
    /// The whole Bun process is stopped, the host writes the CALL, then SIGKILL:
    /// the handler never runs (census 0).
    ProcessStopped,
    /// SIGKILL while the handler waits inside its own body (census 1).
    InsideHandler,
}

/// Criterion 3 (AT-13b): after the host wrote the CALL, the end of the link is
/// `KELD-WV-013` whether or not the handler ran. The two arms' census differs
/// (0 and 1) under the same code, which is why that code cannot mean "not
/// sent". The successor never runs the call. Control: the not-dispatched
/// oracle refuses the call.
fn run_after_write(arm: AfterWrite) {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let call = match arm {
        AfterWrite::ProcessStopped => "after-write-stopped",
        AfterWrite::InsideHandler => "held-after-write",
    };
    let labels = ["g1-baseline", call, "successor-ready"];
    let steps: Vec<Value> = labels
        .iter()
        .map(|label| json!({"do":"invoke","label":label}))
        .collect();
    let mut run = Launch::start(
        &fixture,
        &Scenario::new("gh1095-after-write", Value::from(steps)),
    );
    let first = run.bun;
    let baseline = run.step();
    match arm {
        AfterWrite::ProcessStopped => {
            stop_process(first);
            run.send_step();
            run.stderr.await_count(KIPC_CALL, 2);
        }
        AfterWrite::InsideHandler => {
            run.send_step();
            run.await_where("hold inside the handler", |report| {
                report["phase"] == "recovery-handler-pending" && report["pid"] == first
            });
        }
    }
    let successor = run.crash_and_hold_successor();
    run.admit(successor);
    run.adopt_ready(2, successor);
    let staged = run.await_count("step", 2);
    let last = run.step();
    let journal = run.journal.clone();
    let stderr = run.quit();
    println!(
        "KELD_GH1095_AT13B arm={arm:?} first_bun={first} second_bun={successor} call={} first_handled={:?} successor_handled={:?} kipc_calls={}",
        outcome(&staged, call),
        handled_labels(&journal, first),
        handled_labels(&journal, successor),
        marker_count(&stderr, KIPC_CALL),
    );
    succeeded_on(&baseline, "g1-baseline", first).expect("healthy G1 call");
    rejected_with(&staged, call, OUTCOME_UNKNOWN, Some(LINK_DETAIL))
        .expect("criterion 3: a call after the dispatch point is outcome unknown");
    assert!(
        rejected_with(&staged, call, NOT_DISPATCHED, None).is_err(),
        "control: a written call is never not-dispatched"
    );
    succeeded_on(&last, "successor-ready", successor).expect("healthy successor call");
    assert_settled_once(&last, &labels);
    let mut expected_first = vec!["g1-baseline".to_owned()];
    if arm == AfterWrite::InsideHandler {
        expected_first.push(call.to_owned());
    }
    assert_eq!(handled_labels(&journal, first), expected_first);
    assert_eq!(handled_labels(&journal, successor), ["successor-ready"]);
    assert_eq!(marker_count(&stderr, KIPC_CALL), labels.len());
}

#[test]
fn gh1095_call_written_to_a_stopped_bun_is_outcome_unknown() {
    run_after_write(AfterWrite::ProcessStopped);
}

#[test]
fn gh1095_call_killed_inside_its_handler_is_outcome_unknown() {
    run_after_write(AfterWrite::InsideHandler);
}

/// Criterion 4 (AT-13c): the page reloads while its call is held inside the
/// handler; the old document's own pagehide witness reports that call as
/// `KELD-WV-013` with the coded `Error` shape, and the handler census is 1.
/// The held handler then ends and its late outcome reaches no document.
fn run_reload(pending: bool) -> (Value, Vec<String>, String) {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let mut steps = vec![json!({"do":"invoke","label":"g1-baseline"})];
    if pending {
        steps.push(json!({"do":"invoke-pending","label":"held-at-reload"}));
    }
    steps.push(json!({"do":"reload"}));
    let mut run = Launch::start(
        &fixture,
        &Scenario::new("gh1095-reload", Value::from(steps)),
    );
    let bun = run.bun;
    let baseline = run.step();
    succeeded_on(&baseline, "g1-baseline", bun).expect("healthy call before the reload");
    if pending {
        run.step();
        run.await_where("hold inside the handler", |report| {
            report["phase"] == "recovery-handler-pending" && report["pid"] == bun
        });
    }
    run.send_step();
    let unload = run.await_count("page-unload", 1);
    if pending {
        // The handler is still held; ending it now delivers a late outcome
        // that no document may settle.
        run.observer
            .send_recovery_command(RecoveryCommand::RetirePending(bun));
    }
    let journal = run.journal.clone();
    let stderr = run.quit();
    (unload, handled_labels(&journal, bun), stderr)
}

#[test]
fn gh1095_pagehide_while_pending_is_outcome_unknown() {
    let (unload, handled, stderr) = run_reload(true);
    println!(
        "KELD_GH1095_AT13C unload={unload} handled={handled:?} kipc_calls={}",
        marker_count(&stderr, KIPC_CALL)
    );
    let held = outcome(&unload, "held-at-reload");
    assert_eq!(held["ok"], false, "{held}");
    assert_eq!(held["code"], OUTCOME_UNKNOWN, "criterion 4: {held}");
    assert_eq!(held["error"], PAGEHIDE_MESSAGE, "{held}");
    assert_eq!(held["name"], "Error");
    assert!(
        terminals(&unload, "held-at-reload").is_empty(),
        "the old document had no host result for the held call"
    );
    // Census 1: the call reached the handler, so the page cannot know it
    // was not run.
    assert_eq!(handled, ["g1-baseline", "held-at-reload"]);
    assert_eq!(marker_count(&stderr, KIPC_CALL), 2);
}

/// Control for criterion 4: a reload with no call pending reports no
/// rejection, so the pagehide oracle does not fabricate one.
#[test]
fn gh1095_pagehide_with_no_call_pending_rejects_nothing() {
    let (unload, handled, _stderr) = run_reload(false);
    let outcomes = unload["outcomes"].as_array().expect("outcomes");
    assert_eq!(outcomes.len(), 1, "{unload}");
    assert_eq!(outcomes[0]["ok"], true, "{unload}");
    assert_eq!(handled, ["g1-baseline"]);
}

/// Criteria 7 and 12 (AT-S arm d, spec r2): the call is held inside the
/// handler while the page makes a main-frame navigation that commits a real
/// successor document. The old document settles the call exactly once, on
/// pagehide, as `KELD-WV-013` (its own beacon carries its own nonce); the
/// handler census is 1; the released handler's late outcome reaches no
/// document, so the successor records only its own call. With `navigate`
/// false (the control) the same held call settles once with its real reply.
fn run_navigation(navigate: bool) -> (Value, Value, Vec<String>, u32) {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let mut steps = vec![
        json!({"do":"invoke","label":"g1-baseline"}),
        json!({"do":"invoke-pending","label":"held-at-navigation"}),
    ];
    steps.push(if navigate {
        json!({"do":"navigate"})
    } else {
        json!({"do":"await-pending"})
    });
    let mut scenario = Scenario::new("gh1095-navigation", Value::from(steps));
    scenario.successor_steps = json!([{"do":"invoke","label":"successor-call"}]);
    let mut run = Launch::start(&fixture, &scenario);
    let bun = run.bun;
    let baseline = run.step();
    succeeded_on(&baseline, "g1-baseline", bun).expect("healthy call before the navigation");
    let started = run.step();
    run.await_where("hold inside the handler", |report| {
        report["phase"] == "recovery-handler-pending" && report["pid"] == bun
    });
    let (first, second) = if navigate {
        run.send_step();
        let unload = run.await_count("page-unload", 1);
        let successor_ready = run.await_count("page-ready", 2);
        assert_ne!(
            successor_ready["documentNonce"], unload["documentNonce"],
            "the navigation committed a new document"
        );
        assert_eq!(unload["documentNonce"], started["documentNonce"]);
        // Release the held handler. Its outcome reaches the host dispatch
        // thread, which serves the successor's next call only after it.
        run.observer
            .send_recovery_command(RecoveryCommand::RetirePending(bun));
        run.await_where("released handler", |report| {
            report["phase"] == "handler-released" && report["pid"] == bun
        });
        // The old document never reported its navigate step, so wait on the
        // successor's own step by label.
        run.send_step();
        let successor_call = run.await_where("successor step", |report| {
            report["phase"] == "step" && report["label"] == "successor-call"
        });
        (unload, successor_call)
    } else {
        run.observer
            .send_recovery_command(RecoveryCommand::RetirePending(bun));
        let settled = run.step();
        (settled, Value::Null)
    };
    let journal = run.journal.clone();
    let _stderr = run.quit();
    (first, second, handled_labels(&journal, bun), bun)
}

#[test]
fn gh1095_committing_navigation_settles_the_old_call_once_on_pagehide() {
    let (unload, successor, handled, bun) = run_navigation(true);
    println!(
        "KELD_GH1095_ATSD unload_outcomes={} successor={} handled={handled:?}",
        unload["outcomes"], successor["outcomes"]
    );
    let held = outcome(&unload, "held-at-navigation");
    assert_eq!(held["ok"], false, "{held}");
    assert_eq!(held["code"], OUTCOME_UNKNOWN, "{held}");
    assert_eq!(held["error"], PAGEHIDE_MESSAGE, "{held}");
    assert_eq!(
        unload["outcomes"].as_array().expect("outcomes").len(),
        2,
        "baseline and the one pagehide settlement: {unload}"
    );
    // The successor settled only its own call: the late outcome of the held
    // call reached no document.
    succeeded_on(&successor, "successor-call", bun).expect("the successor serves");
    assert_eq!(
        successor["outcomes"].as_array().expect("outcomes").len(),
        1,
        "{successor}"
    );
    assert_eq!(
        successor["terminals"].as_array().expect("terminals").len(),
        1,
        "{successor}"
    );
    // Census 1 for the held call, then the successor's own call.
    assert_eq!(
        handled,
        ["g1-baseline", "held-at-navigation", "successor-call"]
    );
}

/// Control for AT-S arm d: no navigation. The same held call settles once
/// with its real reply, so the arm's KELD-WV-013 comes from the navigation.
#[test]
fn gh1095_without_navigation_the_held_call_settles_with_its_reply() {
    let (settled, _, handled, bun) = run_navigation(false);
    succeeded_on(&settled, "held-at-navigation", bun).expect("the real reply settles it");
    assert_eq!(terminals(&settled, "held-at-navigation").len(), 1);
    assert_eq!(handled, ["g1-baseline", "held-at-navigation"]);
}

/// Criterion 8 (AT-12c): after `KELD-WV-012` in the restart gap, the app's one
/// resend is a new invoke made from a later real OS click. The test waits for
/// the successor's Ready only as its own precondition; the page sees no
/// signal. The resend runs exactly once, on the successor; G1 never ran the
/// call; a second click resends nothing; Keld never replays the call.
#[test]
fn gh1095_one_resend_after_not_dispatched_runs_once_on_a_user_click() {
    let (first_click, second_click, census) = run_resend(true);
    let ResendCensus {
        first,
        successor,
        first_handled,
        successor_handled,
        kipc_calls,
        renderer_admits,
    } = census;
    println!(
        "KELD_GH1095_AT12C first_bun={first} second_bun={successor} first_click={first_click} second_click_resent={} first_handled={first_handled:?} successor_handled={successor_handled:?} kipc_calls={kipc_calls} renderer_admits={renderer_admits}",
        second_click["resent"],
    );
    assert_eq!(first_click["resent"], true);
    succeeded_on(&first_click, "gap-resend", successor)
        .expect("criterion 8: the one resend runs on the successor");
    assert_ne!(
        request_nonce(&first_click, "gap"),
        request_nonce(&first_click, "gap-resend"),
        "the resend is a new invoke, not a replay"
    );
    assert_eq!(second_click["resent"], false, "at most one resend");
    assert_eq!(
        second_click["requests"].as_array().expect("requests").len(),
        3,
        "the second click made no call"
    );
    // Census for the logical action: exactly 1, on the successor.
    assert_eq!(first_handled, ["g1-baseline"]);
    assert_eq!(successor_handled, ["gap-resend"]);
    assert_eq!(kipc_calls, 2, "baseline and the one resend crossed kipc");
    assert_eq!(renderer_admits, 3);
}

/// Control for criterion 8: the same click before the successor is admitted
/// gets `KELD-WV-012` again; the page surfaces it, and a later click after the
/// successor is ready resends nothing. No generation ever runs the call.
#[test]
fn gh1095_resend_before_the_successor_is_admitted_is_not_dispatched_again() {
    let (first_click, second_click, census) = run_resend(false);
    println!(
        "KELD_GH1095_AT12C_CONTROL first_click={first_click} second_click_resent={} first_handled={:?} successor_handled={:?} kipc_calls={}",
        second_click["resent"], census.first_handled, census.successor_handled, census.kipc_calls,
    );
    assert_eq!(first_click["resent"], true);
    rejected_with(
        &first_click,
        "gap-resend",
        NOT_DISPATCHED,
        Some(NO_GENERATION_DETAIL),
    )
    .expect("a resend with no admitted generation is not dispatched either");
    assert_eq!(second_click["resent"], false, "no second resend");
    assert_eq!(census.first_handled, ["g1-baseline"]);
    assert!(census.successor_handled.is_empty(), "the call never ran");
    assert_eq!(census.kipc_calls, 1, "only the baseline crossed kipc");
}

struct ResendCensus {
    first: u32,
    successor: u32,
    first_handled: Vec<String>,
    successor_handled: Vec<String>,
    kipc_calls: usize,
    renderer_admits: usize,
}

/// One restart-gap call, then two real clicks. `ready_first` admits the
/// successor and waits for its Ready before the first click; otherwise the
/// first click lands in the gap and the second after Ready.
fn run_resend(ready_first: bool) -> (Value, Value, ResendCensus) {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let steps = json!([
        {"do":"invoke","label":"g1-baseline"},
        {"do":"invoke","label":"gap"},
        {"do":"arm-resend","label":"gap"},
    ]);
    let mut run = Launch::start(&fixture, &Scenario::new("gh1095-resend", steps));
    let first = run.bun;
    let baseline = run.step();
    succeeded_on(&baseline, "g1-baseline", first).expect("healthy G1 call");
    let successor = run.crash_and_hold_successor();
    let gap = run.step();
    rejected_with(&gap, "gap", NOT_DISPATCHED, Some(NO_GENERATION_DETAIL))
        .expect("the gap call is not dispatched");
    run.step();
    if ready_first {
        run.admit(successor);
        run.adopt_ready(2, successor);
    }
    let first_click = run.click(1);
    if !ready_first {
        run.admit(successor);
        run.adopt_ready(2, successor);
    }
    let second_click = run.click(2);
    let journal = run.journal.clone();
    let stderr = run.quit();
    let census = ResendCensus {
        first,
        successor,
        first_handled: handled_labels(&journal, first),
        successor_handled: handled_labels(&journal, successor),
        kipc_calls: marker_count(&stderr, KIPC_CALL),
        renderer_admits: marker_count(&stderr, RENDERER_ADMIT),
    };
    (first_click, second_click, census)
}

/// AT-12d's premise, checked on the shipping host. Spec gh1095 §7 asks for a
/// page call before the first generation is admitted, with the page already
/// loaded. While G1 holds before its first lifecycle call, so no HELLO has
/// been sent and nothing is admitted, the host owns no on-screen window and
/// no page has reported: the window is created only after the authenticated
/// HELLO (architecture 01 §5; `run_app` awaits the bound generation before it
/// creates the `WebKit` engine and the bridge window). No renderer call can
/// exist in the startup gap, so criterion 1's startup clause is carried by the
/// restart gap (AT-12a, AT-12c). This is the test that must fail before an
/// AT-12d arm is needed. Control: after admission the same census sees one
/// window, the page binds and its first call is served.
#[test]
fn gh1095_no_renderer_exists_before_the_first_admission() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let mut scenario = Scenario::new("gh1095-startup", json!([{"do":"invoke","label":"first"}]));
    scenario.admit_first = false;
    let mut run = Launch::start(&fixture, &scenario);
    let bun = run.boots[0].pid;
    // Snapshots at the hold, not waits. The boot report is the positive
    // event: a host that presented its window before spawning Bun would have
    // it on screen before Bun could report.
    let windows_at_hold = native_windows(run.host_pid, TITLE);
    let pages_at_hold = run
        .journal
        .iter()
        .filter(|report| report["phase"] == "page-ready")
        .count();
    run.admit(bun);
    run.adopt_ready(1, bun);
    run.await_presentation();
    let first = run.step();
    let journal = run.journal.clone();
    let stderr = run.quit();
    println!(
        "KELD_GH1095_AT12D_PREMISE bun={bun} windows_at_hold={windows_at_hold:?} pages_at_hold={pages_at_hold} first={} handled={:?} renderer_admits={} kipc_calls={}",
        outcome(&first, "first"),
        handled_labels(&journal, bun),
        marker_count(&stderr, RENDERER_ADMIT),
        marker_count(&stderr, KIPC_CALL),
    );
    assert!(
        windows_at_hold.is_empty(),
        "a window existed before the first authenticated HELLO: the startup gap is reachable from a page and AT-12d needs its arm"
    );
    assert_eq!(
        pages_at_hold, 0,
        "a page reported before the first admission"
    );
    succeeded_on(&first, "first", bun).expect("the first call after admission is served");
    assert!(
        rejected_with(&first, "first", NOT_DISPATCHED, None).is_err(),
        "control: a served call is not not-dispatched"
    );
    assert_settled_once(&first, &["first"]);
    assert_eq!(handled_labels(&journal, bun), ["first"]);
    assert_eq!(marker_count(&stderr, RENDERER_ADMIT), 1);
    assert_eq!(marker_count(&stderr, KIPC_CALL), 1);
}

struct ReturningRun {
    settled: Value,
    last: Value,
    first: u32,
    successor: Option<u32>,
    first_handled: Vec<String>,
    successor_handled: Vec<String>,
    kipc_calls: usize,
}

/// One call whose handler reports `returning` as the last thing it does.
/// With `kill`, the test SIGKILLs Bun on that report, so the kill races the
/// reply the host is about to read (S15); the successor is then admitted and
/// serves a call. Without it (the control), the same report precedes a plain
/// reply.
fn run_returning(kill: bool) -> ReturningRun {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let mut labels = vec!["g1-baseline", "returning"];
    if kill {
        labels.push("successor-ready");
    }
    let steps: Vec<Value> = labels
        .iter()
        .map(|label| json!({"do":"invoke","label":label}))
        .collect();
    let mut run = Launch::start(
        &fixture,
        &Scenario::new("gh1095-returning", Value::from(steps)),
    );
    let first = run.bun;
    let baseline = run.step();
    succeeded_on(&baseline, "g1-baseline", first).expect("healthy G1 call");
    run.send_step();
    run.await_where("the handler's returning report", |report| {
        report["phase"] == "returning" && report["pid"] == first
    });
    let mut successor = None;
    if kill {
        kill_pid(first);
        let next = run.hold_successor(first);
        run.admit(next);
        run.adopt_ready(2, next);
        successor = Some(next);
    }
    let settled = run.await_count("step", 2);
    let last = if kill { run.step() } else { settled.clone() };
    let journal = run.journal.clone();
    let stderr = run.quit();
    ReturningRun {
        settled,
        last,
        first,
        successor,
        first_handled: handled_labels(&journal, first),
        successor_handled: successor
            .map(|next| handled_labels(&journal, next))
            .unwrap_or_default(),
        kipc_calls: marker_count(&stderr, KIPC_CALL),
    }
}

/// Criterion 7 at S15 (AT-13d): Bun is killed as its reply leaves. The page
/// gets the reply or `KELD-WV-013`, never `KELD-WV-012`; exactly one
/// terminal; the successor never runs the call. Which of the two outcomes
/// occurred is printed, not asserted (spec §7). The signal goes through
/// `/bin/kill`, a real process, so it lands a few milliseconds after the
/// report; on this Mac the reply has left by then in every sample, and the
/// arm then proves exactly once for a reply followed by the link's end. The
/// outcome-unknown branch of the same retirement is pinned by AT-13b.
#[test]
fn gh1095_bun_killed_while_returning_settles_once_as_reply_or_outcome_unknown() {
    let run = run_returning(true);
    let successor = run.successor.expect("a successor was admitted");
    let result = outcome(&run.settled, "returning");
    let observed = if result["ok"] == true {
        succeeded_on(&run.settled, "returning", run.first).expect("the reply left before the kill");
        "reply"
    } else {
        rejected_with(
            &run.settled,
            "returning",
            OUTCOME_UNKNOWN,
            Some(LINK_DETAIL),
        )
        .expect("criterion 7 (S15): a reply in flight at the kill is outcome unknown");
        "outcome-unknown"
    };
    println!(
        "KELD_GH1095_AT13D outcome={observed} first_bun={} second_bun={successor} returning={result} first_handled={:?} successor_handled={:?} kipc_calls={}",
        run.first, run.first_handled, run.successor_handled, run.kipc_calls,
    );
    assert!(
        rejected_with(&run.settled, "returning", NOT_DISPATCHED, None).is_err(),
        "a written call is never not-dispatched"
    );
    assert_eq!(
        terminals(&run.settled, "returning").len(),
        1,
        "exactly one terminal"
    );
    succeeded_on(&run.last, "successor-ready", successor).expect("healthy successor call");
    assert_settled_once(&run.last, &["g1-baseline", "returning", "successor-ready"]);
    // Census 1 on G1, which was returning; 0 on the successor: no replay.
    assert_eq!(run.first_handled, ["g1-baseline", "returning"]);
    assert_eq!(run.successor_handled, ["successor-ready"]);
    assert_eq!(run.kipc_calls, 3);
}

/// Control for AT-13d: the same `returning` report with the kill disabled
/// precedes a plain reply, so an outcome-unknown result comes from the kill,
/// not from the harness.
#[test]
fn gh1095_returning_report_without_a_kill_precedes_the_reply() {
    let run = run_returning(false);
    succeeded_on(&run.settled, "returning", run.first).expect("a plain reply");
    assert_eq!(terminals(&run.settled, "returning").len(), 1);
    assert_settled_once(&run.settled, &["g1-baseline", "returning"]);
    assert_eq!(run.first_handled, ["g1-baseline", "returning"]);
    assert_eq!(run.kipc_calls, 2);
}

struct LateRun {
    late: Value,
    first_click: Value,
    second_click: Value,
    first: u32,
    successor: u32,
    first_handled: Vec<String>,
    successor_handled: Vec<String>,
    kipc_calls: usize,
    renderer_admits: usize,
    registrations: usize,
}

/// A baseline call on G1, then G1 is killed and its successor admitted. With
/// `Registration::InsideReady` the successor holds inside `whenReady` with no
/// handler while the page's next call reaches it; the test releases the hold
/// only after that call settled, then drives two real clicks. With
/// `Registration::BeforeReady` (the control) the handler exists before the
/// link opens, so the same call is served.
fn run_late_registration(registration: Registration) -> LateRun {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let steps = json!([
        {"do":"invoke","label":"g1-baseline"},
        {"do":"invoke","label":"late"},
        {"do":"arm-resend","label":"late"},
    ]);
    let mut scenario = Scenario::new("gh1095-late-registration", steps);
    scenario.registration = registration;
    let mut run = Launch::start(&fixture, &scenario);
    let first = run.bun;
    let inside = registration == Registration::InsideReady;
    if inside {
        run.release_registration(first);
    }
    let baseline = run.step();
    succeeded_on(&baseline, "g1-baseline", first).expect("healthy G1 call");
    let successor = run.crash_and_hold_successor();
    run.admit(successor);
    run.adopt_ready(2, successor);
    if inside {
        // Test-owned precondition: the successor is admitted and holds
        // before registering, so the next call finds no handler.
        run.await_registration_hold(successor);
    }
    let late = run.step();
    run.step();
    if inside {
        run.release_registration(successor);
    }
    let first_click = run.click(1);
    let second_click = run.click(2);
    let journal = run.journal.clone();
    let stderr = run.quit();
    LateRun {
        late,
        first_click,
        second_click,
        first,
        successor,
        first_handled: handled_labels(&journal, first),
        successor_handled: handled_labels(&journal, successor),
        kipc_calls: marker_count(&stderr, KIPC_CALL),
        renderer_admits: marker_count(&stderr, RENDERER_ADMIT),
        registrations: journal
            .iter()
            .filter(|report| report["phase"] == "handler-registered")
            .count(),
    }
}

/// Criteria 5 and 8 across a restart (AT-API2b): a handler registered inside
/// `whenReady` leaves a window after the restart in which the page's call
/// reaches the successor, runs no app code and gets `KELD-API-002` with the
/// coded `Error` shape; the host passes Bun's code through unchanged. Once the
/// handler exists, one real OS click makes the one resend, which runs once on
/// the successor; a second click resends nothing.
#[test]
fn gh1095_handler_registered_inside_when_ready_answers_api_002_after_restart() {
    let run = run_late_registration(Registration::InsideReady);
    println!(
        "KELD_GH1095_ATAPI2B first_bun={} second_bun={} late={} first_click_resent={} second_click_resent={} first_handled={:?} successor_handled={:?} kipc_calls={} renderer_admits={} registrations={}",
        run.first,
        run.successor,
        outcome(&run.late, "late"),
        run.first_click["resent"],
        run.second_click["resent"],
        run.first_handled,
        run.successor_handled,
        run.kipc_calls,
        run.renderer_admits,
        run.registrations,
    );
    rejected_with(&run.late, "late", NO_HANDLER, Some(NO_HANDLER_DETAIL))
        .expect("criterion 5: no handler, so no application code ran");
    assert!(
        rejected_with(&run.late, "late", NOT_DISPATCHED, None).is_err(),
        "control: the call was written, so it is not not-dispatched"
    );
    assert!(
        rejected_with(&run.late, "late", OUTCOME_UNKNOWN, None).is_err(),
        "control: Bun's own API code is forwarded, not rewritten"
    );
    assert_eq!(run.first_click["resent"], true);
    succeeded_on(&run.first_click, "late-resend", run.successor)
        .expect("criterion 8: the one resend runs once the handler exists");
    assert_ne!(
        request_nonce(&run.first_click, "late"),
        request_nonce(&run.first_click, "late-resend"),
        "the resend is a new invoke, not a replay"
    );
    assert_eq!(run.second_click["resent"], false, "at most one resend");
    assert_settled_once(&run.second_click, &["g1-baseline", "late", "late-resend"]);
    // Census: 0 for the call that found no handler, 1 for the logical action.
    assert_eq!(run.first_handled, ["g1-baseline"]);
    assert_eq!(run.successor_handled, ["late-resend"]);
    assert_eq!(
        run.kipc_calls, 3,
        "every call crossed kipc, including the one Bun refused"
    );
    assert_eq!(run.renderer_admits, 3);
    assert_eq!(run.registrations, 2, "both generations registered late");
}

/// Control for AT-API2b: a handler registered before `whenReady` exists
/// before the link opens, so the same call after the restart is served and no
/// `KELD-API-002` occurs; a click after a served call resends nothing.
#[test]
fn gh1095_handler_registered_before_when_ready_is_served_after_restart() {
    let run = run_late_registration(Registration::BeforeReady);
    succeeded_on(&run.late, "late", run.successor).expect("the successor serves the call");
    assert!(
        rejected_with(&run.late, "late", NO_HANDLER, None).is_err(),
        "no KELD-API-002 with the handler registered first"
    );
    assert_eq!(run.first_click["resent"], false);
    assert_eq!(run.second_click["resent"], false);
    assert_settled_once(&run.second_click, &["g1-baseline", "late"]);
    assert_eq!(run.first_handled, ["g1-baseline"]);
    assert_eq!(run.successor_handled, ["late"]);
    assert_eq!(run.kipc_calls, 2);
    assert_eq!(run.registrations, 0);
}
