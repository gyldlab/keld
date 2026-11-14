//! Independent X10 assertions over the actual live-role/ledger challenge.
use super::observation::{Key, same_scope};
use crate::support::original_bridge_run::{QueuedCloseRecord, QueuedCloseTrace};
use serde_json::Value;

fn controls(census: &Value) -> &[Value] {
    let rows = census["controls"]
        .as_array()
        .expect("actual fixed CONTROL census");
    assert_eq!(rows.len(), 4);
    assert_eq!(census["slot_limit"], 4);
    assert_eq!(census["byte_limit"], 65536);
    for (slot, row) in rows.iter().enumerate() {
        if !row.is_null() {
            assert_eq!(row["slot"], slot);
        }
    }
    rows
}

fn spare(census: &Value) {
    assert!(
        controls(census).iter().filter(|row| !row.is_null()).count() < 4,
        "fresh refusal cannot receive credit from a full CONTROL pool"
    );
    let retained = census["accounted_bytes"]
        .as_u64()
        .expect("actual global accounted bytes");
    assert!(
        retained <= 65536 - 1025,
        "one valid probe fits actual byte budget"
    );
}

pub(super) fn boundary(record: &Value, key: &Key) {
    assert!(same_scope(record, key));
    assert_eq!(record["phase"], "closed-producer-probed");
    assert!(
        record["caller_id"]
            .as_u64()
            .expect("exact reader-stop allocation")
            > 0
    );
    assert_eq!(record["encoded"], 1);
    assert_eq!(record["accounted"], 1025);
    let positive = &record["before_close"];
    assert_eq!(positive["caller_current"], serde_json::json!([true, true]));
    assert_eq!(positive["scope_closed"], serde_json::json!([false, false]));
    assert_eq!(positive["admitted"], true);
    assert_eq!(positive["admission_calls"], 1);
    assert_eq!(positive["reserved"], true);
    spare(&positive["before"]);
    let proof = &positive["release"];
    let before = controls(&positive["before"]);
    let held = controls(&proof["held"]);
    let added: Vec<_> = (0..4)
        .filter(|&slot| before[slot].is_null() && !held[slot].is_null())
        .collect();
    assert_eq!(
        added.len(),
        1,
        "one actual temporary reservation, not a supplied receipt"
    );
    let slot = added[0];
    let charge = &held[slot];
    assert_eq!(charge["window"]["id"], key.window);
    assert_eq!(charge["window"]["incarnation"], key.incarnation);
    assert_eq!(charge["encoded"], 1);
    assert_eq!(charge["retained"], 1025);
    assert_eq!(charge["application_done"], true);
    assert_eq!(charge["primary_write"], false);
    assert_eq!(charge["native_released"], false);
    assert!(
        charge["native_release_id"]
            .as_u64()
            .expect("actual probe release allocation")
            > 0
    );
    assert_eq!(
        proof["held"]["accounted_bytes"]
            .as_u64()
            .expect("held charge"),
        positive["before"]["accounted_bytes"]
            .as_u64()
            .expect("prior charge")
            + 1025
    );
    let dropped = controls(&proof["dropped"]);
    for index in 0..4 {
        if index != slot {
            assert_eq!(
                held[index], before[index],
                "probe changes no unrelated CONTROL"
            );
            assert_eq!(dropped[index], held[index]);
        }
    }
    let mut released = charge.clone();
    released["native_released"] = Value::Bool(true);
    assert_eq!(
        dropped[slot], released,
        "same original token Drop reports actual release"
    );
    assert_eq!(
        proof["dropped"]["accounted_bytes"],
        proof["held"]["accounted_bytes"]
    );
    assert_eq!(
        proof["returned"], positive["before"],
        "real reaper returns exact slot/bytes before production close"
    );

    let negative = &record["after_close"];
    assert_eq!(
        negative["caller_current"],
        serde_json::json!([true, true]),
        "role loss must not masquerade as closed-scope refusal"
    );
    assert_eq!(negative["scope_closed"], serde_json::json!([true, true]));
    spare(&negative["before"]);
    assert_eq!(negative["admitted"], false);
    assert_eq!(
        negative["admission_calls"], 0,
        "closed producer never enters native closure"
    );
    assert_eq!(negative["reserved"], false);
    for phase in ["held", "dropped", "returned"] {
        assert_eq!(
            negative["release"][phase], negative["before"],
            "refused fresh offer creates or retains no CONTROL charge"
        );
    }
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
    assert_eq!(rows.len(), 1, "one actual {kind}/{phase} observation");
    rows[0]
}

pub(super) fn complete(trace: &str, key: &Key) {
    let trace = QueuedCloseTrace::from_complete(trace);
    let probe = one(&trace, "fresh", "closed-producer-probed", |value| {
        same_scope(value, key)
    });
    boundary(&probe.value, key);
    let offers = one(&trace, "scope", "offers-closed", |value| {
        same_scope(value, key)
    });
    let closed = one(&trace, "owner", "controller-closed", |value| {
        value["view"] == key.view
    });
    assert!(
        offers.end < probe.end && probe.end < closed.end,
        "same-STA fresh refusal follows actual scope close and precedes actual controller Close"
    );
}
