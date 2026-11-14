//! X9 cancellation without replacement, using the existing native/process owners.
//! Pending-hold schedules, replacement and same-URI/AC06 remain separate.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]

use crate::original_bridge_observation as observation;

use crate::support::original_bridge_run::{self as run, Fixture, OriginalRun};
use serde_json::Value;
use std::path::PathBuf;

fn selected(name: &str) -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")));
    run::assert_d_path(&path);
    path
}

fn assert_cancelled_a(record: &Value, view: u64) {
    let id = record["cancel_native_id"]
        .as_u64()
        .expect("actual callback NavigationId");
    assert_ne!(id, 0);
    assert_eq!(
        record["set_cancel_succeeded"], true,
        "actual same-args SetCancel result"
    );
    assert_eq!(record["before"]["pending_id"], id);
    assert!(
        record["after"]["pending_id"].is_null(),
        "X9_PAGE_PENDING_NOT_CLEARED: {record}"
    );
    for phase in ["before", "after"] {
        assert_eq!(record[phase]["document"]["view"], view);
        assert_eq!(record[phase]["document_current"], true);
        assert_eq!(record[phase]["source_current"], true);
        assert_eq!(
            record[phase]["callback_depth"], 1,
            "same active native callback"
        );
        // The callback hold still applies inside this observation. Only the
        // later real A effect proves normal posting resumes after callback exit.
        assert_eq!(record[phase]["posts_held"], true);
    }
    assert_ne!(
        record["before"]["document"]["native_commit"], id,
        "distinct actual navigation"
    );
    assert_eq!(record["before"]["document"], record["after"]["document"]);
    assert_eq!(record["after"]["retained_a_current"], true);
    assert_eq!(
        record["same_native_lease"], true,
        "same allocation and full native stamp"
    );
}

fn assert_second_native_effect(page: &Value, before: &Value) {
    assert!(page["error"].is_null(), "actual page failure: {page}");
    assert_eq!(
        page["first"], before["first"],
        "later events cannot replace A's first snapshot"
    );
    assert_eq!(page["routeMarker"], before["routeMarker"]);
    assert_eq!(page["hrefAfterCancel"], before["first"]["href"]);
    assert_eq!(page["issuedCount"], 2);
    assert_eq!(page["expectedCall"], 2);
    assert_eq!(page["cancelRequested"], true);
    assert_eq!(page["resumed"], true);
    let markers = page["markers"].as_array().expect("actual native markers");
    assert_eq!(markers.len(), 3);
    for (marker, expected) in markers.iter().zip(["ready", "cancel", "resume"]) {
        assert_eq!(marker["phase"], expected);
        assert_eq!(marker["document"], before["routeMarker"]);
    }
    let raw = page["rawRecords"]
        .as_array()
        .expect("actual native port records");
    assert!(!raw.is_empty() && raw.len() <= 12);
    let actual: Vec<Value> = raw
        .iter()
        .map(|record| {
            assert_eq!(record["sourceIsNativePort"], true);
            serde_json::from_str(record["text"].as_str().expect("actual native text"))
                .expect("actual native JSON")
        })
        .collect();
    let calls: Vec<_> = actual
        .iter()
        .filter(|record| record["kind"] == "app-call")
        .collect();
    let terminals = page["terminals"]
        .as_array()
        .expect("actual terminal records");
    assert_eq!(calls.len(), 2, "exactly two original Function entries");
    assert_eq!(terminals.len(), 2);
    for (index, (call, terminal)) in calls.iter().zip(terminals).enumerate() {
        let ordinal = u64::try_from(index + 1).expect("two fixed calls");
        assert_eq!(call["v"], 2);
        assert_eq!(call["method"], "sendMessage");
        assert_eq!(call["call"], ordinal);
        assert_eq!(call["document"], before["routeMarker"]);
        assert_eq!(terminal["v"], 2);
        assert_eq!(terminal["kind"], "send-result");
        assert_eq!(terminal["message"], ordinal);
        assert_eq!(terminal["document"], before["routeMarker"]);
        assert_eq!(terminal["disposition"]["status"], "consumed");
        assert!(
            actual.iter().any(|record| record == terminal),
            "terminal must be actual native bytes"
        );
    }
    assert_eq!(terminals[0], before["terminal"]);
    assert_eq!(page["terminal"], terminals[1]);
}

fn assert_pending_cancelled_a(pending_page: &Value, before: &Value) {
    assert_eq!(pending_page["first"], before["first"]);
    assert_eq!(pending_page["issuedCount"], 1);
    assert_eq!(pending_page["resumed"], false);
    let target = pending_page["cancelTarget"]
        .as_str()
        .expect("fixed distinct page URL");
    assert!(target.ends_with("/x9-cancelled-target.html"));
    assert_ne!(pending_page["cancelTarget"], before["first"]["href"]);
}

