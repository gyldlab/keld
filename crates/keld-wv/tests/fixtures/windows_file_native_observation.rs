//! Validates independently recorded native/page observations; it does not grant authority.

use serde_json::{Value, json};

pub(super) const SCHEMA: &str = "keld.w3-q22-native-observation/v1";

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn rows<'a>(receipt: &'a Value, phase: &str, kind: &str) -> Vec<&'a Value> {
    receipt["events"]
        .as_array()
        .map_or_else(Vec::new, |events| {
            events
                .iter()
                .filter(|event| event["phase"] == phase && event["kind"] == kind)
                .collect()
        })
}

fn one<'a>(receipt: &'a Value, phase: &str, kind: &str) -> Result<&'a Value, String> {
    let found = rows(receipt, phase, kind);
    if found.len() != 1 {
        return Err(format!(
            "{phase}: expected one {kind}, observed {}",
            found.len()
        ));
    }
    Ok(found[0])
}

fn successful_navigation(receipt: &Value, phase: &str) -> Result<(), String> {
    let start = one(receipt, phase, "navigation-starting")?;
    let content = one(receipt, phase, "content-loading")?;
    let dom = one(receipt, phase, "dom-content-loaded")?;
    let end = one(receipt, phase, "navigation-completed")?;
    require(
        start["data"]["uri"] == receipt["uri"],
        "native top-level URI differs from the owned file",
    )?;
    let id = start["data"]["navigation_id"]
        .as_u64()
        .ok_or("missing native navigation id")?;
    for event in [content, dom, end] {
        require(
            event["data"]["navigation_id"].as_u64() == Some(id),
            "mixed native navigation identities",
        )?;
    }
    let order = [start, content, dom, end].map(|event| event["seq"].as_u64().unwrap_or(0));
    require(
        order[0] > 0 && order.windows(2).all(|pair| pair[0] < pair[1]),
        "native commit/load events are missing or out of order",
    )?;
    require(
        content["data"]["is_error_page"] == false,
        "native content is an error page",
    )?;
    require(
        end["data"]["success"] == true,
        "native navigation did not succeed",
    )?;
    let resources: Vec<_> = rows(receipt, phase, "resource-request")
        .into_iter()
        .filter(|event| event["data"]["uri"] == receipt["uri"])
        .collect();
    require(
        resources.len() == 1,
        "healthy top-level load lacks its independent exact-URI resource observation",
    )?;
    require(
        resources[0]["data"]["resource_context"] == 1,
        "owned document was not reported as DOCUMENT",
    )?;
    check_page(receipt, phase)
}

fn check_page(receipt: &Value, phase: &str) -> Result<(), String> {
    let page = &receipt["pages"][phase];
    require(
        page["nonce"] == receipt["nonce"],
        "page fixture nonce is absent or wrong",
    )?;
    require(
        page["href"] == receipt["uri"],
        "page does not retain its exact file:// URL",
    )
}

fn header_is(event: &Value, value: Option<&str>) -> bool {
    let header = &event["data"]["header"];
    match value {
        Some(value) => header["state"] == "present" && header["value"] == value,
        None => header["state"] == "absent",
    }
}

fn checked_header_observations(receipt: &Value) -> Result<(), String> {
    for event in receipt["events"].as_array().ok_or("missing event array")? {
        require(
            event["data"].get("error").is_none(),
            "native callback/getter failed; absence is not evidence",
        )?;
        if [
            "navigation-starting",
            "frame-navigation-starting",
            "resource-request",
        ]
        .contains(&event["kind"].as_str().unwrap_or_default())
        {
            let state = event["data"]["header"]["state"].as_str();
            require(
                matches!(state, Some("absent" | "present")),
                "header getter was not observed successfully",
            )?;
        }
    }
    Ok(())
}

