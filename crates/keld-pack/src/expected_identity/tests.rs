//! Contract tests for the canonical expected-app-identity payload (KEL-254 A3 §4, T2b).

use super::*;

// Independent literal oracle, written by hand from the layout and never produced by the
// encoder: domain tag, then 0x0f + "com.example.app", 0x06 + "stable",
// 0x0b + "windows-x64", then the 32 key bytes 0x00..=0x1f.
const GOLDEN: &[u8] = b"keld.expected-app-identity/v1\0\
\x0fcom.example.app\
\x06stable\
\x0bwindows-x64\
\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\
\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f";

fn golden_key() -> [u8; EXPECTED_APP_IDENTITY_KEY_BYTES] {
    let mut key = [0_u8; EXPECTED_APP_IDENTITY_KEY_BYTES];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::try_from(index).unwrap_or(u8::MAX);
    }
    key
}

fn detail(result: Result<ExpectedAppIdentityPayload, PackError>) -> &'static str {
    match result {
        Err(PackError::ExpectedIdentityInvalid { detail }) => detail,
        other => panic!("expected KELD-PACK-005, got {other:?}"),
    }
}

/// Builds payload bytes with arbitrary (possibly invalid) field bytes, bypassing the
/// encoder so decode-side bounds are exercised independently.
fn raw(app_id: &[u8], channel: &[u8], target: &[u8]) -> Vec<u8> {
    let mut out = EXPECTED_APP_IDENTITY_DOMAIN.to_vec();
    for field in [app_id, channel, target] {
        out.push(u8::try_from(field.len()).unwrap_or(u8::MAX));
        out.extend_from_slice(field);
    }
    out.extend_from_slice(&golden_key());
    out
}

#[test]
fn golden_decodes_to_exact_fields() {
    let payload = ExpectedAppIdentityPayload::decode(GOLDEN);
    let Ok(payload) = payload else {
        panic!("golden payload must decode: {payload:?}");
    };
    assert_eq!(payload.app_id(), "com.example.app");
    assert_eq!(payload.channel(), "stable");
    assert_eq!(payload.target(), "windows-x64");
    assert_eq!(payload.update_public_key(), &golden_key());
}

#[test]
fn encode_reproduces_golden_byte_for_byte() {
    let payload =
        ExpectedAppIdentityPayload::new("com.example.app", "stable", "windows-x64", golden_key());
    let Ok(payload) = payload else {
        panic!("golden fields must be accepted: {payload:?}");
    };
    assert_eq!(payload.encode(), GOLDEN);
}

#[test]
fn every_truncation_refuses() {
    for len in 0..GOLDEN.len() {
        assert!(
            ExpectedAppIdentityPayload::decode(&GOLDEN[..len]).is_err(),
            "truncation to {len} bytes was accepted"
        );
    }
}

#[test]
fn trailing_byte_refuses() {
    let mut bytes = GOLDEN.to_vec();
    bytes.push(0);
    assert_eq!(
        detail(ExpectedAppIdentityPayload::decode(&bytes)),
        "public key length or trailing bytes"
    );
}

#[test]
fn every_domain_byte_mutation_refuses() {
    for index in 0..EXPECTED_APP_IDENTITY_DOMAIN.len() {
        let mut bytes = GOLDEN.to_vec();
        bytes[index] ^= 0x01;
        assert_eq!(
            detail(ExpectedAppIdentityPayload::decode(&bytes)),
            "domain tag",
            "domain byte {index} mutation was accepted"
        );
    }
    let mut next_version = GOLDEN.to_vec();
    let digit = EXPECTED_APP_IDENTITY_DOMAIN.len() - 2;
    next_version[digit] = b'2';
    assert_eq!(
        detail(ExpectedAppIdentityPayload::decode(&next_version)),
        "domain tag"
    );
}

