//! Receipt-observer calibration and one-fault controls. No native acceptance is implied.

use super::{SCHEMA, assess};
use serde_json::{Value, json};

fn event(phase: &str, kind: &str, data: Value, seq: u64) -> Value {
    let mut event = json!({"phase":phase,"kind":kind,"seq":seq});
    event["data"] = data;
    event
}

fn sample() -> Value {
    let mut events = Vec::new();
    for (phase, id) in [("host-header", 11), ("plain", 12), ("page", 13)] {
        let header = if phase == "host-header" {
            json!({"state":"present","value":"attempt"})
        } else {
            json!({"state":"absent"})
        };
        events.push(event(
            phase,
            "navigation-starting",
            json!({"uri":"file:///D:/owned/doc.html","navigation_id":id,"header":header}),
            1,
        ));
        events.push(event(
            phase,
            "resource-request",
            json!({"uri":"file:///D:/owned/doc.html","resource_context":1,"header":header}),
            2,
        ));
        events.push(event(
            phase,
            "content-loading",
            json!({"navigation_id":id,"is_error_page":false}),
            3,
        ));
        events.push(event(
            phase,
            "dom-content-loaded",
            json!({"navigation_id":id}),
            4,
        ));
        events.push(event(
            phase,
            "navigation-completed",
            json!({"navigation_id":id,"success":true}),
            5,
        ));
    }
    events.push(event(
        "iframe",
        "frame-navigation-starting",
        json!({"uri":"file:///D:/owned/doc.html","navigation_id":14,"header":{"state":"absent"}}),
        1,
    ));
    events.push(event(
        "iframe",
        "resource-request",
        json!({"uri":"file:///D:/owned/doc.html","resource_context":1,"header":{"state":"absent"}}),
        2,
    ));
    events.push(event(
        "iframe",
        "frame-navigation-completed",
        json!({"navigation_id":14,"success":true}),
        3,
    ));
    events.push(event(
        "cleanup",
        "browser-process-exited",
        json!({"pid":1234,"normal":true}),
        20,
    ));
    // Trace::record uses one global sequence, not a counter that resets per phase.
    for (index, observed) in events.iter_mut().enumerate() {
        observed["seq"] = json!(index + 1);
    }
    let page = json!({"nonce":"probe","href":"file:///D:/owned/doc.html","frames":[]});
    json!({
        "schema":SCHEMA,"helper":"windows_file_native_probe_child","errors":[],"runtime_version":"fixture-observer-calibration","devtools_enabled_readback":false,
        "message_pump_calibration":{"ready_before_queue_drained":true,"returned_on_readiness":true,"queued":100,"remaining_at_readiness":99,"readiness_polls":3},
        "nonce":"probe","attempt_nonce":"attempt","uri":"file:///D:/owned/doc.html","browser_pid":1234,"events":events,
        "host_request":{"uri":"file:///D:/owned/doc.html","sent_value":"attempt","create_hresult":"S_OK","navigate_hresult":"S_OK","input_header":{"state":"present","value":"attempt"}},
        "pages":{"host-header":page,"plain":page,"page":page,"iframe":{"nonce":"probe","href":"file:///D:/owned/doc.html","frames":[{"nonce":"probe","href":"file:///D:/owned/doc.html"}]}},
        "evaluations":{
            "execute_string":{"hresult":"S_OK","body":"execute-string-probe"},
            "execute_promise":{"hresult":"S_OK","body":{}},
            "cdp_string":{"hresult":"S_OK","body":{"result":{"type":"string","value":"cdp-string-probe"}}},
            "cdp_promise":{"hresult":"S_OK","body":{"result":{"type":"string","value":"cdp-promise-probe"}}},
            "cdp_throw":{"hresult":"S_OK","body":{
                "result":{"type":"object","subtype":"error","className":"TypeError","description":"TypeError: throw-probe\n    at <anonymous>:1:16"},
                "exceptionDetails":{"exception":{"type":"object","subtype":"error","className":"TypeError","description":"TypeError: throw-probe\n    at <anonymous>:1:16"}}
            }},
            "cdp_reject":{"hresult":"S_OK","body":{
                "result":{"type":"object","subtype":"error","className":"RangeError","description":"RangeError: reject-probe\n    at <anonymous>:1:42"},
                "exceptionDetails":{"exception":{"type":"object","subtype":"error","className":"RangeError","description":"RangeError: reject-probe\n    at <anonymous>:1:42"}}
            }},
            "cdp_recovery":{"hresult":"S_OK","body":{"result":{"type":"string","value":"cdp-recovery-probe"}}}
        },
        "cleanup":{"fatal_process_exit_required":false,"controller_closed":true,"window_destroyed":true,"profile_removed":true,"com_uninitialized":true,"faults":[]}
    })
}

#[test]
fn exhausted_native_queue_is_not_hidden_by_a_calibration_summary() {
    let mut receipt = sample();
    receipt["message_pump_calibration"]["remaining_at_readiness"] = json!(0);
    assert!(assess(&receipt).is_err());
}

