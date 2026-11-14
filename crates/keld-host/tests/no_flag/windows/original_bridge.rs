//! X2/X4 use a separately qualified instrumented CLI/Host pair, not default-image Q5 evidence.
//! The input locators below never override production admission or qualification.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]

use crate::original_bridge_observation as observation;
use crate::support::original_bridge_run as run;

use run::{Fixture, OriginalRun};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn selected(name: &str) -> PathBuf {
    let value =
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")));
    run::assert_d_path(&value);
    value
}

fn remove_one_grant(value: &mut Value) -> usize {
    match value {
        Value::Array(items) => {
            let before = items.len();
            items.retain(|item| item.as_str() != Some("el:app-load-finished"));
            before - items.len() + items.iter_mut().map(remove_one_grant).sum::<usize>()
        }
        Value::Object(items) => items.values_mut().map(remove_one_grant).sum(),
        _ => 0,
    }
}

fn assert_same_fixture_pair(allowed: &Path, denied: &Path) {
    for relative in [
        "keld.config.ts",
        "keld.build.ts",
        "package.json",
        "package-lock.json",
        "main/app.mjs",
        "bootstrap/native-preload.cjs",
        "client/assets/editor.html",
        "client/assets/original-bridge-probe.js",
    ] {
        assert_eq!(
            fs::read(allowed.join(relative)).expect("allowed input"),
            fs::read(denied.join(relative)).expect("denied input"),
            "{relative}"
        );
    }
    let mut left: Value = serde_json::from_slice(
        &fs::read(allowed.join("keld.permissions.jsonc")).expect("allowed permission"),
    )
    .expect("fixture uses ordinary JSON subset of JSONC");
    let right: Value = serde_json::from_slice(
        &fs::read(denied.join("keld.permissions.jsonc")).expect("denied permission"),
    )
    .expect("fixture uses ordinary JSON subset of JSONC");
    assert_eq!(left["profile"], "legacy");
    assert_eq!(remove_one_grant(&mut left), 1, "one exact existing grant");
    assert_eq!(left, right, "only el:app-load-finished may differ");
}

#[test]
#[ignore = "Ramani native lease, separately qualified instrumented keld+Host and prepared original fixture projects required"]
fn windows_original_first_author_script_and_actual_allowed_denied_carriage() {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let allowed = selected("KELD_ORIGINAL_NATIVE_ALLOWED_PROJECT");
    let denied = selected("KELD_ORIGINAL_NATIVE_DENIED_PROJECT");
    assert_same_fixture_pair(&allowed, &denied);
    // Each mode gets a fresh process/document/profile; call counters cannot
    // collide with a previous page's sequence. The final allowed run proves
    // recovery from the denied case, not merely an initial healthy snapshot.
    for (project, permitted) in [(&allowed, true), (&denied, false), (&allowed, true)] {
        for mode in ["facade", "raw"] {
            let mut run = OriginalRun::start(&cli, project, Fixture::Bridge);
            run.send(&format!("OPEN {mode}"));
            let created = run.record("CREATED ");
            let id = created["id"].as_u64().expect("actual created id");
            let loaded = run.record("LOADED ");
            assert_eq!(loaded["id"], id);
            let association = run.window_view(id);
            let view = association["view"]
                .as_u64()
                .expect("actual associated WebviewId");
            let page = run.page_terminal(view);
            observation::assert_first_author_script(&page, &run.hello);
            observation::assert_native_terminal(&page, mode, permitted);
            run.send("COUNTERS");
            let counters = run.record("COUNTERS ");
            let expected = u64::from(permitted);
            assert_eq!(counters[mode], expected, "actual Bun listener {mode}");
            assert_eq!(counters[if mode == "raw" { "facade" } else { "raw" }], 0);
            assert_eq!(counters["invalid"], 0);
            println!(
                "KELD_ORIGINAL_X2_X4 {}",
                serde_json::json!({
                    "mode":mode,"permitted":permitted,"page":page,"counters":counters,
                    "hello":run.hello,"created":created,"loaded":loaded,"association":association,
                })
            );
            run.finish();
        }
    }
}

#[test]
#[ignore = "Ramani native lease, separately qualified instrumented keld+Host and prepared exact inline fixture required"]
fn windows_original_first_inline_observes_qualified_preload() {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_NATIVE_INLINE_PROJECT");
    let mut run = OriginalRun::start(&cli, &project, Fixture::FirstInline);
    run.send("OPEN facade");
    let created = run.record("CREATED ");
    let id = created["id"].as_u64().expect("actual created id");
    let association = run.window_view(id);
    let view = association["view"]
        .as_u64()
        .expect("actual associated WebviewId");
    // The frozen first snapshot is scored before LOADED or readiness can hide
    // an omitted preload behind a later bind/terminal timeout.
    let first = run.page_first_author(view);
    observation::assert_first_author_snapshot(&first["first"], &run.hello);
    assert!(
        first["error"].is_null(),
        "actual page failure after first snapshot: {first}"
    );
    let loaded = run.record("LOADED ");
    assert_eq!(loaded["id"], id);
    let page = if first["terminal"].is_null() {
        run.page_terminal(view)
    } else {
        first.clone()
    };
    assert_eq!(
        first["first"], page["first"],
        "later readiness cannot repair first"
    );
    observation::assert_native_terminal(&page, "facade", true);
    run.send("COUNTERS");
    let counters = run.record("COUNTERS ");
    assert_eq!(counters["facade"], 1, "actual Bun listener effect");
    assert_eq!(counters["raw"], 0);
    assert_eq!(counters["invalid"], 0);
    println!(
        "KELD_ORIGINAL_FIRST_INLINE {}",
        serde_json::json!({
            "first":first,"page":page,"counters":counters,"hello":run.hello,
            "created":created,"loaded":loaded,"association":association,
        })
    );
    run.finish();
}

#[test]
#[ignore = "Ramani native lease and the separately qualified shipping CLI/Host pair required"]
fn windows_original_boot8_stage_refuses_missing_and_unsupported_dev_leases() {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_NATIVE_ALLOWED_PROJECT");
    let mut run = OriginalRun::start(&cli, &project, Fixture::Bridge);
    run.assert_boot8_lease_refusals();

    // The original CLI-owned BOOT8 stage remains healthy after both refused probes.
    run.send("OPEN facade");
    let created = run.record("CREATED ");
    let id = created["id"].as_u64().expect("actual created id");
    assert_eq!(run.record("LOADED ")["id"], id);
    let association = run.window_view(id);
    let view = association["view"].as_u64().expect("actual WebviewId");
    let page = run.page_terminal(view);
    observation::assert_first_author_script(&page, &run.hello);
    observation::assert_native_terminal(&page, "facade", true);
    run.send("COUNTERS");
    let counters = run.record("COUNTERS ");
    assert_eq!(counters["facade"], 1);
    assert_eq!(counters["raw"], 0);
    assert_eq!(counters["invalid"], 0);
    run.finish();
}