fn check_evaluations(receipt: &Value) -> Result<(), String> {
    let nonce = receipt["nonce"].as_str().ok_or("missing fixture nonce")?;
    let expected = format!("settled-{nonce}");
    let evals = &receipt["evaluations"];
    for name in [
        "execute_string",
        "execute_promise",
        "cdp_string",
        "cdp_promise",
        "cdp_throw",
    ] {
        require(
            evals[name]["hresult"] == "S_OK",
            "native evaluation callback failed or is missing",
        )?;
    }
    require(
        evals["execute_string"]["body"] == expected,
        "ExecuteScript string control did not execute",
    )?;
    require(
        evals["execute_promise"]["body"] != expected,
        "ExecuteScript Promise control did not distinguish settled delivery",
    )?;
    for name in ["cdp_string", "cdp_promise"] {
        require(
            evals[name]["body"]["result"]["value"] == expected,
            "actual CDP value did not settle to the fixture value",
        )?;
    }
    let error = evals["cdp_throw"]["body"]["exceptionDetails"]["exception"]["description"]
        .as_str()
        .ok_or("missing actual script exception")?;
    require(
        error.contains(&format!("throw-{nonce}")),
        "script exception belongs to another expression",
    )
}

fn check_same_uri_frame(receipt: &Value) -> Result<(), String> {
    let frame_start = one(receipt, "iframe", "frame-navigation-starting")?;
    let frame_end = one(receipt, "iframe", "frame-navigation-completed")?;
    require(
        frame_start["data"]["uri"] == receipt["uri"] && frame_end["data"]["success"] == true,
        "same-URI iframe lacks native start/success evidence",
    )?;
    require(
        frame_start["data"]["navigation_id"] == frame_end["data"]["navigation_id"],
        "iframe native identities differ",
    )?;
    require(
        rows(receipt, "iframe", "navigation-starting").is_empty(),
        "iframe cell unexpectedly navigated the top-level document",
    )?;
    check_page(receipt, "iframe")?;
    let frames = receipt["pages"]["iframe"]["frames"]
        .as_array()
        .ok_or("missing iframe page output")?;
    require(
        frames.len() == 1
            && frames[0]["nonce"] == receipt["nonce"]
            && frames[0]["href"] == receipt["uri"],
        "owned iframe did not independently report its file content",
    )?;
    let frame_resources = rows(receipt, "iframe", "resource-request");
    require(
        frame_resources
            .iter()
            .filter(|event| event["data"]["uri"] == receipt["uri"])
            .count()
            == 1,
        "iframe cell lacks one exact-URI resource observation",
    )?;
    Ok(())
}

