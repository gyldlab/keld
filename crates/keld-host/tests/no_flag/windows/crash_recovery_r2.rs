//! GH-1081 round-2 research only (never merged), Windows `WebView2`.
//! (1) Reload of the stock scaffold page in the staged no-flag host: page-JS
//! `location.reload()`, F5 and Ctrl+R (`WebView2` browser accelerator keys are
//! left at their default), with a real-URL reload as the negative control.
//! (3) OS-typed draft across a real Bun crash. No production hook: input is
//! `SendInput` (pointer) and `SendKeys` from a test PowerShell process;
//! observation is the page's own reports plus a read-only screen capture.

use crate::crash_recovery_r1::{Launch, deadline};
use crate::crash_recovery_r2_probe::{
    DRAFT, Probe, await_successor, is_page_fetch, is_unload, same_document, scaffold_html,
};
use crate::support::control::read_control_line;
use crate::support::product::ProductFixture;
use crate::wait_for_host_window;
use serde_json::{Value, json};
use std::fs;
use std::io::Write as _;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Bound for "a reloaded document reports"; the real-URL control's successor
/// arrives far inside it (printed per iteration as `reload_ms`).
const SUCCESSOR_BOUND: Duration = Duration::from_secs(10);

const INPUT_SCRIPT: &str = r##"param([string]$Mode, [Int64]$Hwnd, [string]$Keys = "")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class Gh1081R2 {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT point);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[Gh1081R2]::SetProcessDPIAware() | Out-Null
$h = [IntPtr]$Hwnd
$origin = New-Object Gh1081R2+POINT
[Gh1081R2]::ClientToScreen($h, [ref]$origin) | Out-Null
$rect = New-Object Gh1081R2+RECT
[Gh1081R2]::GetClientRect($h, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$hh = $rect.Bottom - $rect.Top
$result = [ordered]@{ mode = $Mode; x = $origin.X; y = $origin.Y; width = $w; height = $hh }
if ($Mode -eq "pixels") {
  $bmp = New-Object System.Drawing.Bitmap $w, $hh
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($origin.X, $origin.Y, 0, 0, $bmp.Size)
  $dark = 0; $light = 0; $total = 0
  for ($row = 0; $row -lt 16; $row++) {
    for ($col = 0; $col -lt 16; $col++) {
      $px = [Math]::Min([int]($w * (0.2 + 0.6 * $col / 15)), $w - 1)
      $py = [Math]::Min([int]($hh * (0.2 + 0.6 * $row / 15)), $hh - 1)
      $c = $bmp.GetPixel($px, $py)
      $total++
      if ([Math]::Max($c.R, [Math]::Max($c.G, $c.B)) -lt 64) { $dark++ }
      if ([Math]::Min($c.R, [Math]::Min($c.G, $c.B)) -gt 200) { $light++ }
    }
  }
  $c = $bmp.GetPixel([int]($w / 2), [int]($hh / 2))
  $result.dark = $dark / $total
  $result.light = $light / $total
  $result.center = "#{0:x2}{1:x2}{2:x2}" -f $c.R, $c.G, $c.B
  $g.Dispose(); $bmp.Dispose()
} else {
  $result.set_foreground = [Gh1081R2]::SetForegroundWindow($h)
  [Gh1081R2]::SetCursorPos($origin.X + [int]($w / 2), $origin.Y + [int]($hh / 2)) | Out-Null
  [Gh1081R2]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [Gh1081R2]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  $result.foreground_after_click = ([Gh1081R2]::GetForegroundWindow() -eq $h)
  if ($Keys -ne "") { [System.Windows.Forms.SendKeys]::SendWait($Keys) }
  $result.keys = $Keys
  $result.foreground_after_keys = ([Gh1081R2]::GetForegroundWindow() -eq $h)
}
$result | ConvertTo-Json -Compress
"##;

fn input_tool(root: &Path) -> PathBuf {
    let path = root.join("gh1081-r2-input.ps1");
    fs::write(&path, INPUT_SCRIPT).expect("write r2 input tool");
    path
}

fn run_tool(tool: &Path, mode: &str, hwnd: u64, keys: &str) -> Value {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(tool)
        .args(["-Mode", mode, "-Hwnd", &hwnd.to_string(), "-Keys", keys])
        .output()
        .expect("run r2 input tool");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    serde_json::from_str(stdout.trim()).unwrap_or_else(|_| json!({"error": format!("{output:?}")}))
}

#[derive(Clone, Copy, Debug)]
enum Arm {
    JsReload,
    Key(&'static str),
    RealUrlControl,
}

impl Arm {
    fn plans(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            Self::JsReload => (&["beat", "reload"], &[]),
            Self::Key(_) => (&[], &[]),
            Self::RealUrlControl => (&["real"], &["beat", "reload"]),
        }
    }
}

fn reload_arm(scaffold: &str, arm: Arm) -> Value {
    let fixture = ProductFixture::new();
    let tools = tempfile::tempdir().expect("r2 tool scratch");
    let tool = input_tool(tools.path());
    let (stage_plan, real_plan) = arm.plans();
    let mut probe = Probe::bind(scaffold, stage_plan, real_plan, false);
    fs::write(fixture.project.join("index.html"), probe.page()).expect("probe renderer");
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("bind r2 control");
    let mut launch = Launch::start(&fixture, &control);
    let host = launch.host;
    let first = probe.beat(deadline());
    let window = wait_for_host_window(host, deadline());
    let hwnd = window["handle"].as_u64().expect("HWND");
    let mut current = first.clone();
    let mut real_page = Value::Null;
    if let Arm::RealUrlControl = arm {
        probe.release();
        real_page = await_successor(&mut probe, first["doc"].as_str().unwrap_or(""), deadline())
            .expect("real-URL control page reported");
        current = real_page.clone();
    }
    let before = probe.poll(deadline());
    let pixels_before = run_tool(&tool, "pixels", hwnd, "");
    let triggered = Instant::now();
    let input = match arm {
        Arm::JsReload | Arm::RealUrlControl => {
            probe.release();
            Value::Null
        }
        Arm::Key(keys) => run_tool(&tool, "input", hwnd, keys),
    };
    let previous = current["doc"].as_str().unwrap_or("").to_owned();
    let successor = await_successor(&mut probe, &previous, triggered + SUCCESSOR_BOUND);
    let reload_ms = successor.as_ref().map(|_| triggered.elapsed().as_millis());
    while probe.try_next(Instant::now()).is_some() {}
    let pixels_after = run_tool(&tool, "pixels", hwnd, "");
    let window_after = wait_for_host_window(host, deadline());
    writeln!(launch.writer, "ECHO3").expect("request echo 3");
    launch.writer.flush().expect("flush echo 3");
    let echo3 = read_control_line(&mut launch.reader);
    let record = json!({
        "arm": format!("{arm:?}"), "host": host, "window": window,
        "window_after": window_after, "first": first, "real_page": real_page,
        "before": before, "pixels_before": pixels_before, "input": input,
        "successor": successor, "reload_ms": reload_ms,
        "unload_beacons": probe.count(is_unload),
        "real_page_fetches": probe.count(is_page_fetch),
        "pixels_after": pixels_after, "echo3_after_reload": echo3,
        "documents": probe.documents(),
    });
    eprintln!("KELD_GH1081_R2_RELOAD {record}");
    launch.quit();
    record
}

/// Reload matrix on the stock scaffold page. Asserts only oracle validity
/// (dark scaffold on screen before the trigger; the control reloaded).
#[test]
fn gh1081_r2_windows_reload_matrix_on_scaffold_page() {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let arms = [
        Arm::RealUrlControl,
        Arm::JsReload,
        Arm::Key("{F5}"),
        Arm::Key("^r"),
    ];
    let records: Vec<Value> = arms.iter().map(|arm| reload_arm(&scaffold, *arm)).collect();
    for record in &records {
        assert!(
            record["pixels_before"]["dark"].as_f64().unwrap_or(0.0) > 0.9,
            "oracle invalid: the scaffold page was not on screen before the trigger: {record}"
        );
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

/// Task 3: real `SendInput`/`SendKeys` keystrokes into a textarea on the stock
/// scaffold page, read back by the page's own input reports, across one real
/// Bun crash. NC: a host relaunch is a new document with an empty draft.
#[test]
fn gh1081_r2_windows_typed_draft_survives_bun_crash() {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let fixture = ProductFixture::new();
    let tools = tempfile::tempdir().expect("r2 tool scratch");
    let tool = input_tool(tools.path());
    let mut probe = Probe::bind(&scaffold, &[], &[], true);
    fs::write(fixture.project.join("index.html"), probe.page()).expect("draft renderer");
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("bind draft control");
    let mut launch = Launch::start(&fixture, &control);
    let host = launch.host;
    let first_bun = launch.bun;
    let first = probe.beat(deadline());
    let window = wait_for_host_window(host, deadline());
    let hwnd = window["handle"].as_u64().expect("HWND");
    let input = run_tool(&tool, "input", hwnd, DRAFT);
    let typed = probe.await_draft(DRAFT, deadline());
    let before = probe.poll(deadline());
    let to_ready = launch.crash_and_accept_successor(&control);
    let after = probe.poll(deadline());
    let window_after = wait_for_host_window(host, deadline());
    let documents = probe.documents();
    let verdict = same_document(&before, &after);
    eprintln!(
        "KELD_GH1081_R2_DRAFT host={host} first_bun={first_bun} second_bun={} crash_to_ready_ms={} window={window} window_after={window_after} first={first} input={input} typed={typed:?} before={before} after={after} documents={documents:?} verdict={verdict:?}",
        launch.bun,
        to_ready.as_millis()
    );
    let second_bun = launch.bun;
    launch.quit();
    let relaunch = Launch::start(&fixture, &control);
    let relaunched = await_successor(&mut probe, before["doc"].as_str().unwrap_or(""), deadline())
        .expect("relaunched document reported");
    let control_verdict = same_document(&before, &relaunched);
    eprintln!("KELD_GH1081_R2_DRAFT_NC relaunched={relaunched} verdict={control_verdict:?}");
    relaunch.quit();

    assert!(
        typed.is_some(),
        "OS keystrokes did not reach the textarea: {input}"
    );
    assert_eq!(before["draft"], DRAFT, "draft before the crash");
    assert_eq!(
        before["trusted"].as_u64(),
        Some(DRAFT.len() as u64),
        "every input event was trusted: {before}"
    );
    verdict.expect("same renderer document across the Bun crash");
    assert_ne!(first_bun, second_bun, "fresh Bun process");
    assert_eq!(after["draft"], DRAFT, "draft intact after the Bun crash");
    assert_eq!(documents.len(), 1, "exactly one document across the crash");
    assert_eq!(window["handle"], window_after["handle"], "same HWND");
    assert!(control_verdict.is_err(), "NC: a relaunch passed the oracle");
    assert_eq!(relaunched["draft"], "", "NC: relaunched draft is empty");
}
