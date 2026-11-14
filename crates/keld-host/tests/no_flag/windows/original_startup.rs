//! X5 missing-registration/fixed-clock slice on the ordinary qualified CLI.
//! The hostile preload is separately admitted; it is not product preload evidence.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]

#[path = "support/original_startup_observation.rs"]
mod observation;

use crate::original_bridge_observation as healthy_observation;
use crate::support::original_bridge_run::{self as run, Fixture, OriginalRun};
use std::path::{Path, PathBuf};

fn selected(name: &str) -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")));
    run::assert_d_path(&path);
    path
}

fn healthy(cli: &Path, project: &Path, phase: &str) {
    let mut run = OriginalRun::start(cli, project, Fixture::Bridge);
    run.send("OPEN facade");
    let created = run.record("CREATED ");
    let id = created["id"].as_u64().expect("actual created window");
    let loaded = run.record("LOADED ");
    assert_eq!(loaded["id"], id);
    let association = run.window_view(id);
    let view = association["view"].as_u64().expect("actual native view");
    let page = run.page_terminal(view);
    healthy_observation::assert_first_author_script(&page, &run.hello);
    healthy_observation::assert_native_terminal(&page, "facade", true);
    run.send("COUNTERS");
    let counters = run.record("COUNTERS ");
    assert_eq!(counters["facade"], 1, "actual existing Bun listener");
    assert_eq!(counters["raw"], 0);
    assert_eq!(counters["invalid"], 0);
    println!(
        "KELD_ORIGINAL_X5_HEALTHY {}",
        serde_json::json!({
            "phase":phase,"created":created,"loaded":loaded,"association":association,
            "page":page,"counters":counters,"hello":run.hello,
        })
    );
    run.finish(); // Preserve every healthy owner assertion and disposal contract.
}

#[test]
#[ignore = "Ramani native lease, separately qualified instrumented CLI/Host and exact hostile startup fixture required"]
fn windows_original_missing_registration_preserves_native_qualification_deadline() {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let allowed = selected("KELD_ORIGINAL_NATIVE_ALLOWED_PROJECT");
    let hostile = selected("KELD_ORIGINAL_NATIVE_MISSING_REGISTRATION_PROJECT");
    assert_ne!(allowed, hostile, "hostile input is separately admitted");
    healthy(&cli, &allowed, "before");
    let mut run = OriginalRun::start(&cli, &hostile, Fixture::MissingRegistration);
    run.send("CREATE");
    let created = run.record("CREATED ");
    let window = created["id"].as_u64().expect("actual created window");
    let association = run.window_view(window);
    let view = association["view"].as_u64().expect("actual native view");
    println!(
        "KELD_ORIGINAL_X5_ASSOCIATION {}",
        serde_json::json!({"created":created,"association":association,"hello":run.hello})
    );
    // Same owner captures the real browser/HWND/process handles before issuing
    // LOAD. No QUIT or test-controlled timeout supplies the failure outcome.
    run.finish_missing_registration(view, |trace, control| {
        observation::assert_missing_registration(trace, control, view, window);
    });
    healthy(&cli, &allowed, "after");
}
