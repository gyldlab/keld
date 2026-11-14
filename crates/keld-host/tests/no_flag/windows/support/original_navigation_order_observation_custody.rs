//! Original Event ownership through actual release, Drop, Close and quiescence.
use super::{Key, custody, end, one, rows, value};
use serde_json::Value;

pub(super) fn assert_disposed(receipt: &Value, key: &Key, entered: bool) {
    let dropped = one(receipt, "drops");
    let released = one(receipt, "releases");
    for (row, phase) in [
        (dropped, "native-storage-dropped"),
        (released, "release-observed"),
    ] {
        let row = value(row);
        assert_eq!(row["phase"], phase);
        assert!(custody::same_event(row, key));
        assert_eq!(row["released"], true);
    }
    assert!(
        end(released) < end(dropped),
        "actual owner release precedes its physical Drop"
    );
    assert_eq!(
        value(dropped)["post_phase"],
        if entered { "returned" } else { "cancelled" }
    );
    let retired = rows(receipt, "retirements");
    assert!(retired.len() <= 1);
    if entered && retired.is_empty() {
        assert_eq!(
            rows(receipt, "consumed").len(),
            1,
            "a real consumer terminal explains release without retirement"
        );
    } else {
        assert_eq!(retired.len(), 1, "one actual original Event retirement");
        let row = value(&retired[0]);
        assert!(custody::same_event(row, key));
        assert_eq!(row["retirement_sent"], true);
        assert_eq!(
            row["retirement"],
            if entered {
                "outcome-unknown"
            } else {
                "not-dispatched"
            }
        );
        if entered {
            assert_eq!(row["actual_closed"], true);
        }
        assert!(end(&retired[0]) < end(released));
    }
    let closed = one(receipt, "controller_closes");
    assert_eq!(value(closed)["view"], key.view);
    assert_eq!(value(closed)["actual_closed"], true);
    assert!(end(one(receipt, "marker_posts")) < end(closed));
    assert!(
        rows(receipt, "view_drains")
            .iter()
            .any(|row| value(row)["view"] == key.view
                && value(row)["drained"] == true
                && end(row) > end(dropped)
                && end(row) > end(closed)),
        "actual same view drains after its storage Drop and Close"
    );
    let scopes = rows(receipt, "quiescent_scopes");
    assert!(!scopes.is_empty(), "actual same Entry physical quiescence");
    for row in scopes {
        let row = value(row);
        assert!(custody::physically_quiescent(row, key));
        let scope = &row["scope"];
        assert_eq!(scope["unknown_native_data"], 0);
        assert_eq!(scope["unknown_native_controls"], 0);
        for field in ["data", "controls"] {
            for slot in scope[field]
                .as_array()
                .expect("fixed native census")
                .iter()
                .filter(|slot| !slot.is_null())
            {
                assert_eq!(
                    slot["native_released"], true,
                    "no original allocation may remain charged as native owned"
                );
            }
        }
    }
}
