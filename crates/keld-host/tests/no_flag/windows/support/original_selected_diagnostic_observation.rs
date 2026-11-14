//! Independent exact-key/physical-position assertions for the selected diagnostic.
use super::super::observation::{Key, same_event, same_scope, target_in};
use crate::support::original_bridge_run::{QueuedCloseRecord, QueuedCloseTrace};
use serde_json::Value;

#[path = "original_selected_diagnostic_observation/restoration.rs"]
mod restoration;

pub(super) fn event_key(record: &Value, association: &Value) -> Key {
    assert_eq!(record["phase"], "event-admitted");
    assert_eq!(record["window"], association["window"]);
    assert_eq!(record["incarnation"], association["incarnation"]);
    assert_eq!(record["offers_closed"], false);
    let view = association["view"].as_u64().expect("actual native view");
    let data = target_in(record, view, 0).expect("one actual main-to-renderer DATA");
    assert_eq!(data["native_released"], false);
    assert_eq!(record["scope"]["native_storage_quiescent"], false);
    let key = Key {
        window: association["window"].as_u64().expect("actual WindowId"),
        incarnation: association["incarnation"]
            .as_u64()
            .expect("actual WindowRef incarnation"),
        view,
        navigation: data["navigation"]
            .as_u64()
            .expect("actual native document generation"),
        message: data["message"]
            .as_u64()
            .expect("actual target Event correlation"),
        release_id: data["native_release_id"]
            .as_u64()
            .expect("actual native allocation"),
        retained: data["retained"]
            .as_u64()
            .expect("actual retained DATA bytes"),
    };
    assert!(key.message > 0 && key.release_id > 0 && key.retained > 0);
    key
}

pub(super) fn physical_held(record: &Value) {
    assert_eq!(record["phase"], "packet-held");
    let packet = &record["packet"];
    assert_eq!(
        record["actual_packet"], *packet,
        "actual borrowed Box observed now"
    );
    assert_eq!(packet["index"], 0);
    // KI frame Event has wire discriminator 4 (keld-ipc/src/frame.rs).
    assert_eq!(packet["kind"], 4);
    assert_eq!(packet["compat_control"], true);
    assert_eq!(packet["correlation"], 0);
    for field in [
        "serial",
        "payload_id",
        "payload_bytes",
        "target_id",
        "reader_stop_id",
    ] {
        assert!(packet[field].as_u64().expect("actual packet scalar") > 0);
    }
    same_writing_position(record, packet);
    assert_eq!(record["value_position"]["receipt_live"], true);
}

fn same_writing_position(record: &Value, packet: &Value) {
    let position = &record["value_position"];
    assert_eq!(position["serial"], packet["serial"]);
    assert_eq!(position["phase"], "writing");
    assert_eq!(position["target_id"], packet["target_id"]);
    assert_eq!(
        position["packet_in_slot"], false,
        "same actual Box is worker-local"
    );
}

pub(super) fn selected_full(record: &Value, key: &Key, held: &Value) -> Value {
    assert_eq!(record["phase"], "value-full-restored");
    assert_eq!(
        record["value_full"], true,
        "actual WriterRefusal::Full branch"
    );
    assert!(
        record["retirement_failed"].is_null(),
        "no completed retirement result"
    );
    let selected = &record["selected"];
    assert!(selected.is_object(), "actual selected Refused source");
    assert_eq!(selected["webview"], key.view);
    assert_eq!(selected["navigation"], key.navigation);
    assert_eq!(selected["message"], key.message);
    assert_eq!(selected["reader_stop_id"], held["packet"]["reader_stop_id"]);
    assert_eq!(selected["canonical_code"], true);
    for name in ["code_id", "code_bytes", "message_id", "message_bytes"] {
        assert!(
            selected[name]
                .as_u64()
                .expect("actual source allocation scalar")
                > 0
        );
    }
    retained_source(record, key, selected);
    let renderer = &record["renderer_to_main"];
    assert_eq!(renderer["webview"], key.view);
    assert_eq!(renderer["navigation"], key.navigation);
    assert_eq!(renderer["reader_stop_id"], held["packet"]["reader_stop_id"]);
    assert_eq!(
        renderer["primary_write"], true,
        "genuine renderer VALUE receipt retained"
    );
    assert!(
        renderer["retained"]
            .as_u64()
            .expect("actual renderer DATA bytes")
            > 0
    );
    same_writing_position(record, &held["packet"]);
    selected.clone()
}

