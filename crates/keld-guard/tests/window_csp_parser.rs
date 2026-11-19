//! D1 syntax regressions through the existing shared manifest parser.
use keld_guard::{ManifestError, parse_manifest};

#[test]
fn malformed_or_unknown_window_web_policy_is_not_ignored() {
    for input in [
        r#"{"windows":null}"#,
        r#"{"windows":[]}"#,
        r#"{"windows":{"":{"web":{"csp":"original-editor-offline"}}}}"#,
        r#"{"windows":{"editor":null}}"#,
        r#"{"windows":{"editor":[]}}"#,
        r#"{"windows":{"editor":{"web":null}}}"#,
        r#"{"windows":{"editor":{"web":[]}}}"#,
        r#"{"windows":{"editor":{"web":{"csp":null}}}}"#,
        r#"{"windows":{"editor":{"web":{"csp":false}}}}"#,
        r#"{"windows":{"editor":{"web":{"csp":"default"}}}}"#,
        r#"{"windows":{"editor":{"web":{"csp":"unknown"}}}}"#,
        r#"{"windows":{"editor":{"web":{"csp":{"original-editor-offline":null}}}}}"#,
        r#"{"windows":{"editor":{"web":{"hid":true}}}}"#,
        r#"{"windows":{"editor":{"web":{"camera":true}}}}"#,
    ] {
        let error = parse_manifest(input).expect_err("reserved window web policy cannot disappear");
        assert!(
            matches!(error, ManifestError::Parse { .. }),
            "{input}: {error:?}"
        );
        assert_eq!(error.code(), "KELD-GUARD005", "{input}");
    }
}

#[test]
fn window_policy_keeps_existing_unique_key_and_profile_boundaries() {
    for input in [
        r#"{"windows":{"editor":{"web":{"csp":"original-editor-offline","csp":"original-editor-offline"}}}}"#,
        r#"{"windows":{"editor":{"profile":"legacy","web":{"csp":"original-editor-offline"}}}}"#,
    ] {
        assert!(parse_manifest(input).is_err(), "{input}");
    }
    assert!(parse_manifest(r#"{"profile":"legacy","windows":{"editor":{"channels":[],"web":{"csp":"original-editor-offline"}}},"unrelated":true}"#).is_ok());
    assert!(
        parse_manifest("{}").is_ok(),
        "old blank/all-deny launches remain valid"
    );
}