fn check_cleanup(receipt: &Value) -> Result<(), String> {
    require(
        receipt["cleanup"]["fatal_process_exit_required"] == false,
        "native creation cleanup requires fatal helper exit",
    )?;
    let pid = receipt["browser_pid"]
        .as_u64()
        .ok_or("missing native browser PID")?;
    let exited = one(receipt, "cleanup", "browser-process-exited")?;
    require(
        pid > 0 && exited["data"]["pid"].as_u64() == Some(pid) && exited["data"]["normal"] == true,
        "normal exit of the owned browser was not observed",
    )?;
    require(
        receipt["cleanup"]["controller_closed"] == true
            && receipt["cleanup"]["window_destroyed"] == true
            && receipt["cleanup"]["profile_removed"] == true
            && receipt["cleanup"]["com_uninitialized"] == true,
        "owned native resources were not released normally",
    )?;
    require(
        receipt["cleanup"]["faults"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "native cleanup recorded a fault",
    )?;
    Ok(())
}

fn host_header_observations(receipt: &Value) -> Result<(bool, bool), String> {
    let request = &receipt["host_request"];
    require(
        request["uri"] == receipt["uri"] && request["sent_value"] == receipt["attempt_nonce"],
        "Host request did not target its fixture",
    )?;
    let created = request["create_hresult"]
        .as_str()
        .ok_or("no native CreateWebResourceRequest result")?;
    let nonce = receipt["attempt_nonce"]
        .as_str()
        .ok_or("missing native attempt marker")?;
    require(
        receipt["attempt_nonce"] != receipt["nonce"],
        "native attempt marker is exposed as the page fixture nonce",
    )?;
    let mut host_start_header = false;
    let mut host_resource_header = false;
    if created == "S_OK" {
        require(
            request["input_header"]["state"] == "present"
                && request["input_header"]["value"] == nonce,
            "request-object readback did not contain the sent nonce",
        )?;
        let navigation = request["navigate_hresult"]
            .as_str()
            .ok_or("no native NavigateWithWebResourceRequest result")?;
        if navigation == "S_OK" {
            let start = one(receipt, "host-header", "navigation-starting")?;
            let end = one(receipt, "host-header", "navigation-completed")?;
            require(
                start["data"]["uri"] == receipt["uri"]
                    && start["data"]["navigation_id"] == end["data"]["navigation_id"],
                "custom native request terminal is not attributed to its start",
            )?;
            if end["data"]["success"] == true {
                successful_navigation(receipt, "host-header")?;
                host_start_header = header_is(start, Some(nonce));
                let resources = rows(receipt, "host-header", "resource-request");
                host_resource_header = resources.iter().any(|event| {
                    event["data"]["uri"] == receipt["uri"] && header_is(event, Some(nonce))
                });
            }
        } else {
            require(
                navigation.starts_with("0x"),
                "native rejection is missing its HRESULT",
            )?;
        }
    } else {
        require(
            created.starts_with("0x"),
            "request creation rejection is missing its HRESULT",
        )?;
    }
    Ok((host_start_header, host_resource_header))
}

/// A complete experiment may refute Q22. This result never approves a carrier or product path.
pub(super) fn assess(receipt: &Value) -> Result<Value, String> {
    require(
        receipt["schema"] == SCHEMA,
        "wrong or missing native receipt schema",
    )?;
    require(
        receipt["helper"] == "windows_file_native_probe_child",
        "native helper did not execute",
    )?;
    require(
        receipt["errors"].as_array().is_some_and(Vec::is_empty),
        "native run recorded errors or timed out",
    )?;
    require(
        receipt["runtime_version"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "missing actual WebView2 runtime version",
    )?;
    require(
        receipt["devtools_enabled_readback"] == false,
        "DevTools-disabled configuration was not read back",
    )?;
    let pump = &receipt["message_pump_calibration"];
    require(
        pump["returned_on_readiness"] == true
            && pump["queued"] == 100
            && pump["remaining_at_readiness"]
                .as_u64()
                .is_some_and(|remaining| (1..=100).contains(&remaining))
            && pump["readiness_polls"]
                .as_u64()
                .is_some_and(|polls| polls >= 2),
        "native message-pump readiness calibration is missing or failed",
    )?;
    checked_header_observations(receipt)?;
    successful_navigation(receipt, "plain")?;
    successful_navigation(receipt, "page")?;
    check_same_uri_frame(receipt)?;
    check_evaluations(receipt)?;
    check_cleanup(receipt)?;
    let (host_start_header, host_resource_header) = host_header_observations(receipt)?;
    let controls_absent = ["plain", "page", "iframe"].iter().all(|phase| {
        [
            "navigation-starting",
            "frame-navigation-starting",
            "resource-request",
        ]
        .iter()
        .all(|kind| {
            rows(receipt, phase, kind)
                .iter()
                .all(|event| header_is(event, None))
        })
    });
    Ok(json!({
        "observation_complete": true,
        "q22_a_header_in_native_navigation_start": host_start_header,
        "q22_b_header_in_owned_document_resource_request": host_resource_header,
        "same_uri_page_frame_and_plain_controls_without_header": controls_absent,
        "q22_a_b_header_propagation_observed": host_start_header && host_resource_header && controls_absent,
        "scope": "raw native header feasibility only; no concurrent same-URI association race, reload semantics, file containment, facade delivery, identity retirement, B22 authority or product acceptance"
    }))
}

#[cfg(test)]
#[path = "windows_file_native_observation_tests.rs"]
mod tests;