fn coordination_delivery(page: &Value, phase: &str) -> u64 {
    let markers = page["markers"].as_array().expect("actual native markers");
    let selected: Vec<_> = markers
        .iter()
        .filter(|marker| marker["phase"] == phase)
        .collect();
    assert_eq!(selected.len(), 1, "one actual {phase} coordination marker");
    let marker = selected[0];
    assert_eq!(marker["document"], page["routeMarker"]);
    let delivery = marker["delivery"].as_u64().expect("actual marker delivery");
    let raw = page["rawRecords"]
        .as_array()
        .expect("actual native records");
    assert!(!raw.is_empty() && raw.len() <= 12);
    let mut matching = 0;
    for record in raw {
        let text = record["text"].as_str().expect("actual native text");
        assert!(text.len() <= 2048);
        let value: Value = serde_json::from_str(text).expect("actual native JSON");
        if value["kind"] == "event" && value["delivery"] == delivery {
            assert_eq!(record["sourceIsNativePort"], true);
            assert_eq!(value["v"], 2);
            assert_eq!(value["channel"], "args-obj");
            assert_eq!(value["document"], marker["document"]);
            matching += 1;
        }
    }
    assert_eq!(
        matching, 1,
        "marker delivery comes from one actual native event"
    );
    delivery
}

#[test]
#[ignore = "Ramani native lease, separately qualified instrumented keld+Host and prepared exact cancellation fixture required"]
fn windows_original_cancelled_page_navigation_retains_same_a_and_resumes_ipc() {
    let cli = selected("KELD_ORIGINAL_NATIVE_CLI");
    let project = selected("KELD_ORIGINAL_CANCELLED_NAVIGATION_PROJECT");
    let mut run = OriginalRun::start(&cli, &project, Fixture::CancelledNavigation);
    run.send("OPEN");
    let created = run.record("CREATED ");
    let id = created["id"].as_u64().expect("actual public window id");
    let loaded = run.record("LOADED ");
    assert_eq!(loaded["id"], id);
    let association = run.window_view(id);
    let view = association["view"]
        .as_u64()
        .expect("actual WebviewId association");
    let before = run.page_terminal(view);
    observation::assert_first_author_script(&before, &run.hello);
    observation::assert_native_terminal(&before, "facade", true);
    assert_eq!(before["issuedCount"], 1);
    assert_eq!(before["cancelRequested"], false);
    run.send("COUNTERS");
    let before_counts = run.record("COUNTERS ");
    assert_eq!(
        before_counts,
        serde_json::json!({"before":1,"after":0,"invalid":0})
    );

    // A page effect does not prove its earlier coordination event released DATA.
    let ready_completion =
        run.native_event_storage_cleared(view, coordination_delivery(&before, "ready"), None);
    run.send("CANCEL");
    assert_eq!(run.record("CANCEL_SENT ")["id"], id);
    let cancelled = run.native_record("KELD_ORIGINAL_PAGE_CANCELLATION ");
    assert_cancelled_a(&cancelled, view);
    let pending_page = run.page_when(view, |page| page["cancelRequested"] == true);
    assert_pending_cancelled_a(&pending_page, &before);
    run.send("COUNTERS");
    assert_eq!(
        run.record("COUNTERS "),
        before_counts,
        "navigation supplies no extra IPC effect"
    );

    let navigation = cancelled["before"]["document"]["generation"]
        .as_u64()
        .expect("actual retained A generation");
    assert_eq!(ready_completion["navigation"], navigation);
    let cancel_completion = run.native_event_storage_cleared(
        view,
        coordination_delivery(&pending_page, "cancel"),
        Some(navigation),
    );
    // The actual native cancellation and exact coordination DATA clear release
    // this command. No timer or page flag substitutes for either native fact.
    run.send("RESUME");
    assert_eq!(run.record("RESUME_SENT ")["id"], id);
    let after = run.page_when(view, |page| {
        page["terminals"]
            .as_array()
            .is_some_and(|items| items.len() == 2)
    });
    assert_second_native_effect(&after, &before);
    run.send("COUNTERS");
    let after_counts = run.record("COUNTERS ");
    assert_eq!(
        after_counts,
        serde_json::json!({"before":1,"after":1,"invalid":0})
    );
    let hello = run.hello.clone();
    run.finish_with(|trace, control| {
        let cancellations: Vec<Value> = trace
            .lines()
            .filter_map(|line| line.strip_prefix("KELD_ORIGINAL_PAGE_CANCELLATION "))
            .map(|text| serde_json::from_str(text).expect("actual cancellation JSON"))
            .collect();
        assert_eq!(
            cancellations,
            vec![cancelled.clone()],
            "exactly one observed page cancellation"
        );
        let effects: Vec<Value> = control
            .lines()
            .filter_map(|line| line.strip_prefix("IPC_EFFECT "))
            .map(|text| serde_json::from_str(text).expect("actual Bun effect JSON"))
            .collect();
        assert_eq!(effects.len(), 2);
        for (effect, phase) in effects.iter().zip(["before", "after"]) {
            assert_eq!(effect["phase"], phase);
            assert_eq!(effect["count"], 1);
            assert_eq!(
                effect["value"],
                serde_json::json!({"probe":"keld-original-x9","phase":phase})
            );
        }
        println!(
            "KELD_ORIGINAL_X9_CANCELLED_A {}",
            serde_json::json!({
                "before":before,"cancelled":cancelled,"after":after,
                "before_counts":before_counts,"after_counts":after_counts,
                "created":created,"loaded":loaded,"association":association,"hello":hello,
                "readyCompletion":ready_completion,"cancelCompletion":cancel_completion,
            })
        );
    });
}
