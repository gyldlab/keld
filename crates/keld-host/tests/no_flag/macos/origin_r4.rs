//! GH-1081 round-4 research only (never merged), macOS `WKWebView`: KEL-79
//! origin experiments X0 (engine census), X1 (origin, secure context,
//! storage), X2 (reload matrix) and the X3 byte-source atom, in the isolated
//! `gh1081_origin_lab` harness. Arms: inline HTML string (control) and a
//! placeholder per-app custom scheme. Input is real `CoreGraphics` events through
//! the round-2 Swift tool; observation is the page's own reports, the harness's
//! control plane and a read-only `screencapture` of the harness window.
//!
//! The real-host X3 control row is the existing
//! `gh1081_e4_typed_draft_document_and_heap_survive_bun_crash`; the workflow
//! selects it next to these rows. A candidate arm on the real host would need
//! a production hook (`keld-wv` scheme registration in `media.rs` and a
//! `NavTarget::Url` boot in `app_session.rs`), so it is not run.

use crate::crash_recovery_r2::{compile_tool, pixels, tool_json};
use crate::crash_recovery_r2_probe::scaffold_html;
use crate::origin_r4_lab::{
    Arm, Driver, Lab, Trigger, assert_oracles, command_output, placeholder_scheme, run_arm,
    runner_census, summarize,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) struct MacDriver {
    pub(crate) tool: PathBuf,
    pub(crate) root: tempfile::TempDir,
}

impl Driver for MacDriver {
    fn window(&mut self, pid: u32, title: &str, deadline: Instant) -> Value {
        loop {
            let found = tool_json(&self.tool, &["find", &pid.to_string(), title]);
            if found["ok"] == true {
                return found;
            }
            assert!(
                Instant::now() < deadline,
                "no on-screen lab window for pid {pid}: {found}"
            );
            thread::park_timeout(Duration::from_millis(50));
        }
    }

    fn pixels(&mut self, window: &Value, label: &str) -> Value {
        let id = window["window"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
            .expect("CG window id");
        pixels(&self.tool, id, self.root.path(), label)
    }

    fn input(
        &mut self,
        lab: &mut Lab,
        _window: &Value,
        _geometry: &Value,
        title: &str,
        trigger: Trigger,
    ) -> Value {
        let action = match trigger {
            Trigger::Key(key) => key,
            Trigger::ContextMenu => "context-reload",
            other => panic!("not an OS input trigger: {other:?}"),
        };
        tool_json(&self.tool, &["input", &lab.pid.to_string(), title, action])
    }
}

/// X0: the engine and image this job ran on (recorded, not asserted beyond
/// readability of the `WebKit` framework version).
#[test]
fn gh1081_r4_macos_x0_engine_census() {
    let webkit = command_output(
        "/usr/bin/defaults",
        &[
            "read",
            "/System/Library/Frameworks/WebKit.framework/Resources/version.plist",
        ],
    );
    let census = json!({
        "runner": runner_census(),
        "sw_vers": command_output("/usr/bin/sw_vers", &[]),
        "uname": command_output("/usr/bin/uname", &["-a"]),
        "webkit_framework_version_plist": webkit,
        "safari": command_output(
            "/usr/bin/defaults",
            &["read", "/Applications/Safari.app/Contents/Info.plist", "CFBundleShortVersionString"],
        ),
        "swiftc": command_output("/usr/bin/xcrun", &["swiftc", "--version"]),
    });
    eprintln!("KELD_GH1081_R4_CENSUS os=macos {census}");
    assert!(
        webkit.contains("CFBundleVersion"),
        "WebKit framework version.plist is readable: {webkit}"
    );
}

/// X1 + X2 + X3 byte-source atom over the inline control and the custom-scheme
/// arm. Asserts oracle validity only (see `assert_oracles`).
#[test]
fn gh1081_r4_macos_origin_reload_matrix() {
    let scratch = tempfile::tempdir().expect("scaffold scratch");
    let scaffold = scaffold_html(scratch.path());
    let tool = compile_tool(scratch.path());
    let mut driver = MacDriver {
        tool,
        root: tempfile::tempdir().expect("pixels scratch"),
    };
    let scheme = placeholder_scheme();
    let plan = [
        (Arm::Inline, Trigger::RealUrlControl),
        (Arm::Inline, Trigger::JsReload),
        (Arm::Inline, Trigger::ApiReload),
        (Arm::Inline, Trigger::Key("cmd-r")),
        (Arm::Inline, Trigger::ContextMenu),
        (Arm::Scheme, Trigger::JsReload),
        (Arm::Scheme, Trigger::ApiReload),
        (Arm::Scheme, Trigger::Key("cmd-r")),
        (Arm::Scheme, Trigger::ContextMenu),
    ];
    let records: Vec<Value> = plan
        .iter()
        .map(|(arm, trigger)| run_arm(&mut driver, &scaffold, &scheme, *arm, *trigger))
        .collect();
    eprintln!(
        "KELD_GH1081_R4_SUMMARY os=macos scheme={scheme} rows={}",
        summarize(&records)
    );
    assert_oracles(&records);
}
