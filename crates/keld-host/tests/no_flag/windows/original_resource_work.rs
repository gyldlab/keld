//! X14 resource-work slice. No actual-Post, callback-stack, dialog or same-URI claim.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]

use crate::original_bridge_observation as bridge;
#[path = "support/original_resource_observation.rs"]
mod observation;

use crate::support::original_bridge_run::{self as run, Fixture, OriginalRun, ResourceTrace};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn selected(name: &str) -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")));
    run::assert_d_path(&path);
    path
}

fn create_barrier(directory: &Path, name: &str) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))
        .expect("fresh parent-owned resource barrier");
    file.write_all(b"actual fixture observation completed\n")
        .expect("write resource barrier");
    file.flush().expect("flush resource barrier");
}

fn fresh_healthy(cli: &Path, project: &Path) {
    let mut run = OriginalRun::start(cli, project, Fixture::Bridge);
    run.send("OPEN facade");
    let created = run.record("CREATED ");
    let id = created["id"].as_u64().expect("actual fresh window");
    assert_eq!(run.record("LOADED ")["id"], id);
    let association = run.window_view(id);
    let view = association["view"]
        .as_u64()
        .expect("actual fresh native view");
    let page = run.page_terminal(view);
    bridge::assert_first_author_script(&page, &run.hello);
    bridge::assert_native_terminal(&page, "facade", true);
    run.send("COUNTERS");
    let counters = run.record("COUNTERS ");
    assert_eq!(counters["facade"], 1);
    assert_eq!(counters["raw"], 0);
    assert_eq!(counters["invalid"], 0);
    run.finish();
}

#[test]
#[ignore = "Ramani native lease, separately qualified instrumented CLI/Host, exact resource and healthy projects required"]
fn windows_original_pending_resource_retains_same_ticket_across_bun_loss_close_and_quit() {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_NATIVE_RESOURCE_PROJECT");
    let healthy = selected("KELD_ORIGINAL_NATIVE_ALLOWED_PROJECT");
    run::assert_d_path(&std::env::temp_dir());
    // Declared first so the existing emergency process owner drops before these
    // test-owned barrier files on failure. Emergency cleanup is never a pass.
    let barrier = tempfile::Builder::new()
        .prefix("keld-x14-resource-")
        .tempdir()
        .expect("D-only resource fixture barrier");
    run::assert_d_path(barrier.path());
    let mut run = OriginalRun::start_resource(&cli, &project, barrier.path());
    let first_hello = run.hello.clone();
    run.send("OPEN");
    let created = run.record("CREATED ");
    let window = created["id"].as_u64().expect("actual resource window");
    let association = run.window_view(window);
    let view = association["view"]
        .as_u64()
        .expect("actual resource native view");
    let page = run.page_terminal(view);
    bridge::assert_first_author_script(&page, &first_hello);
    bridge::assert_native_terminal(&page, "facade", true);
    let effect = run.record("IPC_EFFECT ");
    assert_eq!(effect["count"], 1);
    assert_eq!(effect["bunPid"], first_hello["bunPid"]);
    let bytes = include_bytes!("fixtures/x14-held-resource.svg").len();
    let mut observed = ResourceTrace::default();
    let held = run.resource_record(&mut observed, "hold", "held");
    let ticket = observation::assert_held(&held, "held", view, bytes);
    assert_eq!(held["cancelled"], false);

    let next_bun = run.crash_and_reconnect_resource(window);
    assert_ne!(first_hello["bunPid"], next_bun);
    create_barrier(barrier.path(), "checkpoint");
    let checkpoint = run.resource_record(&mut observed, "hold", "checkpoint");
    assert_eq!(
        observation::assert_held(&checkpoint, "checkpoint", view, bytes),
        ticket
    );
    assert_eq!(checkpoint["cancelled"], false);

    run.retain_resource_native_tree();
    run.send("CLOSE");
    let requested = run.resource_control_record("CLOSE_REQUESTED ");
    assert_eq!(requested["id"], window);
    assert_eq!(requested["bunPid"], next_bun);
    let closed = run.resource_record(&mut observed, "custody", "controller-closed");
    observation::assert_controller_closed(&closed, &ticket, view);
    create_barrier(barrier.path(), "release");
    let released = run.resource_record(&mut observed, "hold", "released");
    assert_eq!(
        observation::assert_held(&released, "released", view, bytes),
        ticket
    );
    assert_eq!(released["cancelled"], true);
    assert_eq!(run.resource_control_record("CLOSED ")["id"], window);
    run.resource_control_record("ALL_CLOSED ");

    // The worker's released row precedes Completed publication. Observe the
    // actual same Pending drop before terminal Quit starts session retirement.
    let dropped = run.resource_record(&mut observed, "custody", "pending-dropped");
    assert_eq!(dropped["prior"]["ticket"], ticket);
    assert_eq!(dropped["ledgerReleaseSucceeded"], true);

    // The task is released after actual Close and before the ordinary terminal
    // Quit. No forced shutdown or held-Quit success is inferred.
    run.finish_with(|trace, control| {
        observation::assert_complete(trace, control, &ticket, view, bytes);
        println!(
            "KELD_ORIGINAL_X14_RESOURCE_WORK {}",
            serde_json::json!({
                "ticket": ticket, "view": view, "window": window,
                "oldBun": first_hello["bunPid"], "nextBun": next_bun,
                "held": held, "checkpoint": checkpoint, "controllerClosed": closed,
                "released": released, "freshHealthyRequired": true,
            })
        );
    });
    barrier
        .close()
        .expect("dispose barrier after product process/profile/EOF receipts");
    fresh_healthy(&cli, &healthy);
    println!("KELD_ORIGINAL_X14_FRESH_HEALTHY_AFTER_RESOURCE_DRAIN");
}
