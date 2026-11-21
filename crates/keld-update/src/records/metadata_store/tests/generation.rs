//! Generation D-23 typed graph/phase checks; no signature or native custody proof.

use super::{generation, refresh, refusal, role};
use crate::records::metadata_store::{
    GenerationPhase, GenerationRecord, MetadataRoleReference, decode_metadata_generation,
    encode_metadata_generation,
};

fn role_mut(record: &mut GenerationRecord, slot: usize) -> &mut MetadataRoleReference {
    match slot {
        0 => &mut record.root,
        1 => record.timestamp.as_mut().expect("fixture timestamp"),
        2 => record.snapshot.as_mut().expect("fixture snapshot"),
        _ => record.targets.as_mut().expect("fixture targets"),
    }
}

#[test]
fn ready_generation_exact_field_order_nested_role_refs_and_roundtrip() {
    let record = generation();
    let bytes = encode_metadata_generation(&record).expect("ready generation");
    let expected = format!(
        concat!(
            r#"{{"schema":"keld.metadata-generation/v1","repositoryId":"{}","#,
            r#""sequence":1,"recoveryEpoch":0,"previousGenerationSha256":null,"policySha256":"{}","#,
            r#""utcFloorMillis":1700000000000,"phase":"ready","#,
            r#""root":{{"sha256":"{}","length":1024,"version":1}},"#,
            r#""timestamp":{{"sha256":"{}","length":512,"version":2}},"#,
            r#""snapshot":{{"sha256":"{}","length":4096,"version":3}},"#,
            r#""targets":{{"sha256":"{}","length":65536,"version":4}},"rootRefresh":null}}"#
        ),
        "11".repeat(32),
        "44".repeat(32),
        "55".repeat(32),
        "66".repeat(32),
        "77".repeat(32),
        "88".repeat(32)
    );
    assert_eq!(
        bytes,
        expected.as_bytes(),
        "wire format must have exact D23 order"
    );
    assert_eq!(
        decode_metadata_generation(&bytes).expect("generation"),
        record
    );
}

