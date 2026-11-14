//! Pair an actual moved-source snapshot with that preparation's actual restoration.
//! Output order is not causality: the same guarded active attempt is the witness.
use super::{Key, QueuedCloseTrace, Value};

pub(super) fn needed_attempt(record: &Value) -> Option<u64> {
    let charge = &record["main_to_renderer"];
    if charge["terminal_preparing"]
        .as_bool()
        .expect("actual preparation state")
    {
        Some(
            charge["source_attempt"]
                .as_u64()
                .filter(|value| (1..=128).contains(value))
                .expect("bounded actual active source attempt"),
        )
    } else {
        assert!(
            charge["source_attempt"].is_null(),
            "resident source has no active attempt"
        );
        None
    }
}

fn preparing(record: &Value, key: &Key, selected: &Value, attempt: u64) {
    super::unadmitted_charge(record, key, selected);
    let charge = &record["main_to_renderer"];
    assert!(
        charge["selected"].is_null(),
        "source is in the actual preparation"
    );
    assert_eq!(charge["terminal_preparing"], true);
    assert_eq!(charge["source_attempt"], attempt);
}

pub(super) fn retained(
    trace: &QueuedCloseTrace,
    record: &Value,
    key: &Key,
    selected: &Value,
) -> Option<Value> {
    let Some(attempt) = needed_attempt(record) else {
        super::retired_source(record, key, selected);
        return None;
    };
    preparing(record, key, selected, attempt);
    super::retired_state(record);
    let same_attempt = |row: &Value| {
        row["source_attempt"] == attempt
            && row["selected"]["webview"] == key.view
            && row["selected"]["navigation"] == key.navigation
            && row["selected"]["message"] == key.message
    };
    let source = super::one(trace, "selected", "source-selected", same_attempt);
    assert_eq!(source.value["selected"], *selected, "original source owner");
    preparing(&source.value, key, selected, attempt);
    let restored = super::one(
        trace,
        "selected",
        "source-restored-after-denied-target",
        same_attempt,
    );
    assert_eq!(
        restored.value["selected"], *selected,
        "same restored source allocations"
    );
    for row in [&source.value, &restored.value] {
        assert_eq!(row["value_full"], false);
        assert!(row["retirement_failed"].is_null());
        assert!(row["retirement_refusal"].is_null());
    }
    // This is the captured post-helper record, not an inferred later ledger row.
    super::retired_source(&restored.value, key, selected);
    let before = record["main_to_renderer"]
        .as_object()
        .expect("actual moved charge");
    let after = restored.value["main_to_renderer"]
        .as_object()
        .expect("actual restored charge");
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    for (field, value) in before {
        if !matches!(
            field.as_str(),
            "selected" | "terminal_preparing" | "source_attempt"
        ) {
            assert_eq!(
                after.get(field),
                Some(value),
                "same original charge field {field}"
            );
        }
    }
    Some(restored.value.clone())
}

#[cfg(test)]
#[path = "restoration/tests.rs"]
mod tests;
