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

#[test]
fn callback_wakes_only_after_publishing_the_actual_page_result() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let woke = std::cell::Cell::new(false);
    super::publish_reply(&sender, Ok("actual page result".to_owned()), || {
        assert_eq!(
            receiver
                .try_recv()
                .expect("reply precedes wake")
                .expect("actual result"),
            "actual page result"
        );
        woke.set(true);
        Ok(())
    })
    .expect("callback delivery");
    assert!(
        woke.get(),
        "accepted page callback never woke its event loop"
    );
}

#[test]
fn callback_error_also_wakes_with_the_original_failure() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let woke = std::cell::Cell::new(false);
    let code = windows::Win32::Foundation::E_UNEXPECTED;
    super::publish_reply(&sender, Err(windows::core::Error::from(code)), || {
        assert_eq!(
            receiver
                .try_recv()
                .expect("failure precedes wake")
                .expect_err("script failed")
                .code(),
            code
        );
        woke.set(true);
        Ok(())
    })
    .expect("failure delivery");
    assert!(woke.get(), "failed page callback never woke its event loop");
}

#[test]
fn callback_cannot_report_success_when_its_live_receiver_cannot_be_woken() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let code = windows::Win32::Foundation::E_UNEXPECTED;
    let error = super::publish_reply(&sender, Ok("actual".to_owned()), || {
        Err(windows::core::Error::from(code))
    })
    .expect_err("a failed live-receiver wake must be returned");
    assert_eq!(error.code(), code);
    assert_eq!(
        receiver
            .try_recv()
            .expect("published reply")
            .expect("actual result"),
        "actual"
    );
}

#[test]
fn callback_after_receiver_retirement_never_posts_another_wake() {
    let (sender, receiver) = std::sync::mpsc::channel();
    drop(receiver);
    super::publish_reply(&sender, Ok("retired".to_owned()), || {
        panic!("retired page callback must not post a wake")
    })
    .expect("receiver retirement is complete");
}
