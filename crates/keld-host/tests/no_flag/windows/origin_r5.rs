//! GH-1081 round-5 research only (never merged), Windows `WebView2`: KEL-79
//! origin experiments X4–X9 in the isolated `gh1081_origin_lab` harness (see
//! `origin_r5_lab.rs` for the experiment flows and `origin_r4.rs` for X0–X3).
//!
//! Windows specifics: every launch gets its own user data folder (the store
//! root X5 greps, after the `msedgewebview2.exe` processes that name it have
//! exited); the `InPrivate` arm is an `ICoreWebView2ControllerOptions` profile;
//! the X9 renderer is the `COREWEBVIEW2_PROCESS_KIND_RENDERER` entry of the
//! environment's `GetProcessInfos` (the harness `procs` command), killed with
//! `taskkill /F`; the engine's own event is `ProcessFailed`. The `https`
//! host-backed arm (A3) runs beside the custom scheme (A1) where the design
//! names it.
//!
//! Skipped: the explicit-directory store arm (a `WebKitGTK` concept);
//! candidate arms on the real host (production hook: scheme registration in
//! `keld-wv/src/webview2/mod.rs` environment options and a `NavTarget::Url`
//! boot in `app_session.rs`); Bun `ECHO3` during X9.

use crate::crash_recovery_r2::input_tool;
use crate::crash_recovery_r2_probe::scaffold_html;
use crate::origin_r4::WinDriver;
use crate::origin_r4_lab::{Arm, Lab, LaunchSpec, command_output, placeholder_scheme};
use crate::origin_r5_lab::{
    Driver5, Exit, Ids, StoreArm, X4Arm, X8Case, assert_x4_control, assert_x5_controls,
    assert_x6_controls, assert_x7_controls, assert_x8_controls, assert_x9_controls, run_x4, run_x5,
    run_x6, run_x7, run_x8, run_x9, summarize,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

fn powershell(script: &str) -> String {
    command_output(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )
}

/// Bounded wait until `Get-Process` no longer lists `pid`.
fn process_gone(pid: u32, deadline: Instant) -> bool {
    let script =
        format!("(Get-Process -Id {pid} -ErrorAction SilentlyContinue | Measure-Object).Count");
    loop {
        if powershell(&script).trim() == "0" {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::park_timeout(Duration::from_millis(100));
    }
}

/// How many `msedgewebview2.exe` processes name `user_data` on their command line.
fn browser_processes(user_data: &Path) -> String {
    let folder = user_data.display().to_string().replace('\'', "''");
    powershell(&format!(
        "(Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | Where-Object {{ $_.CommandLine -like '*{folder}*' }} | Measure-Object).Count"
    ))
}

fn win_driver(scratch: &Path) -> WinDriver {
    WinDriver {
        tool: input_tool(scratch),
        user_data: Vec::new(),
    }
}

impl Driver5 for WinDriver {
    fn apply_store(&mut self, spec: &mut LaunchSpec, store: StoreArm, ids: &Ids) {
        spec.user_data = Some(ids.user_data.path().to_path_buf());
        spec.second_user_data = Some(ids.second_user_data.path().to_path_buf());
        spec.store = String::from(match store {
            StoreArm::Ephemeral => "ephemeral",
            StoreArm::Persistent => "persistent",
            StoreArm::InPrivate => "inprivate",
            StoreArm::Directory => panic!("dir: is not a WebView2 store arm"),
        });
    }

    fn store_roots(&self, _store: StoreArm, ids: &Ids) -> Vec<PathBuf> {
        vec![ids.user_data.path().to_path_buf()]
    }

    fn process_baseline(&mut self) -> Value {
        Value::Null
    }

    fn renderer_pids(&mut self, lab: &mut Lab, _baseline: &Value) -> Value {
        let listed = lab
            .command("procs", Instant::now() + Duration::from_secs(10))
            .unwrap_or_default();
        let mut pids = Vec::new();
        let mut all = Vec::new();
        for entry in listed.split(',') {
            if let Some((pid, kind)) = entry.split_once(':') {
                all.push(entry.to_owned());
                if kind == "1"
                    && let Ok(pid) = pid.parse::<u32>()
                {
                    pids.push(pid);
                }
            }
        }
        json!({"pids": pids, "procs": all, "raw": listed})
    }

    fn kill_pid(&mut self, pid: u32) -> Value {
        let taskkill = command_output("taskkill", &["/F", "/PID", &pid.to_string()]);
        let gone = process_gone(pid, Instant::now() + Duration::from_secs(5));
        json!({"pid": pid, "taskkill": taskkill, "gone": gone})
    }

    fn await_engine_exit(
        &mut self,
        _store: StoreArm,
        ids: &Ids,
        _baseline: &Value,
        deadline: Instant,
    ) -> Value {
        loop {
            let count = browser_processes(ids.user_data.path());
            let settled = count.trim() == "0";
            if settled || Instant::now() >= deadline {
                return json!({"browser_processes": count, "settled": settled});
            }
            thread::park_timeout(Duration::from_millis(100));
        }
    }
}

fn setup() -> (tempfile::TempDir, String, WinDriver, String) {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let driver = win_driver(scratch.path());
    (scratch, scaffold, driver, placeholder_scheme())
}

fn report(experiment: &str, scheme: &str, records: &[Value]) {
    eprintln!(
        "KELD_GH1081_R5_SUMMARY os=windows x={experiment} scheme={scheme} rows={}",
        summarize(records)
    );
}

/// X4: two views in one browser process (one user data folder); same host
/// (control), distinct hosts, same host in a second environment.
#[test]
fn gh1081_r5_windows_x4_two_app_isolation() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let records: Vec<Value> = [
        X4Arm::SameHostShared,
        X4Arm::DistinctHostShared,
        X4Arm::SameHostSeparate,
    ]
    .iter()
    .map(|arm| run_x4(&mut driver, &scaffold, &scheme, *arm))
    .collect();
    report("x4", &scheme, &records);
    for record in &records {
        assert_x4_control(record);
    }
}

/// X5: sentinels in a user data folder (clean quit, kill), an `InPrivate`
/// profile, and the https arm; byte grep of the folder; relaunch read-back.
#[test]
fn gh1081_r5_windows_x5_session_storage_quit_cleanup() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let plan = [
        (Arm::Scheme, StoreArm::Persistent, Exit::Quit),
        (Arm::Scheme, StoreArm::Persistent, Exit::Kill),
        (Arm::Scheme, StoreArm::InPrivate, Exit::Quit),
        (Arm::Https, StoreArm::Persistent, Exit::Quit),
    ];
    let records: Vec<Value> = plan
        .iter()
        .map(|(arm, store, exit)| run_x5(&mut driver, &scaffold, &scheme, *arm, *store, *exit))
        .collect();
    report("x5", &scheme, &records);
    for record in &records {
        assert_x5_controls(record);
    }
}

