//! GH-1081 round-4 research only (never merged), Linux `WebKitGTK` under Xvfb:
//! KEL-79 origin experiments X0 (engine census), X1 (origin, secure context,
//! storage), X2 (reload matrix) and the X3 byte-source atom, in the isolated
//! `gh1081_origin_lab` harness. Arms: inline HTML string (control) and a
//! placeholder per-app custom scheme. Input is XTEST via xdotool; observation
//! is the page's own reports, the harness's control plane and read-only X11
//! pixels.
//!
//! The `register_uri_scheme_as_local` storage negative control from the design
//! (A1-local) is not run: reaching the `WebKitGTK` security manager needs a
//! direct `webkit2gtk` dev-dependency edge that keld-wv does not have (wry does
//! not re-export it), and the denial oracle is already validated by the inline
//! arm's `SecurityError`. The real-host X3 control row is the existing
//! `gh1081_r2_linux_typed_draft_survives_bun_crash`; a candidate arm on the real
//! host would need a production hook (`keld-wv/src/media.rs` scheme
//! registration and a `NavTarget::Url` boot in `app_session.rs`).

use crate::crash_recovery_r1::host_windows;
use crate::crash_recovery_r2::{context_menu_reload, focus_and_click, pixels, xdotool};
use crate::crash_recovery_r2_probe::scaffold_html;
use crate::origin_r4_lab::{
    Arm, Driver, Lab, Trigger, assert_oracles, command_output, placeholder_scheme, run_arm,
    runner_census, summarize,
};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) struct LinuxDriver {
    pub(crate) census: PathBuf,
}

fn window_id(window: &Value) -> u64 {
    window["window"].as_u64().expect("X11 window id")
}

impl Driver for LinuxDriver {
    fn window(&mut self, pid: u32, title: &str, deadline: Instant) -> Value {
        loop {
            let (ids, rows) = host_windows(&self.census, pid, title);
            if let Some(id) = ids.first() {
                return json!({"window": id, "rows": rows});
            }
            assert!(
                Instant::now() < deadline,
                "no titled X11 window for pid {pid}: {rows}"
            );
            thread::park_timeout(Duration::from_millis(50));
        }
    }

    fn pixels(&mut self, window: &Value, _label: &str) -> Value {
        pixels(&self.census, window_id(window))
    }

    fn input(
        &mut self,
        _lab: &mut Lab,
        window: &Value,
        geometry: &Value,
        _title: &str,
        trigger: Trigger,
    ) -> Value {
        let id = window_id(window);
        match trigger {
            Trigger::Key(key) => {
                let mut inputs = focus_and_click(id, geometry, "1");
                inputs.push(xdotool(&["key", key]));
                json!({"inputs": inputs})
            }
            Trigger::ContextMenu => {
                let (inputs, popup) = context_menu_reload(&self.census, id, geometry);
                json!({"inputs": inputs, "popup": popup})
            }
            other => panic!("not an OS input trigger: {other:?}"),
        }
    }
}

/// X0: the installed `WebKitGTK` packages and the image this job ran on.
#[test]
fn gh1081_r4_linux_x0_engine_census() {
    let packages = command_output(
        "dpkg-query",
        &[
            "-W",
            "-f=${Package} ${Version}\n",
            "libwebkit2gtk-4.1-0",
            "libjavascriptcoregtk-4.1-0",
            "libgtk-3-0t64",
            "libsoup-3.0-0",
            "xvfb",
            "xdotool",
        ],
    );
    let census = json!({
        "runner": runner_census(),
        "dpkg": packages,
        "pkg_config_webkit2gtk_4_1": command_output("pkg-config", &["--modversion", "webkit2gtk-4.1"]),
        "uname": command_output("uname", &["-a"]),
        "os_release": fs::read_to_string("/etc/os-release").unwrap_or_default(),
        "display": std::env::var("DISPLAY").ok(),
        "session_type": std::env::var("XDG_SESSION_TYPE").ok(),
    });
    eprintln!("KELD_GH1081_R4_CENSUS os=linux {census}");
    assert!(
        packages.contains("libwebkit2gtk-4.1-0 "),
        "installed WebKitGTK version is readable: {packages}"
    );
}

/// X1 + X2 + X3 byte-source atom over the inline control and the custom-scheme
/// arm. Asserts oracle validity only (see `assert_oracles`).
#[test]
fn gh1081_r4_linux_origin_reload_matrix() {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let census = scratch.path().join("gh1081-x11-census.py");
    fs::write(
        &census,
        include_str!("../../fixtures/crash_recovery_r1_x11_windows.py"),
    )
    .expect("write X11 census");
    let mut driver = LinuxDriver { census };
    let scheme = placeholder_scheme();
    let plan = [
        (Arm::Inline, Trigger::RealUrlControl),
        (Arm::Inline, Trigger::JsReload),
        (Arm::Inline, Trigger::ApiReload),
        (Arm::Inline, Trigger::Key("F5")),
        (Arm::Inline, Trigger::Key("ctrl+r")),
        (Arm::Inline, Trigger::ContextMenu),
        (Arm::Scheme, Trigger::JsReload),
        (Arm::Scheme, Trigger::ApiReload),
        (Arm::Scheme, Trigger::Key("F5")),
        (Arm::Scheme, Trigger::Key("ctrl+r")),
        (Arm::Scheme, Trigger::ContextMenu),
    ];
    let records: Vec<Value> = plan
        .iter()
        .map(|(arm, trigger)| run_arm(&mut driver, &scaffold, &scheme, *arm, *trigger))
        .collect();
    eprintln!(
        "KELD_GH1081_R4_SUMMARY os=linux scheme={scheme} rows={}",
        summarize(&records)
    );
    assert_oracles(&records);
}
