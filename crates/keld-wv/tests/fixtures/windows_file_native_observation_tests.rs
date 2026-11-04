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
            json!({"state":"present","value":"probe"})
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
    let page = json!({"nonce":"probe","href":"file:///D:/owned/doc.html","frames":[]});
    json!({
        "schema":SCHEMA,"helper":"windows_file_native_probe_child","errors":[],"runtime_version":"fixture-observer-calibration","devtools_enabled_readback":false,
        "nonce":"probe","uri":"file:///D:/owned/doc.html","browser_pid":1234,"events":events,
        "host_request":{"uri":"file:///D:/owned/doc.html","sent_value":"probe","create_hresult":"S_OK","navigate_hresult":"S_OK","input_header":{"state":"present","value":"probe"}},
        "pages":{"host-header":page,"plain":page,"page":page,"iframe":{"nonce":"probe","href":"file:///D:/owned/doc.html","frames":[{"nonce":"probe","href":"file:///D:/owned/doc.html"}]}},
        "evaluations":{
            "execute_string":{"hresult":"S_OK","body":"settled-probe"},"execute_promise":{"hresult":"S_OK","body":{}},
            "cdp_string":{"hresult":"S_OK","body":{"result":{"value":"settled-probe"}}},
            "cdp_promise":{"hresult":"S_OK","body":{"result":{"value":"settled-probe"}}},
            "cdp_throw":{"hresult":"S_OK","body":{"exceptionDetails":{"exception":{"description":"TypeError: throw-probe"}}}}
        },
        "cleanup":{"controller_closed":true,"window_destroyed":true,"profile_removed":true,"com_uninitialized":true,"faults":[]}
    })
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
        assess(&receipt).expect("complete calibration")["q22_prerequisites_observed"],
        true
    );
    receipt["events"][0]["data"]["header"] = json!({"state":"absent"});
    assert_eq!(
        assess(&receipt).expect("complete negative observation")["q22_prerequisites_observed"],
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
        assess(&receipt).expect("rejection plus healthy controls")["q22_prerequisites_observed"],
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
        assess(&receipt).expect("complete contaminated control")["q22_prerequisites_observed"],
        false
    );
}