/// X6: the request corpus against the strict handler, the two mutation
/// controls, the loose-filter control, and the https arm.
#[test]
fn gh1081_r5_windows_x6_path_safety() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let plan = [
        (Arm::Scheme, "strict", "anchored"),
        (Arm::Scheme, "naive", "anchored"),
        (Arm::Scheme, "nohost", "anchored"),
        (Arm::Scheme, "strict", "loose"),
        (Arm::Https, "strict", "anchored"),
        (Arm::Https, "strict", "loose"),
    ];
    let records: Vec<Value> = plan
        .iter()
        .map(|(arm, handler, filter)| {
            run_x6(&mut driver, &scaffold, &scheme, *arm, handler, Some(filter))
        })
        .collect();
    report("x6", &scheme, &records);
    for record in &records {
        assert_x6_controls(record);
    }
}

/// X7: browser APIs on the inline control, the custom scheme, the https arm
/// and the real-URL control page.
#[test]
fn gh1081_r5_windows_x7_browser_apis() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let records = vec![
        run_x7(&mut driver, &scaffold, &scheme, Arm::Inline, false),
        run_x7(&mut driver, &scaffold, &scheme, Arm::Scheme, false),
        run_x7(&mut driver, &scaffold, &scheme, Arm::Https, false),
        run_x7(&mut driver, &scaffold, &scheme, Arm::Inline, true),
    ];
    report("x7", &scheme, &records);
    for record in &records {
        assert_x7_controls(record);
    }
}

/// X8: handler 3xx answers (fetch and navigation), inbound 302 and
/// `location =` from the external page, the http→http controls, and the
/// https arm's fetch and inbound cases.
#[test]
fn gh1081_r5_windows_x8_redirects() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let plan = [
        (Arm::Scheme, X8Case::FetchRedirects),
        (Arm::Scheme, X8Case::NavExternal),
        (Arm::Scheme, X8Case::NavOtherScheme),
        (Arm::Scheme, X8Case::InboundRedirect),
        (Arm::Scheme, X8Case::InboundLocation),
        (Arm::Inline, X8Case::ControlHttpRedirect),
        (Arm::Inline, X8Case::ControlFetchRedirect),
        (Arm::Https, X8Case::FetchRedirects),
        (Arm::Https, X8Case::InboundRedirect),
        (Arm::Https, X8Case::InboundLocation),
    ];
    let records: Vec<Value> = plan
        .iter()
        .map(|(arm, case)| run_x8(&mut driver, &scaffold, &scheme, *arm, *case))
        .collect();
    report("x8", &scheme, &records);
    for record in &records {
        assert_x8_controls(record);
    }
}

/// X9: the `WebView2` renderer killed from outside under each load path, then
/// reload.
#[test]
fn gh1081_r5_windows_x9_renderer_crash() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let records = vec![
        run_x9(&mut driver, &scaffold, &scheme, Arm::Inline),
        run_x9(&mut driver, &scaffold, &scheme, Arm::Scheme),
        run_x9(&mut driver, &scaffold, &scheme, Arm::Https),
    ];
    report("x9", &scheme, &records);
    for record in &records {
        assert_x9_controls(record);
    }
}
