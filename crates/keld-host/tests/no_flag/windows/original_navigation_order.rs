//! Actual original Event/API entry versus a later Host-owned B navigation.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]

use super::{fresh_healthy, marker, observation as custody, selected};
use crate::original_bridge_observation as bridge;
use crate::support::original_bridge_run::{self as run, Fixture, OriginalRun, QueuedCloseTrace};
#[path = "support/original_navigation_order_observation.rs"]
mod observation;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    BeforeEntry,
    AfterReturn,
    SyntheticBeforeEntry,
}
impl Order {
    const fn name(self) -> &'static str {
        match self {
            Self::BeforeEntry => "before-entry",
            Self::AfterReturn => "after-return",
            Self::SyntheticBeforeEntry => "synthetic-before-entry",
        }
    }
    const fn fixture(self) -> Fixture {
        match self {
            Self::BeforeEntry => Fixture::NavigationBefore,
            Self::AfterReturn => Fixture::NavigationAfter,
            Self::SyntheticBeforeEntry => Fixture::NavigationSynthetic,
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep the actual native ordering, held original DATA, B observation, normal cleanup and healthy follower in one visible scenario."
)]
fn navigation_order_case(order: Order) {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_NATIVE_NAVIGATION_PROJECT");
    let healthy = selected("KELD_ORIGINAL_NATIVE_ALLOWED_PROJECT");
    run::assert_d_path(&std::env::temp_dir());
    let barrier = tempfile::Builder::new()
        .prefix("keld-x10-navigation-")
        .tempdir()
        .expect("D-only owned navigation control directory");
    let mut run =
        OriginalRun::start_navigation_order(&cli, &project, barrier.path(), order.fixture());
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
    assert_eq!(initial["documentKind"], "A");
    assert_eq!(initial["entered"], 0);
    let ready_delivery = initial["routeDelivery"]
        .as_u64()
        .expect("actual A marker Event");
    run.native_event_storage_cleared(view, ready_delivery, None);
    run.send("COUNTERS");
    let counters = run.record("COUNTERS ");
    assert_eq!(counters["A"], 1);
    assert_eq!(counters["B"], 0);
    assert_eq!(counters["invalid"], 0);
    run.retain_resource_native_tree();
    let mut trace = QueuedCloseTrace::default();
    marker(barrier.path(), "arm");
    if order != Order::SyntheticBeforeEntry {
        let armed = run.queued_close_record(&mut trace, "hold", |value| {
            value["phase"]
                == if order == Order::AfterReturn {
                    "armed"
                } else {
                    "held"
                }
        });
        assert_eq!(
            armed["mode"],
            if order == Order::AfterReturn {
                "after-next-event"
            } else {
                "before-intake"
            }
        );
    }
    run.send("SEND");
    assert_eq!(run.record("SEND_RETURNED ")["id"], window);
    let admitted = run.queued_close_record(&mut trace, "scope", |value| {
        value["phase"] == "event-admitted"
            && value["window"] == window
            && custody::target_in(value, view, ready_delivery).is_some()
    });
    let key = custody::admission(&admitted, &association, ready_delivery);
    let a_commit = run.queued_close_record(&mut trace, "navigation", |value| {
        value["phase"] == "content-committed"
            && value["state"]["view"] == view
            && value["state"]["generation"] == key.navigation
    });

    if order == Order::AfterReturn {
        let held = run.queued_close_record(&mut trace, "hold", |value| value["phase"] == "held");
        assert_eq!(held["delivery"], key.message);
        assert_eq!(held["webview"], key.view);
        assert_eq!(held["navigation"], key.navigation);
        run.queued_close_record(&mut trace, "navigation", |value| {
            observation::target_post(value, &key) && value["return_sequence"].is_u64()
        });
    } else if order == Order::SyntheticBeforeEntry {
        // Do not pre-filter result/call count: the actual-wrapper falsifier must
        // reach normal cleanup, then fail the exact unchanged final assertion.
        run.queued_close_record(&mut trace, "navigation", |value| {
            observation::target_post(value, &key) && value["synthetic_enter"].is_object()
        });
    }

    // This later command schedules ordinary Host navigation. In the after cell
    // only actual API return was awaited; no A consumption barrier is inserted.
    run.send("NAVIGATE_B");
    let b_start = run.queued_close_record(&mut trace, "navigation", |value| {
        value["phase"] == "navigation-starting"
            && value["state"]["view"] == view
            && value["admitted"] == true
            && value["native_id"] != a_commit["native_id"]
            && value["state"]["generation"].is_null()
    });
    let b_commit = run.queued_close_record(&mut trace, "navigation", |value| {
        value["phase"] == "content-committed"
            && value["state"]["view"] == view
            && value["state"]["generation"]
                .as_u64()
                .is_some_and(|generation| generation != key.navigation)
            && value["native_id"] == b_start["native_id"]
    });
    if order != Order::SyntheticBeforeEntry {
        marker(barrier.path(), "release");
        run.queued_close_record(&mut trace, "hold", |value| value["phase"] == "released");
    }
    assert_eq!(run.record("NAVIGATED ")["id"], window);
    let b_page = run.navigation_page_terminal(view);
    bridge::assert_first_author_script(&b_page, &run.hello);
    bridge::assert_native_terminal(&b_page, "facade", true);
    assert_eq!(b_page["routeMarkerKind"], "route-state");
    assert!(b_page["routeDelivery"].is_null());
    // B's actual CONTROL marker owns no DATA delivery. Its normal original
    // sendMessage/Consumed terminal above is independent of A's retained Event.
    run.send("COUNTERS");
    let final_counts = run.record("COUNTERS ");
    assert_eq!(final_counts["A"], 1);
    assert_eq!(final_counts["B"], 1);
    assert_eq!(final_counts["invalid"], 0);
    run.send("CLOSE");
    assert_eq!(run.record("CLOSE_REQUESTED ")["id"], window);
    assert_eq!(run.record("CLOSED ")["id"], window);
    run.record("ALL_CLOSED ");
    // An API-returned Event may remain unconsumed. Its actual controller Close
    // is the retirement fence; never turn native storage drop into an earlier
    // A-consumption barrier before observing B.
    run.queued_close_record(&mut trace, "owner", |value| {
        value["phase"] == "native-storage-dropped" && custody::same_event(value, &key)
    });
    let mut receipt = serde_json::Value::Null;
    run.finish_with(|native, control| {
        assert!(control.contains("QUIT_ACCEPTED "));
        receipt = observation::capture(
            native,
            &key,
            order.name(),
            [&initial, &b_page],
            [&admitted, &a_commit, &b_start, &b_commit],
        );
    });
    barrier
        .close()
        .expect("fixture barrier disposal follows actual product cleanup");
    fresh_healthy(&cli, &healthy);
    println!(
        "KELD_ORIGINAL_X10_FRESH_HEALTHY_AFTER_NAVIGATION case={}",
        order.name()
    );
    println!("KELD_ORIGINAL_X10_NAVIGATION_ORDER {receipt}");
    observation::assert_complete(&receipt);
}

#[test]
#[ignore = "Ramani native owner, qualified instrumented pair and private navigation/healthy projects required"]
fn windows_original_navigation_before_event_entry_rejects_old_document() {
    navigation_order_case(Order::BeforeEntry);
}

#[test]
#[ignore = "Ramani native owner, qualified instrumented pair and private navigation/healthy projects required"]
fn windows_original_event_entry_before_navigation_never_leaks_to_new_document() {
    navigation_order_case(Order::AfterReturn);
}

#[test]
#[ignore = "Ramani native owner, qualified instrumented pair and private navigation/healthy projects required"]
fn windows_original_synthetic_navigation_callback_suppresses_actual_post() {
    navigation_order_case(Order::SyntheticBeforeEntry);
}
