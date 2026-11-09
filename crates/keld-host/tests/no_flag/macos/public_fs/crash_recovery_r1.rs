//! GH-1081 round-1 crash-recovery experiments. Research only; never merged.
//! E8 gap/pre-handler rejection, E4 OS-typed draft and same document, F11
//! sessionStorage, and the F4 `WebKit` process census around a real Bun kill.
//! No production hook: every hold is an ordinary fixture HTTP wait owned by the
//! existing recovery observer, and the host is the unmodified staged binary.
use super::observer::{Observation, RecoveryCommand, RecoveryPhase};
use super::process_image::process_image_path;
use super::project::{Policy, PublicFsProject, TITLE};
use super::{RecoveryBoot, pid, record_recovery_boot, spawn_shipping_host};
use crate::renderer_bridge::CLICK_SCRIPT;
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::{
    NativeWindowObserver, await_same_native_windows, native_windows,
};
use crate::support::process::{await_process_gone, kill_pid, parent_process, process_exists};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Output};
use std::time::{Duration, Instant};

const DRAFT: &str = "Kal milte hain";
const GAP_DETAIL: &str = "application call is unavailable or already pending";
const TYPE_SCRIPT: &str = include_str!("../../../fixtures/crash_recovery_r1_type.swift");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandlerMode {
    /// `channels.handle` at module top, before the first lifecycle call.
    BeforeReady,
    /// `channels.handle` inside `whenReady`, held by the test before registration.
    InsideReady,
}

impl HandlerMode {
    fn literal(self) -> &'static str {
        match self {
            Self::BeforeReady => "\"before-ready\"",
            Self::InsideReady => "\"inside-ready\"",
        }
    }
}

/// One staged no-flag host launch and its ordinary observation journal.
struct Experiment {
    observer: Observation,
    cleanup: ShippingLaunchCleanup,
    _lease: ChildStdin,
    _stage: keld_cli::boot::DevBootStage,
    bun_image: PathBuf,
    census_tool: PathBuf,
    webkit_baseline: Vec<u64>,
    host_pid: u32,
    guardian: u32,
    bun: u32,
    window: Vec<u32>,
    journal: Vec<Value>,
    boots: Vec<RecoveryBoot>,
    steps: usize,
}

impl Experiment {
    fn launch(
        fixture: &PublicFsProject,
        name: &str,
        mode: HandlerMode,
        steps: &Value,
        reload_steps: &Value,
    ) -> Self {
        let observer = Observation::bind_recovery();
        let port = observer.port.to_string();
        let app = include_str!("../../../fixtures/crash_recovery_r1_app.ts")
            .replace("__PORT__", &port)
            .replace("__HANDLER_MODE__", mode.literal());
        let page = include_str!("../../../fixtures/crash_recovery_r1_page.html")
            .replace("__PORT__", &port)
            .replace("__STEPS__", &steps.to_string())
            .replace("__RELOAD_STEPS__", &reload_steps.to_string());
        let (stage, bun_image) = fixture.stage_experiment(name, &app, &page);
        let census_tool = compile_webkit_census(fixture.owned_root());
        // Round 1: launchd responsibility for WebKit helpers is the runner's
        // session process, not the host, so attribute by first appearance.
        let webkit_baseline: Vec<u64> = webkit_rows(&census_tool)
            .iter()
            .filter_map(|row| row["pid"].as_u64())
            .collect();
        let mut presentation = NativeWindowObserver::arm_for_title(&fixture.census, TITLE);
        let mut cleanup = spawn_shipping_host(&stage);
        let host = cleanup.cli.as_mut().expect("host custody");
        let host_pid = host.id();
        let lease = host.stdin.take().expect("retain dev lease until host exit");
        let mut experiment = Self {
            observer,
            cleanup,
            _lease: lease,
            _stage: stage,
            bun_image,
            census_tool,
            webkit_baseline,
            host_pid,
            guardian: 0,
            bun: 0,
            window: Vec::new(),
            journal: Vec::new(),
            boots: Vec::new(),
            steps: 0,
        };
        let boot = experiment.await_boot(1);
        experiment.admit(boot);
        experiment.adopt_ready(1, boot);
        if mode == HandlerMode::InsideReady {
            experiment.release_registration(1);
        }
        experiment.await_count("page-ready", 1);
        experiment.window = presentation.expect_initial(host_pid, "gh1081-r1");
        assert_eq!(experiment.window.len(), 1);
        experiment
    }