#[test]
fn field_upper_bounds_are_exact() {
    let at_max = raw(&[b'a'; 255], &[b'c'; 16], &[b't'; 64]);
    assert!(ExpectedAppIdentityPayload::decode(&at_max).is_ok());
    assert_eq!(
        detail(ExpectedAppIdentityPayload::decode(&raw(
            b"app",
            &[b'c'; 17],
            b"t"
        ))),
        "channel"
    );
    assert_eq!(
        detail(ExpectedAppIdentityPayload::decode(&raw(
            b"app",
            b"c",
            &[b't'; 65]
        ))),
        "target"
    );
    let long_app_id = "a".repeat(256);
    assert_eq!(
        detail(ExpectedAppIdentityPayload::new(
            &long_app_id,
            "stable",
            "windows-x64",
            golden_key()
        )),
        "app id"
    );
}

#[test]
fn empty_fields_refuse_on_encode_and_decode() {
    for (fields, name) in [
        (("", "stable", "windows-x64"), "app id"),
        (("app", "", "windows-x64"), "channel"),
        (("app", "stable", ""), "target"),
    ] {
        assert_eq!(
            detail(ExpectedAppIdentityPayload::new(
                fields.0,
                fields.1,
                fields.2,
                golden_key()
            )),
            name
        );
        assert_eq!(
            detail(ExpectedAppIdentityPayload::decode(&raw(
                fields.0.as_bytes(),
                fields.1.as_bytes(),
                fields.2.as_bytes()
            ))),
            name
        );
    }
}

#[test]
fn control_and_non_utf8_bytes_refuse() {
    for bad in ["app\n", "a\u{7f}p", "a\u{85}p", "\u{0}"] {
        assert_eq!(
            detail(ExpectedAppIdentityPayload::new(
                bad,
                "stable",
                "windows-x64",
                golden_key()
            )),
            "app id"
        );
        assert_eq!(
            detail(ExpectedAppIdentityPayload::decode(&raw(
                bad.as_bytes(),
                b"stable",
                b"windows-x64"
            ))),
            "app id"
        );
    }
    assert_eq!(
        detail(ExpectedAppIdentityPayload::decode(&raw(
            b"app",
            &[0xff, 0xfe],
            b"windows-x64"
        ))),
        "channel"
    );
}

#[test]
fn derived_length_bounds_match_the_spec_values_and_the_encoder() {
    // KEL-19 container spec §4 states the bounds as 68 and 400; the literals are the
    // independent oracle for the derivation.
    assert_eq!(MIN_PAYLOAD_BYTES, 68);
    assert_eq!(MAX_PAYLOAD_BYTES, 400);
    let shortest = ExpectedAppIdentityPayload::new("a", "c", "t", golden_key());
    let Ok(shortest) = shortest else {
        panic!("one-byte fields must be accepted: {shortest:?}");
    };
    assert_eq!(shortest.encode().len(), MIN_PAYLOAD_BYTES);
    let longest = ExpectedAppIdentityPayload::decode(&raw(&[b'a'; 255], &[b'c'; 16], &[b't'; 64]));
    let Ok(longest) = longest else {
        panic!("fields at their bounds must be accepted: {longest:?}");
    };
    assert_eq!(longest.encode().len(), MAX_PAYLOAD_BYTES);
}

#[test]
fn error_has_stable_code_and_fix_guidance() {
    let error = PackError::ExpectedIdentityInvalid {
        detail: "domain tag",
    };
    assert_eq!(error.code(), "KELD-PACK-005");
    let text = error.to_string();
    assert!(text.starts_with("KELD-PACK-005: "), "{text}");
    assert!(text.contains("(domain tag)"), "{text}");
    assert!(text.contains("never hand-edit"), "{text}");
    assert!(text.contains("packaging configuration"), "{text}");
    assert!(
        text.contains("rebuild the image (`keld-host.exe` or `keld-updater-helper.exe`)"),
        "{text}"
    );
    assert!(
        !text.replace("keld-host.exe", "").contains("host"),
        "{text}"
    );
}
