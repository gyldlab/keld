//! Checks actual creating-STA and shared observer sequence order.
use super::{
    Key, custody, end, not_entered, number, one, returned, rows, stamp, storage, synthetic_post,
    target_post, value,
};
use serde_json::Value;

fn callback_bracket(receipt: &Value, cut: &Value) {
    let position = number(&cut["sequence"]);
    let enter = rows(receipt, "callbacks")
        .iter()
        .map(value)
        .filter(|v| v["phase"] == "callback-enter" && number(&v["sequence"]) < position)
        .max_by_key(|v| number(&v["sequence"]))
        .expect("actual native callback entry");
    let leave = rows(receipt, "callbacks")
        .iter()
        .map(value)
        .filter(|v| v["phase"] == "callback-leave" && number(&v["sequence"]) > position)
        .min_by_key(|v| number(&v["sequence"]))
        .expect("actual native callback exit");
    assert_eq!(enter["state"]["callback_depth"], 1);
    assert_eq!(cut["state"]["callback_depth"], 1);
    assert_eq!(leave["state"]["callback_depth"], 0);
}
pub(super) fn navigation(receipt: &Value, key: &Key) -> (u64, u64, u64) {
    let a = value(&receipt["a_commit"]);
    let start = value(&receipt["b_start"]);
    let b = value(&receipt["b_commit"]);
    let a_commit = number(&a["native_id"]);
    let b_commit = number(&b["native_id"]);
    let b_generation = number(&b["state"]["generation"]);
    assert!(a_commit != 0 && b_commit != 0 && a_commit != b_commit);
    assert_ne!(b_generation, key.navigation);
    assert_eq!(a["phase"], "content-committed");
    assert_eq!(b["phase"], "content-committed");
    stamp(&a["state"], key.view, key.navigation, a_commit);
    stamp(&b["state"], key.view, b_generation, b_commit);
    assert_eq!(start["phase"], "navigation-starting");
    assert_eq!(start["state"]["view"], key.view);
    assert_eq!(start["native_id"], b_commit);
    assert_eq!(start["admitted"], true);
    assert!(start["state"]["generation"].is_null() && start["state"]["native_commit"].is_null());
    assert!(
        number(&a["sequence"]) < number(&start["sequence"])
            && number(&start["sequence"]) < number(&b["sequence"])
    );
    for raw in rows(receipt, "callbacks") {
        let row = value(raw);
        assert_ne!(row["phase"], "observation-overflow");
        assert_eq!(row["invalid"], false);
        assert_eq!(row["call_active"], false);
        assert!(row["in_call_records"].as_u64().is_none_or(|n| n == 0));
        assert_eq!(row["state"]["view"], key.view);
        assert_eq!(row["state"]["posting"], false);
        assert_eq!(row["state"]["reentry_failed"], false);
    }
    for cut in [a, start, b] {
        assert!(
            rows(receipt, "callbacks")
                .iter()
                .any(|row| value(row) == cut)
        );
        callback_bracket(receipt, cut);
    }
    (a_commit, b_generation, b_commit)
}

fn synthetic(post: &Value, key: &Key, commit: u64, b_start: u64) {
    not_entered(post);
    assert_eq!(
        post["role_live_before"], true,
        "zero calls alone cannot prove the real callback gate"
    );
    assert_eq!(post["target"]["callback_depth"], 0);
    for (field, depth) in [
        ("synthetic_enter", 1),
        ("final_gate", 1),
        ("synthetic_leave", 0),
    ] {
        stamp(&post[field]["state"], key.view, key.navigation, commit);
        assert_eq!(post[field]["state"]["callback_depth"], depth);
        assert_eq!(post[field]["state"]["posting"], false);
    }
    let enter = number(&post["synthetic_enter"]["sequence"]);
    let gate = number(&post["final_gate"]["sequence"]);
    let decision = number(&post["decision_sequence"]);
    let leave = number(&post["synthetic_leave"]["sequence"]);
    assert!(
        number(&post["attempt_sequence"]) < enter
            && enter < gate
            && gate < decision
            && decision < leave
            && leave < b_start
    );
}

