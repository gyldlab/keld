//! Independent receipt checks over actual native rows and complete B port bytes.
//! Offsets associate raw rows; only same-STA rows or the per-view sequence order events.
use super::super::observation::{self as custody, Key};
use crate::support::original_bridge_run::{QueuedCloseRecord, QueuedCloseTrace};
use serde_json::{Value, json};

const CANARY: &[u8] = b"\x06\x01\x00\x05\x12\x00keld-x10-target-v1";

pub(super) fn target_post(value: &Value, key: &Key) -> bool {
    value["phase"] == "post-attempt"
        && value["target"]["view"] == key.view
        && value["target"]["generation"] == key.navigation
        && value["event_storage"]["delivery"] == key.message
}
fn wrapped(row: &QueuedCloseRecord) -> Value {
    json!({"end": row.end, "value": row.value})
}
fn selected(trace: &QueuedCloseTrace, kind: &str, keep: impl Fn(&Value) -> bool) -> Value {
    Value::Array(
        trace
            .records
            .iter()
            .filter(|row| row.kind == kind && keep(&row.value))
            .map(wrapped)
            .collect(),
    )
}
fn cut(trace: &QueuedCloseTrace, kind: &str, value: &Value) -> Value {
    // Matching is structural only. Every semantic assertion follows both normal endings.
    let matches: Vec<_> = trace
        .records
        .iter()
        .filter(|row| row.kind == kind && row.value == *value)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "one exact raw record for the observed cut"
    );
    wrapped(matches[0])
}
fn page_readbacks(native: &str, view: u64) -> Value {
    let prefix = format!("PAGE_READBACK {view} ");
    let mut end = 0;
    let mut rows = Vec::new();
    for line in native.split_inclusive('\n') {
        end += line.len();
        if let Some(text) = line.trim_end_matches(['\r', '\n']).strip_prefix(&prefix) {
            let actual: Value =
                serde_json::from_str(text).expect("actual native ExecuteScript readback");
            rows.push(json!({"end":end,"value":actual}));
        }
    }
    Value::Array(rows)
}

pub(super) fn capture(
    native: &str,
    key: &Key,
    case: &str,
    pages: [&Value; 2],
    cuts: [&Value; 4],
) -> Value {
    let trace = QueuedCloseTrace::from_complete(native);
    let owner = |phase: &str| {
        selected(&trace, "owner", |v| {
            v["phase"] == phase && custody::same_event(v, key)
        })
    };
    let view = |phase: &str| {
        selected(&trace, "owner", |v| {
            v["phase"] == phase && v["view"] == key.view
        })
    };
    json!({
        "schema": 1, "case": case,
        "key": {"window":key.window,"incarnation":key.incarnation,"view":key.view,
            "navigation":key.navigation,"message":key.message,"native_release_id":key.release_id,"retained":key.retained},
        "page_a":pages[0],"page_b":pages[1],
        "page_readbacks":page_readbacks(native,key.view),
        "admission":cut(&trace,"scope",cuts[0]),"a_commit":cut(&trace,"navigation",cuts[1]),
        "b_start":cut(&trace,"navigation",cuts[2]),"b_commit":cut(&trace,"navigation",cuts[3]),
        "target_posts":selected(&trace,"navigation",|v| target_post(v,key)),
        "marker_posts":selected(&trace,"navigation",|v| v["phase"] == "post-attempt"
            && v["target"]["view"] == key.view && v["target"]["generation"] == cuts[3]["state"]["generation"]
            && v["carrier_kind"] == "route-state" && v["route_ready"] == true),
        "consumed":selected(&trace,"navigation",|v| v["phase"] == "consumer-observed"
            && v["consumed"]["view"] == key.view && v["consumed"]["delivery"] == key.message
            && v["consumed"]["generation"] == key.navigation),
        "callbacks":selected(&trace,"navigation",|v| v["phase"] == "observation-overflow"
            || v["invalid"] == true || v["call_active"] == true || v["in_call_records"].as_u64().is_some_and(|n| n != 0)
            || (v["state"]["view"] == key.view && matches!(v["phase"].as_str(),
                Some("callback-enter" | "callback-leave" | "navigation-starting" | "content-committed")))),
        "ui_deliveries":owner("ui-delivery"),"owner_posts":owner("post-return"),
        "retirements":owner("retirement-handed-off"),"releases":owner("release-observed"),
        "drops":owner("native-storage-dropped"),"controller_closes":view("controller-closed"),
        "view_drains":view("view-drain"),
        "quiescent_scopes":selected(&trace,"scope",|v| custody::physically_quiescent(v,key)),
        "hold":selected(&trace,"hold",|_| true),
    })
}