fn unadmitted_charge(record: &Value, key: &Key, selected: &Value) {
    let charge = &record["main_to_renderer"];
    assert_eq!(charge["webview"], key.view);
    assert_eq!(charge["navigation"], key.navigation);
    assert_eq!(charge["message"], key.message);
    assert_eq!(charge["reader_stop_id"], selected["reader_stop_id"]);
    assert_eq!(charge["pending_reply"], key.message);
    assert_eq!(charge["primary_write"], false);
    assert_eq!(charge["retirement_write"], false);
    assert_eq!(charge["consumer_done"], true);
    assert_eq!(charge["native_done"], true);
    assert_eq!(charge["native_storage_id"], key.release_id);
    assert_eq!(charge["native_storage_released"], false);
    assert_eq!(charge["event_release_sent"], false);
    assert!(charge["retained"].as_u64().expect("retained same source") >= key.retained);
}

pub(super) fn retained_source(record: &Value, key: &Key, selected: &Value) {
    unadmitted_charge(record, key, selected);
    let charge = &record["main_to_renderer"];
    assert_eq!(
        charge["selected"], *selected,
        "same code/message allocations and lengths"
    );
    assert_eq!(charge["terminal_preparing"], false);
    assert!(
        charge["source_attempt"].is_null(),
        "no active preparation remains"
    );
}

pub(super) fn restoration_attempt(record: &Value) -> Option<u64> {
    restoration::needed_attempt(record)
}

pub(super) fn role_revoked(
    trace: &QueuedCloseTrace,
    record: &Value,
    key: &Key,
    held: &Value,
    selected: &Value,
) -> Option<Value> {
    assert_eq!(record["phase"], "role-revoked-before-drain");
    assert!(
        record["retirement_failed"].is_null(),
        "pre-drain observation cannot claim a completed retirement result"
    );
    let restored = restoration::retained(trace, record, key, selected);
    same_writing_position(record, &held["packet"]);
    if let Some(restored) = &restored {
        same_writing_position(restored, &held["packet"]);
    }
    restored
}

fn retired_state(record: &Value) {
    assert_eq!(record["main_to_renderer"]["reader_stopped"], true);
    assert_eq!(record["main_to_renderer"]["retired"], true);
    assert_eq!(
        record["main_to_renderer"]["native_retirement"],
        "outcome-unknown"
    );
}

fn retired_source(record: &Value, key: &Key, selected: &Value) {
    retained_source(record, key, selected);
    retired_state(record);
}

fn role_result(record: &Value) {
    assert_eq!(record["phase"], "role-retirement-return");
    assert_eq!(
        record["retirement_failed"], true,
        "actual post-drain canonical retirement remains unqualified"
    );
    assert_eq!(
        record["retirement_refusal"], "selected-diagnostic",
        "actual exact selected diagnostic refusal, never a generic failed retirement"
    );
    assert_ne!(
        record["value_position"]["phase"], "writing",
        "retirement return follows the same real physical write quiescence"
    );
}

#[cfg(test)]
fn role_failure(record: &Value, key: &Key, selected: &Value) {
    role_result(record);
    retired_source(record, key, selected);
}

fn one<'a>(
    trace: &'a QueuedCloseTrace,
    kind: &str,
    phase: &str,
    accepts: impl Fn(&Value) -> bool,
) -> &'a QueuedCloseRecord {
    let found: Vec<_> = trace
        .records
        .iter()
        .filter(|row| row.kind == kind && row.value["phase"] == phase && accepts(&row.value))
        .collect();
    assert_eq!(found.len(), 1, "exactly one actual {kind}/{phase}");
    found[0]
}