#[test]
fn root_refresh_and_roles_incomplete_states_have_explicit_canonical_nulls() {
    let mut record = generation();
    record.phase = GenerationPhase::RootRefresh;
    record.root_refresh = Some(refresh());
    record.timestamp = None;
    record.snapshot = None;
    record.targets = None;
    let bytes = encode_metadata_generation(&record).expect("root refresh");
    let text = String::from_utf8(bytes.clone()).expect("ASCII JSON");
    assert!(text.contains(r#""previousGenerationSha256":null"#));
    assert!(text.contains(r#""timestamp":null,"snapshot":null,"targets":null"#));
    let fingerprint = format!(
        r#""rootRefresh":{{"timestampKeysSha256":"{}","snapshotKeysSha256":"{}"}}"#,
        "91".repeat(32),
        "92".repeat(32)
    );
    assert!(text.ends_with(&format!("{fingerprint}}}")));
    assert_eq!(decode_metadata_generation(&bytes).expect("refresh"), record);

    record.phase = GenerationPhase::Roles;
    record.root_refresh = None;
    record.timestamp = Some(role(0x66, 512, 2));
    let bytes = encode_metadata_generation(&record).expect("incomplete roles");
    assert_eq!(decode_metadata_generation(&bytes).expect("roles"), record);
    let text = String::from_utf8(bytes).expect("UTF-8");
    assert!(text.ends_with(r#""rootRefresh":null}"#));
}

#[test]
fn phases_require_consistent_root_refresh_and_ready_role_set() {
    let mut record = generation();
    for drop_role in 0..3 {
        let mut missing = record.clone();
        match drop_role {
            0 => missing.timestamp = None,
            1 => missing.snapshot = None,
            _ => missing.targets = None,
        }
        assert!(encode_metadata_generation(&missing).is_err());
    }
    record.root_refresh = Some(refresh());
    assert!(
        encode_metadata_generation(&record).is_err(),
        "ready + refresh denied"
    );
    record.phase = GenerationPhase::Roles;
    assert!(
        encode_metadata_generation(&record).is_err(),
        "roles + refresh denied"
    );
    record.phase = GenerationPhase::RootRefresh;
    assert!(
        encode_metadata_generation(&record).is_ok(),
        "root refresh with fingerprints"
    );
    record.root_refresh = None;
    assert!(
        encode_metadata_generation(&record).is_err(),
        "root refresh without fingerprints denied"
    );
}

#[test]
fn role_reference_version_and_per_role_profile_limits_are_closed() {
    for (slot, max) in [
        (0_usize, 65_536_u64),
        (1_usize, 16_384),
        (2_usize, 262_144),
        (3_usize, 1_048_576),
    ] {
        let mut record = generation();
        role_mut(&mut record, slot).length = max;
        assert!(
            encode_metadata_generation(&record).is_ok(),
            "role {slot} at D21 maximum"
        );
        role_mut(&mut record, slot).length = max + 1;
        assert!(
            encode_metadata_generation(&record).is_err(),
            "role {slot} above D21 maximum"
        );
        role_mut(&mut record, slot).length = 0;
        assert!(
            encode_metadata_generation(&record).is_err(),
            "zero role length"
        );
        role_mut(&mut record, slot).length = 1;
        role_mut(&mut record, slot).version = 0;
        assert!(
            encode_metadata_generation(&record).is_err(),
            "zero role version"
        );
    }
}

#[test]
fn sequence_previous_digest_and_utc_floor_have_representable_boundaries() {
    let mut record = generation();
    record.sequence = 0;
    assert!(
        encode_metadata_generation(&record).is_err(),
        "generation zero"
    );
    record.sequence = 1;
    record.previous_generation_sha256 = Some([0x19; 32]);
    assert!(
        encode_metadata_generation(&record).is_err(),
        "genesis previous must be null"
    );
    record.sequence = 2;
    assert!(
        encode_metadata_generation(&record).is_ok(),
        "linked later generation"
    );
    record.previous_generation_sha256 = None;
    assert!(
        encode_metadata_generation(&record).is_err(),
        "later generation must name previous"
    );
    record.sequence = 1;
    record.utc_floor_millis = u64::MAX;
    assert!(
        encode_metadata_generation(&record).is_err(),
        "unrepresentable signed UTC milliseconds"
    );
    record.utc_floor_millis = i64::MAX as u64;
    assert!(
        encode_metadata_generation(&record).is_ok(),
        "64-bit nonnegative storage representation; OS clock must separately verify support"
    );
}

#[test]
fn generation_refuses_reordered_unknown_duplicate_and_noncanonical_raw_bytes() {
    let text = String::from_utf8(encode_metadata_generation(&generation()).expect("ready"))
        .expect("ASCII");
    let schema = r#""schema":"keld.metadata-generation/v1""#;
    let repo = format!(r#""repositoryId":"{}""#, "11".repeat(32));
    let prefix = format!("{{{schema},{repo},");
    assert!(text.starts_with(&prefix));
    let swapped = text.replacen(&prefix, &format!("{{{repo},{schema},"), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&swapped).expect("reordered JSON"),
        serde_json::from_str::<serde_json::Value>(&text).expect("original JSON")
    );
    assert_eq!(
        refusal(decode_metadata_generation(swapped.as_bytes())),
        "record differs from its canonical typed encoding"
    );

    let nested_first = format!(r#""sha256":"{}","length":1024"#, "55".repeat(32));
    let nested_swapped = format!(r#""length":1024,"sha256":"{}""#, "55".repeat(32));
    let nested = text.replacen(&nested_first, &nested_swapped, 1);
    assert_ne!(nested, text);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&nested).expect("nested swap"),
        serde_json::from_str::<serde_json::Value>(&text).expect("original")
    );
    assert_eq!(
        refusal(decode_metadata_generation(nested.as_bytes())),
        "record differs from its canonical typed encoding"
    );
    for invalid in [
        text.replacen(schema, r#""schema":"bad/v1""#, 1),
        text.replacen(schema, &format!("{schema},{}", r#""schema":"bad/v1""#), 1),
        text.replacen(
            r#""recoveryEpoch":0"#,
            r#""extra":true,"recoveryEpoch":0"#,
            1,
        ),
        text.replacen(r#""sequence":1"#, r#""sequence":0"#, 1),
        text.replacen(r#""sequence":1"#, r#""sequence":-1"#, 1),
        text.replacen(r#""sequence":1"#, r#""sequence":01"#, 1),
        text.replacen(r#""sequence":1"#, r#""sequence":1.0"#, 1),
        text.replacen(r#""version":1"#, r#""version":0"#, 1),
        text.replacen(r#""length":1024"#, r#""length":0"#, 1),
        text.replacen(r#""length":1024"#, r#""length":65537"#, 1),
        text.replacen(r#""rootRefresh":null"#, r#""rootRefresh":{}"#, 1),
        text.replacen("\"policySha256\":\"", "\"policySha256\":\"AA", 1),
        text.replacen(
            &format!(
                r#""root":{{"sha256":"{}","length":1024,"version":1}},"#,
                "55".repeat(32)
            ),
            "",
            1,
        ),
        format!(" {text}"),
        format!("{text}\n"),
    ] {
        if invalid != text {
            assert!(decode_metadata_generation(invalid.as_bytes()).is_err());
        }
    }
    assert!(decode_metadata_generation(&vec![b' '; 4_097]).is_err());
    let mut truncated = text.into_bytes();
    truncated.pop();
    assert!(decode_metadata_generation(&truncated).is_err());
}

#[test]
fn roles_phase_refuses_missing_role_prerequisites_and_accepts_ordered_prefixes() {
    let mut roles = generation();
    roles.phase = GenerationPhase::Roles;
    for (has_timestamp, has_snapshot, has_targets) in [
        (false, true, true),
        (false, false, true),
        (true, false, true),
    ] {
        let mut invalid = roles.clone();
        if !has_timestamp {
            invalid.timestamp = None;
        }
        if !has_snapshot {
            invalid.snapshot = None;
        }
        if !has_targets {
            invalid.targets = None;
        }
        assert_eq!(
            refusal(encode_metadata_generation(&invalid)),
            "D-23 roles phase requires ordered timestamp, snapshot and targets"
        );
    }
    for (has_timestamp, has_snapshot) in [(false, false), (true, false), (true, true)] {
        let mut valid = roles.clone();
        if !has_timestamp {
            valid.timestamp = None;
        }
        if !has_snapshot {
            valid.snapshot = None;
        }
        valid.targets = None;
        let bytes = encode_metadata_generation(&valid).expect("ordered role prefix");
        assert_eq!(
            decode_metadata_generation(&bytes).expect("ordered prefix readback"),
            valid
        );
    }

    let canonical =
        String::from_utf8(encode_metadata_generation(&roles).expect("all roles recorded"))
            .expect("ASCII JSON");
    let old = format!(
        r#""timestamp":{{"sha256":"{}","length":512,"version":2}}"#,
        "66".repeat(32)
    );
    let missing = canonical.replacen(&old, r#""timestamp":null"#, 1);
    assert_ne!(missing, canonical);
    assert!(serde_json::from_str::<serde_json::Value>(&missing).is_ok());
    assert_eq!(
        refusal(decode_metadata_generation(missing.as_bytes())),
        "D-23 roles phase requires ordered timestamp, snapshot and targets"
    );
}

#[test]
fn genesis_requires_zero_recovery_epoch_but_successor_may_advance() {
    let mut invalid = generation();
    invalid.recovery_epoch = 1;
    assert_eq!(
        refusal(encode_metadata_generation(&invalid)),
        "D-23 genesis recovery epoch must be zero"
    );
    let canonical = String::from_utf8(encode_metadata_generation(&generation()).expect("genesis"))
        .expect("ASCII JSON");
    let changed = canonical.replacen(r#""recoveryEpoch":0"#, r#""recoveryEpoch":1"#, 1);
    assert_ne!(changed, canonical);
    assert!(serde_json::from_str::<serde_json::Value>(&changed).is_ok());
    assert_eq!(
        refusal(decode_metadata_generation(changed.as_bytes())),
        "D-23 genesis recovery epoch must be zero"
    );
    let mut successor = generation();
    successor.sequence = 2;
    successor.previous_generation_sha256 = Some([0x19; 32]);
    successor.recovery_epoch = 1;
    let bytes = encode_metadata_generation(&successor).expect("successor");
    assert_eq!(
        decode_metadata_generation(&bytes).expect("successor readback"),
        successor
    );
}