    /// Receive one ordinary report. The host-minted possession token in a boot
    /// report is kept only in `boots`, never in the printed journal.
    fn receive(&mut self, deadline: Instant) {
        let received = self.try_receive(deadline);
        assert!(
            received,
            "gh1081 observation before deadline; journal={:?}",
            self.journal
        );
    }

    /// One report before `deadline`, or `false` when none arrived.
    fn try_receive(&mut self, deadline: Instant) -> bool {
        let Ok(report) = self
            .observer
            .reports
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        else {
            return false;
        };
        if report["phase"] == "recovery-app-boot" {
            let boot = record_recovery_boot(report, &mut self.journal);
            self.boots.push(boot);
        } else {
            eprintln!("KELD_GH1081_OBSERVATION {report}");
            assert_ne!(report["phase"], "error", "fixture error: {report}");
            self.journal.push(report);
        }
        true
    }

    fn await_where(&mut self, label: &str, matches: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + EVENT_DEADLINE;
        loop {
            if let Some(found) = self.journal.iter().find(|report| matches(report)) {
                return found.clone();
            }
            assert!(Instant::now() < deadline, "gh1081 {label} before deadline");
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

    fn count(&self, phase: &str) -> usize {
        self.journal
            .iter()
            .filter(|report| report["phase"] == phase)
            .count()
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
    fn adopt_ready(&mut self, count: usize, boot: u32) -> Value {
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
        ready
    }

    /// Drop the held registration wait of the `count`-th inside-ready app.
    fn release_registration(&mut self, count: usize) {
        let hold = self.await_count("recovery-handler-pending", count);
        assert_eq!(
            pid(&hold, "pid"),
            self.bun,
            "hold belongs to the current app"
        );
        self.observer
            .send_recovery_command(RecoveryCommand::RetirePending(self.bun));
        let registered = self.await_count("handler-registered", count);
        assert_eq!(pid(&registered, "pid"), self.bun);
    }

    /// Release the page's one long-poll wait and collect its next step report.
    fn step(&mut self) -> Value {
        self.observer
            .send_recovery_command(RecoveryCommand::Invoke(RecoveryPhase::Before));
        self.steps += 1;
        self.await_count("step", self.steps)
    }

    /// SIGKILL only the admitted Bun; return the held successor's PID before
    /// its admission, plus the kill-to-successor-boot interval (test clock).
    fn crash_and_hold_successor(&mut self) -> (u32, Instant, Duration) {
        let retired = self.bun;
        let started = Instant::now();
        kill_pid(retired);
        let successor = self.await_boot(self.boots.len() + 1);
        let to_boot = started.elapsed();
        assert_ne!(successor, retired, "a fresh Bun generation");
        await_process_gone(retired);
        assert!(
            process_exists(self.host_pid) && process_exists(self.guardian),
            "Bun loss terminated the host or guardian"
        );
        assert_eq!(parent_process(successor), self.guardian, "successor parent");
        (successor, started, to_boot)
    }

    fn assert_same_window(&self) {
        assert_eq!(
            await_same_native_windows(self.host_pid, TITLE, &self.window),
            self.window,
            "Bun loss replaced the host-owned native window"
        );
    }

    /// Focus the full-window textarea with the shared real pointer click, then
    /// post real key events; await the page's own trusted input record.
    fn type_draft(&mut self) {
        let host = self.host_pid.to_string();
        for (label, script, argument) in
            [("click", CLICK_SCRIPT, TITLE), ("type", TYPE_SCRIPT, DRAFT)]
        {
            let output = Command::new("/usr/bin/xcrun")
                .args(["swift", "-e", script, &host, argument])
                .output()
                .expect("real CoreGraphics input action");
            eprintln!("KELD_GH1081_INPUT {label} {output:?}");
            assert!(output.status.success(), "{label}: {output:?}");
        }
        let typed = self.await_where("typed draft", |report| {
            report["phase"] == "draft-input" && report["value"] == DRAFT
        });
        assert_eq!(typed["trusted"], true, "OS key events, not script input");
    }

    /// `WebKit` XPC services attributed to this host: launchd-responsible for
    /// it, or absent before its launch (the GUI test binary is serialized).
    fn webkit_census(&self, label: &str) -> Vec<(u64, String)> {
        let rows = webkit_rows(&self.census_tool);
        let mut owned: Vec<(u64, String)> = rows
            .iter()
            .filter(|row| {
                row["responsible"] == u64::from(self.host_pid)
                    || row["pid"]
                        .as_u64()
                        .is_some_and(|pid| !self.webkit_baseline.contains(&pid))
            })
            .map(|row| {
                (
                    row["pid"].as_u64().expect("census PID"),
                    row["service"].as_str().expect("census service").to_owned(),
                )
            })
            .collect();
        owned.sort();
        eprintln!(
            "KELD_GH1081_WEBKIT_CENSUS label={label} host={} owned={owned:?} baseline={:?} all={}",
            self.host_pid,
            self.webkit_baseline,
            Value::from(rows)
        );
        owned
    }

    /// Release the current app's Quit and assert product cleanup.
    fn quit(mut self) -> Output {
        self.observer
            .send_recovery_command(RecoveryCommand::Quit(self.bun));
        let output = wait_child_output(
            self.cleanup.cli.take().expect("host custody"),
            EVENT_DEADLINE,
        );
        eprintln!("KELD_GH1081_HOST_OUTPUT {output:?}");
        assert!(output.status.success(), "{output:?}");
        for process in [self.bun, self.guardian, self.host_pid] {
            await_process_gone(process);
        }
        assert_eq!(native_windows(self.host_pid, TITLE).len(), 0);
        self.cleanup.bun_group = None;
        output
    }
}

fn webkit_rows(tool: &Path) -> Vec<Value> {
    let output = Command::new(tool).output().expect("run WebKit census");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "WebKit census: {stdout}");
    let rows: Value = serde_json::from_str(
        stdout
            .trim()
            .strip_prefix("WEBKIT_CENSUS ")
            .expect("census record"),
    )
    .expect("census JSON");
    rows["rows"].as_array().expect("census rows").clone()
}

fn compile_webkit_census(root: &Path) -> PathBuf {
    let source = root.join("gh1081-webkit-census.swift");
    let executable = root.join("gh1081-webkit-census");
    fs::write(
        &source,
        include_str!("../../../fixtures/crash_recovery_r1_webkit_census.swift"),
    )
    .expect("write WebKit census");
    let output = Command::new("/usr/bin/xcrun")
        .args(["swiftc", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile WebKit census");
    assert!(output.status.success(), "compile WebKit census: {output:?}");
    executable
}

/// Independent same-document oracle: two renderer-created random values and a
/// JS-heap object. A reload or replacement mints new values.
fn same_document(before: &Value, after: &Value) -> Result<(), String> {
    let mut differences = Vec::new();
    for key in ["documentNonce", "javascriptToken"] {
        if before[key].as_str().is_none_or(|value| value.len() != 32) || before[key] != after[key] {
            differences.push(format!("{key}: {} -> {}", before[key], after[key]));
        }
    }
    if before["heap"]["token"] != after["heap"]["token"] {
        differences.push(format!(
            "heap.token: {} -> {}",
            before["heap"]["token"], after["heap"]["token"]
        ));
    }
    if differences.is_empty() {
        Ok(())
    } else {
        Err(differences.join("; "))
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

/// The passive bridge result envelopes recorded for one request.
fn terminals<'a>(report: &'a Value, label: &str) -> Vec<&'a Value> {
    let request = report["requests"]
        .as_array()
        .expect("page requests")
        .iter()
        .find(|request| request["label"] == label)
        .unwrap_or_else(|| panic!("no {label} request in {report}"));
    report["terminals"]
        .as_array()
        .expect("page terminals")
        .iter()
        .filter(|terminal| terminal["nonce"] == request["nonce"])
        .collect()
}

fn rejected_with(
    report: &Value,
    label: &str,
    code: &str,
    detail: Option<&str>,
) -> Result<(), String> {
    let result = outcome(report, label);
    let observed = terminals(report, label);
    let matches = result["ok"] == false
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

fn handler_nonces(journal: &[Value], bun: u32) -> Vec<String> {
    journal
        .iter()
        .filter(|report| report["phase"] == "handler" && report["pid"] == bun)
        .map(|report| {
            let message: Value =
                serde_json::from_str(report["message"].as_str().expect("handler message"))
                    .expect("handler message JSON");
            message["nonce"].as_str().expect("request nonce").to_owned()
        })
        .collect()
}

fn request_nonce(report: &Value, label: &str) -> String {
    report["requests"]
        .as_array()
        .expect("page requests")
        .iter()
        .find(|request| request["label"] == label)
        .and_then(|request| request["nonce"].as_str())
        .unwrap_or_else(|| panic!("no {label} nonce"))
        .to_owned()
}

fn marker_count(stderr: &str, marker: &str) -> usize {
    stderr.lines().filter(|line| line.contains(marker)).count()
}

fn bind_navigations(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| line.split("KELD_KEL142_BIND ").nth(1))
        .filter_map(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix("navigation="))
        })
        .map(str::to_owned)
        .collect()
}

/// E4 + F11(a) + F4: an OS-typed draft, the document and a JS-heap value
/// across one real Bun SIGKILL, with exactly one page-ready.
#[test]
fn gh1081_e4_typed_draft_document_and_heap_survive_bun_crash() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let steps = json!([
        {"do":"report","label":"before-crash"},
        {"do":"invoke","label":"successor-call"},
        {"do":"report","label":"after-recovery"},
    ]);
    let mut run = Experiment::launch(
        &fixture,
        "gh1081-e4",
        HandlerMode::BeforeReady,
        &steps,
        &json!([]),
    );
    let first = run.bun;
    let webkit_before = run.webkit_census("before-crash");
    run.type_draft();
    let before = run.step();
    assert_eq!(before["draft"], DRAFT, "{before}");

    let (successor, started, to_boot) = run.crash_and_hold_successor();
    run.admit(successor);
    run.adopt_ready(2, successor);
    let to_ready = started.elapsed();
    let webkit_after = run.webkit_census("after-recovery");
    run.assert_same_window();
    let call = run.step();
    let after = run.step();
    let verdict = same_document(&before, &after);
    eprintln!(
        "KELD_GH1081_E4 host={} window={:?} first_bun={first} second_bun={successor} kill_to_successor_boot_ms={} kill_to_successor_ready_ms={} before_nonce={} after_nonce={} before_token={} after_token={} draft_after={} session_after={} page_ready={} webkit_before={webkit_before:?} webkit_after={webkit_after:?} verdict={verdict:?}",
        run.host_pid,
        run.window,
        to_boot.as_millis(),
        to_ready.as_millis(),
        before["documentNonce"],
        after["documentNonce"],
        before["javascriptToken"],
        after["javascriptToken"],
        after["draft"],
        after["session"],
        run.count("page-ready"),
    );
    verdict.expect("E4: same renderer document across the Bun crash");
    assert_eq!(after["draft"], DRAFT, "E4: typed draft survived");
    assert_eq!(
        after["heap"]["trustedInputs"],
        before["heap"]["trustedInputs"]
    );
    assert_eq!(run.count("page-ready"), 1, "E4: exactly one page-ready");
    succeeded_on(&call, "successor-call", successor).expect("successor serves the same document");
    // F11(a) is recorded above: round 1 observed `SecurityError` in the
    // opaque-origin stage document, so storage cannot hold the draft at all.
    let output = run.quit();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let navigations = bind_navigations(&stderr);
    eprintln!("KELD_GH1081_E4_HOST bind_navigations={navigations:?}");
    assert_eq!(marker_count(&stderr, "KELD_KEL142_KIPC_CALL "), 1);
    assert!(
        navigations.windows(2).all(|pair| pair[0] == pair[1]),
        "one host navigation generation: {navigations:?}"
    );
    // F4 last, so the E4 verdict above is recorded independently of it.
    let content = |rows: &[(u64, String)]| -> Vec<u64> {
        rows.iter()
            .filter(|(_, service)| service == "com.apple.WebKit.WebContent")
            .map(|(pid, _)| *pid)
            .collect()
    };
    assert!(
        !content(&webkit_before).is_empty(),
        "F4: no WebContent process is attributed to the host: {webkit_before:?}"
    );
    assert_eq!(
        content(&webkit_before),
        content(&webkit_after),
        "F4: WebContent process changed across the Bun crash"
    );
}

/// E4 negative control + F11(b): a renderer `location.reload()` in the same
/// window must fail the same-document oracle; the sessionStorage draft is read.
#[test]
fn gh1081_e4_nc_renderer_reload_fails_same_document_and_reads_session_storage() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let steps = json!([
        {"do":"report","label":"before-reload"},
        {"do":"reload"},
    ]);
    let reload_steps = json!([{"do":"report","label":"after-reload"}]);
    let mut run = Experiment::launch(
        &fixture,
        "gh1081-e4-nc",
        HandlerMode::BeforeReady,
        &steps,
        &reload_steps,
    );
    run.type_draft();
    let before = run.step();
    assert_eq!(before["draft"], DRAFT, "{before}");
    run.observer
        .send_recovery_command(RecoveryCommand::Invoke(RecoveryPhase::Before));
    // Round 1 observed no second page-ready after a reload of the stage
    // document; wait for either a successor document or the deadline, and
    // keep the independent unload witness either way.
    let deadline = Instant::now() + EVENT_DEADLINE;
    while run.count("page-ready") < 2 && run.try_receive(deadline) {}
    let reloaded = (run.count("page-ready") >= 2).then(|| run.await_count("page-ready", 2));
    let after = reloaded.as_ref().map(|_| run.step());
    let unloaded = run.count("page-unload");
    let verdict = after.as_ref().map(|after| same_document(&before, after));
    eprintln!(
        "KELD_GH1081_E4_NC before_nonce={} unload_beacons={unloaded} successor_document={} after_nonce={} navigation_type={} restored={} restored_textarea={} draft_after={} verdict={verdict:?}",
        before["documentNonce"],
        reloaded.is_some(),
        after
            .as_ref()
            .map_or(Value::Null, |after| after["documentNonce"].clone()),
        reloaded
            .as_ref()
            .map_or(Value::Null, |ready| ready["navigationType"].clone()),
        reloaded
            .as_ref()
            .map_or(Value::Null, |ready| ready["restored"].clone()),
        reloaded
            .as_ref()
            .map_or(Value::Null, |ready| ready["restoredTextarea"].clone()),
        after
            .as_ref()
            .map_or(Value::Null, |after| after["draft"].clone()),
    );
    // NC: the original document must be gone. Either a successor document
    // fails the same-document oracle, or the original document unloaded and
    // nothing can answer for its nonce any more.
    match verdict {
        Some(verdict) => assert!(verdict.is_err(), "NC: a reload passed the oracle"),
        None => assert_eq!(
            unloaded, 1,
            "NC: reload neither unloaded nor replaced the document"
        ),
    }
    run.assert_same_window();
    let output = run.quit();
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!(
        "KELD_GH1081_E4_NC_HOST bind_navigations={:?}",
        bind_navigations(&stderr)
    );
}

