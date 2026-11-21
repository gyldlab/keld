//! D-4 private v2 wire tests. Bytes below are independent literal spec vectors.
//! A successful test does not verify Ed25519, `WinTrust`, or commissioned signing.

use super::{DOMAIN, ExpectedAppIdentityV2, V2IdentityFields};
use crate::{ExpectedAppIdentityPayload, PackError};

const APP: &str = "com.example.app";
const OID: &str = "1.2.840.113556.1.8000.2554";
const LITERAL_PREFIX: &[u8] = b"keld.expected-app-identity/v2\0\
    \x0fcom.example.app\
    \x06stable\
    \x0bwindows-x64";

fn fields<'a>(
    app_id: &'a str,
    channel: &'a str,
    target: &'a str,
    profile_oid: &'a str,
) -> V2IdentityFields<'a> {
    V2IdentityFields {
        app_id,
        channel,
        target,
        update_public_key: [0x11; 32],
        profile_oid,
        issuer_policy_sha256: [0x22; 32],
        repository_id: [0x33; 32],
        repository_policy_sha256: [0x44; 32],
        discovery_policy_sha256: [0x55; 32],
    }
}

fn golden() -> Vec<u8> {
    let mut out = LITERAL_PREFIX.to_vec();
    out.extend_from_slice(&[0x11; 32]); // Raw key observation, not a key-validation pass.
    out.push(0x01); // Microsoft Artifact Signing Public Trust provider.
    out.extend_from_slice(b"\x00\x1a");
    out.extend_from_slice(OID.as_bytes());
    out.extend_from_slice(&[0x22; 32]); // issuer policy SHA-256
    out.extend_from_slice(&[0x33; 32]); // commissioned repository ID
    out.extend_from_slice(&[0x44; 32]); // repository policy SHA-256
    out.extend_from_slice(&[0x55; 32]); // discovery policy SHA-256
    out.extend_from_slice(&[1, 1, 1, 2]); // direct, PerUserDirect, strict, root-v2
    assert_eq!(out.len(), 258, "independent D-4 fixed vector");
    out
}

fn detail(result: Result<ExpectedAppIdentityV2, PackError>) -> &'static str {
    match result {
        Err(PackError::ExpectedIdentityInvalid { detail }) => detail,
        other => panic!("expected KELD-PACK-005, got {other:?}"),
    }
}

#[test]
fn literal_d4_golden_and_roundtrip_are_byte_exact() {
    assert_eq!(DOMAIN.len(), 30);
    let expected = golden();
    let typed = ExpectedAppIdentityV2::new(fields(APP, "stable", "windows-x64", OID))
        .expect("exact canonical input must be accepted as bytes");
    assert_eq!(typed.encode().expect("encode"), expected);
    assert_eq!(
        ExpectedAppIdentityV2::decode(&expected).expect("decode"),
        typed
    );
}

#[test]
fn every_prefix_truncation_and_any_trailing_byte_refuse() {
    let encoded = golden();
    for len in 0..encoded.len() {
        assert!(
            ExpectedAppIdentityV2::decode(&encoded[..len]).is_err(),
            "truncation at {len}"
        );
    }
    let mut surplus = encoded;
    surplus.push(0);
    assert_eq!(
        detail(ExpectedAppIdentityV2::decode(&surplus)),
        "v2 trailing bytes"
    );
}

#[test]
fn v1_and_v2_domains_never_share_an_accepting_decoder() {
    let old = ExpectedAppIdentityPayload::new(APP, "stable", "windows-x64", [0x11; 32])
        .expect("v1 byte grammar");
    assert!(ExpectedAppIdentityV2::decode(&old.encode()).is_err());
    assert!(ExpectedAppIdentityPayload::decode(&golden()).is_err());
    for index in 0..DOMAIN.len() {
        let mut mutated = golden();
        mutated[index] ^= 1;
        assert_eq!(detail(ExpectedAppIdentityV2::decode(&mutated)), "v2 domain");
    }
}

#[test]
fn channel_and_target_are_closed_not_a_generic_v1_text_field() {
    for channel in ["stable", "beta", "canary"] {
        let expected = ExpectedAppIdentityV2::new(fields(APP, channel, "windows-x64", OID))
            .expect("D-WIRE's three channels");
        assert_eq!(
            ExpectedAppIdentityV2::decode(&expected.encode().expect("encode")).expect("decode"),
            expected
        );
    }
    for invalid_channel in ["", "STABLE", "stAble", "nightly", "prod", "stable\n"] {
        assert_eq!(
            detail(ExpectedAppIdentityV2::new(fields(
                APP,
                invalid_channel,
                "windows-x64",
                OID
            ))),
            "v2 channel",
            "{invalid_channel:?}"
        );
    }
    for invalid_target in ["", "Windows-x64", "linux-x64", "windows-arm64"] {
        assert_eq!(
            detail(ExpectedAppIdentityV2::new(fields(
                APP,
                "stable",
                invalid_target,
                OID
            ))),
            "v2 target"
        );
    }
    let mut changed = golden();
    let channel_at = DOMAIN.len() + 1 + APP.len() + 1;
    changed[channel_at + 2] = b'A';
    assert_eq!(
        detail(ExpectedAppIdentityV2::decode(&changed)),
        "v2 channel"
    );
}