pub(super) fn target_order(receipt: &Value, key: &Key, case: &str, a_commit: u64) -> bool {
    let posts = rows(receipt, "target_posts");
    let b_start = number(&receipt["b_start"]["value"]["sequence"]);
    let mut entries = 0;
    let mut previous = 0;
    for (index, row) in posts.iter().enumerate() {
        let post = value(row);
        assert!(target_post(post, key));
        stamp(&post["target"], key.view, key.navigation, a_commit);
        storage(post, key.message);
        assert_eq!(
            post["event_storage"],
            value(&posts[0])["event_storage"],
            "every attempt retains the SAME original Event allocations"
        );
        assert!(number(&post["attempt_sequence"]) > previous);
        assert!(
            end(row) < end(one(receipt, "drops")),
            "original allocation remains alive through the actual wrapper"
        );
        previous = number(&post["decision_sequence"]);
        if post["synthetic_enter"].is_object() {
            assert_eq!(case, "synthetic-before-entry");
            assert_eq!(
                index, 0,
                "synthetic hold covers the first actual Event wrapper attempt"
            );
            synthetic(post, key, a_commit, b_start);
            previous = number(&post["synthetic_leave"]["sequence"]);
        } else {
            assert!(post["synthetic_enter"].is_null() && post["synthetic_leave"].is_null());
            if post["entry_sequence"].is_null() {
                not_entered(post);
            } else {
                returned(post);
                entries += 1;
                assert_eq!(post["role_live_before"], true);
                stamp(
                    &post["final_gate"]["state"],
                    key.view,
                    key.navigation,
                    a_commit,
                );
                assert_eq!(post["final_gate"]["state"]["callback_depth"], 0);
                assert_eq!(post["final_gate"]["state"]["posting"], false);
                assert!(number(&post["return_sequence"]) < b_start);
            }
        }
    }
    assert!(
        entries <= 1,
        "the same original Event can enter the actual API only once"
    );
    if case == "after-return" {
        assert_eq!(entries, 1);
    }
    if case == "before-entry" {
        assert_eq!(entries, 0);
    }
    if case == "synthetic-before-entry" {
        synthetic_post(receipt);
    }
    entries == 1
}

pub(super) fn owner_order(receipt: &Value, key: &Key, case: &str, entered: bool) {
    let after = case == "after-return";
    let owner_posts = rows(receipt, "owner_posts");
    assert_eq!(owner_posts.len(), usize::from(entered));
    for row in owner_posts {
        assert!(custody::same_event(value(row), key));
        assert_eq!(value(row)["native_ok"], true);
        assert_eq!(value(row)["post_phase"], "returned");
        assert!(end(row) < end(&receipt["b_start"]));
    }
    let intake: Vec<_> = rows(receipt, "ui_deliveries")
        .iter()
        .filter(|row| value(row)["kind"] == "event")
        .collect();
    assert_eq!(intake.len(), 1);
    assert!(custody::same_event(value(intake[0]), key));
    assert_eq!(value(intake[0])["hold_active"], after);
    if case == "before-entry" {
        assert!(end(&receipt["b_commit"]) < end(intake[0]));
    } else {
        assert!(end(intake[0]) < end(&rows(receipt, "target_posts")[0]));
    }
    let holds = rows(receipt, "hold");
    if case == "synthetic-before-entry" {
        assert_eq!(holds, &[] as &[Value]);
    } else {
        let held: Vec<_> = holds
            .iter()
            .filter(|row| value(row)["phase"] == "held")
            .collect();
        let released: Vec<_> = holds
            .iter()
            .filter(|row| value(row)["phase"] == "released")
            .collect();
        assert_eq!(held.len(), 1);
        assert_eq!(released.len(), 1);
        let mode = if after {
            "after-next-event"
        } else {
            "before-intake"
        };
        assert_eq!(value(held[0])["mode"], mode);
        assert_eq!(value(released[0])["mode"], mode);
        assert!(end(held[0]) < end(released[0]));
        assert!(
            end(&receipt["b_commit"]) < end(released[0]),
            "B native commit is observed before the parent releases the actual hold"
        );
        if after {
            assert_eq!(value(held[0])["webview"], key.view);
            assert_eq!(value(held[0])["navigation"], key.navigation);
            assert_eq!(value(held[0])["delivery"], key.message);
        } else {
            assert!(end(released[0]) < end(intake[0]));
        }
    }
}

pub(super) fn consumer_order(receipt: &Value, key: &Key, entered: bool, a_commit: u64) {
    let consumed = rows(receipt, "consumed");
    assert!(consumed.len() <= 1);
    if !entered {
        assert!(
            consumed.is_empty(),
            "never-entered A cannot gain a consumer receipt"
        );
    }
    for row in consumed {
        let row = value(row);
        assert_eq!(row["phase"], "consumer-observed");
        assert_eq!(row["invalid"], false);
        assert_eq!(row["call_active"], false);
        assert_eq!(row["consumed"]["view"], key.view);
        assert_eq!(row["consumed"]["generation"], key.navigation);
        assert_eq!(row["consumed"]["native_commit"], a_commit);
        assert_eq!(row["consumed"]["delivery"], key.message);
        let actual_return = rows(receipt, "target_posts")
            .iter()
            .map(value)
            .find(|post| post["return_sequence"].is_u64())
            .expect("actual original API return");
        assert!(number(&row["sequence"]) > number(&actual_return["return_sequence"]));
        // Preserve the exact shared sequence; do not infer absence or pending-message discard.
    }
}
