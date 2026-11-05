//! Policy-admission atom: exact authoring failures, independent of native auth.
use keld_guard::parse_manifest;

#[test]
fn malformed_channels_reject_the_entire_policy_as_guard005() {
    for source in [
        r#"{"windows":null}"#,
        r#"{"windows":[]}"#,
        r#"{"windows":{"editor":null}}"#,
        r#"{"windows":{"editor":[]}}"#,
        r#"{"windows":{"editor":{"channels":null}}}"#,
        r#"{"windows":{"editor":{"channels":true}}}"#,
        r#"{"windows":{"editor":{"channels":"el:ok"}}}"#,
        r#"{"windows":{"editor":{"channels":["el:ok",0]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:ok","el:*"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:ok","el:\u002a"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:"]}}}"#,
        r#"{"windows":{"editor":{"channels":["rendererReq"]}}}"#,
        r#"{"windows":{"editor":{"channels":["EL:rendererReq"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:bad\u0000"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:bad\u001f"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:bad\u007f"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:bad\u0085"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:bad\u009f"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:\uD800"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:\uDC00"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:ok"],"channels":[]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:ok"],"\u0063hannels":[]}}}"#,
        r#"{"windows":{"editor":{"channels":[]},"\u0065ditor":{"channels":[]}}}"#,
    ] {
        assert_eq!(
            parse_manifest(source)
                .err()
                .as_ref()
                .map(keld_guard::ManifestError::code),
            Some("KELD-GUARD005"),
            "{source}"
        );
    }
}

#[test]
fn suffix_limit_counts_decoded_utf8_bytes_excluding_the_el_prefix()
-> Result<(), Box<dyn std::error::Error>> {
    for suffix in ["a".repeat(256), "é".repeat(128), "😀".repeat(64)] {
        assert_eq!(suffix.len(), 256);
        let source = format!(r#"{{"windows":{{"editor":{{"channels":["el:{suffix}"]}}}}}}"#);
        parse_manifest(&source)?;
        let source = format!(r#"{{"windows":{{"editor":{{"channels":["el:{suffix}a"]}}}}}}"#);
        assert_eq!(
            parse_manifest(&source)
                .err()
                .as_ref()
                .map(keld_guard::ManifestError::code),
            Some("KELD-GUARD005")
        );
    }
    Ok(())
}

#[test]
fn missing_channels_empty_channels_and_unrelated_metadata_stay_valid()
-> Result<(), Box<dyn std::error::Error>> {
    for source in [
        "{}",
        r#"{"profile":"legacy","windows":{"editor":{}}}"#,
        r#"{"windows":{"editor":{"channels":[]}}}"#,
        r#"{"metadata":true,"windows":{"editor":{"caption":"app","channels":["el:a","el:a","el: fs.read","el:bad%20name"]}}}"#,
        r#"{"windows":{"editor":{"channels":["el:\u0072endererReq","el:😀","el:e\u0301"]}}}"#,
    ] {
        parse_manifest(source)?;
    }
    Ok(())
}
