//! Raw same-source X11 pressure and physical disposal assertions.
use crate::support::original_bridge_run::QueuedCloseTrace;
use serde_json::Value;

fn rows(scope: &Value) -> Vec<&Value> {
    let rows = scope["controls"]
        .as_array()
        .expect("four actual CONTROL positions");
    assert_eq!(rows.len(), 4);
    assert_eq!(scope["unknown_native_controls"], 0);
    assert_eq!(scope["unknown_native_data"], 0);
    rows.iter().filter(|row| !row.is_null()).collect()
}

pub(super) fn filled(value: &Value, association: &Value, held: &Value) {
    assert_eq!(value["phase"], "filled");
    assert_eq!(
        value["fifth_full"], true,
        "actual fifth canonical producer returned Full"
    );
    assert_eq!(value["window"], association["window"]);
    assert_eq!(value["incarnation"], association["incarnation"]);
    assert_eq!(value["main"]["view"], association["view"]);
    assert_eq!(value["renderer"]["view"], association["view"]);
    assert_eq!(value["main"]["navigation"], value["renderer"]["navigation"]);
    assert_eq!(value["caller_allocation"], held["packet"]["reader_stop_id"]);
    for key in ["main", "renderer"] {
        assert!(
            value[key]["message"]
                .as_u64()
                .expect("actual original message")
                > 0
        );
        assert!(
            value[key]["navigation"]
                .as_u64()
                .expect("actual original document")
                > 0
        );
    }
    assert!(
        value["main_native_id"]
            .as_u64()
            .expect("actual Event token")
            > 0
    );
    assert!(
        value["renderer_retained"]
            .as_u64()
            .expect("actual renderer DATA")
            > 0
    );
    let ids = value["control_ids"]
        .as_array()
        .expect("original four native tokens");
    assert_eq!(ids.len(), 4);
    for (index, id) in ids.iter().enumerate() {
        assert!(id.as_u64().expect("actual native token identity") > 0);
        assert!(
            !ids[..index].contains(id),
            "four distinct original token allocations"
        );
    }
    let controls = rows(&value["scope"]);
    assert_eq!(controls.len(), 4);
    for (slot, control) in controls.iter().enumerate() {
        assert_eq!(control["slot"], slot);
        assert_eq!(control["native_release_id"], ids[slot]);
        assert_eq!(control["native_released"], false);
        assert_eq!(control["application_done"], true);
        assert_eq!(control["primary_write"], false);
        assert_eq!(
            control["encoded"], 1,
            "same queued typed RouteState, before native encoding"
        );
        assert_eq!(
            control["retained"], 1025,
            "existing paid native CONTROL representation"
        );
    }
    let data = value["scope"]["data"]
        .as_array()
        .expect("two actual DATA positions");
    let data: Vec<_> = data.iter().filter(|row| !row.is_null()).collect();
    assert_eq!(
        data.len(),
        1,
        "native-token snapshot covers the original M2R DATA"
    );
    assert_eq!(data[0]["native_release_id"], value["main_native_id"]);
    assert_eq!(data[0]["message"], value["main"]["message"]);
    assert_eq!(data[0]["native_released"], false);
    assert_eq!(data[0]["consumer_done"], false);
    assert_eq!(data[0]["native_done"], false);
    assert_eq!(value["scope"]["native_storage_quiescent"], false);
}

pub(super) fn same_pressure(value: &Value, full: &Value) -> bool {
    value["phase"] == "progress"
        && value["window"] == full["window"]
        && value["incarnation"] == full["incarnation"]
        && value["main"] == full["main"]
        && value["renderer"] == full["renderer"]
        && value["main_native_id"] == full["main_native_id"]
        && value["caller_allocation"] == full["caller_allocation"]
        && value["control_ids"] == full["control_ids"]
}

pub(super) fn both_original_sources(value: &Value, full: &Value) {
    assert!(same_pressure(value, full));
    for direction in ["main", "renderer"] {
        let charge = &value["state"][direction];
        assert_eq!(charge["key"], full[direction]);
        assert_eq!(charge["caller_allocation"], full["caller_allocation"]);
        assert!(
            charge["retained"]
                .as_u64()
                .expect("same actual DATA charge")
                > 0
        );
        assert_eq!(charge["retired"], false);
    }
    assert_eq!(
        value["state"]["renderer"]["retained"],
        full["renderer_retained"]
    );
}

pub(super) fn controls_held(value: &Value, full: &Value) -> bool {
    if !same_pressure(value, full) || value["state"]["controls_in_flight"] != 4 {
        return false;
    }
    let Some(controls) = value["state"]["scope"]["controls"].as_array() else {
        return false;
    };
    let Some(ids) = full["control_ids"].as_array() else {
        return false;
    };
    controls.len() == 4
        && controls.iter().enumerate().all(|(slot, row)| {
            row["native_release_id"] == ids[slot]
                && row["native_released"] == false
                && row["application_done"] == true
                && row["primary_write"] == false
        })
}

