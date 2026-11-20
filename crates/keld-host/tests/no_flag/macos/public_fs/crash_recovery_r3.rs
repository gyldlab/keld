//! GH-1081 round-3 experiments (research only; never merged; draft PR #1082).
//! The exact code the app page gets today for its own Echo call when Bun dies
//! at each request stage, and when the Bun handler is missing, registered late
//! or throws. Same staged no-flag host, page and observer as round 1; the app
//! body is `crash_recovery_r3_app.ts`. No production hook: every hold is an
//! ordinary fixture HTTP wait or a block in the app's own code, every kill is a
//! real signal, and every wait is on a journal report, a host stderr marker,
//! a file the app wrote, or a kernel process state (app stderr is not live on
//! macOS; see `StderrTap`).
use super::*;
use crate::support::process::{await_process_state, process_state};

const LINK_DETAIL: &str = "application link ended before the renderer reply";
const API_CODE: &str = "KELD-API-001";
const KIPC_CALL: &str = "KELD_KEL142_KIPC_CALL ";
const MAIN_BLOCKED: &str = "KELD_GH1081_R3_MAIN_BLOCKED ";
const AFTER_HANDLER: &str = "KELD_GH1081_R3_AFTER_HANDLER ";
/// Grace for the negative control's absence check: a live relay would show
/// the app's line within this after the page already saw the reply that the
/// app wrote the line before.
const LIVE_GRACE: Duration = Duration::from_secs(3);

/// The s4 getter's live record: a file under the fixture's own temp root. Bun
/// is not OS-sandboxed on macOS (`keld-runtime` `macos_guardian.rs`:
/// supervisor cleanup, not App Sandbox containment), and its stderr is not
/// live (see `StderrTap`).
fn after_handler_marker(fixture: &PublicFsProject) -> PathBuf {
    fixture.owned_root().join("gh1081-r3-after-handler.json")
}

impl Experiment {
    /// Launch with the round-3 app body.
    fn launch_r3(
        fixture: &PublicFsProject,
        name: &str,
        mode: HandlerMode,
        main_block: bool,
        labels: &[&str],
        release_inside_ready: bool,
    ) -> Self {
        let steps: Vec<Value> = labels
            .iter()
            .map(|label| json!({"do":"invoke","label":label}))
            .collect();
        let marker_path = serde_json::to_string(&after_handler_marker(fixture).to_string_lossy())
            .expect("marker path JSON");
        let app = |port: &str| {
            include_str!("../../../fixtures/crash_recovery_r3_app.ts")
                .replace("__PORT__", port)
                .replace("__HANDLER_MODE__", mode.literal())
                .replace("__MAIN_BLOCK__", if main_block { "true" } else { "false" })
                .replace("__AFTER_HANDLER_MARKER__", &marker_path)
        };
        Self::launch_with(
            fixture,
            name,
            app,
            mode,
            &Value::from(steps),
            &json!([]),
            release_inside_ready,
        )
    }

    /// Release the page's next invoke without waiting for its report.
    fn send_step(&mut self) {
        self.observer
            .send_recovery_command(RecoveryCommand::Invoke(RecoveryPhase::Before));
        self.steps += 1;
    }
}

/// One page call's observed terminal, for the printed record.
fn terminal_of(report: &Value, label: &str) -> Value {
    json!({
        "label": label,
        "outcome": outcome(report, label),
        "terminals": terminals(report, label),
    })
}

/// The handler records (from the HTTP journal) for one generation, by label.
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

fn phase_count(journal: &[Value], phase: &str, bun: u32) -> usize {
    journal
        .iter()
        .filter(|report| report["phase"] == phase && report["pid"] == bun)
        .count()
}

