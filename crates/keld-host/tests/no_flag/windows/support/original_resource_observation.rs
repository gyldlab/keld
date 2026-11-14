//! Independent scoring of real Work, Pending, ledger and Reader observations.
use crate::support::original_bridge_run::{ResourceRecord, ResourceTrace};
use serde_json::Value;

const ASSET: &str = "x14-held-resource.svg";

pub(super) fn assert_held(value: &Value, phase: &str, view: u64, bytes: usize) -> String {
    assert_eq!(value["phase"], phase);
    assert_eq!(value["view"], view);
    assert_eq!(value["asset"], ASSET);
    assert_eq!(value["bodyBytes"], bytes);
    assert_eq!(value["creditCoversBody"], true);
    assert!(
        value["accountBytes"]
            .as_u64()
            .expect("actual retained account")
            >= bytes as u64
    );
    let ticket = value["ticket"]
        .as_str()
        .expect("actual retained Work ticket");
    assert_ne!(ticket, "");
    ticket.to_owned()
}

pub(super) fn assert_controller_closed(value: &Value, ticket: &str, view: u64) {
    assert_eq!(value["phase"], "controller-closed");
    assert_eq!(value["session"]["view"], view);
    assert_eq!(
        value["retiring"], false,
        "Close precedes the ordinary terminal Quit"
    );
    assert_eq!(value.get("fault"), Some(&Value::Null));
    let pending = value["session"]["pending"]
        .as_array()
        .expect("actual Pending census");
    let matching: Vec<_> = pending.iter().filter(|p| p["ticket"] == ticket).collect();
    assert_eq!(
        matching.len(),
        1,
        "same Pending survives actual controller Close"
    );
    let pending = matching[0];
    assert_eq!(pending["view"], view);
    assert_eq!(pending["asset"], ASSET);
    assert_eq!(pending["pendingInMap"], true);
    assert_eq!(pending["admitting"], false);
    assert_eq!(pending["cancelled"], true);
    assert_eq!(pending["metadataCreditOwned"], true);
    assert_eq!(pending["bodyCreditInPending"], false);
    assert_eq!(pending["completedInPending"], false);
    assert_eq!(pending["ledger"]["stopped"], true);
    assert_eq!(pending["ledger"]["workerActive"], true);
    assert_eq!(value["reader"]["phase"], "retained");
    assert_eq!(value["reader"]["activeTicket"], ticket);
    assert_eq!(value["reader"]["shutdown"], false);
    assert_eq!(value["reader"]["joined"], false);
    assert_eq!(value["reader"]["unproved"], false);
    assert_eq!(value["reader"]["workerRetained"], true);
    assert_eq!(value["reader"]["waitRetained"], true);
    assert_eq!(value["reader"]["resultAvailable"], true);
    assert!(value["reader"]["resultTicket"].is_null());
    assert_eq!(value["reader"]["wakeFailed"], false);
}

fn one<'a>(records: &'a [ResourceRecord], kind: &str, phase: &str) -> &'a ResourceRecord {
    let matching: Vec<_> = records
        .iter()
        .filter(|record| record.kind == kind && record.value["phase"] == phase)
        .collect();
    assert_eq!(matching.len(), 1, "one actual {kind}/{phase} observation");
    matching[0]
}

pub(super) fn assert_complete(trace: &str, control: &str, ticket: &str, view: u64, bytes: usize) {
    // from_complete applies the early-drop oracle to every captured custody row,
    // including any row that arrived after the live scenario's last poll.
    let observed = ResourceTrace::from_complete(trace);
    let records = &observed.records;
    let held = one(records, "hold", "held");
    let checkpoint = one(records, "hold", "checkpoint");
    let closed = one(records, "custody", "controller-closed");
    let released = one(records, "hold", "released");
    let dropped = one(records, "custody", "pending-dropped");
    let retiring = one(records, "custody", "session-retiring");
    let drained = one(records, "custody", "session-drained");
    for (record, phase) in [
        (held, "held"),
        (checkpoint, "checkpoint"),
        (released, "released"),
    ] {
        assert_eq!(assert_held(&record.value, phase, view, bytes), ticket);
    }
    assert_eq!(held.value["cancelled"], false);
    assert_eq!(checkpoint.value["cancelled"], false);
    assert_eq!(released.value["cancelled"], true);
    assert_controller_closed(&closed.value, ticket, view);
    let prior = &dropped.value["prior"];
    assert_eq!(prior["ticket"], ticket);
    assert_eq!(prior["view"], view);
    assert_eq!(prior["asset"], ASSET);
    assert_eq!(prior["cancelled"], true);
    assert_eq!(prior["ledger"]["workerActive"], false);
    assert_eq!(prior["completedInPending"], true);
    assert_eq!(prior["completedBodyBytes"], bytes);
    assert_eq!(prior["completedCreditCoversBody"], true);
    assert_eq!(dropped.value["ledgerReleaseSucceeded"], true);
    assert!(
        held.end < checkpoint.end
            && checkpoint.end < closed.end
            && closed.end < released.end
            && released.end < dropped.end
            && dropped.end < retiring.end
            && retiring.end < drained.end,
        "same-ticket ordering must be held, restarted checkpoint, actual Close, release, actual Pending drop, Quit retirement, final drain"
    );
    assert_eq!(drained.value["retiring"], true);
    assert_eq!(drained.value.get("fault"), Some(&Value::Null));
    for key in [
        "pendingCount",
        "admittingCount",
        "viewCount",
        "filterTransitionCount",
        "closingFilterCount",
    ] {
        assert_eq!(
            drained.value["session"][key], 0,
            "final actual session {key}"
        );
    }
    assert_eq!(drained.value["session"]["ledgerEmpty"], true);
    assert_eq!(drained.value["session"]["ledgerActiveDelivery"], false);
    let reader = &drained.value["reader"];
    assert_eq!(reader["phase"], "retained");
    assert_eq!(reader["shutdown"], true);
    assert_eq!(reader["joined"], true);
    assert_eq!(reader["unproved"], false);
    assert_eq!(reader["workerRetained"], false);
    assert_eq!(reader["waitRetained"], false);
    assert_eq!(reader["resultAvailable"], true);
    assert!(reader["activeTicket"].is_null());
    assert!(reader["resultTicket"].is_null());
    assert_eq!(reader["wakeFailed"], false);
    for prefix in ["CLOSE_EVENT ", "CLOSED ", "ALL_CLOSED ", "QUIT_ACCEPTED "] {
        assert_eq!(
            control
                .lines()
                .filter(|line| line.starts_with(prefix))
                .count(),
            1,
            "one actual recovered fixture {prefix}"
        );
    }
    assert!(!control.contains("FIXTURE_ERROR "));
}