pub(super) fn controls_disposed(value: &Value, full: &Value) -> bool {
    if !same_pressure(value, full) || value["state"]["scope"]["unknown_native_controls"] != 0 {
        return false;
    }
    let Some(controls) = value["state"]["scope"]["controls"].as_array() else {
        return false;
    };
    let Some(ids) = full["control_ids"].as_array() else {
        return false;
    };
    let existing: Vec<_> = controls.iter().filter(|row| !row.is_null()).collect();
    controls.len() == 4
        && value["state"]["controls_in_flight"].as_u64() == u64::try_from(existing.len()).ok()
        && existing.iter().all(|row| {
            ids.contains(&row["native_release_id"])
                && row["native_released"] == true
                && row["application_done"] == true
                && row["primary_write"] == false
        })
}

pub(super) fn empty(value: &Value, full: &Value) -> bool {
    same_pressure(value, full)
        && value["state"]["main"].is_null()
        && value["state"]["renderer"].is_null()
        && value["state"]["controls_in_flight"] == 0
        && value["state"]["scope"]["native_storage_quiescent"] == true
        && value["state"]["scope"]["unknown_native_controls"] == 0
        && value["state"]["scope"]["unknown_native_data"] == 0
        && value["state"]["scope"]["controls"]
            .as_array()
            .is_some_and(|controls| controls.len() == 4 && controls.iter().all(Value::is_null))
}

pub(super) fn physical_held(held: &Value) {
    assert_eq!(held["phase"], "packet-held");
    assert_eq!(
        held["actual_packet"], held["packet"],
        "same actual borrowed Packet"
    );
    let packet = &held["packet"];
    assert_eq!(packet["index"], 0);
    assert_eq!(packet["kind"], 4, "actual Event frame");
    assert_eq!(packet["compat_control"], true);
    assert_eq!(packet["correlation"], 0);
    for field in [
        "serial",
        "payload_id",
        "payload_bytes",
        "target_id",
        "reader_stop_id",
    ] {
        assert!(
            packet[field]
                .as_u64()
                .expect("actual Packet allocation/identity")
                > 0
        );
    }
    assert_eq!(held["value_position"]["phase"], "writing");
    assert_eq!(held["value_position"]["serial"], packet["serial"]);
    assert_eq!(held["value_position"]["target_id"], packet["target_id"]);
    assert_eq!(held["value_position"]["packet_in_slot"], false);
    assert_eq!(held["value_position"]["receipt_live"], true);
}

pub(super) fn physical_written(record: &Value, held: &Value, dropped: bool) {
    assert_eq!(
        record["phase"],
        if dropped {
            "packet-dropped"
        } else {
            "packet-returned"
        }
    );
    assert_eq!(
        record["packet"], held["packet"],
        "same position, serial, target and Box"
    );
    assert_eq!(
        record["outcome"], "written",
        "actual physical write returned successfully"
    );
    if dropped {
        assert!(
            record["actual_packet"].is_null(),
            "original Packet no longer exists"
        );
    } else {
        assert_eq!(record["actual_packet"], held["packet"]);
    }
}

pub(super) fn native_page(page: &Value, initial: &Value, full: &Value, terminal: bool) {
    assert_eq!(page["first"], initial["first"]);
    assert_eq!(page["rawChallenge"], initial["rawChallenge"]);
    assert_eq!(page["registered"], true);
    assert_eq!(page["routeReady"], true);
    assert_eq!(page["issued"], true);
    assert_eq!(page["entered"], 1);
    assert_eq!(page["targetDelivery"], full["main"]["message"]);
    assert!(page["error"].is_null());
    let initial_count = initial["readyCount"]
        .as_u64()
        .expect("actual initial ready count");
    assert_eq!(page["readyCount"], initial_count + 4);
    let old = initial["rawRecords"]
        .as_array()
        .expect("initial real native records");
    let raw = page["rawRecords"]
        .as_array()
        .expect("actual real native records");
    assert!(raw.len() <= 12 && raw.len() >= old.len());
    assert_eq!(&raw[..old.len()], old.as_slice(), "same original prefix");
    let actual: Vec<Value> = raw
        .iter()
        .map(|row| {
            assert_eq!(row["sourceIsNativePort"], true);
            serde_json::from_str(row["text"].as_str().expect("actual native text"))
                .expect("actual native JSON")
        })
        .collect();
    let added = &actual[old.len()..];
    let ready: Vec<_> = added
        .iter()
        .filter(|row| row["kind"] == "route-state")
        .collect();
    assert_eq!(
        ready.len(),
        4,
        "exact four new actual canonical native RouteStates"
    );
    for row in ready {
        assert_eq!(row["v"], 2);
        assert_eq!(row["document"], initial["rawChallenge"]);
        assert_eq!(row["ready"], true);
    }
    let events: Vec<_> = actual.iter().filter(|row| row["kind"] == "event").collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["v"], 2);
    assert_eq!(events[0]["document"], initial["rawChallenge"]);
    assert_eq!(events[0]["delivery"], full["main"]["message"]);
    assert_eq!(events[0]["channel"], "args-obj");
    if terminal {
        let result = &page["terminal"];
        assert!(
            actual.contains(result),
            "terminal is an actual native port record"
        );
        assert_eq!(result["v"], 2);
        assert_eq!(result["kind"], "send-result");
        assert_eq!(result["document"], initial["rawChallenge"]);
        assert_eq!(result["message"], full["renderer"]["message"]);
        assert_eq!(result["disposition"]["status"], "consumed");
    }
}