/// E8 with the handler registered inside `whenReady` (Electron-style).
#[test]
fn gh1081_e8_calls_in_the_gap_and_before_an_inside_ready_handler() {
    run_e8(HandlerMode::InsideReady);
}

/// E8 with the handler registered before the first lifecycle call.
#[test]
fn gh1081_e8_calls_in_the_gap_and_after_a_before_ready_handler() {
    run_e8(HandlerMode::BeforeReady);
}

fn run_e8(mode: HandlerMode) {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let mut labels = vec!["g1-baseline", "gap"];
    if mode == HandlerMode::InsideReady {
        labels.push("pre-handler");
    }
    labels.push("successor-ready");
    let steps: Vec<Value> = labels
        .iter()
        .map(|label| json!({"do":"invoke","label":label}))
        .collect();
    let mut run = Experiment::launch(&fixture, "gh1081-e8", mode, &Value::from(steps), &json!([]));
    let first = run.bun;
    let baseline = run.step();
    succeeded_on(&baseline, "g1-baseline", first).expect("healthy G1 call");

    let (successor, _, _) = run.crash_and_hold_successor();
    // Test-owned precondition: G1 is gone and the successor has booted but
    // has not been admitted, so it has opened no app link and holds no HELLO.
    let gap = run.step();
    run.admit(successor);
    run.adopt_ready(2, successor);
    let pre_handler = if mode == HandlerMode::InsideReady {
        let hold = run.await_count("recovery-handler-pending", 2);
        assert_eq!(
            pid(&hold, "pid"),
            successor,
            "successor holds before registration"
        );
        let report = run.step();
        run.release_registration(2);
        Some(report)
    } else {
        None
    };
    let last = run.step();
    let journal = run.journal.clone();
    let output = run.quit();
    let stderr = String::from_utf8_lossy(&output.stderr);

    let gap_verdict = rejected_with(&gap, "gap", "KELD-WV-011", Some(GAP_DETAIL));
    let pre_verdict = pre_handler
        .as_ref()
        .map(|report| rejected_with(report, "pre-handler", "KELD-API-001", None));
    let ready_verdict = succeeded_on(&last, "successor-ready", successor);
    let first_handled = handler_nonces(&journal, first);
    let successor_handled = handler_nonces(&journal, successor);
    eprintln!(
        "KELD_GH1081_E8 mode={mode:?} first_bun={first} second_bun={successor} gap={} pre_handler={} successor_ready={} terminals={} first_handled={first_handled:?} successor_handled={successor_handled:?} renderer_admits={} kipc_calls={} gap_verdict={gap_verdict:?} pre_verdict={pre_verdict:?} ready_verdict={ready_verdict:?}",
        outcome(&gap, "gap"),
        pre_handler
            .as_ref()
            .map_or(Value::Null, |report| outcome(report, "pre-handler").clone()),
        outcome(&last, "successor-ready"),
        last["terminals"],
        marker_count(&stderr, "KELD_KEL142_RENDERER_ADMIT "),
        marker_count(&stderr, "KELD_KEL142_KIPC_CALL "),
    );
    let records = stderr_handler_records(&stderr);
    assert_eq!(
        (
            handler_nonces(&records, first),
            handler_nonces(&records, successor)
        ),
        (first_handled.clone(), successor_handled.clone()),
        "HTTP and stderr handler channels agree"
    );
    gap_verdict.expect("E8: gap call rejection");
    if let Some(verdict) = pre_verdict {
        verdict.expect("E8: pre-handler call rejection");
    }
    ready_verdict.expect("E8: successor call after its handler exists");
    // Exactly one terminal per call, and the page saw every call settle.
    let requests = last["requests"].as_array().expect("requests").len();
    assert_eq!(requests, labels.len());
    assert_eq!(
        last["outcomes"].as_array().expect("outcomes").len(),
        requests
    );
    for label in &labels {
        assert_eq!(terminals(&last, label).len(), 1, "one terminal for {label}");
    }
    // No replay: G1 handled only its baseline; G2 handled only the final call.
    assert_eq!(first_handled, [request_nonce(&last, "g1-baseline")]);
    assert_eq!(successor_handled, [request_nonce(&last, "successor-ready")]);
    // Host-side census: every call was admitted by the bridge; the gap call was
    // never forwarded over kipc, and nothing else was dropped or duplicated.
    assert_eq!(
        marker_count(&stderr, "KELD_KEL142_RENDERER_ADMIT "),
        requests
    );
    assert_eq!(
        marker_count(&stderr, "KELD_KEL142_KIPC_CALL "),
        requests - 1
    );
    assert_eq!(marker_count(&stderr, "KELD_GH1081_HANDLER "), 2);
    // Negative controls: each rejection oracle refuses the healthy call, and
    // the two rejections are distinguishable from each other.
    assert!(rejected_with(&last, "successor-ready", "KELD-WV-011", Some(GAP_DETAIL)).is_err());
    assert!(rejected_with(&last, "successor-ready", "KELD-API-001", None).is_err());
    if let Some(report) = &pre_handler {
        assert!(rejected_with(report, "pre-handler", "KELD-WV-011", Some(GAP_DETAIL)).is_err());
    }
}

/// The app's handler records as copied to host stderr: an independent second
/// channel for the same records the HTTP journal holds.
fn stderr_handler_records(stderr: &str) -> Vec<Value> {
    stderr
        .lines()
        .filter_map(|line| line.split("KELD_GH1081_HANDLER ").nth(1))
        .map(|record| serde_json::from_str(record).expect("handler stderr record"))
        .collect()
}
