//! Anchor/current wire bytes, authority-free identity and strict refusal cases.

use super::{APP, SID, anchor, current, refusal};
use crate::records::metadata_store::{
    EnrollmentState, decode_metadata_anchor, decode_metadata_current, encode_metadata_anchor,
    encode_metadata_current,
};

#[test]
fn anchor_and_current_have_exact_spec_field_order_and_roundtrip_bytes() {
    let anchor = anchor();
    let anchor_bytes = encode_metadata_anchor(&anchor).expect("canonical anchor");
    let expected_anchor = format!(
        r#"{{"schema":"keld.metadata-anchor/v1","repositoryId":"{}","appId":"{APP}","ownerSid":"{SID}","enrollmentState":"pending","initialGenerationSha256":"{}"}}"#,
        "11".repeat(32),
        "22".repeat(32)
    );
    assert_eq!(anchor_bytes, expected_anchor.as_bytes());
    assert_eq!(
        decode_metadata_anchor(&anchor_bytes).expect("roundtrip anchor"),
        anchor
    );

    let mut established = anchor.clone();
    established.enrollment_state = EnrollmentState::Established;
    let established_bytes = encode_metadata_anchor(&established).expect("established");
    assert_eq!(
        String::from_utf8(established_bytes.clone()).expect("ASCII JSON"),
        expected_anchor.replace(
            r#""enrollmentState":"pending""#,
            r#""enrollmentState":"established""#
        )
    );
    assert_eq!(
        decode_metadata_anchor(&established_bytes).expect("established roundtrip"),
        established
    );

    let current = current();
    let current_bytes = encode_metadata_current(&current).expect("canonical current");
    let expected_current = format!(
        r#"{{"schema":"keld.metadata-current/v1","repositoryId":"{}","sequence":1,"generationSha256":"{}"}}"#,
        "11".repeat(32),
        "33".repeat(32)
    );
    assert_eq!(current_bytes, expected_current.as_bytes());
    assert_eq!(
        decode_metadata_current(&current_bytes).expect("current roundtrip"),
        current
    );
}

#[test]
fn anchor_rejects_noncanonical_app_sid_and_inconsistent_state_bytes() {
    let anchor = anchor();
    let canonical =
        String::from_utf8(encode_metadata_anchor(&anchor).expect("canonical")).expect("ASCII");
    let oversized = "a".repeat(256);
    for invalid in [
        "Other.App",
        "other/app",
        "org..test",
        "é",
        "-start",
        "app_",
        oversized.as_str(),
    ] {
        let mut changed = anchor.clone();
        changed.app_id = invalid.to_owned();
        assert!(
            encode_metadata_anchor(&changed).is_err(),
            "encoded {invalid:?}"
        );
        let changed_json = canonical.replace(APP, invalid);
        assert!(serde_json::from_str::<serde_json::Value>(&changed_json).is_ok());
        assert!(
            decode_metadata_anchor(changed_json.as_bytes()).is_err(),
            "decoded {invalid:?}"
        );
    }
    let mut changed = anchor;
    changed.owner_sid = "S-1-5-021-100-200-300-1001".to_owned();
    assert!(encode_metadata_anchor(&changed).is_err());
    let changed_json = canonical.replace(SID, &changed.owner_sid);
    assert!(decode_metadata_anchor(changed_json.as_bytes()).is_err());
    let unknown_state = canonical.replace(
        r#""enrollmentState":"pending""#,
        r#""enrollmentState":"unknown""#,
    );
    assert!(decode_metadata_anchor(unknown_state.as_bytes()).is_err());
}

