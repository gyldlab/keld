//! `ExpectedAppIdentity` decode and field-exact match (KEL-254 A3 §4, task T2b).
//!
//! Payload bytes come only from keld-pack's canonical encoder, as the spec requires;
//! keld-pack's own golden vector pins the byte layout independently.

use super::*;

/// Identity-point encoding: decodes as an Ed25519 point but is weak.
const WEAK_PUBLIC_KEY: [u8; 32] = [
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn payload(app_id: &str, channel: &str, target: &str, key: [u8; 32]) -> Vec<u8> {
    keld_pack::ExpectedAppIdentityPayload::new(app_id, channel, target, key)
        .expect("fixture fields are within keld-pack bounds")
        .encode()
}

fn release_key() -> [u8; 32] {
    signing_key().verifying_key().to_bytes()
}

fn decoded() -> ExpectedAppIdentity {
    ExpectedAppIdentity::decode(&payload(APP_ID, "stable", TARGET, release_key()))
        .expect("canonical payload with a valid release key decodes")
}

fn invalid_detail(error: UpdateError) -> String {
    assert_code(&error, "KELD-UPDATE-017");
    match error {
        UpdateError::ExpectedIdentityInvalid { detail } => detail,
        other => panic!("expected KELD-UPDATE-017, got {other:?}"),
    }
}

#[test]
fn canonical_payload_decodes_to_exact_expectation() {
    let expected = decoded();
    assert_eq!(expected.app_id(), APP_ID);
    assert_eq!(expected.channel(), Channel::Stable);
    assert_eq!(expected.target(), TARGET);
    assert_eq!(
        expected.signing_key_id(),
        &SigningKeyId::from_public_key(&release_key())
    );
    for (spelling, channel) in [("beta", Channel::Beta), ("canary", Channel::Canary)] {
        let other = ExpectedAppIdentity::decode(&payload(APP_ID, spelling, TARGET, release_key()))
            .expect("every supported channel spelling decodes");
        assert_eq!(other.channel(), channel);
    }
}

#[test]
fn unsupported_channel_refuses() {
    for spelling in ["Stable", "nightly", "stable "] {
        let error = ExpectedAppIdentity::decode(&payload(APP_ID, spelling, TARGET, release_key()))
            .expect_err("only exact v0 channel spellings are expectations");
        assert_eq!(invalid_detail(error), "unsupported channel");
    }
}

#[test]
fn weak_release_key_refuses() {
    let error = ExpectedAppIdentity::decode(&payload(APP_ID, "stable", TARGET, WEAK_PUBLIC_KEY))
        .expect_err("a weak key cannot anchor update trust");
    assert_eq!(invalid_detail(error), "Ed25519 public key is weak");
}

#[test]
fn noncanonical_payload_refuses_with_keld_pack_detail() {
    let mut bytes = payload(APP_ID, "stable", TARGET, release_key());
    bytes.push(0);
    let error = ExpectedAppIdentity::decode(&bytes).expect_err("trailing byte is refused");
    assert_eq!(
        invalid_detail(error),
        "payload public key length or trailing bytes"
    );
    let error = ExpectedAppIdentity::decode(b"not a payload").expect_err("wrong domain");
    assert_eq!(invalid_detail(error), "payload domain tag");
}

#[test]
fn record_must_match_each_expected_field_exactly() {
    let expected = decoded();
    expected
        .require_matches(&expected_identity())
        .expect("fixture record carries the expected identity");

    let substitutions: [IdentitySubstitution; 4] = [
        (ProvenanceField::AppId, |record| {
            record.app_id = "dev.keld.other".to_owned();
        }),
        (ProvenanceField::Channel, |record| {
            record.channel = Channel::Beta;
        }),
        (ProvenanceField::Target, |record| {
            record.target = "windows-arm64".to_owned();
        }),
        (ProvenanceField::SigningKey, |record| {
            record.signing_key_id = SigningKeyId::from_public_key(&[8_u8; 32]);
        }),
    ];
    for (field, substitute) in substitutions {
        let mut record = expected_identity();
        substitute(&mut record);
        let error = expected
            .require_matches(&record)
            .expect_err("a substituted expected field must refuse");
        assert_code(&error, "KELD-UPDATE-003");
        assert!(
            matches!(&error, UpdateError::ProvenanceMismatch { field: found, .. } if *found == field),
            "{field:?} substitution reported {error:?}"
        );
    }
}

#[test]
fn record_fields_outside_the_expectation_do_not_match_here() {
    let expected = decoded();
    let mut record = expected_identity();
    record.install_root = PathBuf::from(r"C:\Elsewhere");
    record.update_root = PathBuf::from(r"C:\Elsewhere\updates");
    record.install_mode = DirectInstallMode::PerUserDirect;
    record.profile_digest = ProfileDigest([9_u8; 32]);
    expected
        .require_matches(&record)
        .expect("roots, mode and profile are anchored by the located file identity and OS profile");
}

#[test]
fn error_has_stable_code_and_rebuild_guidance() {
    let error = UpdateError::ExpectedIdentityInvalid {
        detail: "unsupported channel".to_owned(),
    };
    assert_eq!(error.code(), "KELD-UPDATE-017");
    let text = error.to_string();
    assert!(text.starts_with("KELD-UPDATE-017: "), "{text}");
    assert!(text.contains("(unsupported channel)"), "{text}");
    assert!(text.contains("Rebuild the host"), "{text}");
}