#[test]
fn app_bytes_are_bounded_ascii_without_duplicate_kel135_policy() {
    for invalid_app in ["", "café", "app\u{00e9}"] {
        assert_eq!(
            detail(ExpectedAppIdentityV2::new(fields(
                invalid_app,
                "stable",
                "windows-x64",
                OID
            ))),
            "v2 app id",
            "{invalid_app:?}"
        );
    }
    assert!(
        ExpectedAppIdentityV2::new(fields(&"a".repeat(255), "stable", "windows-x64", OID)).is_ok()
    );
    assert_eq!(
        detail(ExpectedAppIdentityV2::new(fields(
            &"a".repeat(256),
            "stable",
            "windows-x64",
            OID
        ))),
        "v2 app id"
    );
    // These are byte-form admissible but NOT canonical KEL-135 app identities.
    // The single KEL-135 owner must refuse them before any authority is minted.
    for syntactically_noncanonical in ["Com.example", "a..b", "app-"] {
        assert!(
            ExpectedAppIdentityV2::new(fields(
                syntactically_noncanonical,
                "stable",
                "windows-x64",
                OID
            ))
            .is_ok()
        );
    }
}

#[test]
fn profile_oid_rejects_noncanonical_arcs_and_generic_ekus() {
    for oid in [
        "",
        "1",
        "1.",
        ".1",
        "1..2",
        "3.1",
        "1.40",
        "1.03",
        "2.00",
        "-1.2",
        "+1.2",
        "1.2.",
        "1.2.4294967296",
        "1.2.a",
        "1.2.ü",
        "1.3.6.1.5.5.7.3.3",
        "1.3.6.1.4.1.311.97.1.0",
    ] {
        assert_eq!(
            detail(ExpectedAppIdentityV2::new(fields(
                APP,
                "stable",
                "windows-x64",
                oid
            ))),
            "v2 profile OID",
            "{oid:?}"
        );
    }
    let many = (0..64)
        .map(|index| if index == 1 { "2" } else { "1" })
        .collect::<Vec<_>>()
        .join(".");
    assert!(ExpectedAppIdentityV2::new(fields(APP, "stable", "windows-x64", &many)).is_ok());
    let too_many = format!("{many}.1");
    assert_eq!(
        detail(ExpectedAppIdentityV2::new(fields(
            APP,
            "stable",
            "windows-x64",
            &too_many
        ))),
        "v2 profile OID"
    );
}

#[test]
fn fixed_provider_owner_installation_principal_and_root_values_refuse_mutations() {
    let checks = [
        (97, "v2 provider"),
        (254, "v2 owner"),
        (255, "v2 installation mode"),
        (256, "v2 principal model"),
        (257, "v2 root policy"),
    ];
    for (offset, name) in checks {
        let mut altered = golden();
        altered[offset] ^= 1;
        assert_eq!(
            detail(ExpectedAppIdentityV2::decode(&altered)),
            name,
            "offset {offset}"
        );
    }
}

#[test]
fn four_independent_digests_and_repository_are_not_conflated() {
    let original = ExpectedAppIdentityV2::decode(&golden()).expect("golden");
    for (offset, expected) in [
        (126, "issuer"),
        (158, "repository"),
        (190, "repository policy"),
        (222, "discovery"),
    ] {
        let mut altered = golden();
        altered[offset] ^= 1;
        let changed = ExpectedAppIdentityV2::decode(&altered)
            .unwrap_or_else(|error| panic!("{expected} test bytes must remain canonical: {error}"));
        assert_ne!(
            changed, original,
            "{expected} must remain independently bound"
        );
        assert_eq!(changed.encode().expect("re-encode"), altered, "{expected}");
    }
}

#[test]
fn nonzero_padding_or_extra_bytes_are_never_part_of_the_payload() {
    let mut bytes = golden();
    bytes.push(0xff);
    assert!(ExpectedAppIdentityV2::decode(&bytes).is_err());
    let mut bytes = golden();
    bytes[98] = 0;
    bytes[99] = 0;
    assert_eq!(
        detail(ExpectedAppIdentityV2::decode(&bytes)),
        "v2 profile OID"
    );
}
