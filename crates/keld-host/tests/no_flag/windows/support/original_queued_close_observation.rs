//! Independent assertions over actual bounded Core/native-owner records.
use crate::support::original_bridge_run::{QueuedCloseRecord, QueuedCloseTrace};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(super) struct Key {
    pub(super) window: u64,
    pub(super) incarnation: u64,
    pub(super) view: u64,
    pub(super) navigation: u64,
    pub(super) message: u64,
    pub(super) release_id: u64,
    pub(super) retained: u64,
}
fn rows<'a>(scope: &'a Value, field: &str) -> Vec<&'a Value> {
    scope[field]
        .as_array()
        .expect("fixed native census array")
        .iter()
        .filter(|value| !value.is_null())
        .collect()
}
pub(super) fn target_in(record: &Value, view: u64, prior: u64) -> Option<&Value> {
    record["scope"]["data"].as_array()?.iter().find(|data| {
        data["webview"] == view && data["message"].as_u64().is_some_and(|id| id != prior)
    })
}
fn known_scope(scope: &Value) {
    assert_eq!(scope["unknown_native_data"], 0);
    assert_eq!(scope["unknown_native_controls"], 0);
}
pub(super) fn admission(record: &Value, association: &Value, prior: u64) -> Key {
    assert_eq!(record["phase"], "event-admitted");
    let window = association["window"].as_u64().expect("actual WindowId");
    let incarnation = association["incarnation"]
        .as_u64()
        .expect("actual WindowRef incarnation");
    let view = association["view"].as_u64().expect("actual native view");
    assert_eq!(record["window"], window);
    assert_eq!(record["incarnation"], incarnation);
    assert_eq!(record["offers_closed"], false);
    let scope = &record["scope"];
    known_scope(scope);
    assert!(
        rows(scope, "controls")
            .iter()
            .all(|control| control["native_released"] == true),
        "startup native CONTROL must be physically released before target admission"
    );
    let data = target_in(record, view, prior).expect("actual target Event DATA");
    assert_eq!(data["native_released"], false);
    assert_eq!(scope["native_storage_quiescent"], false);
    let key = Key {
        window,
        incarnation,
        view,
        navigation: data["navigation"]
            .as_u64()
            .expect("actual document generation"),
        message: data["message"].as_u64().expect("actual delivery"),
        release_id: data["native_release_id"]
            .as_u64()
            .expect("actual native storage identity"),
        retained: data["retained"].as_u64().expect("actual retained charge"),
    };
    assert!(key.release_id != 0 && key.retained > 0 && key.message != prior);
    assert_eq!(
        rows(scope, "data")
            .iter()
            .filter(|row| row["native_released"] == false)
            .count(),
        1
    );
    key
}
pub(super) fn same_event(value: &Value, key: &Key) -> bool {
    value["view"] == key.view
        && value["navigation"] == key.navigation
        && value["message"] == key.message
}
pub(super) fn same_scope(value: &Value, key: &Key) -> bool {
    value["window"] == key.window && value["incarnation"] == key.incarnation
}
pub(super) fn retained_at_close(record: &Value, key: &Key) -> u64 {
    assert!(same_scope(record, key));
    assert_eq!(record["phase"], "offers-closed");
    assert_eq!(
        record["offers_closed"], true,
        "actual Registry producer state"
    );
    let scope = &record["scope"];
    known_scope(scope);
    let data = rows(scope, "data")
        .into_iter()
        .find(|data| {
            data["webview"] == key.view
                && data["navigation"] == key.navigation
                && data["message"] == key.message
        })
        .expect("same exact Event remains charged");
    assert_eq!(data["native_release_id"], key.release_id);
    assert_eq!(data["native_released"], false);
    assert!(
        data["retained"]
            .as_u64()
            .expect("actual retained DATA bytes")
            >= key.retained
    );
    assert_eq!(scope["native_storage_quiescent"], false);
    let controls: Vec<_> = rows(scope, "controls")
        .into_iter()
        .filter(|control| control["native_released"] == false)
        .collect();
    assert_eq!(
        controls.len(),
        1,
        "one actual queued close RouteState CONTROL"
    );
    let control = controls[0];
    let retained = control["retained"]
        .as_u64()
        .expect("CONTROL retained bytes");
    let encoded = control["encoded"].as_u64().expect("CONTROL encoded bytes");
    assert!(encoded >= 1 && retained >= encoded + 1024);
    assert_eq!(control["application_done"], true);
    let id = control["native_release_id"]
        .as_u64()
        .expect("actual CONTROL identity");
    assert_ne!(id, 0);
    id
}
pub(super) fn physically_quiescent(record: &Value, key: &Key) -> bool {
    same_scope(record, key)
        && record["phase"] == "storage-polled"
        && record["offers_closed"] == true
        && record["scope"]["native_storage_quiescent"] == true
}
fn one<'a>(
    trace: &'a QueuedCloseTrace,
    kind: &str,
    phase: &str,
    accepted: impl Fn(&Value) -> bool,
) -> &'a QueuedCloseRecord {
    let rows: Vec<_> = trace
        .records
        .iter()
        .filter(|row| row.kind == kind && row.value["phase"] == phase && accepted(&row.value))
        .collect();
    assert_eq!(rows.len(), 1, "one actual {kind}/{phase} receipt");
    rows[0]
}
pub(super) fn bounded_retirement_wakes(trace: &QueuedCloseTrace, key: &Key) {
    let retired = one(trace, "owner", "bridge-retire-wake-attempt", |value| {
        value["view"] == key.view
    });
    let closed = one(trace, "owner", "bridge-close-wake-attempt", |value| {
        value["view"] == key.view
    });
    let actual = one(trace, "owner", "controller-closed", |value| {
        value["view"] == key.view
    });
    assert!(
        retired.end < closed.end && closed.end < actual.end,
        "same-STA first retirement and Close wake attempts precede actual Close notification"
    );
}

