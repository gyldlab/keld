//! GH-1081 round-2 research only (never merged), macOS `WKWebView`.
//! Reload of the stock `keld create` scaffold page in the staged no-flag host:
//! page-JS `location.reload()`, Cmd+R, and the `WebKit` context-menu Reload,
//! with a real-URL reload as the negative control. No production hook: input
//! is real CoreGraphics events, observation is the page's own reports, a
//! read-only `screencapture` of the host window and, when the test build has
//! the existing `profile-test-hooks` feature, the host's own page-load trace.

use crate::crash_recovery_r2_probe::{
    Probe, await_successor, is_page_fetch, is_unload, scaffold_html,
};
use crate::support::control::read_control_line;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::product::ProductFixture;
use crate::support::recovery_cycle::{RecoveryCycle, RecoveryGeneration};
use crate::support::{EVENT_DEADLINE, TITLE};
use serde_json::{Value, json};
use std::fs;
use std::io::Write as _;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Bound for "a reloaded document reports"; the real-URL control's successor
/// arrives far inside it (printed per iteration as `reload_ms`).
const SUCCESSOR_BOUND: Duration = Duration::from_secs(10);

fn deadline() -> Instant {
    Instant::now() + EVENT_DEADLINE
}

/// Compiles the round-2 Swift input/pixels tool (round 4 adds its `find` mode).
pub(crate) fn compile_tool(root: &Path) -> PathBuf {
    let source = root.join("gh1081-r2-macos.swift");
    let executable = root.join("gh1081-r2-macos");
    fs::write(
        &source,
        include_str!("../../fixtures/crash_recovery_r2_macos.swift"),
    )
    .expect("write r2 macOS tool");
    let output = Command::new("/usr/bin/xcrun")
        .args(["swiftc", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile r2 macOS tool");
    assert!(output.status.success(), "compile r2 macOS tool: {output:?}");
    executable
}

pub(crate) fn tool_json(tool: &Path, args: &[&str]) -> Value {
    let output = Command::new(tool)
        .args(args)
        .output()
        .expect("run r2 macOS tool");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    serde_json::from_str(stdout.trim()).unwrap_or_else(|_| json!({"error": format!("{output:?}")}))
}

pub(crate) fn pixels(tool: &Path, window: u32, root: &Path, label: &str) -> Value {
    let png = root.join(format!("gh1081-r2-{label}.png"));
    tool_json(
        tool,
        &["pixels", &window.to_string(), &png.to_string_lossy()],
    )
}

/// The same launch shape as `RecoveryCycle::launch`, with the probe page and
/// the host's existing page-load trace switch.
fn launch(fixture: &ProductFixture, name: &str) -> (RecoveryCycle, keld_cli::boot::DevBootStage) {
    let stage = fixture.stage();
    let control_path = fixture.root.path().join(format!("{name}.sock"));
    let listener = UnixListener::bind(&control_path).expect("bind r2 control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking r2 control");
    let mut presentation = fixture.observe_initial_window();
    let mut child = Command::new(stage.host())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T1B_CONTROL", &control_path)
        .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch r2 no-flag host");
    let dev_lease_writer = child.stdin.take();
    let host_pid = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let mut current = RecoveryGeneration::accept(&listener, name);
    current.expect_ready_and_echoes();
    cleanup.bun_group = Some(current.bun_pid);
    let group = current.bun_pid;
    let host = cleanup.release();
    let window = presentation.expect_initial(host_pid, name);
    assert_eq!(window.len(), 1, "one host window: {window:?}");
    (
        RecoveryCycle {
            host: Some(host),
            dev_lease_writer,
            host_pid,
            listener,
            window,
            current: Some(current),
            process_groups: vec![group],
        },
        stage,
    )
}

fn quit(mut cycle: RecoveryCycle) -> String {
    let current = cycle.current.as_mut().expect("current generation");
    current.writer.write_all(b"QUIT\n").expect("quit");
    current.expect_line("QUIT_REPLY");
    current.expect_line("LINK_EOF");
    let output = cycle.wait_host();
    assert!(output.status.success(), "r2 host exit: {output:?}");
    cycle.assert_current_group_gone();
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[derive(Clone, Copy, Debug)]
enum Arm {
    JsReload,
    CmdR,
    ContextMenu,
    RealUrlControl,
}

impl Arm {
    fn plans(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            Self::JsReload => (&["beat", "reload"], &[]),
            Self::CmdR | Self::ContextMenu => (&[], &[]),
            Self::RealUrlControl => (&["real"], &["beat", "reload"]),
        }
    }
}

fn reload_arm(scaffold: &str, tool: &Path, arm: Arm) -> Value {
    let fixture = ProductFixture::new("gh1081-r2-reload");
    let (stage_plan, real_plan) = arm.plans();
    let mut probe = Probe::bind(scaffold, stage_plan, real_plan, false);
    fs::write(fixture.project.join("index.html"), probe.page()).expect("probe renderer");
    let (mut cycle, _stage) = launch(&fixture, "gh1081-r2");
    let host = cycle.host_pid;
    let window = cycle.window[0];
    let first = probe.beat(deadline());
    let mut current = first.clone();
    let mut real_page = Value::Null;
    if let Arm::RealUrlControl = arm {
        probe.release();
        real_page = await_successor(&mut probe, first["doc"].as_str().unwrap_or(""), deadline())
            .expect("real-URL control page reported");
        current = real_page.clone();
    }
    let before = probe.poll(deadline());
    let pixels_before = pixels(tool, window, fixture.root.path(), "before");
    let triggered = Instant::now();
    let input = match arm {
        Arm::JsReload | Arm::RealUrlControl => {
            probe.release();
            Value::Null
        }
        Arm::CmdR => tool_json(tool, &["input", &host.to_string(), TITLE, "cmd-r"]),
        Arm::ContextMenu => tool_json(tool, &["input", &host.to_string(), TITLE, "context-reload"]),
    };
    let previous = current["doc"].as_str().unwrap_or("").to_owned();
    let successor = await_successor(&mut probe, &previous, triggered + SUCCESSOR_BOUND);
    let reload_ms = successor.as_ref().map(|_| triggered.elapsed().as_millis());
    while probe.try_next(Instant::now()).is_some() {}
    let pixels_after = pixels(tool, window, fixture.root.path(), "after");
    let generation = cycle.current.as_mut().expect("current generation");
    generation
        .writer
        .write_all(b"ECHO3\n")
        .expect("request echo 3");
    let echo3 = read_control_line(&mut generation.reader);
    let unloads = probe.count(is_unload);
    let fetches = probe.count(is_page_fetch);
    let documents = probe.documents();
    let stderr = quit(cycle);
    let page_loads: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("KELD_KEL135_PAGE_LOAD") || line.contains("KELD_KEL142_BIND"))
        .collect();
    let record = json!({
        "arm": format!("{arm:?}"), "host": host, "window": window, "first": first,
        "real_page": real_page, "before": before, "pixels_before": pixels_before,
        "input": input, "successor": successor, "reload_ms": reload_ms,
        "unload_beacons": unloads, "real_page_fetches": fetches,
        "pixels_after": pixels_after, "echo3_after_reload": echo3,
        "documents": documents, "host_page_loads": page_loads,
    });
    eprintln!("KELD_GH1081_R2_RELOAD {record}");
    record
}

/// Reload matrix on the stock scaffold page. Asserts only oracle validity
/// (the control reloaded into a new document); every arm is recorded.
#[test]
fn gh1081_r2_macos_reload_matrix_on_scaffold_page() {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let tool = compile_tool(scratch.path());
    let arms = [
        Arm::RealUrlControl,
        Arm::JsReload,
        Arm::CmdR,
        Arm::ContextMenu,
    ];
    let records: Vec<Value> = arms
        .iter()
        .map(|arm| reload_arm(&scaffold, &tool, *arm))
        .collect();
    for record in &records {
        assert_eq!(record["echo3_after_reload"], "ECHO3", "{record}");
    }
    let control = &records[0];
    assert!(
        control["successor"]["real"] == true && control["successor"]["nav"] == "reload",
        "NC: the real-URL page did not reload into a new document: {control}"
    );
    assert_eq!(
        control["real_page_fetches"], 2,
        "NC: reload re-fetched the URL"
    );
}
