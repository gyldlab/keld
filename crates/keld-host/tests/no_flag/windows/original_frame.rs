//! Real child-owned `Frame2` ingress under the unchanged original file-source CSP.
//! This instrumented Host fixture supplies no default-image Q5 qualification.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]

#[path = "support/original_frame_observation.rs"]
mod observation;

use crate::support::original_bridge_run::{Fixture, OriginalRun, assert_d_path};
use std::path::PathBuf;

fn selected(name: &str) -> PathBuf {
    let value =
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")));
    assert_d_path(&value);
    value
}

#[test]
#[ignore = "Ramani native lease, separately qualified instrumented CLI/Host and byte-pinned initial-frame project required"]
fn windows_original_initial_blank_child_ingress_is_separate_from_top_dispatch() {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_NATIVE_FRAME_PROJECT");
    let mut run = OriginalRun::start(&cli, &project, Fixture::InitialFrame);
    run.send("OPEN frame");
    let created = run.record("CREATED ");
    let window = created["id"]
        .as_u64()
        .expect("actual public BrowserWindow id");
    let loaded = run.record("LOADED ");
    assert_eq!(loaded["id"], window);
    let association = run.window_view(window);
    let view = association["view"]
        .as_u64()
        .expect("actual associated WebviewId");

    let ready = run.native_record("KELD_X3_FRAME ");
    observation::assert_ready_record(&ready, view);
    run.send("ARM_FRAME");
    assert_eq!(run.record("COORDINATED ")["phase"], "arm-frame");
    let ingress = run.native_record("KELD_X3_FRAME ");
    // Wait only for supplementary child DOM diagnostics, not a synthetic
    // native receipt. The separate actual callback above is mandatory.
    let frame_page = run.page_when(view, |page| page["frame"]["sent"] == true);
    observation::assert_actual_frame_ingress(&frame_page, &ingress, view);
    run.send("COUNTERS");
    observation::assert_no_effect_before_top(&run.record("COUNTERS "));
    let arm_completion = run.consumed_event_storage_cleared(view, &frame_page);

    run.send("TOP_CONTROL");
    assert_eq!(run.record("COORDINATED ")["phase"], "top-control");
    let page = run.page_terminal(view);
    run.send("COUNTERS");
    let counters = run.record("COUNTERS ");
    observation::assert_healthy_top(&page, &counters);
    println!(
        "KELD_ORIGINAL_X3_INITIAL_BLANK {}",
        serde_json::json!({
            "created":created,"loaded":loaded,"association":association,"ready":ready,
            "ingress":ingress,"page":page,"counters":counters,"armCompletion":arm_completion,
        })
    );
    run.finish_with(|stderr, control| {
        observation::assert_complete_native_trace(stderr, &page, view);
        observation::assert_complete_control_trace(control);
    });
}
