//! Passive X3 fixture assertions. These functions confer no renderer authority.
//! The native record must come from the actual retained `Frame2` callback.
use serde_json::Value;

fn actual_epoch(page: &Value) -> &str {
    let epoch = page["rawChallenge"]
        .as_str()
        .expect("actual top app-call epoch");
    assert_eq!(epoch.len(), 32);
    assert!(
        epoch
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    epoch
}

pub(super) fn assert_ready_record(record: &Value, view: u64) {
    assert_eq!(record["kind"], "ready", "actual Frame2 registration return");
    assert_eq!(record["view"], view);
    assert_eq!(record["senderMatchesTop"], true);
    assert_eq!(record["messageRegistrationReturned"], true);
    assert_eq!(record["destroyedRegistrationReturned"], true);
    assert_eq!(record["alreadyDestroyed"], false);
}

pub(super) fn assert_actual_frame_ingress(page: &Value, record: &Value, view: u64) {
    assert!(page["error"].is_null(), "page stimulus failure: {page}");
    assert_eq!(record["kind"], "message", "actual Frame2 event is required");
    assert_eq!(record["view"], view);
    assert_eq!(record["senderMatchesCreatedFrame"], true);
    assert_eq!(record["source"], "about:blank");
    let outer = record["outerJson"]
        .as_str()
        .expect("actual WebMessageAsJson");
    assert!(outer.len() <= 4096, "bounded native observer text");
    let raw: String =
        serde_json::from_str(outer).expect("complete native JSON must encode exactly one String");
    let epoch = actual_epoch(page);
    // Byte equality against this fixed stimulus is an observation oracle, not
    // a second production envelope decoder. Embedded NUL/trailing text fails.
    let expected = format!(
        r#"{{"v":1,"kind":"call","document":"{epoch}","call":2,"method":"sendMessage","args":["app-load-finished",{{"k":"object","entries":[["probe","keld-original-x3"],["path","frame"]]}}]}}"#
    );
    assert_eq!(raw, expected, "actual complete child-port envelope");
    let frame = &page["frame"];
    assert_eq!(frame["top"], false);
    assert_eq!(frame["href"], "about:blank");
    assert_eq!(frame["ownPort"], true);
    assert_eq!(frame["sent"], true);
    assert!(frame["error"].is_null());
    for name in [
        "request",
        "registerMsgListener",
        "sendMessage",
        "listenOnce",
    ] {
        assert_eq!(frame["frameworkMethods"][name], "undefined");
    }
    assert_eq!(page["frameInserted"], true);
    assert_eq!(page["armed"], true);
}

pub(super) fn assert_no_effect_before_top(counters: &Value) {
    assert_eq!(counters["frame"], 0, "actual Bun child listener effect");
    assert_eq!(
        counters["top"], 0,
        "healthy control has not yet been issued"
    );
    assert_eq!(counters["invalid"], 0);
}

pub(super) fn assert_healthy_top(page: &Value, counters: &Value) {
    assert!(page["error"].is_null(), "page control failure: {page}");
    assert_eq!(page["topIssued"], true);
    assert_eq!(counters["frame"], 0, "child must remain undispatched");
    assert_eq!(counters["top"], 1, "actual public Bun listener effect");
    assert_eq!(counters["invalid"], 0);
    let terminal = &page["terminal"];
    assert_eq!(terminal["kind"], "send-result");
    assert_eq!(terminal["message"], 1);
    assert_eq!(terminal["document"], actual_epoch(page));
    assert_eq!(terminal["disposition"]["status"], "consumed");
    let records = page["rawRecords"]
        .as_array()
        .expect("actual top port records");
    assert!(!records.is_empty() && records.len() <= 12);
    let mut saw_entry = false;
    let mut saw_terminal = false;
    for record in records {
        let text = record["text"].as_str().expect("actual native top text");
        assert!(text.len() <= 2048);
        let value: Value = serde_json::from_str(text).expect("complete native top JSON");
        if value["kind"] == "app-call" && value["call"] == 2 {
            assert_eq!(
                record["topIssued"], true,
                "no child-caused top Function entry"
            );
            assert_eq!(value["document"], actual_epoch(page));
            assert_eq!(value["method"], "sendMessage");
            saw_entry = true;
        }
        saw_terminal |= &value == terminal;
    }
    assert!(saw_entry, "actual healthy original Function entry");
    assert!(saw_terminal, "actual healthy native terminal");
}

pub(super) fn assert_complete_control_trace(control: &str) {
    let mut top_effects = 0_usize;
    for line in control.lines() {
        assert!(
            !line.starts_with("IPC_INVALID "),
            "invalid real application effect"
        );
        assert!(
            !line.starts_with("FIXTURE_ERROR "),
            "actual fixture operation failed"
        );
        if let Some(text) = line.strip_prefix("IPC_EFFECT ") {
            let event: Value = serde_json::from_str(text).expect("actual IPC effect record");
            assert_eq!(event["path"], "top", "no child effect through actual EOF");
            assert_eq!(event["count"], 1);
            top_effects += 1;
        }
    }
    assert_eq!(
        top_effects, 1,
        "exact healthy real effect through process exit"
    );
}

pub(super) fn assert_complete_native_trace(trace: &str, page: &Value, view: u64) {
    let mut ready = 0_usize;
    let mut messages = 0_usize;
    for line in trace.lines() {
        assert!(
            !line.starts_with("KELD_X3_FRAME_FAULT "),
            "native observer fault"
        );
        if let Some(text) = line.strip_prefix("KELD_X3_FRAME ") {
            assert!(text.len() <= 32768, "bounded actual native record");
            let record: Value = serde_json::from_str(text).expect("actual Frame2 trace");
            let kind = record["kind"].as_str().expect("actual native record kind");
            assert!(matches!(kind, "ready" | "message" | "destroyed"));
            assert_eq!(record["view"], view);
            if kind == "ready" {
                assert_ready_record(&record, view);
                ready += 1;
            } else if kind == "message" {
                assert_actual_frame_ingress(page, &record, view);
                messages += 1;
            } else {
                assert_eq!(record["senderMatchesCreatedFrame"], true);
            }
        }
    }
    assert_eq!(ready, 1, "one actual frame registration");
    assert_eq!(
        messages, 1,
        "one actual child-native post through process exit"
    );
}
