//! The fixed observation protocol preserves actual page values and rejects gaps.
use super::{compact_readback, parse_interval};
use std::time::Duration;

#[test]
fn page_interval_is_positive_whole_milliseconds() {
    assert_eq!(
        parse_interval("100").expect("interval"),
        Duration::from_millis(100)
    );
    assert_eq!(
        parse_interval("1").expect("interval"),
        Duration::from_millis(1)
    );
    for bad in [
        "",
        "0",
        "-1",
        "+1",
        " 100",
        "1.0",
        "ms",
        "18446744073709551616",
    ] {
        let error = parse_interval(bad).expect_err("invalid interval");
        assert!(
            error
                .to_string()
                .contains("KELD_TEST_WINDOWS_PAGE_READBACK"),
            "{error}"
        );
    }
}

#[test]
fn readback_keeps_actual_values_and_a_single_record_line() {
    let input = r#"{
        "screenX": -214, "screenY": 12.5, "innerWidth": 731,
        "visibilityState": "hidden", "hasFocus": false
    }"#;
    let result = compact_readback(input).expect("actual page observation");
    assert!(!result.contains('\n'));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result).expect("compact JSON"),
        serde_json::from_str::<serde_json::Value>(input).expect("input JSON"),
    );
}

#[test]
fn missing_or_wrongly_typed_page_values_cannot_be_printed_as_observations() {
    for bad in [
        "null",
        "true",
        r#""a string""#,
        "{}",
        r#"{"screenX":0,"screenY":0,"innerWidth":0,"visibilityState":"visible"}"#,
        r#"{"screenX":"0","screenY":0,"innerWidth":0,"visibilityState":"visible","hasFocus":true}"#,
        r#"{"screenX":0,"screenY":0,"innerWidth":0,"visibilityState":null,"hasFocus":true}"#,
    ] {
        assert!(compact_readback(bad).is_err(), "invalid observation: {bad}");
    }
}