fn rows<'a>(receipt: &'a Value, field: &str) -> &'a [Value] {
    receipt[field].as_array().expect("actual receipt array")
}
fn one<'a>(receipt: &'a Value, field: &str) -> &'a Value {
    let values = rows(receipt, field);
    assert_eq!(values.len(), 1, "one actual {field} record");
    &values[0]
}
fn number(value: &Value) -> u64 {
    value.as_u64().expect("actual unsigned native scalar")
}
fn end(row: &Value) -> u64 {
    let value = number(&row["end"]);
    assert!(value > 0, "actual raw byte end");
    value
}
fn value(row: &Value) -> &Value {
    end(row);
    assert!(row["value"].is_object());
    &row["value"]
}
fn synthetic_post(receipt: &Value) -> &Value {
    let held: Vec<_> = rows(receipt, "target_posts")
        .iter()
        .map(value)
        .filter(|post| post["synthetic_enter"].is_object())
        .collect();
    assert_eq!(held.len(), 1, "one actual synthetic-held wrapper attempt");
    held[0]
}
fn key(receipt: &Value) -> Key {
    let key = &receipt["key"];
    Key {
        window: number(&key["window"]),
        incarnation: number(&key["incarnation"]),
        view: number(&key["view"]),
        navigation: number(&key["navigation"]),
        message: number(&key["message"]),
        release_id: number(&key["native_release_id"]),
        retained: number(&key["retained"]),
    }
}
fn stamp(state: &Value, view: u64, generation: u64, commit: u64) {
    assert_eq!(state["view"], view, "same actual native view");
    assert_eq!(
        state["generation"], generation,
        "same actual document generation"
    );
    assert_eq!(state["native_commit"], commit, "same actual native commit");
    assert_eq!(state["reentry_failed"], false);
}
fn quiet_post(post: &Value) {
    assert_eq!(post["phase"], "post-attempt");
    assert_eq!(post["call_active"], false);
    assert_eq!(post["invalid"], false);
    assert_eq!(
        post["in_call_records"], 0,
        "actual in-call callbacks cannot qualify"
    );
    assert_eq!(post["in_call_max_depth"], 0);
    assert!(post["first_in_call_sequence"].is_null() && post["last_in_call_sequence"].is_null());
    assert!(post["unwind_sequence"].is_null());
    assert!(number(&post["attempt_sequence"]) < number(&post["decision_sequence"]));
    number(&post["calls_before"]);
    number(&post["calls_after"]);
}
fn storage(post: &Value, delivery: u64) {
    assert_eq!(post["carrier_kind"], "event");
    assert!(post["route_ready"].is_null());
    let storage = &post["event_storage"];
    assert_eq!(storage["delivery"], delivery);
    for field in ["payload_id", "payload_capacity", "wide_id", "wide_capacity"] {
        assert!(
            number(&storage[field]) > 0,
            "same actual retained Event backing"
        );
    }
}
fn returned(post: &Value) {
    quiet_post(post);
    assert_eq!(post["result"], "returned");
    assert_eq!(post["native_ok"], true);
    assert_eq!(
        number(&post["calls_after"]),
        number(&post["calls_before"]) + 1
    );
    let attempt = number(&post["attempt_sequence"]);
    let gate = number(&post["final_gate"]["sequence"]);
    let entry = number(&post["entry_sequence"]);
    let returned = number(&post["return_sequence"]);
    assert!(
        attempt < gate
            && gate < entry
            && entry < returned
            && returned < number(&post["decision_sequence"])
    );
}
fn not_entered(post: &Value) {
    quiet_post(post);
    assert_eq!(post["calls_after"], post["calls_before"]);
    assert!(post["entry_sequence"].is_null() && post["return_sequence"].is_null());
    assert_eq!(post["result"], "not-admitted");
    assert!(post["native_ok"].is_null());
}

fn admission(receipt: &Value, key: &Key) {
    let admitted = value(&receipt["admission"]);
    let association = json!({"window":key.window,"incarnation":key.incarnation,"view":key.view});
    let actual = custody::admission(
        admitted,
        &association,
        number(&receipt["page_a"]["routeDelivery"]),
    );
    assert_eq!(actual.navigation, key.navigation);
    assert_eq!(actual.message, key.message);
    assert_eq!(actual.release_id, key.release_id);
    assert_eq!(actual.retained, key.retained);
}
pub(super) fn assert_complete(receipt: &Value) {
    // The actual-wrapper mutant must complete both normal profiles and fail HERE.
    if receipt["case"] == "synthetic-before-entry" {
        let post = synthetic_post(receipt);
        assert_eq!(
            post["calls_after"], post["calls_before"],
            "synthetic callback suppresses actual API entry"
        );
    }
    assert_eq!(receipt["schema"], 1);
    let case = receipt["case"].as_str().expect("fixed navigation case");
    assert!(matches!(
        case,
        "before-entry" | "after-return" | "synthetic-before-entry"
    ));
    for row in rows(receipt, "target_posts") {
        let post = value(row);
        assert!(post["document_epoch"].is_string());
        assert_eq!(post["document_epoch"], receipt["page_a"]["routeMarker"]);
    }
    let key = key(receipt);
    admission(receipt, &key);
    let (a_commit, b_generation, b_commit) = order::navigation(receipt, &key);
    page::assert_b(receipt, &key, b_generation, b_commit);
    let entered = order::target_order(receipt, &key, case, a_commit);
    order::owner_order(receipt, &key, case, entered);
    order::consumer_order(receipt, &key, entered, a_commit);
    disposal::assert_disposed(receipt, &key, entered);
}

#[path = "original_navigation_order_observation_custody.rs"]
mod disposal;
#[path = "original_navigation_order_observation_order.rs"]
mod order;
#[path = "original_navigation_order_observation_page.rs"]
mod page;
#[cfg(test)]
#[path = "original_navigation_order_observation_tests.rs"]
mod tests;
