//! GH-1081 round-5 research only (never merged), macOS `WKWebView`: KEL-79
//! origin experiments X4–X9 in the isolated `gh1081_origin_lab` harness (see
//! `origin_r5_lab.rs` for the experiment flows and `origin_r4.rs` for X0–X3).
//!
//! macOS specifics: the persistent store is a `WKWebsiteDataStore` identified
//! by a per-launch UUID (wry `with_data_store_identifier`, macOS 14+); the
//! store roots grepped by X5 are `~/Library/WebKit` and the harness's cache
//! directory; the X9 content process is the `com.apple.WebKit.WebContent` XPC
//! service that appears after the launch (it is launchd's child, not the
//! harness's, so the census diffs the system-wide set); the engine's own event
//! is wry's `webViewWebContentProcessDidTerminate:` delegate.
//!
//! Skipped: `InPrivate` and explicit-directory store arms (not `WKWebView`
//! concepts); candidate arms on the real host (production hook: scheme
//! registration in `keld-wv/src/media.rs` and a `NavTarget::Url` boot in
//! `app_session.rs`); Bun `ECHO3` during X9 (the harness has no Bun).

use crate::crash_recovery_r2::compile_tool;
use crate::crash_recovery_r2_probe::scaffold_html;
use crate::origin_r4::MacDriver;
use crate::origin_r4_lab::{Arm, Lab, LaunchSpec, command_output, placeholder_scheme};
use crate::origin_r5_lab::{
    Driver5, Exit, Ids, StoreArm, X4Arm, X8Case, assert_x4_control, assert_x5_controls,
    assert_x6_controls, assert_x7_controls, assert_x8_controls, assert_x9_controls, run_x4, run_x5,
    run_x6, run_x7, run_x8, run_x9, summarize, unix_kill,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const WEBCONTENT: &str = "com.apple.WebKit.WebContent";
const NETWORKING: &str = "com.apple.WebKit.Networking";

fn pgrep(pattern: &str) -> Vec<u32> {
    command_output("/usr/bin/pgrep", &["-f", pattern])
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

fn mac_driver(scratch: &Path) -> MacDriver {
    MacDriver {
        tool: compile_tool(scratch),
        root: tempfile::tempdir().expect("pixels scratch"),
    }
}

impl Driver5 for MacDriver {
    fn apply_store(&mut self, spec: &mut LaunchSpec, store: StoreArm, ids: &Ids) {
        match store {
            StoreArm::Ephemeral => {
                spec.store = String::from("ephemeral");
                spec.store_id = None;
            }
            StoreArm::Persistent => {
                spec.store = String::from("persistent");
                spec.store_id = Some(ids.store_id.clone());
            }
            StoreArm::InPrivate | StoreArm::Directory => {
                panic!("{} is not a WKWebView store arm", store.name())
            }
        }
    }

    fn store_roots(&self, _store: StoreArm, _ids: &Ids) -> Vec<PathBuf> {
        let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
        vec![
            home.join("Library").join("WebKit"),
            home.join("Library")
                .join("Caches")
                .join("gh1081_origin_lab"),
        ]
    }

    fn process_baseline(&mut self) -> Value {
        json!({"webcontent": pgrep(WEBCONTENT), "networking": pgrep(NETWORKING)})
    }

    fn renderer_pids(&mut self, _lab: &mut Lab, baseline: &Value) -> Value {
        let before: Vec<u64> = baseline["webcontent"]
            .as_array()
            .map(|pids| pids.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        let after = pgrep(WEBCONTENT);
        let pids: Vec<u32> = after
            .iter()
            .copied()
            .filter(|pid| !before.contains(&u64::from(*pid)))
            .collect();
        let rows: Vec<String> = command_output("/bin/ps", &["-axo", "pid=,ppid=,comm="])
            .lines()
            .filter(|line| line.contains("WebKit"))
            .map(|line| line.trim().to_owned())
            .collect();
        json!({"pids": pids, "before": before, "after": after, "ps": rows})
    }

    fn kill_pid(&mut self, pid: u32) -> Value {
        unix_kill(pid)
    }

    fn await_engine_exit(
        &mut self,
        _store: StoreArm,
        _ids: &Ids,
        baseline: &Value,
        deadline: Instant,
    ) -> Value {
        let base = baseline["networking"].as_array().map_or(0, Vec::len);
        loop {
            let now = pgrep(NETWORKING);
            let settled = now.len() <= base;
            if settled || Instant::now() >= deadline {
                return json!({"networking_baseline": base, "networking_now": now, "settled": settled});
            }
            thread::park_timeout(Duration::from_millis(50));
        }
    }
}

fn setup() -> (tempfile::TempDir, String, MacDriver, String) {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let driver = mac_driver(scratch.path());
    (scratch, scaffold, driver, placeholder_scheme())
}

fn report(experiment: &str, scheme: &str, records: &[Value]) {
    eprintln!(
        "KELD_GH1081_R5_SUMMARY os=macos x={experiment} scheme={scheme} rows={}",
        summarize(records)
    );
}

/// X4: two views in one process, one identified store; same host (control),
/// distinct hosts, same host in a separate store.
#[test]
fn gh1081_r5_macos_x4_two_app_isolation() {
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

/// X5: sentinels in `nonPersistent` vs identified stores; clean quit vs kill;
/// byte grep of `~/Library/WebKit`; relaunch read-back.
#[test]
fn gh1081_r5_macos_x5_session_storage_quit_cleanup() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let plan = [
        (StoreArm::Ephemeral, Exit::Quit),
        (StoreArm::Persistent, Exit::Quit),
        (StoreArm::Persistent, Exit::Kill),
    ];
    let records: Vec<Value> = plan
        .iter()
        .map(|(store, exit)| run_x5(&mut driver, &scaffold, &scheme, Arm::Scheme, *store, *exit))
        .collect();
    report("x5", &scheme, &records);
    for record in &records {
        assert_x5_controls(record);
    }
}

/// X6: the request corpus against the strict handler and the two mutation
/// controls (naive path-join, host check off).
#[test]
fn gh1081_r5_macos_x6_path_safety() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let records: Vec<Value> = ["strict", "naive", "nohost"]
        .iter()
        .map(|handler| run_x6(&mut driver, &scaffold, &scheme, Arm::Scheme, handler, None))
        .collect();
    report("x6", &scheme, &records);
    for record in &records {
        assert_x6_controls(record);
    }
}

/// X7: browser APIs on the inline control, the custom scheme and the real-URL
/// control page.
#[test]
fn gh1081_r5_macos_x7_browser_apis() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let records = vec![
        run_x7(&mut driver, &scaffold, &scheme, Arm::Inline, false),
        run_x7(&mut driver, &scaffold, &scheme, Arm::Scheme, false),
        run_x7(&mut driver, &scaffold, &scheme, Arm::Inline, true),
    ];
    report("x7", &scheme, &records);
    for record in &records {
        assert_x7_controls(record);
    }
}

/// X8: handler 3xx answers (fetch and navigation), inbound 302 and
/// `location =` from the external page, plus the http→http controls.
#[test]
fn gh1081_r5_macos_x8_redirects() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let plan = [
        (Arm::Scheme, X8Case::FetchRedirects),
        (Arm::Scheme, X8Case::NavExternal),
        (Arm::Scheme, X8Case::NavOtherScheme),
        (Arm::Scheme, X8Case::InboundRedirect),
        (Arm::Scheme, X8Case::InboundLocation),
        (Arm::Inline, X8Case::ControlHttpRedirect),
        (Arm::Inline, X8Case::ControlFetchRedirect),
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

/// X9: `WebContent` killed from outside under each load path, then reload.
#[test]
fn gh1081_r5_macos_x9_renderer_crash() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let records = vec![
        run_x9(&mut driver, &scaffold, &scheme, Arm::Inline),
        run_x9(&mut driver, &scaffold, &scheme, Arm::Scheme),
    ];
    report("x9", &scheme, &records);
    for record in &records {
        assert_x9_controls(record);
    }
}