#[test]
fn fatal_pending_creation_is_not_orderly_cleanup() {
    let mut receipt = sample();
    receipt["cleanup"]["fatal_process_exit_required"] = json!(true);
    assert!(assess(&receipt).is_err());
}

#[test]
fn missing_native_events_cannot_pass() {
    let mut receipt = sample();
    receipt["events"] = json!([]);
    assert!(assess(&receipt).is_err());
}

#[test]
fn wrong_page_nonce_cannot_pass() {
    let mut receipt = sample();
    receipt["pages"]["iframe"]["frames"][0]["nonce"] = json!("foreign");
    assert!(assess(&receipt).is_err());
}

#[test]
fn fabricated_completion_order_cannot_pass() {
    let mut receipt = sample();
    receipt["events"][9]["seq"] = json!(1);
    assert!(assess(&receipt).is_err());
}

#[test]
fn measured_header_loss_is_refutation_not_a_missing_observation() {
    let mut receipt = sample();
    assert_eq!(
        assess(&receipt).expect("complete calibration")["q22_a_b_header_propagation_observed"],
        true
    );
    receipt["events"][0]["data"]["header"] = json!({"state":"absent"});
    assert_eq!(
        assess(&receipt).expect("complete negative observation")["q22_a_b_header_propagation_observed"],
        false
    );
}

#[test]
fn native_method_rejection_is_preserved_as_refutation() {
    let mut receipt = sample();
    receipt["host_request"]["navigate_hresult"] = json!("0x80070057");
    receipt["events"]
        .as_array_mut()
        .expect("array")
        .retain(|event| event["phase"] != "host-header");
    assert_eq!(
        assess(&receipt).expect("rejection plus healthy controls")["q22_a_b_header_propagation_observed"],
        false
    );
}

#[test]
fn getter_failure_is_not_converted_to_header_absence() {
    let mut receipt = sample();
    receipt["events"][0]["data"]["header"] = json!({"state":"error","hresult":"0x80004005"});
    assert!(assess(&receipt).is_err());
}

#[test]
fn unobserved_browser_exit_cannot_be_hidden_by_profile_deletion() {
    let mut receipt = sample();
    receipt["events"]
        .as_array_mut()
        .expect("array")
        .retain(|event| event["kind"] != "browser-process-exited");
    assert!(assess(&receipt).is_err());
}

#[test]
fn cleanup_observer_removal_failure_cannot_pass() {
    let mut receipt = sample();
    receipt["cleanup"]["faults"] = json!(["remove BrowserProcessExited: 0x80004005"]);
    assert!(assess(&receipt).is_err());
}

#[test]
fn enabled_devtools_cannot_claim_the_disabled_configuration() {
    let mut receipt = sample();
    receipt["devtools_enabled_readback"] = json!(true);
    assert!(assess(&receipt).is_err());
}

#[test]
fn a_header_on_the_same_uri_iframe_refutes_attribution() {
    let mut receipt = sample();
    receipt["events"][15]["data"]["header"] = json!({"state":"present","value":"probe"});
    assert_eq!(
        assess(&receipt).expect("complete contaminated control")["q22_a_b_header_propagation_observed"],
        false
    );
}

#[test]
fn missing_execute_promise_body_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["execute_promise"]
        .as_object_mut()
        .expect("evaluation object")
        .remove("body");
    assert!(
        assess(&receipt).is_err(),
        "missing native result was accepted"
    );
}

#[test]
fn mixed_success_and_exception_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_string"]["body"]["exceptionDetails"] = json!({});
    assert!(
        assess(&receipt).is_err(),
        "contradictory success was accepted"
    );
}

#[test]
fn wrong_success_result_type_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_promise"]["body"]["result"]["type"] = json!("object");
    assert!(
        assess(&receipt).is_err(),
        "wrong native result type was accepted"
    );
}

#[test]
fn stack_only_error_marker_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_throw"]["body"]["exceptionDetails"]["exception"]["description"] =
        json!("TypeError: unrelated\n    at throw-probe (unrelated.js:1:1)");
    assert!(
        assess(&receipt).is_err(),
        "stack text impersonated the error value"
    );
}

#[test]
fn copied_cdp_completion_cannot_pass() {
    for (target, source) in [
        ("cdp_promise", "cdp_string"),
        ("cdp_string", "cdp_promise"),
        ("cdp_recovery", "cdp_promise"),
    ] {
        let mut receipt = sample();
        receipt["evaluations"][target] = receipt["evaluations"][source].clone();
        assert!(assess(&receipt).is_err(), "{source} impersonated {target}");
    }
}

#[test]
fn missing_rejected_promise_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]
        .as_object_mut()
        .expect("evaluations")
        .remove("cdp_reject");
    assert!(
        assess(&receipt).is_err(),
        "unobserved rejection was accepted"
    );
}

#[test]
fn throw_substituted_for_rejection_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_reject"] = receipt["evaluations"]["cdp_throw"].clone();
    assert!(
        assess(&receipt).is_err(),
        "synchronous throw replaced Promise rejection"
    );
}

