//! Root-enabled fixed metadata only; the parent authenticates native source/lease.
use super::{WvError, bridge_error};
use serde_json::{Value, json};

const SCRIPT: &str = include_str!("boot_diagnostic.js");
const PAGE_SCRIPT: &str = include_str!("boot_diagnostic_page.js");
const COMMON: &str = include_str!("boot_diagnostic_common.js");
const COMMON_SLOT: &str = "__KELD_BOOT_DIAGNOSTIC_COMMON__";

pub(super) fn app_script() -> String {
    SCRIPT.replace(COMMON_SLOT, COMMON)
}
pub(super) fn page_script() -> String {
    PAGE_SCRIPT.replace(COMMON_SLOT, COMMON)
}

pub(super) fn enabled() -> bool {
    std::env::var_os("KELD_TEST_BOOT_DIAGNOSTIC").is_some_and(|value| value == "1")
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, WvError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| bridge_error("boot diagnostic text missing"))
}
fn choice<'a>(value: &'a Value, key: &str, allowed: &[&str]) -> Result<&'a str, WvError> {
    let selected = text(value, key)?;
    if allowed.contains(&selected) {
        Ok(selected)
    } else {
        Err(bridge_error("boot diagnostic metadata choice refused"))
    }
}
fn number(value: &Value, key: &str) -> Result<u64, WvError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| u32::try_from(*value).is_ok())
        .ok_or_else(|| bridge_error("boot diagnostic bounded number missing"))
}
fn boolean(value: &Value, key: &str) -> Result<bool, WvError> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| bridge_error("boot diagnostic boolean missing"))
}
fn closed(value: &Value, keys: &[&str]) -> Result<(), WvError> {
    let object = value
        .as_object()
        .ok_or_else(|| bridge_error("boot diagnostic object missing"))?;
    if object.len() != keys.len() || object.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err(bridge_error("boot diagnostic metadata shape refused"));
    }
    Ok(())
}

fn records(value: &Value) -> Result<Vec<Value>, WvError> {
    let rows = value
        .as_array()
        .filter(|rows| rows.len() <= 8)
        .ok_or_else(|| bridge_error("boot diagnostic error row bound exceeded"))?;
    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        closed(
            row,
            &[
                "kind",
                "class",
                "resource",
                "line",
                "column",
                "directive",
                "blocked",
            ],
        )?;
        records.push(json!({
            "kind":choice(row,"kind",&["js-error","resource-error","rejection","csp"] )?,
            "class":choice(row,"class",&["TypeError","ReferenceError","SyntaxError","RangeError","Error","unknown"] )?,
            "resource":choice(row,"resource",&["bootstrap","app-bundle","electron-app","inert-control","other"] )?,
            "line":number(row,"line")?,"column":number(row,"column")?,
            "directive":choice(row,"directive",&["script","style","connect","other"] )?,
            "blocked":choice(row,"blocked",&["inline","eval","source","other"] )?,
        }));
    }
    Ok(records)
}

fn page_report(value: &Value) -> Result<Value, WvError> {
    closed(value, &["status", "records", "dropped"])?;
    let status = choice(value, "status", &["ok", "absent", "oversize", "malformed"])?;
    let rows = records(
        value
            .get("records")
            .ok_or_else(|| bridge_error("boot page records missing"))?,
    )?;
    let dropped = number(value, "dropped")?;
    if status != "ok" && (!rows.is_empty() || dropped != 0) {
        return Err(bridge_error("invalid boot page status carried metadata"));
    }
    Ok(
        json!({"trust":"untrusted-page-dom","native_authority":false,"status":status,"records":rows,"dropped":dropped}),
    )
}

pub(super) fn record(value: &Value, view: u32, navigation: u32) -> Result<Value, WvError> {
    closed(
        value,
        &[
            "v",
            "phase",
            "ua",
            "ua_truncated",
            "ready_state",
            "dom_loaded",
            "loaded",
            "records",
            "dropped",
            "page",
        ],
    )?;
    if value.get("v").and_then(Value::as_u64) != Some(1) {
        return Err(bridge_error("boot diagnostic version refused"));
    }
    let phase = choice(
        value,
        "phase",
        &[
            "bound",
            "dom",
            "load",
            "js-error",
            "resource-error",
            "rejection",
            "csp",
            "page",
        ],
    )?;
    let ua = text(value, "ua")?;
    if ua.chars().count() > 512 || ua.len() > 2048 {
        return Err(bridge_error("boot diagnostic user agent bound exceeded"));
    }
    let ready = choice(
        value,
        "ready_state",
        &["loading", "interactive", "complete"],
    )?;
    let records = records(
        value
            .get("records")
            .ok_or_else(|| bridge_error("boot diagnostic records missing"))?,
    )?;
    let page = page_report(
        value
            .get("page")
            .ok_or_else(|| bridge_error("boot page report missing"))?,
    )?;
    Ok(
        json!({"view":view,"navigation":navigation,"world":"keld.app-preload","native_source_current":true,"page_globals_observed":false,"page_error_visibility":"requires-inert-native-control","phase":phase,"user_agent":ua,"ua_truncated":boolean(value,"ua_truncated")?,"ready_state":ready,"dom_loaded":boolean(value,"dom_loaded")?,"loaded":boolean(value,"loaded")?,"records":records,"dropped":number(value,"dropped")?,"page_report":page}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        json!({"v":1,"phase":"load","ua":"actual-native-UA","ua_truncated":false,"ready_state":"complete","dom_loaded":true,"loaded":true,"records":[{"kind":"js-error","class":"TypeError","resource":"inert-control","line":7,"column":1,"directive":"other","blocked":"other"}],"dropped":0,"page":{"status":"absent","records":[],"dropped":0}})
    }
    #[test]
    fn bounded_boot_record_is_redacted_and_never_page_global_evidence() {
        let report = record(&fixture(), 7, 11).expect("bounded readonly metadata");
        assert_eq!(report["view"], 7);
        assert_eq!(report["navigation"], 11);
        assert_eq!(report["page_globals_observed"], false);
        assert_eq!(report["page_report"]["trust"], "untrusted-page-dom");
        assert_eq!(report["page_report"]["native_authority"], false);
        let mut untrusted = fixture();
        untrusted["page"] = json!({"status":"ok","records":[{"kind":"js-error","class":"TypeError","resource":"inert-control","line":1,"column":1,"directive":"other","blocked":"other"}],"dropped":0});
        let page = record(&untrusted, 7, 11).expect("bounded untrusted page metadata");
        assert_eq!(
            page["page_report"]["records"][0]["resource"],
            "inert-control"
        );
        untrusted["page"]["records"][0]["message"] = json!("must never be logged");
        assert!(record(&untrusted, 7, 11).is_err());
        assert!(
            report.get("document").is_none()
                && report.get("href").is_none()
                && report.get("message").is_none()
        );
        let mut malicious = fixture();
        malicious["records"][0]["message"] = json!("must never log");
        assert!(record(&malicious, 7, 11).is_err());
        let mut huge = fixture();
        huge["ua"] = json!("x".repeat(513));
        assert!(record(&huge, 7, 11).is_err());
        let mut overflow = fixture();
        overflow["records"] = json!(vec![fixture()["records"][0].clone(); 9]);
        assert!(record(&overflow, 7, 11).is_err());
        let mut payload = fixture();
        payload["records"][0]["resource"] = json!("https://private.invalid/secret");
        assert!(record(&payload, 7, 11).is_err());
    }
}
