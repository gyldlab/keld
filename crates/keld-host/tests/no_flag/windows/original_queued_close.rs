//! X10 exact-controller close schedules and live closed-producer refusal.
//! Selected diagnostic and in-call
//! reentrant Post/Close are separate obligations and receive no credit here.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]

use crate::original_bridge_observation as bridge;
#[path = "support/original_fresh_offer_observation.rs"]
mod fresh_offer;
#[path = "original_native_capacity.rs"]
mod native_capacity;
#[path = "original_navigation_order.rs"]
mod navigation_order;
#[path = "support/original_queued_close_observation.rs"]
mod observation;
#[path = "original_selected_diagnostic.rs"]
mod selected_diagnostic;
use crate::support::original_bridge_run::{self as run, Fixture, OriginalRun, QueuedCloseTrace};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn selected(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")));
    run::assert_d_path(&path);
    path
}
fn marker(directory: &Path, name: &str) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))
        .expect("fresh owned native control marker");
    file.write_all(b"parent observed the required actual boundary\n")
        .expect("marker bytes");
    file.flush().expect("flush control marker");
}
fn fresh_healthy(cli: &Path, project: &Path) {
    let mut run = OriginalRun::start(cli, project, Fixture::Bridge);
    run.send("OPEN facade");
    let window = run.record("CREATED ")["id"]
        .as_u64()
        .expect("fresh actual window");
    assert_eq!(run.record("LOADED ")["id"], window);
    let view = run.window_view(window)["view"]
        .as_u64()
        .expect("fresh actual view");
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

#[derive(Clone, Copy)]
enum CloseCase {
    NeverEntered,
    ReturnedRetained,
    FreshOffers,
}

impl CloseCase {
    fn name(self) -> &'static str {
        match self {
            Self::NeverEntered => "never-entered",
            Self::ReturnedRetained => "returned-retained",
            Self::FreshOffers => "closed-producer",
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep real admission, same-controller Close, storage release and cleanup barriers visible in one scenario."
)]
fn queued_close_case(case: CloseCase) {
    let after_event = matches!(case, CloseCase::ReturnedRetained);
    let probes_fresh_offers = matches!(case, CloseCase::FreshOffers);
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_NATIVE_QUEUED_CLOSE_PROJECT");
    let healthy = selected("KELD_ORIGINAL_NATIVE_ALLOWED_PROJECT");
    run::assert_d_path(&std::env::temp_dir());
    // Existing emergency process owner drops before these fixture files.
    // It never supplies product cleanup or native acceptance credit.
    let barrier = tempfile::Builder::new()
        .prefix("keld-x10-queued-close-")
        .tempdir()
        .expect("D-only owned control directory");
    let mut run = if probes_fresh_offers {
        OriginalRun::start_fresh_offer(&cli, &project, barrier.path())
    } else {
        OriginalRun::start_queued_close(&cli, &project, barrier.path(), after_event)
    };
    run.send("OPEN");
    let window = run.record("CREATED ")["id"]
        .as_u64()
        .expect("actual window");
    assert_eq!(run.record("LOADED ")["id"], window);
    let association = run.window_view(window);
    let view = association["view"].as_u64().expect("actual native view");
    let initial = run.page_terminal(view);
    bridge::assert_first_author_script(&initial, &run.hello);
    bridge::assert_native_terminal(&initial, "facade", true);
    assert_eq!(initial["entered"], 0);
    let ready_delivery = initial["routeDelivery"]
        .as_u64()
        .expect("actual initial native Event");
    run.native_event_storage_cleared(view, ready_delivery, None);
    run.send("COUNTERS");
    let initial_counts = run.record("COUNTERS ");
    assert_eq!(initial_counts["facade"], 1);
    assert_eq!(initial_counts["raw"], 0);
    assert_eq!(initial_counts["invalid"], 0);
    // Reuse the existing exact preclosed-browser HANDLE custody, before the
    // five-second scheduling budget begins; census time cannot extend that budget.
    run.retain_resource_native_tree();
    let mut trace = QueuedCloseTrace::default();

    marker(barrier.path(), "arm");
    let expected_mode = if after_event {
        "after-next-event"
    } else {
        "before-intake"
    };
    let armed = run.queued_close_record(&mut trace, "hold", |v| {
        v["phase"] == if after_event { "armed" } else { "held" }
    });
    assert_eq!(armed["mode"], expected_mode);
    run.send("SEND");
    assert_eq!(run.record("SEND_RETURNED ")["id"], window);
    let admitted = run.queued_close_record(&mut trace, "scope", |v| {
        v["phase"] == "event-admitted"
            && v["window"] == window
            && observation::target_in(v, view, ready_delivery).is_some()
    });
    let key = observation::admission(&admitted, &association, ready_delivery);

    if after_event {
        let held = run.queued_close_record(&mut trace, "hold", |v| v["phase"] == "held");
        assert_eq!(held["mode"], expected_mode);
        assert_eq!(held["webview"], key.view);
        assert_eq!(held["navigation"], key.navigation);
        assert_eq!(held["delivery"], key.message);
        let post = run.queued_close_record(&mut trace, "owner", |v| {
            v["phase"] == "post-return" && observation::same_event(v, &key)
        });
        assert_eq!(post["native_ok"], true);
        assert_eq!(post["post_phase"], "returned");
        let page = run.page_when(view, |page| {
            page["entered"] == 1 && page["targetDelivery"] == key.message
        });
        assert_eq!(page["routeMarker"], initial["routeMarker"]);
        // Real consumer/native halves settle, while the original EventReleased
        // outcome and native body remain in their actual retained owners.
        run.queued_close_record(&mut trace, "diagnostic", |v| {
            let charge = &v["charge"];
            charge["webview"] == key.view
                && charge["navigation"] == key.navigation
                && charge["message"] == key.message
                && charge["consumer_done"] == true
                && charge["native_done"] == true
                && charge["event_release_sent"] == true
                && charge["native_storage_released"] == false
        });
    } else {
        let page = run.page_when(view, |page| page["terminal"].is_object());
        assert_eq!(
            page["entered"], 0,
            "held Event has not executed its real page listener"
        );
        assert!(page["targetDelivery"].is_null());
    }

    run.send("CLOSE");
    assert_eq!(run.record("CLOSE_REQUESTED ")["id"], window);
    let offers = run.queued_close_record(&mut trace, "scope", |v| {
        v["phase"] == "offers-closed" && observation::same_scope(v, &key)
    });
    let control_id = observation::retained_at_close(&offers, &key);
    let fresh = probes_fresh_offers.then(|| {
        let record = run.queued_close_record(&mut trace, "fresh", |value| {
            value["phase"] == "closed-producer-probed" && observation::same_scope(value, &key)
        });
        fresh_offer::boundary(&record, &key);
        record
    });
    let closed = run.queued_close_record(&mut trace, "owner", |v| {
        v["phase"] == "controller-closed" && v["view"] == view
    });
    assert_eq!(closed["actual_closed"], true);
    assert_eq!(
        closed["native_storage_quiescent"], false,
        "same controller Close cannot dispose queued DATA/CONTROL"
    );
    let blocked_drain = run.queued_close_record(&mut trace, "owner", |v| {
        v["phase"] == "view-drain" && v["view"] == view && v["drained"] == false
    });
    assert_eq!(blocked_drain["drained"], false);

    // Parent releases only after the independent actual same-controller Close
    // and retained charge observations. Cross-thread trace order is not the barrier.
    marker(barrier.path(), "release");
    run.queued_close_record(&mut trace, "hold", |v| v["phase"] == "released");
    run.queued_close_record(&mut trace, "owner", |v| {
        v["phase"] == "native-storage-dropped" && observation::same_event(v, &key)
    });
    run.queued_close_record(&mut trace, "owner", |v| {
        v["phase"] == "route-storage-dropped"
            && v["view"] == view
            && v["navigation"] == key.navigation
            && v["ready"] == false
    });
    run.queued_close_record(&mut trace, "scope", |v| {
        observation::physically_quiescent(v, &key)
    });
    assert_eq!(run.record("CLOSED ")["id"], window);
    run.record("ALL_CLOSED ");

    run.finish_with(|native, control| {
        observation::complete(native, &key, after_event, control_id);
        if probes_fresh_offers {
            fresh_offer::complete(native, &key);
        }
        assert!(control.contains("QUIT_ACCEPTED "));
        println!(
            "KELD_ORIGINAL_X10_QUEUED_CLOSE {}",
            serde_json::json!({
                "case": case.name(),
                "freshOffer": fresh,
                "window": key.window, "incarnation": key.incarnation, "view": key.view,
                "navigation": key.navigation, "message": key.message,
                "nativeReleaseId": key.release_id, "controlReleaseId": control_id,
                "admitted": admitted, "offersClosed": offers, "controllerClosed": closed,
                "freshHealthyRequired": true,
            })
        );
    });
    barrier
        .close()
        .expect("fixture barrier disposal follows actual product cleanup");
    fresh_healthy(&cli, &healthy);
    println!(
        "KELD_ORIGINAL_X10_FRESH_HEALTHY_AFTER_DRAIN case={}",
        case.name()
    );
}

#[test]
#[ignore = "Ramani native lease, qualified instrumented pair and exact queued-close/healthy projects required"]
fn windows_original_queued_event_and_route_state_retire_after_same_controller_close() {
    queued_close_case(CloseCase::NeverEntered);
}

#[test]
#[ignore = "Ramani native lease, qualified instrumented pair and exact queued-close/healthy projects required"]
fn windows_original_returned_event_retains_storage_through_same_controller_close() {
    queued_close_case(CloseCase::ReturnedRetained);
}

#[test]
#[ignore = "Ramani native lease, qualified instrumented pair and exact queued-close/healthy projects required"]
fn windows_original_closed_producer_refuses_fresh_offers_after_actual_scope_close() {
    queued_close_case(CloseCase::FreshOffers);
}