#[test]
fn anchor_and_current_refuse_actual_reordered_fields_with_same_json_values() {
    let anchor =
        String::from_utf8(encode_metadata_anchor(&anchor()).expect("anchor")).expect("ASCII");
    let first = r#""schema":"keld.metadata-anchor/v1""#;
    let second = format!(r#""repositoryId":"{}""#, "11".repeat(32));
    let original = format!("{{{first},{second},");
    let swapped = format!("{{{second},{first},");
    assert!(anchor.starts_with(&original));
    let reordered = anchor.replacen(&original, &swapped, 1);
    assert_ne!(reordered, anchor);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reordered).expect("swapped anchor"),
        serde_json::from_str::<serde_json::Value>(&anchor).expect("original anchor")
    );
    assert_eq!(
        refusal(decode_metadata_anchor(reordered.as_bytes())),
        "record differs from its canonical typed encoding"
    );

    let current =
        String::from_utf8(encode_metadata_current(&current()).expect("current")).expect("ASCII");
    let first = r#""schema":"keld.metadata-current/v1""#;
    let second = format!(r#""repositoryId":"{}""#, "11".repeat(32));
    let original = format!("{{{first},{second},");
    let swapped = format!("{{{second},{first},");
    assert!(current.starts_with(&original));
    let reordered = current.replacen(&original, &swapped, 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reordered).expect("swapped current"),
        serde_json::from_str::<serde_json::Value>(&current).expect("original current")
    );
    assert_eq!(
        refusal(decode_metadata_current(reordered.as_bytes())),
        "record differs from its canonical typed encoding"
    );
}

#[test]
fn malformed_anchor_and_current_are_refused_at_their_independent_caps() {
    let anchor =
        String::from_utf8(encode_metadata_anchor(&anchor()).expect("anchor")).expect("ASCII");
    let current =
        String::from_utf8(encode_metadata_current(&current()).expect("current")).expect("ASCII");
    for invalid in [
        anchor.replacen(
            r#""schema":"keld.metadata-anchor/v1""#,
            r#""schema":"wrong/v1""#,
            1,
        ),
        anchor.replacen(
            r#""schema":"keld.metadata-anchor/v1""#,
            r#""schema":"keld.metadata-anchor/v1","schema":"keld.metadata-anchor/v1""#,
            1,
        ),
        anchor.replacen(r#""ownerSid":"#, r#""foreign":0,"ownerSid":"#, 1),
        anchor.replacen("\"repositoryId\":\"", "\"repositoryId\":\"AA", 1),
        format!(" {anchor}"),
        format!("{anchor}\n"),
    ] {
        assert_ne!(invalid, anchor);
        assert!(decode_metadata_anchor(invalid.as_bytes()).is_err());
    }
    let escaped_sid = anchor.replacen("\"ownerSid\":\"S", "\"ownerSid\":\"\\u0053", 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&escaped_sid).expect("escaped SID JSON"),
        serde_json::from_str::<serde_json::Value>(&anchor).expect("canonical SID JSON")
    );
    assert_eq!(
        refusal(decode_metadata_anchor(escaped_sid.as_bytes())),
        "record differs from its canonical typed encoding"
    );
    for invalid in [
        current.replacen(
            r#""schema":"keld.metadata-current/v1""#,
            r#""schema":"wrong/v1""#,
            1,
        ),
        current.replacen(r#""sequence":1"#, r#""sequence":0"#, 1),
        current.replacen(r#""sequence":1"#, r#""sequence":-1"#, 1),
        current.replacen(r#""sequence":1"#, r#""sequence":01"#, 1),
        current.replacen(r#""sequence":1"#, r#""sequence":1.0"#, 1),
        current.replacen("\"generationSha256\":\"", "\"generationSha256\":\"AA", 1),
        current.replacen(r#","sequence":1"#, "", 1),
        format!("{current}\n"),
    ] {
        assert_ne!(invalid, current);
        assert!(decode_metadata_current(invalid.as_bytes()).is_err());
    }
    assert!(decode_metadata_anchor(&vec![b' '; 2_049]).is_err());
    assert!(decode_metadata_current(&vec![b' '; 513]).is_err());
    let mut cut_anchor = anchor.into_bytes();
    cut_anchor.pop();
    assert!(decode_metadata_anchor(&cut_anchor).is_err());
    let mut cut_current = current.into_bytes();
    cut_current.pop();
    assert!(decode_metadata_current(&cut_current).is_err());
}

#[test]
fn missing_current_sequence_is_valid_json_but_not_a_valid_record() {
    let original =
        String::from_utf8(encode_metadata_current(&current()).expect("canonical current"))
            .expect("ASCII JSON");
    let missing = original.replacen(r#","sequence":1"#, "", 1);
    assert_ne!(missing, original);
    assert!(
        serde_json::from_str::<serde_json::Value>(&missing).is_ok(),
        "missing sequence must remain syntactically valid JSON"
    );
    assert!(decode_metadata_current(missing.as_bytes()).is_err());
}
