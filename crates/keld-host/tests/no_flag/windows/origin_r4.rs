//! GH-1081 round-4 research only (never merged), Windows `WebView2`: KEL-79
//! origin experiments X0 (engine census), X1 (origin, secure context,
//! storage), X2 (reload matrix) and the X3 byte-source atom, in the isolated
//! `gh1081_origin_lab` harness. Arms: inline HTML string (control), a
//! placeholder per-app custom scheme, the same scheme without an authority
//! component (negative control: opaque origin) and a host-backed
//! `https://app-a.invalid/` URL. Input is `SendInput`/`SendKeys` through the
//! round-2 PowerShell tool; observation is the page's own reports, the
//! harness's control plane and a read-only screen capture of the client area.
//!
//! The real-host X3 control row is the existing
//! `gh1081_r2_windows_typed_draft_survives_bun_crash`; the workflow selects it
//! next to these rows. A candidate arm on the real host would need a production
//! hook (scheme registration in `keld-wv/src/webview2/mod.rs` environment
//! options and a `NavTarget::Url` boot in `app_session.rs`), so it is not run.

use crate::crash_recovery_r2::{input_tool, run_tool};
use crate::crash_recovery_r2_probe::scaffold_html;
use crate::origin_r4_lab::{
    Arm, Driver, Lab, Trigger, assert_oracles, command_output, placeholder_scheme, run_arm,
    runner_census, summarize,
};
use crate::support::window::wait_for_window_titled;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct WinDriver {
    tool: PathBuf,
    /// One fresh `WebView2` user data folder per launch (dev-mode ephemeral
    /// shape); kept until the test ends so the browser can release its files.
    user_data: Vec<tempfile::TempDir>,
}

fn hwnd(window: &Value) -> u64 {
    window["handle"].as_u64().expect("HWND")
}

impl Driver for WinDriver {
    fn window(&mut self, pid: u32, title: &str, deadline: Instant) -> Value {
        wait_for_window_titled(pid, title, deadline)
    }

    fn pixels(&mut self, window: &Value, _label: &str) -> Value {
        run_tool(&self.tool, "pixels", hwnd(window), "")
    }

    fn input(
        &mut self,
        lab: &mut Lab,
        window: &Value,
        _geometry: &Value,
        _title: &str,
        trigger: Trigger,
    ) -> Value {
        let handle = hwnd(window);
        match trigger {
            Trigger::Key(keys) => run_tool(&self.tool, "input", handle, keys),
            Trigger::ContextMenu => {
                // A real right click; the harness reports the engine's
                // ContextMenuRequested (with the item labels) when the menu
                // opens, then the Refresh mnemonic is typed into the open menu.
                let click = run_tool(&self.tool, "context", handle, "");
                let menu = lab.await_event("EV ctxmenu", Instant::now() + Duration::from_secs(5));
                let keys = if menu.is_some() {
                    run_tool(&self.tool, "keys", handle, "r")
                } else {
                    Value::Null
                };
                json!({"right_click": click, "menu": menu, "keys": keys})
            }
            other => panic!("not an OS input trigger: {other:?}"),
        }
    }

    fn user_data(&mut self) -> Option<PathBuf> {
        let directory = tempfile::tempdir().expect("WebView2 user data folder");
        let path = directory.path().to_path_buf();
        self.user_data.push(directory);
        Some(path)
    }
}

/// X0: the Evergreen runtime (the loader's documented probe), the SDK pin and
/// the image this job ran on.
#[test]
fn gh1081_r4_windows_x0_engine_census() {
    let runtime = keld_wv::webview2::runtime_version();
    let census = json!({
        "runner": runner_census(),
        "webview2_runtime": runtime.as_ref().map_or_else(|error| error.to_string(), Clone::clone),
        "webview2_com_sdk_crate": "webview2-com 0.38.2 (Cargo.lock)",
        "ver": command_output("cmd", &["/c", "ver"]),
        "os": command_output(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command",
              "(Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber | ConvertTo-Json -Compress)"],
        ),
    });
    eprintln!("KELD_GH1081_R4_CENSUS os=windows {census}");
    assert!(
        runtime.is_ok(),
        "WebView2 Evergreen runtime present: {census}"
    );
}

/// X1 + X2 + X3 byte-source atom over the inline control, the custom-scheme
/// arm, its no-authority negative control and the host-backed https arm.
/// Asserts oracle validity only (see `assert_oracles`).
#[test]
fn gh1081_r4_windows_origin_reload_matrix() {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let tools = tempfile::tempdir().expect("input tool scratch");
    let mut driver = WinDriver {
        tool: input_tool(tools.path()),
        user_data: Vec::new(),
    };
    let scheme = placeholder_scheme();
    let plan = [
        (Arm::Inline, Trigger::RealUrlControl),
        (Arm::Inline, Trigger::JsReload),
        (Arm::Inline, Trigger::ApiReload),
        (Arm::Inline, Trigger::Key("{F5}")),
        (Arm::Inline, Trigger::Key("^r")),
        (Arm::Inline, Trigger::ContextMenu),
        (Arm::Scheme, Trigger::JsReload),
        (Arm::Scheme, Trigger::ApiReload),
        (Arm::Scheme, Trigger::Key("{F5}")),
        (Arm::Scheme, Trigger::Key("^r")),
        (Arm::Scheme, Trigger::ContextMenu),
        (Arm::SchemeNoAuthority, Trigger::JsReload),
        (Arm::Https, Trigger::JsReload),
        (Arm::Https, Trigger::ApiReload),
        (Arm::Https, Trigger::Key("{F5}")),
        (Arm::Https, Trigger::Key("^r")),
        (Arm::Https, Trigger::ContextMenu),
    ];
    let records: Vec<Value> = plan
        .iter()
        .map(|(arm, trigger)| run_arm(&mut driver, &scaffold, &scheme, *arm, *trigger))
        .collect();
    eprintln!(
        "KELD_GH1081_R4_SUMMARY os=windows scheme={scheme} rows={}",
        summarize(&records)
    );
    assert_oracles(&records);
}