#[test]
fn missing_post_error_recovery_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]
        .as_object_mut()
        .expect("evaluations")
        .remove("cdp_recovery");
    assert!(
        assess(&receipt).is_err(),
        "unobserved recovery was accepted"
    );
}

#[test]
fn unawaited_cdp_promise_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_promise"]["body"]["result"] =
        json!({"type":"object","subtype":"promise","objectId":"unsettled"});
    assert!(
        assess(&receipt).is_err(),
        "Promise handle replaced its settled value"
    );
}

#[test]
fn error_result_disagrees_with_exception_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_reject"]["body"]["result"]["description"] =
        json!("RangeError: unrelated");
    assert!(
        assess(&receipt).is_err(),
        "inconsistent error result was accepted"
    );
}

#[test]
fn previous_receipt_schema_cannot_qualify_extended_matrix() {
    let mut receipt = sample();
    receipt["schema"] = json!("keld.w3-q22-native-observation/v1");
    assert_eq!(
        assess(&receipt).expect_err("old receipt must be requalified"),
        "wrong or missing native receipt schema"
    );
}

#[test]
fn rejected_promise_serialized_error_is_valid() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_reject"]["body"]["result"] = json!({"type":"object","value":{}});
    assert!(
        assess(&receipt).is_ok(),
        "valid by-value Error representation was refused"
    );
}

#[test]
fn serialized_rejection_without_exception_cannot_pass() {
    let mut receipt = sample();
    let body = &mut receipt["evaluations"]["cdp_reject"]["body"];
    body["result"] = json!({"type":"object","value":{}});
    body.as_object_mut()
        .expect("body")
        .remove("exceptionDetails");
    assert!(
        assess(&receipt).is_err(),
        "serialized value replaced error evidence"
    );
}

#[test]
fn serialized_rejection_with_foreign_error_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_reject"]["body"]["result"] = json!({"type":"object","value":{}});
    receipt["evaluations"]["cdp_reject"]["body"]["exceptionDetails"]["exception"]["description"] =
        json!("RangeError: foreign");
    assert!(
        assess(&receipt).is_err(),
        "another rejection impersonated this expression"
    );
}

#[test]
fn serialized_result_is_not_the_observed_synchronous_throw() {
    let mut receipt = sample();
    receipt["evaluations"]["cdp_throw"]["body"]["result"] = json!({"type":"object","value":{}});
    assert!(
        assess(&receipt).is_err(),
        "Promise representation replaced the throw control"
    );
}

#[test]
fn null_execute_promise_body_cannot_pass() {
    let mut receipt = sample();
    receipt["evaluations"]["execute_promise"]["body"] = Value::Null;
    assert!(
        assess(&receipt).is_err(),
        "null replaced the observed Promise object"
    );
}

#[test]
fn missing_or_mismatched_native_iframe_id_cannot_pass() {
    let mut absent = sample();
    for event in absent["events"].as_array_mut().expect("events").iter_mut() {
        if event["phase"] == "iframe"
            && (event["kind"] == "frame-navigation-starting"
                || event["kind"] == "frame-navigation-completed")
        {
            event["data"]["navigation_id"] = Value::Null;
        }
    }
    assert!(assess(&absent).is_err(), "two missing iframe IDs matched");

    let mut mismatch = sample();
    let completion = mismatch["events"]
        .as_array_mut()
        .expect("events")
        .iter_mut()
        .find(|event| event["phase"] == "iframe" && event["kind"] == "frame-navigation-completed")
        .expect("iframe completion");
    completion["data"]["navigation_id"] = json!(999);
    assert!(assess(&mismatch).is_err(), "wrong iframe ID matched");
    assert!(assess(&sample()).is_ok(), "numeric same-ID control failed");
}

#[test]
fn non_document_iframe_resource_cannot_pass() {
    let mut receipt = sample();
    let resource = receipt["events"]
        .as_array_mut()
        .expect("events")
        .iter_mut()
        .find(|event| event["phase"] == "iframe" && event["kind"] == "resource-request")
        .expect("iframe resource");
    resource["data"]["resource_context"] = json!(6); // SCRIPT, not DOCUMENT (1)
    assert!(
        assess(&receipt).is_err(),
        "non-DOCUMENT resource impersonated iframe"
    );
    assert!(assess(&sample()).is_ok(), "DOCUMENT control failed");
}

#[test]
fn global_native_sequence_cannot_be_duplicated_or_reordered() {
    let mut duplicated = sample();
    let events = duplicated["events"].as_array_mut().expect("events");
    events[6]["seq"] = events[5]["seq"].clone();
    assert!(
        assess(&duplicated).is_err(),
        "duplicate global seq was accepted"
    );

    let mut swapped = sample();
    swapped["events"].as_array_mut().expect("events").swap(5, 6);
    assert!(
        assess(&swapped).is_err(),
        "reordered global events were accepted"
    );
    assert!(assess(&sample()).is_ok(), "ordered global sequence failed");
}
