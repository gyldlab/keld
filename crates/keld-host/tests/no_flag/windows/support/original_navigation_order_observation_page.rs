//! Positive B native marker and unfiltered A-canary byte oracle.
use super::{CANARY, Key, number, one, returned, rows, stamp, value};
use serde_json::Value;

fn payload(value: &Value, expected: &[u8]) -> bool {
    value["payload"].as_array().is_some_and(|bytes| {
        bytes.len() == expected.len()
            && bytes
                .iter()
                .zip(expected)
                .all(|(actual, expected)| actual.as_u64() == Some(u64::from(*expected)))
    })
}
fn all_readbacks(receipt: &Value) {
    let mut matched = false;
    for raw in rows(receipt, "page_readbacks") {
        let readback = value(raw);
        assert_ne!(readback["originalProbeOversize"], true);
        let Some(text) = readback["originalProbe"].as_str() else {
            continue;
        };
        assert!(text.len() <= 32768);
        let page: Value = serde_json::from_str(text).expect("actual first-author page readback");
        if page["documentKind"] != "B" {
            continue;
        }
        matched |= page == receipt["page_b"];
        assert!(
            page["error"].is_null(),
            "a later B page error cannot be hidden by an earlier terminal"
        );
        assert_eq!(page["rawACanaries"], 0);
        let records = page["rawRecords"]
            .as_array()
            .expect("complete B native strings");
        assert!(records.len() <= 12);
        for record in records {
            assert_eq!(record["sourceIsNativePort"], true);
            let text = record["text"].as_str().expect("raw native string");
            assert!(text.len() <= 2048);
            let native: Value = serde_json::from_str(text).expect("raw Host JSON");
            assert!(
                !payload(&native, CANARY),
                "B raw bytes contain no A canary, including foreign epochs"
            );
        }
    }
    assert!(
        matched,
        "chosen B snapshot must be an actual completed-trace readback"
    );
}
pub(super) fn assert_b(receipt: &Value, key: &Key, generation: u64, commit: u64) {
    all_readbacks(receipt);
    let a = &receipt["page_a"];
    let b = &receipt["page_b"];
    assert_eq!(a["documentKind"], "A");
    assert_eq!(a["routeMarkerKind"], "event");
    assert_eq!(b["documentKind"], "B");
    assert_eq!(b["first"]["firstAuthorScript"], true);
    assert_eq!(b["first"]["authorScriptCount"], 1);
    assert_eq!(b["first"]["readyState"], "loading");
    assert_eq!(b["first"]["top"], true);
    assert!(b["error"].is_null());
    assert_eq!(b["registered"], true);
    assert_eq!(b["rawACanaries"], 0);
    assert_eq!(b["entered"], 0);
    assert!(b["targetDelivery"].is_null());
    let document = b["routeMarker"].as_str().expect("actual B marker document");
    assert_eq!(document.len(), 32);
    assert!(
        document
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_ne!(b["routeMarker"], a["routeMarker"]);
    assert_eq!(b["rawChallenge"], document);
    assert_eq!(b["routeMarkerKind"], "route-state");
    assert!(b["routeDelivery"].is_null());
    let raw = b["rawRecords"]
        .as_array()
        .expect("complete prefilter native records");
    assert!(!raw.is_empty() && raw.len() <= 12);
    let mut ready = 0;
    for row in raw {
        assert_eq!(row["sourceIsNativePort"], true);
        let text = row["text"]
            .as_str()
            .expect("actual unfiltered native string");
        assert!(text.len() <= 2048);
        let actual: Value = serde_json::from_str(text).expect("actual raw Host JSON");
        // Deliberately precedes every epoch, kind and channel filter.
        assert!(
            !payload(&actual, CANARY),
            "B raw bytes contain no A canary, including foreign epochs"
        );
        if actual["kind"] == "route-state" && actual["ready"] == true {
            assert_eq!(actual["v"], 2);
            assert_eq!(actual["document"], document);
            ready += 1;
        }
    }
    assert_eq!(ready, 1, "positive B raw native marker is required");
    crate::original_bridge_observation::assert_native_terminal(b, "facade", true);
    let marker = value(one(receipt, "marker_posts"));
    returned(marker);
    stamp(&marker["target"], key.view, generation, commit);
    stamp(&marker["final_gate"]["state"], key.view, generation, commit);
    assert_eq!(marker["role_live_before"], true);
    assert_eq!(marker["target"]["callback_depth"], 0);
    assert_eq!(marker["target"]["posting"], false);
    assert_eq!(marker["final_gate"]["state"]["callback_depth"], 0);
    assert_eq!(marker["final_gate"]["state"]["posting"], false);
    assert!(marker["synthetic_enter"].is_null() && marker["synthetic_leave"].is_null());
    assert_eq!(marker["carrier_kind"], "route-state");
    assert_eq!(marker["route_ready"], true);
    assert_eq!(marker["document_epoch"], document);
    assert!(marker["event_storage"].is_null());
    assert!(
        number(&receipt["b_commit"]["value"]["sequence"]) < number(&marker["attempt_sequence"])
    );
}