pub(super) fn complete(trace: &str, key: &Key, after_event: bool, control_id: u64) {
    let trace = QueuedCloseTrace::from_complete(trace);
    bounded_retirement_wakes(&trace, key);
    one(&trace, "hold", "released", |_| true);
    let closed = one(&trace, "owner", "controller-closed", |v| {
        v["view"] == key.view
    });
    assert_eq!(closed.value["actual_closed"], true);
    assert_eq!(closed.value["native_storage_quiescent"], false);
    let intake = one(&trace, "owner", "ui-delivery", |v| {
        v["kind"] == "event" && same_event(v, key)
    });
    assert_eq!(intake.value["hold_active"], after_event);
    let posts: Vec<_> = trace
        .records
        .iter()
        .filter(|row| {
            row.kind == "owner"
                && row.value["phase"] == "post-return"
                && same_event(&row.value, key)
        })
        .collect();
    if after_event {
        assert_eq!(posts.len(), 1, "one actual target Post return");
        assert_eq!(posts[0].value["native_ok"], true);
        assert_eq!(posts[0].value["post_phase"], "returned");
        // These two receipts are synchronous on the same creating STA, and the
        // parent also waited for real Post return before issuing facade Close.
        assert!(posts[0].end < closed.end);
    } else {
        assert!(
            posts.is_empty(),
            "queued never-entered Event cannot acquire a native return"
        );
        assert!(
            intake.end > closed.end,
            "same-STA UI receipt occurs after actual Close"
        );
    }
    let retired = one(&trace, "owner", "retirement-handed-off", |v| {
        same_event(v, key)
    });
    assert_eq!(retired.value["actual_closed"], true);
    assert_eq!(retired.value["retirement_sent"], true);
    assert_eq!(
        retired.value["retirement"],
        if after_event {
            "outcome-unknown"
        } else {
            "not-dispatched"
        }
    );
    let release = one(&trace, "owner", "release-observed", |v| same_event(v, key));
    assert_eq!(release.value["released"], true);
    let dropped = one(&trace, "owner", "native-storage-dropped", |v| {
        same_event(v, key)
    });
    assert_eq!(dropped.value["released"], true);
    assert_eq!(dropped.value["actual_closed"], true);
    assert_eq!(
        dropped.value["post_phase"],
        if after_event { "returned" } else { "cancelled" }
    );
    let route_intake = one(&trace, "owner", "ui-delivery", |v| {
        v["kind"] == "route-state"
            && v["view"] == key.view
            && v["navigation"] == key.navigation
            && v["ready"] == false
    });
    assert_eq!(route_intake.value["hold_active"], false);
    assert!(
        route_intake.end > closed.end,
        "same-STA route intake follows same actual Close"
    );
    let route_drop = one(&trace, "owner", "route-storage-dropped", |v| {
        v["view"] == key.view && v["navigation"] == key.navigation && v["ready"] == false
    });
    assert!(route_drop.end > closed.end);
    assert_scope_quiescence(&trace, key, control_id);
    let drained = one(&trace, "owner", "view-drain", |v| {
        v["view"] == key.view && v["drained"] == true
    });
    assert!(
        drained.end > dropped.end && drained.end > route_drop.end,
        "same-STA View drain cannot precede actual DATA or RouteState storage Drop"
    );
}

fn assert_scope_quiescence(trace: &QueuedCloseTrace, key: &Key, control_id: u64) {
    let scopes: Vec<_> = trace
        .records
        .iter()
        .filter(|row| row.kind == "scope" && physically_quiescent(&row.value, key))
        .collect();
    assert!(
        !scopes.is_empty(),
        "actual same Entry storage must become physically quiescent"
    );
    for row in scopes {
        let scope = &row.value["scope"];
        known_scope(scope);
        assert!(
            rows(scope, "data")
                .iter()
                .all(|data| data["native_released"] == true)
        );
        assert!(
            rows(scope, "controls")
                .iter()
                .all(|control| control["native_released"] == true)
        );
        assert!(
            rows(scope, "data")
                .iter()
                .all(|data| data["native_release_id"] != key.release_id
                    || data["native_released"] == true)
        );
        assert!(
            rows(scope, "controls")
                .iter()
                .all(|control| control["native_release_id"] != control_id
                    || control["native_released"] == true)
        );
    }
}