/// Every call settled exactly once and the page saw every call settle.
fn assert_settled_once(last: &Value, labels: &[&str]) {
    let requests = last["requests"].as_array().expect("requests").len();
    assert_eq!(requests, labels.len());
    assert_eq!(
        last["outcomes"].as_array().expect("outcomes").len(),
        requests
    );
    for label in labels {
        assert_eq!(terminals(last, label).len(), 1, "one terminal for {label}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrashStage {
    /// G1 killed, successor held before admission: no serving generation.
    BeforeSend,
    /// Whole Bun process stopped, then the host writes the CALL, then SIGKILL.
    AfterSendProcessStopped,
    /// Bun main thread parked, Worker running, then the CALL, then SIGKILL.
    AfterSendMainBlocked,
    /// SIGKILL while the handler waits inside its own body.
    DuringHandler,
    /// SIGKILL after the handler returned, while Keld encodes the reply.
    AfterHandlerBeforeReply,
}

impl CrashStage {
    fn label(self) -> &'static str {
        match self {
            Self::BeforeSend => "s1-before-send",
            Self::AfterSendProcessStopped => "s2a-after-send-stopped",
            Self::AfterSendMainBlocked => "s2b-after-send-main-blocked",
            Self::DuringHandler => "s3-during-handler",
            Self::AfterHandlerBeforeReply => "s4-after-handler",
        }
    }

    /// The handler census this stage must show for its own call on G1.
    fn handled(self) -> bool {
        matches!(self, Self::DuringHandler | Self::AfterHandlerBeforeReply)
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

/// One host launch per stage: a healthy G1 call, the stage call with the kill
/// placed at that stage, then a healthy call on the successor.
fn run_crash_stage(stage: CrashStage) {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let labels = ["g1-baseline", stage.label(), "successor-ready"];
    let mut run = Experiment::launch_r3(
        &fixture,
        "gh1081-r3",
        HandlerMode::BeforeReady,
        stage == CrashStage::AfterSendMainBlocked,
        &labels,
        true,
    );
    let first = run.bun;
    let baseline = run.step();
    let mut evidence = Vec::new();
    let successor = crash_at(
        &mut run,
        stage,
        &after_handler_marker(&fixture),
        &mut evidence,
    );
    run.admit(successor);
    run.adopt_ready(2, successor);
    let staged = run.await_count("step", 2);
    let last = run.step();
    let journal = run.journal.clone();
    let output = run.quit();
    let stderr = String::from_utf8_lossy(&output.stderr);

    let label = stage.label();
    let expected_detail = if stage == CrashStage::BeforeSend {
        GAP_DETAIL
    } else {
        LINK_DETAIL
    };
    let verdict = rejected_with(&staged, label, "KELD-WV-011", Some(expected_detail));
    let first_handled = handled_labels(&journal, first);
    let successor_handled = handled_labels(&journal, successor);
    let kipc_calls = marker_count(&stderr, KIPC_CALL);
    // Negative controls: the stage oracle refuses both healthy calls, and the
    // other WV-011 detail refuses the stage call.
    let other_detail = if stage == CrashStage::BeforeSend {
        LINK_DETAIL
    } else {
        GAP_DETAIL
    };
    let nc_baseline = rejected_with(
        &baseline,
        "g1-baseline",
        "KELD-WV-011",
        Some(expected_detail),
    );
    let nc_successor = rejected_with(
        &last,
        "successor-ready",
        "KELD-WV-011",
        Some(expected_detail),
    );
    let nc_other_detail = rejected_with(&staged, label, "KELD-WV-011", Some(other_detail));
    // App stderr markers: never live (see `StderrTap`), counted from the
    // transcript the guardian printed at exit.
    let main_blocked = marker_count(&stderr, MAIN_BLOCKED);
    let after_handler = marker_count(&stderr, AFTER_HANDLER);
    println!(
        "KELD_GH1081_R3 stage={stage:?} first_bun={first} second_bun={successor} call={} first_handled={first_handled:?} successor_handled={successor_handled:?} returning={} kipc_calls={kipc_calls} renderer_admits={} main_blocked={main_blocked} after_handler={after_handler} verdict={verdict:?} nc_baseline={nc_baseline:?} nc_successor={nc_successor:?} nc_other_detail={nc_other_detail:?} evidence={}",
        terminal_of(&staged, label),
        phase_count(&journal, "handler-returning", first),
        marker_count(&stderr, "KELD_KEL142_RENDERER_ADMIT "),
        Value::from(evidence),
    );
    succeeded_on(&baseline, "g1-baseline", first).expect("healthy G1 call");
    succeeded_on(&last, "successor-ready", successor).expect("healthy successor call");
    verdict.expect("stage call code and detail");
    assert!(nc_baseline.is_err() && nc_successor.is_err() && nc_other_detail.is_err());
    assert_settled_once(&last, &labels);
    // Census: the stage call ran on G1 only at the handler stages, and the
    // successor never ran it (no replay).
    let mut expected_first = vec!["g1-baseline".to_owned()];
    if stage.handled() {
        expected_first.push(label.to_owned());
    }
    assert_eq!(first_handled, expected_first);
    assert_eq!(successor_handled, ["successor-ready"]);
    assert_eq!(
        phase_count(&journal, "handler-returning", first),
        usize::from(stage == CrashStage::AfterHandlerBeforeReply)
    );
    assert_stage_markers(stage, main_blocked, after_handler);
    // Host census: only the before-send call never crossed the dispatch point.
    let expected_kipc = if stage == CrashStage::BeforeSend {
        2
    } else {
        3
    };
    assert_eq!(kipc_calls, expected_kipc);
    assert_eq!(marker_count(&stderr, "KELD_KEL142_RENDERER_ADMIT "), 3);
}

/// Oracle for the app's stage-point stderr markers (post-quit counts; never
/// live, see `StderrTap`): the app reached its point at its own stage only.
fn assert_stage_markers(stage: CrashStage, main_blocked: usize, after_handler: usize) {
    assert_eq!(
        main_blocked,
        usize::from(stage == CrashStage::AfterSendMainBlocked)
    );
    assert_eq!(
        after_handler,
        usize::from(stage == CrashStage::AfterHandlerBeforeReply)
    );
}

/// Places the SIGKILL of G1 at `stage` and returns the held successor.
fn crash_at(
    run: &mut Experiment,
    stage: CrashStage,
    after_handler_marker: &Path,
    evidence: &mut Vec<Value>,
) -> u32 {
    let first = run.bun;
    match stage {
        CrashStage::BeforeSend => {
            let (successor, _, _) = run.crash_and_hold_successor();
            let _ = run.step();
            successor
        }
        CrashStage::AfterSendProcessStopped | CrashStage::AfterSendMainBlocked => {
            if stage == CrashStage::AfterSendMainBlocked {
                let hold = run.await_where("main-block hold", |report| {
                    report["phase"] == "recovery-handler-pending" && report["pid"] == first
                });
                evidence.push(json!({"main_block_hold": hold}));
                run.observer
                    .send_recovery_command(RecoveryCommand::RetirePending(first));
                // Round-3 fix: the app reports through the journal and then
                // parks; its stderr marker is not live (see `StderrTap`).
                let blocking = run.await_where("main-blocking report", |report| {
                    report["phase"] == "main-blocking" && report["pid"] == first
                });
                evidence.push(json!({"main_blocking": blocking}));
            } else {
                stop_process(first);
                evidence.push(json!({"bun_state_before_call": process_state(first)}));
            }
            run.send_step();
            // The host's own marker, printed right after write_frame returned Ok
            // for this CALL: the dispatch point.
            let dispatched = run.stderr.await_count(KIPC_CALL, 2);
            evidence.push(json!({"kipc_call_markers": dispatched}));
            let (successor, _, _) = run.crash_and_hold_successor();
            successor
        }
        CrashStage::DuringHandler => {
            run.send_step();
            let held = run.await_where("hold inside the handler", |report| {
                report["phase"] == "recovery-handler-pending" && report["pid"] == first
            });
            evidence.push(json!({"handler_hold": held}));
            let (successor, _, _) = run.crash_and_hold_successor();
            successor
        }
        CrashStage::AfterHandlerBeforeReply => {
            run.send_step();
            let returned = run.await_where("handler returning", |report| {
                report["phase"] == "handler-returning" && report["pid"] == first
            });
            // Round-3 fix: the getter runs synchronously on the parking main
            // thread, so its live record is the file it renamed into place.
            let encoding = await_file(after_handler_marker);
            evidence.push(json!({"handler_returning": returned, "encoder_marker": encoding}));
            let (successor, _, _) = run.crash_and_hold_successor();
            successor
        }
    }
}

#[test]
fn gh1081_r3_crash_s1_before_send() {
    run_crash_stage(CrashStage::BeforeSend);
}

#[test]
fn gh1081_r3_crash_s2a_after_send_process_stopped() {
    run_crash_stage(CrashStage::AfterSendProcessStopped);
}

#[test]
fn gh1081_r3_crash_s2b_after_send_main_thread_blocked() {
    run_crash_stage(CrashStage::AfterSendMainBlocked);
}

#[test]
fn gh1081_r3_crash_s3_during_handler() {
    run_crash_stage(CrashStage::DuringHandler);
}

#[test]
fn gh1081_r3_crash_s4_after_handler_before_reply() {
    run_crash_stage(CrashStage::AfterHandlerBeforeReply);
}

/// The JSON the app renamed into place at `path`; the deadline is a kill
/// switch and the poll follows `await_process_state`.
fn await_file(path: &Path) -> Value {
    let deadline = Instant::now() + EVENT_DEADLINE;
    loop {
        if let Ok(bytes) = fs::read(path) {
            return serde_json::from_slice(&bytes).expect("app file marker JSON");
        }
        assert!(
            Instant::now() < deadline,
            "gh1081 app file marker {} before deadline",
            path.display()
        );
        thread::yield_now();
    }
}

/// Negative control for the round-3 synchronization points: an app-emitted
/// stderr marker is not observable live on macOS, a host-emitted one is, and
/// the same app marker is countable after quit. Falsifier: the app's
/// `KELD_GH1081_HANDLER` line arriving within `LIVE_GRACE` after the page
/// already saw the reply that the handler wrote the line before.
#[test]
fn gh1081_r3_nc_app_stderr_marker_not_live() {
    let labels = ["nc-stderr-live"];
    let fixture = PublicFsProject::new(Policy::Narrow);
    let mut run = Experiment::launch_r3(
        &fixture,
        "gh1081-r3-nc",
        HandlerMode::BeforeReady,
        false,
        &labels,
        true,
    );
    let bun = run.bun;
    let report = run.step();
    let host_live = run.stderr.await_count(KIPC_CALL, 1);
    let app_live = run
        .stderr
        .try_await_count("KELD_GH1081_HANDLER ", 1, LIVE_GRACE);
    let output = run.quit();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let after_quit = marker_count(&stderr, "KELD_GH1081_HANDLER ");
    println!(
        "KELD_GH1081_R3 arm=nc-stderr-live host_live={} app_live={app_live:?} after_quit={after_quit}",
        host_live.len(),
    );
    succeeded_on(&report, labels[0], bun).expect("healthy call");
    assert_eq!(host_live.len(), 1, "a host marker is live");
    assert!(
        app_live.is_err(),
        "an app stderr marker arrived live: {app_live:?}"
    );
    assert_eq!(
        after_quit, 1,
        "the app marker is in the transcript printed at exit"
    );
}

/// One launch with page calls and no crash.
struct LiveRun {
    last: Value,
    bun: u32,
    handled: Vec<String>,
    effects: usize,
    kipc_calls: usize,
}

fn run_live(mode: HandlerMode, labels: &[&str]) -> LiveRun {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let mut run = Experiment::launch_r3(&fixture, "gh1081-r3-live", mode, false, labels, true);
    let bun = run.bun;
    let mut last = Value::Null;
    for _ in labels {
        last = run.step();
    }
    let journal = run.journal.clone();
    let output = run.quit();
    let stderr = String::from_utf8_lossy(&output.stderr);
    LiveRun {
        last,
        bun,
        handled: handled_labels(&journal, bun),
        effects: phase_count(&journal, "effect", bun),
        kipc_calls: marker_count(&stderr, KIPC_CALL),
    }
}

/// Missing handler: never registered. NC: the same calls with a handler.
#[test]
fn gh1081_r3_handler_missing() {
    let labels = ["missing-1", "missing-2"];
    let LiveRun {
        last: missing,
        handled: missing_handled,
        kipc_calls: missing_kipc,
        ..
    } = run_live(HandlerMode::Never, &labels);
    let LiveRun {
        last: control,
        bun: control_bun,
        handled: control_handled,
        kipc_calls: control_kipc,
        ..
    } = run_live(HandlerMode::BeforeReady, &labels);
    let verdicts: Vec<_> = labels
        .iter()
        .map(|label| rejected_with(&missing, label, API_CODE, None))
        .collect();
    let nc: Vec<_> = labels
        .iter()
        .map(|label| rejected_with(&control, label, API_CODE, None))
        .collect();
    println!(
        "KELD_GH1081_R3 arm=missing calls={} handled={missing_handled:?} kipc_calls={missing_kipc} verdicts={verdicts:?} nc_handled={control_handled:?} nc_kipc_calls={control_kipc} nc_verdicts={nc:?}",
        Value::from(
            labels
                .iter()
                .map(|label| terminal_of(&missing, label))
                .collect::<Vec<_>>()
        ),
    );
    for verdict in verdicts {
        verdict.expect("missing handler call code");
    }
    assert!(missing_handled.is_empty(), "{missing_handled:?}");
    assert_eq!(missing_kipc, 2, "both calls crossed the dispatch point");
    assert_settled_once(&missing, &labels);
    // NC: a registered handler fails the missing-handler oracle.
    assert!(nc.iter().all(Result::is_err), "{nc:?}");
    for label in labels {
        succeeded_on(&control, label, control_bun).expect("NC handler reply");
    }
    assert_eq!(control_handled, labels);
    assert_settled_once(&control, &labels);
}

/// Late registration on a live G1: a call before `channels.handle`, then the
/// registration, then a call after it. NC: the later call.
#[test]
fn gh1081_r3_handler_registered_late() {
    let labels = ["late-before-registration", "late-after-registration"];
    let fixture = PublicFsProject::new(Policy::Narrow);
    let mut run = Experiment::launch_r3(
        &fixture,
        "gh1081-r3-late",
        HandlerMode::InsideReady,
        false,
        &labels,
        false,
    );
    let bun = run.bun;
    let hold = run.await_count("recovery-handler-pending", 1);
    assert_eq!(pid(&hold, "pid"), bun, "G1 holds before registration");
    let before = run.step();
    run.release_registration(1);
    let after = run.step();
    let journal = run.journal.clone();
    let output = run.quit();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let handled = handled_labels(&journal, bun);
    let verdict = rejected_with(&before, labels[0], API_CODE, None);
    let nc = rejected_with(&after, labels[1], API_CODE, None);
    println!(
        "KELD_GH1081_R3 arm=late before={} after={} handled={handled:?} kipc_calls={} verdict={verdict:?} nc={nc:?}",
        terminal_of(&before, labels[0]),
        terminal_of(&after, labels[1]),
        marker_count(&stderr, KIPC_CALL),
    );
    verdict.expect("call before registration code");
    assert!(nc.is_err(), "NC: the post-registration call is not API-001");
    succeeded_on(&after, labels[1], bun).expect("post-registration reply");
    // No replay: registration never ran the earlier call.
    assert_eq!(handled, [labels[1]]);
    assert_eq!(marker_count(&stderr, KIPC_CALL), 2);
    assert_settled_once(&after, &labels);
}

/// A handler that throws after its effect. NC: a healthy call on the same handler.
#[test]
fn gh1081_r3_handler_throws_after_effect() {
    let labels = ["throw-after-effect", "ok-after-throw"];
    let LiveRun {
        last,
        bun,
        handled,
        effects,
        kipc_calls: kipc,
    } = run_live(HandlerMode::BeforeReady, &labels);
    let verdict = rejected_with(&last, labels[0], API_CODE, None);
    let nc = rejected_with(&last, labels[1], API_CODE, None);
    println!(
        "KELD_GH1081_R3 arm=throw call={} handled={handled:?} effects={effects} kipc_calls={kipc} verdict={verdict:?} nc={nc:?}",
        terminal_of(&last, labels[0]),
    );
    verdict.expect("throwing handler call code");
    assert_eq!(effects, 1, "the handler's effect was committed");
    assert!(nc.is_err(), "NC: the healthy call is not API-001");
    succeeded_on(&last, labels[1], bun).expect("healthy call after a throw");
    assert_eq!(handled, labels);
    assert_eq!(kipc, 2);
    assert_settled_once(&last, &labels);
}