fn original_post_returns<'a>(trace: &'a QueuedCloseTrace, full: &Value) -> Vec<&'a Value> {
    trace
        .records
        .iter()
        .filter(|row| {
            row.kind == "owner"
                && row.value["phase"] == "post-return"
                && row.value["view"] == full["main"]["view"]
                && row.value["navigation"] == full["main"]["navigation"]
                && row.value["message"] == full["main"]["message"]
        })
        .map(|row| &row.value)
        .collect()
}

pub(super) fn complete(native: &str, control: &str, full: &Value, held: &Value) {
    let trace = QueuedCloseTrace::from_complete(native);
    let pressure: Vec<_> = trace
        .records
        .iter()
        .filter(|row| row.kind == "pressure")
        .map(|row| &row.value)
        .collect();
    assert_eq!(
        pressure
            .iter()
            .filter(|row| row["phase"] == "filled")
            .count(),
        1
    );
    assert!(pressure.iter().any(|row| empty(row, full)));
    let physical: Vec<_> = trace
        .records
        .iter()
        .filter(|row| row.kind == "physical")
        .collect();
    assert_eq!(
        physical.len(),
        4,
        "one held/released/returned/dropped actual Packet"
    );
    for phase in [
        "packet-held",
        "packet-released",
        "packet-returned",
        "packet-dropped",
    ] {
        assert_eq!(
            physical
                .iter()
                .filter(|row| row.value["phase"] == phase)
                .count(),
            1
        );
    }
    let returned = physical
        .iter()
        .find(|row| row.value["phase"] == "packet-returned")
        .expect("actual physical return");
    let dropped = physical
        .iter()
        .find(|row| row.value["phase"] == "packet-dropped")
        .expect("actual Packet disposal");
    physical_written(&returned.value, held, false);
    physical_written(&dropped.value, held, true);
    assert!(
        returned.end < dropped.end,
        "same worker reports return before original Packet Drop"
    );
    let posts = original_post_returns(&trace, full);
    assert_eq!(posts.len(), 1, "one actual original Event Post return");
    assert_eq!(posts[0]["native_ok"], true);
    assert_eq!(posts[0]["post_phase"], "returned");
    assert_eq!(
        control
            .lines()
            .filter(|line| line.starts_with("IPC_EFFECT "))
            .count(),
        1
    );
    assert_eq!(
        control
            .lines()
            .filter(|line| line.starts_with("LOADED "))
            .count(),
        1
    );
    assert!(!control.contains("FIXTURE_ERROR ") && !control.contains("LOAD_RETURNED_ERROR "));
    assert!(!native.contains("packet-hold-failed"));
    assert!(control.contains("QUIT_ACCEPTED "));
}

#[cfg(test)]
mod tests {
    use super::{QueuedCloseTrace, original_post_returns};

    // Exact emitted native record from the failed v2 run, including its field name.
    const ACTUAL_POST: &str = r#"KELD_X10_NATIVE_OWNER {"actual_closed":false,"message":6,"native_ok":true,"navigation":2,"phase":"post-return","post_phase":"returned","released":false,"retirement":null,"retirement_sent":false,"view":1}"#;

    #[test]
    fn actual_native_owner_message_key_selects_the_original_post_return() {
        let trace = QueuedCloseTrace::from_complete(ACTUAL_POST);
        let full = serde_json::json!({"main":{"view":1,"navigation":2,"message":6}});
        let posts = original_post_returns(&trace, &full);
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0]["message"], 6);
        assert_eq!(posts[0]["native_ok"], true);
        assert_eq!(posts[0]["post_phase"], "returned");
    }

    #[test]
    fn delivery_named_decoy_cannot_substitute_for_the_native_message_key() {
        let decoy = ACTUAL_POST.replace("\"message\":6", "\"delivery\":6,\"message\":7");
        let trace = QueuedCloseTrace::from_complete(&decoy);
        let full = serde_json::json!({"main":{"view":1,"navigation":2,"message":6}});
        assert!(
            original_post_returns(&trace, &full).is_empty(),
            "a matching noncanonical field cannot select a foreign Event"
        );
    }
}
