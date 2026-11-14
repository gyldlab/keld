//! Saved failed-run scalars plus explicit component-only preparation/handoff projections.
//! These cases test the independent oracle; they never relabel the saved native failure.
use std::fmt::Write as _;

use super::*;

fn projection(pre_drain: bool) -> (Key, Value, Value, Vec<Value>) {
    let mut role: Value = serde_json::from_str(r#"{"main_to_renderer":{"consumer_done":true,"event_release_sent":false,"message":6,"native_done":true,"native_retirement":"outcome-unknown","native_storage_id":1873257286160,"native_storage_released":false,"navigation":2,"pending_reply":6,"primary_write":false,"reader_stop_id":1873251721712,"reader_stopped":true,"retained":2026,"retired":true,"retirement_write":false,"selected":null,"terminal_preparing":true,"webview":1},"phase":"role-retirement-return","renderer_to_main":{"consumer_done":false,"event_release_sent":false,"message":1,"native_done":true,"native_retirement":null,"native_storage_id":null,"native_storage_released":null,"navigation":2,"pending_reply":null,"primary_write":true,"reader_stop_id":1873251721712,"reader_stopped":true,"retained":288,"retired":true,"retirement_write":false,"selected":null,"terminal_preparing":false,"webview":1},"retirement_failed":true,"retirement_refusal":"selected-diagnostic","selected":null,"sequence":13,"value_full":false,"value_position":{"packet_in_slot":false,"phase":"complete","receipt_live":true,"serial":5,"target_id":1873251302720}}"#).expect("saved actual moved row");
    let selected: Value = serde_json::from_str(r#"{"canonical_code":true,"code_bytes":15,"code_id":1873251771296,"message":6,"message_bytes":26,"message_id":1873257321392,"navigation":2,"reader_stop_id":1873251721712,"webview":1}"#).expect("saved original allocations");
    let held: Value = serde_json::from_str(r#"{"actual_packet":{"compat_control":true,"correlation":0,"index":0,"kind":4,"payload_bytes":288,"payload_id":1873251163008,"reader_stop_id":1873251721712,"serial":5,"target_id":1873251302720},"outcome":null,"packet":{"compat_control":true,"correlation":0,"index":0,"kind":4,"payload_bytes":288,"payload_id":1873251163008,"reader_stop_id":1873251721712,"serial":5,"target_id":1873251302720},"phase":"packet-held","value_position":{"packet_in_slot":false,"phase":"writing","receipt_live":true,"serial":5,"target_id":1873251302720}}"#).expect("saved real held packet");
    let key = Key {
        window: 1,
        incarnation: 1,
        view: 1,
        navigation: 2,
        message: 6,
        release_id: 1_873_257_286_160,
        retained: 2026,
    };
    // New scalars and a restored projection are component inputs only. Actual
    // native evidence for the new producer must come from a future native run.
    role["main_to_renderer"]["source_attempt"] = Value::from(2);
    role["source_attempt"] = Value::Null;
    if pre_drain {
        role["phase"] = Value::from("role-revoked-before-drain");
        role["retirement_failed"] = Value::Null;
        role["retirement_refusal"] = Value::Null;
        role["value_position"] = held["value_position"].clone();
    }
    let mut source = role.clone();
    source["phase"] = Value::from("source-selected");
    source["selected"] = selected.clone();
    source["source_attempt"] = Value::from(2);
    source["retirement_failed"] = Value::Null;
    source["retirement_refusal"] = Value::Null;
    let mut restored = source.clone();
    restored["phase"] = Value::from("source-restored-after-denied-target");
    restored["main_to_renderer"]["selected"] = selected.clone();
    restored["main_to_renderer"]["terminal_preparing"] = Value::Bool(false);
    restored["main_to_renderer"]["source_attempt"] = Value::Null;
    (key, selected, held, vec![source, role, restored])
}

fn observe(rows: &[Value], key: &Key, selected: &Value, held: &Value) -> Option<Value> {
    let mut raw = String::new();
    for row in rows {
        writeln!(&mut raw, "KELD_X10_SELECTED_DIAGNOSTIC {row}")
            .expect("write component trace to String");
    }
    let trace = QueuedCloseTrace::from_complete(&raw);
    let role = rows
        .iter()
        .find(|row| {
            matches!(
                row["phase"].as_str(),
                Some("role-revoked-before-drain" | "role-retirement-return")
            )
        })
        .expect("one actual cut under test");
    if role["phase"] == "role-revoked-before-drain" {
        super::super::role_revoked(&trace, role, key, held, selected)
    } else {
        super::super::role_result(role);
        retained(&trace, role, key, selected)
    }
}

#[test]
fn moved_source_requires_exact_attempt_handoff_at_both_cuts_in_either_print_order() {
    for pre_drain in [false, true] {
        let (key, selected, held, rows) = projection(pre_drain);
        let expected = rows[2].clone();
        assert_eq!(
            observe(&rows, &key, &selected, &held),
            Some(expected.clone())
        );
        let reordered = [rows[0].clone(), rows[2].clone(), rows[1].clone()];
        assert_eq!(observe(&reordered, &key, &selected, &held), Some(expected));
    }
}

#[test]
fn moved_source_rejects_missing_foreign_reused_and_unrestored_handoffs() {
    for pre_drain in [false, true] {
        let (key, selected, held, rows) = projection(pre_drain);
        let rejected = |changed: &[Value]| {
            assert!(
                std::panic::catch_unwind(|| { observe(changed, &key, &selected, &held) }).is_err(),
                "changed actual restoration must not qualify"
            );
        };
        rejected(&rows[..2]);
        let mut duplicate = rows.clone();
        duplicate.push(rows[2].clone());
        rejected(&duplicate);
        for invalid in [Value::Null, Value::from(1), Value::from(129)] {
            let mut changed = rows.clone();
            changed[2]["source_attempt"] = invalid;
            rejected(&changed);
        }
        let mut earlier = rows.clone();
        earlier[2]["source_attempt"] = Value::from(1);
        let mut previous_source = rows[0].clone();
        previous_source["source_attempt"] = Value::from(1);
        previous_source["main_to_renderer"]["source_attempt"] = Value::from(1);
        earlier.push(previous_source);
        rejected(&earlier); // identical allocations from a different preparation
        let mut source_changed = rows.clone();
        source_changed[0]["selected"]["message_id"] = Value::from(1);
        rejected(&source_changed);
        for (field, replacement) in [
            ("selected", Value::Null),
            ("message", Value::from(7)),
            ("reader_stop_id", Value::from(1)),
            ("native_storage_id", Value::from(1)),
            ("retained", Value::from(2027)),
            ("terminal_preparing", Value::Bool(true)),
            ("source_attempt", Value::from(2)),
            ("primary_write", Value::Bool(true)),
            ("retirement_write", Value::Bool(true)),
            ("native_storage_released", Value::Bool(true)),
            ("event_release_sent", Value::Bool(true)),
            ("reader_stopped", Value::Bool(false)),
            ("retired", Value::Bool(false)),
        ] {
            let mut changed = rows.clone();
            changed[2]["main_to_renderer"][field] = replacement;
            rejected(&changed);
        }
        let mut reaped = rows.clone();
        reaped[2]["main_to_renderer"] = Value::Null;
        rejected(&reaped);
        let mut foreign = rows.clone();
        foreign[2]["selected"]["code_id"] = Value::from(1);
        rejected(&foreign);
        let mut no_actual_preparation = rows;
        no_actual_preparation[0]["main_to_renderer"]["source_attempt"] = Value::from(1);
        rejected(&no_actual_preparation);
    }
}