pub(super) fn complete(
    trace: &str,
    key: &Key,
    held: &Value,
    selected: &Value,
) -> (Value, Option<Value>) {
    let trace = QueuedCloseTrace::from_complete(trace);
    super::super::observation::bounded_retirement_wakes(&trace, key);
    let actual_held = one(&trace, "physical", "packet-held", |_| true);
    assert_eq!(actual_held.value, *held);
    for phase in ["packet-released", "packet-returned", "packet-dropped"] {
        let row = one(&trace, "physical", phase, |_| true);
        assert_eq!(row.value["packet"], held["packet"]);
        same_writing_position(&row.value, &held["packet"]);
        if phase == "packet-dropped" {
            assert!(row.value["actual_packet"].is_null());
        } else {
            assert_eq!(
                row.value["actual_packet"], held["packet"],
                "fresh same actual Box observation"
            );
        }
        if phase == "packet-returned" || phase == "packet-dropped" {
            assert_eq!(
                row.value["outcome"], "outcome-unknown",
                "actual physical return after original peer exit"
            );
        }
    }
    let post = one(&trace, "owner", "post-return", |value| {
        same_event(value, key)
    });
    assert_eq!(post.value["native_ok"], true);
    let closed = one(&trace, "owner", "controller-closed", |value| {
        value["view"] == key.view
    });
    assert_eq!(closed.value["actual_closed"], true);
    assert_eq!(closed.value["native_storage_quiescent"], false);
    assert!(
        post.end < closed.end,
        "actual same-STA Post return precedes Close"
    );
    let retired = one(&trace, "owner", "retirement-handed-off", |value| {
        same_event(value, key)
    });
    assert_eq!(retired.value["actual_closed"], true);
    assert_eq!(retired.value["retirement_sent"], true);
    assert_eq!(retired.value["retirement"], "outcome-unknown");
    assert!(trace.records.iter().any(|row| row.kind == "scope"
        && row.value["phase"] == "offers-closed"
        && same_scope(&row.value, key)
        && row.value["offers_closed"] == true));
    assert!(
        !trace.records.iter().any(|row| row.kind == "owner"
            && row.value["phase"] == "native-storage-dropped"
            && same_event(&row.value, key)),
        "unqualified selected diagnostic cannot fabricate EventReleased/storage Drop"
    );
    let revoked = one(&trace, "selected", "role-revoked-before-drain", |value| {
        value["main_to_renderer"]["message"] == key.message
    });
    let pre_drain_restored = role_revoked(&trace, &revoked.value, key, held, selected);
    let released = one(&trace, "physical", "packet-released", |_| true);
    let returned = one(&trace, "physical", "packet-returned", |_| true);
    let dropped = one(&trace, "physical", "packet-dropped", |_| true);
    assert!(actual_held.end < revoked.end && revoked.end < released.end);
    if let Some(restored) = pre_drain_restored {
        let handoff = one(
            &trace,
            "selected",
            "source-restored-after-denied-target",
            |value| *value == restored,
        );
        assert!(
            handoff.end < released.end,
            "pre-drain source restoration precedes physical release"
        );
    }
    assert!(released.end < returned.end && returned.end < dropped.end);
    let role = one(&trace, "selected", "role-retirement-return", |value| {
        value["main_to_renderer"]["message"] == key.message
    });
    assert!(
        dropped.end < role.end,
        "actual packet Drop precedes final retirement return"
    );
    role_result(&role.value);
    let restored = restoration::retained(&trace, &role.value, key, selected);
    (role.value.clone(), restored)
}

#[test]
fn saved_selected_refusal_rejects_missing_foreign_and_wrong_cause() {
    // Actual v4 row; only the independent oracle is exercised here.
    let record: Value = serde_json::from_str(r#"{"main_to_renderer":{"consumer_done":true,"event_release_sent":false,"message":6,"native_done":true,"native_retirement":"outcome-unknown","native_storage_id":2330016362992,"native_storage_released":false,"navigation":2,"pending_reply":6,"primary_write":false,"reader_stop_id":2329965636624,"reader_stopped":true,"retained":2026,"retired":true,"retirement_write":false,"selected":{"canonical_code":true,"code_bytes":15,"code_id":2329998601888,"message":6,"message_bytes":26,"message_id":2330016354720,"navigation":2,"reader_stop_id":2329965636624,"webview":1},"terminal_preparing":false,"webview":1},"phase":"role-retirement-return","renderer_to_main":{"consumer_done":false,"event_release_sent":false,"message":1,"native_done":true,"native_retirement":null,"native_storage_id":null,"native_storage_released":null,"navigation":2,"pending_reply":null,"primary_write":true,"reader_stop_id":2329965636624,"reader_stopped":true,"retained":308,"retired":true,"retirement_write":false,"selected":null,"terminal_preparing":false,"webview":1},"retirement_failed":true,"retirement_refusal":"selected-diagnostic","selected":null,"sequence":12,"value_full":false,"value_position":{"packet_in_slot":false,"phase":"complete","receipt_live":true,"serial":5,"target_id":2329965448352}}"#).expect("saved actual retirement row");
    let selected = record["main_to_renderer"]["selected"].clone();
    let key = Key {
        window: 1,
        incarnation: 1,
        view: 1,
        navigation: 2,
        message: 6,
        release_id: 2_330_016_362_992,
        retained: 2026,
    };
    role_failure(&record, &key, &selected);
    for cause in [Value::Null, Value::String("selected-terminal".to_owned())] {
        let mut changed = record.clone();
        changed["retirement_refusal"] = cause;
        assert!(std::panic::catch_unwind(|| role_failure(&changed, &key, &selected)).is_err());
    }
    let mut wrong_result = record.clone();
    wrong_result["retirement_failed"] = Value::Bool(false);
    assert!(std::panic::catch_unwind(|| role_failure(&wrong_result, &key, &selected)).is_err());
    let mut foreign = record.clone();
    foreign["main_to_renderer"]["message"] = Value::from(7);
    assert!(std::panic::catch_unwind(|| role_failure(&foreign, &key, &selected)).is_err());
    let mut foreign_source = record;
    foreign_source["main_to_renderer"]["selected"]["code_id"] = Value::from(1);
    assert!(std::panic::catch_unwind(|| role_failure(&foreign_source, &key, &selected)).is_err());
}
