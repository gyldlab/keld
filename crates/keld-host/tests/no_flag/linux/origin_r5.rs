//! GH-1081 round-5 research only (never merged), Linux `WebKitGTK` under Xvfb:
//! KEL-79 origin experiments X4–X9 in the isolated `gh1081_origin_lab` harness
//! (see `origin_r5_lab.rs` for the experiment flows and `origin_r4.rs` for
//! X0–X3).
//!
//! Linux specifics: every launch gets its own `XDG_DATA_HOME` /
//! `XDG_CACHE_HOME`, so the default context's data directory (the F7a
//! prgname question) is created under a test-owned root that X4/X5 list and
//! byte-grep; the explicit-directory arm is wry `WebContext::new(Some(dir))`
//! (the KEL-135 T4 shape); the X9 content process is the `WebKitWebProcess`
//! descendant of the harness (possibly under `bwrap`), found by walking
//! `pgrep -P` and matching `ps -o args=`. wry 0.56.1 exposes no
//! `web-process-terminated` signal, so the engine event column is empty on
//! Linux by construction; the observation is the page's silence, the pixels
//! and the reload outcome.
//!
//! Skipped: `InPrivate` (a `WebView2` concept); candidate arms on the real host
//! (production hook: scheme registration in `keld-wv/src/media.rs` and a
//! `NavTarget::Url` boot in `app_session.rs`); Bun `ECHO3` during X9.

use crate::crash_recovery_r2_probe::scaffold_html;
use crate::origin_r4::LinuxDriver;
use crate::origin_r4_lab::{Arm, Lab, LaunchSpec, command_output, placeholder_scheme};
use crate::origin_r5_lab::{
    Driver5, Exit, Ids, StoreArm, X4Arm, X8Case, assert_x4_control, assert_x5_controls,
    assert_x6_controls, assert_x7_controls, assert_x8_controls, assert_x9_controls, run_x4, run_x5,
    run_x6, run_x7, run_x8, run_x9, summarize, unix_kill,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

fn pgrep(args: &[&str]) -> Vec<u32> {
    command_output("pgrep", args)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

/// Every descendant of `root` (breadth first through `pgrep -P`).
fn descendants(root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(parent) = stack.pop() {
        for child in pgrep(&["-P", &parent.to_string()]) {
            out.push(child);
            stack.push(child);
        }
    }
    out
}

fn linux_driver(scratch: &Path) -> LinuxDriver {
    let census = scratch.join("gh1081-x11-census.py");
    fs::write(
        &census,
        include_str!("../../fixtures/crash_recovery_r1_x11_windows.py"),
    )
    .expect("write X11 census");
    LinuxDriver { census }
}

impl Driver5 for LinuxDriver {
    fn apply_store(&mut self, spec: &mut LaunchSpec, store: StoreArm, ids: &Ids) {
        let data = ids.xdg.path().join("data");
        let cache = ids.xdg.path().join("cache");
        fs::create_dir_all(&data).expect("XDG data root");
        fs::create_dir_all(&cache).expect("XDG cache root");
        spec.env = vec![
            (String::from("XDG_DATA_HOME"), data.display().to_string()),
            (String::from("XDG_CACHE_HOME"), cache.display().to_string()),
        ];
        spec.store = match store {
            StoreArm::Ephemeral => String::from("ephemeral"),
            StoreArm::Persistent => String::from("persistent"),
            StoreArm::Directory => format!("dir:{}", ids.dir.path().display()),
            StoreArm::InPrivate => panic!("inprivate is not a WebKitGTK store arm"),
        };
    }

    fn store_roots(&self, _store: StoreArm, ids: &Ids) -> Vec<PathBuf> {
        vec![ids.xdg.path().to_path_buf(), ids.dir.path().to_path_buf()]
    }

    fn process_baseline(&mut self) -> Value {
        json!({"network_processes": pgrep(&["-f", "WebKitNetworkProcess"])})
    }

    fn renderer_pids(&mut self, lab: &mut Lab, _baseline: &Value) -> Value {
        let mut rows = Vec::new();
        let mut pids = Vec::new();
        for pid in descendants(lab.pid) {
            let args = command_output("ps", &["-o", "args=", "-p", &pid.to_string()]);
            if args.contains("WebKitWebProcess") {
                pids.push(pid);
            }
            rows.push(json!({"pid": pid, "args": args.chars().take(120).collect::<String>()}));
        }
        json!({"pids": pids, "descendants": rows})
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
        let base = baseline["network_processes"].as_array().map_or(0, Vec::len);
        loop {
            let now = pgrep(&["-f", "WebKitNetworkProcess"]);
            let settled = now.len() <= base;
            if settled || Instant::now() >= deadline {
                return json!({"network_baseline": base, "network_now": now, "settled": settled});
            }
            thread::park_timeout(Duration::from_millis(50));
        }
    }
}

fn setup() -> (tempfile::TempDir, String, LinuxDriver, String) {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let driver = linux_driver(scratch.path());
    (scratch, scaffold, driver, placeholder_scheme())
}

fn report(experiment: &str, scheme: &str, records: &[Value]) {
    eprintln!(
        "KELD_GH1081_R5_SUMMARY os=linux x={experiment} scheme={scheme} rows={}",
        summarize(records)
    );
}

/// X4: two views in one process on one default (XDG) context; same host
/// (control), distinct hosts, same host in a separate ephemeral context. The
/// store tree under the test-owned `XDG_DATA_HOME` answers F7a (prgname).
#[test]
fn gh1081_r5_linux_x4_two_app_isolation() {
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

/// X5: sentinels in the ephemeral context, the default XDG context and an
/// explicit data directory; clean quit vs kill; byte grep; relaunch read-back.
#[test]
fn gh1081_r5_linux_x5_session_storage_quit_cleanup() {
    let (_scratch, scaffold, mut driver, scheme) = setup();
    let plan = [
        (StoreArm::Ephemeral, Exit::Quit),
        (StoreArm::Persistent, Exit::Quit),
        (StoreArm::Persistent, Exit::Kill),
        (StoreArm::Directory, Exit::Quit),
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
fn gh1081_r5_linux_x6_path_safety() {
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
fn gh1081_r5_linux_x7_browser_apis() {
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
fn gh1081_r5_linux_x8_redirects() {
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

/// X9: `WebKitWebProcess` killed from outside under each load path, then reload.
#[test]
fn gh1081_r5_linux_x9_renderer_crash() {
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
