use std::io::Cursor;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use ed25519_dalek::{Signer, SigningKey};
use keld_guard::ProfileDigest;

use super::*;

const APP_ID: &str = "dev.keld.fixture";
const TARGET: &str = "windows-x64";
const ZERO_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7_u8; 32])
}

fn expected_identity() -> DirectInstallationIdentity {
    let public_key = signing_key().verifying_key().to_bytes();
    DirectInstallationIdentity {
        app_id: APP_ID.to_owned(),
        channel: Channel::Stable,
        target: TARGET.to_owned(),
        install_root: PathBuf::from(r"C:\Program Files\KeldFixture"),
        update_root: PathBuf::from(r"C:\ProgramData\KeldFixture\updates"),
        signing_key_id: SigningKeyId::from_public_key(&public_key),
        baseline: ArtifactIdentity {
            app_id: APP_ID.to_owned(),
            channel: Channel::Stable,
            target: TARGET.to_owned(),
            version: "1.0.0".to_owned(),
            content_blake3: [1_u8; 32],
        },
        profile_digest: ProfileDigest([2_u8; 32]),
        principal_model: PrincipalModel::StrictDistinctOsPrincipals,
    }
}

fn verifier() -> UpdateVerifier {
    UpdateVerifier::new(
        expected_identity(),
        signing_key().verifying_key().to_bytes(),
    )
    .expect("fixture verifier")
}

fn observation(
    identity: DirectInstallationIdentity,
    owner: InstallOwner,
    floor: Option<&str>,
) -> ProvenanceObservation {
    ProvenanceObservation::Protected {
        record: InstallProvenance { identity, owner },
        version_floor: floor.map(str::to_owned),
    }
}

fn admitted_at(floor: &str) -> AdmittedInstallation {
    verifier()
        .admit(&observation(
            expected_identity(),
            InstallOwner::Direct,
            Some(floor),
        ))
        .expect("fixture provenance")
}

fn sign(bytes: &[u8]) -> Vec<u8> {
    let signature = signing_key().sign(bytes).to_bytes();
    let mut encoded = [0_u8; 88];
    let written = BASE64_STANDARD
        .encode_slice(signature, &mut encoded)
        .expect("88 bytes hold base64 for a 64-byte signature");
    encoded[..written].to_vec()
}

fn release_json(
    version: &str,
    compressed_size: &str,
    compressed_digest: &str,
    content_size: &str,
    content_digest: &str,
    deltas: &str,
) -> String {
    format!(
        r#"{{"version":"{version}","publishedAt":"2026-09-20T00:00:00Z","full":{{"url":"{version}/full.zst","size":{compressed_size},"blake3":"{compressed_digest}","contentSize":{content_size},"contentBlake3":"{content_digest}"}},"deltas":[{deltas}]}}"#
    )
}

fn manifest_json(releases: &str) -> Vec<u8> {
    format!(
        r#"{{"schema":1,"channel":"stable","target":"{TARGET}","app":{{"id":"{APP_ID}"}},"releases":[{releases}]}}"#
    )
    .into_bytes()
}

fn verify_manifest(bytes: &[u8], floor: &str) -> Result<ManifestDecision, UpdateError> {
    admitted_at(floor).verify_manifest(bytes, &sign(bytes))
}

fn digest_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn selected_for(
    compressed_size: u64,
    content_size: u64,
    compressed_digest: &str,
    content_digest: &str,
) -> SelectedFull {
    let release = release_json(
        "2.0.0",
        &compressed_size.to_string(),
        compressed_digest,
        &content_size.to_string(),
        content_digest,
        "",
    );
    let manifest = manifest_json(&release);
    match verify_manifest(&manifest, "1.0.0").expect("valid fixture manifest") {
        ManifestDecision::Update(selected) => *selected,
        ManifestDecision::NoUpdate => panic!("2.0.0 must be above the fixture floor"),
    }
}

fn valid_selected(compressed: &[u8], content: &[u8]) -> SelectedFull {
    selected_for(
        compressed.len() as u64,
        content.len() as u64,
        &digest_hex(compressed),
        &digest_hex(content),
    )
}

fn assert_code(error: &UpdateError, code: &str) {
    assert_eq!(error.code(), code);
    let rendered = error.to_string();
    assert!(rendered.starts_with(code), "{rendered}");
    assert!(rendered.len() > code.len() + 2, "{rendered}");
}

#[test]
fn provenance_refuses_missing_unprotected_and_managed_before_admission() {
    let verifier = verifier();
    let missing = verifier.admit(&ProvenanceObservation::Missing).unwrap_err();
    assert_code(&missing, "KELD-UPDATE-001");

    let unprotected = verifier
        .admit(&ProvenanceObservation::Unprotected {
            record: InstallProvenance {
                identity: expected_identity(),
                owner: InstallOwner::Direct,
            },
        })
        .unwrap_err();
    assert_code(&unprotected, "KELD-UPDATE-001");

    let managed = verifier
        .admit(&observation(
            expected_identity(),
            InstallOwner::Managed {
                mechanism: "msix-store".to_owned(),
            },
            Some("1.0.0"),
        ))
        .unwrap_err();
    assert_code(&managed, "KELD-UPDATE-002");
    assert!(managed.to_string().contains("msix-store"));
}

#[test]
fn provenance_requires_exact_identity_distinct_principals_and_floor() {
    let verifier = verifier();
    let mut mismatched = expected_identity();
    mismatched.update_root.push("other");
    let root_error = verifier
        .admit(&observation(
            mismatched,
            InstallOwner::Direct,
            Some("1.0.0"),
        ))
        .unwrap_err();
    assert_code(&root_error, "KELD-UPDATE-003");
    assert!(matches!(
        root_error,
        UpdateError::ProvenanceMismatch {
            field: ProvenanceField::UpdateRoot,
            ..
        }
    ));

    let mut legacy = expected_identity();
    legacy.principal_model = PrincipalModel::LegacySameUser;
    let legacy_error = verifier
        .admit(&observation(legacy, InstallOwner::Direct, Some("1.0.0")))
        .unwrap_err();
    assert_code(&legacy_error, "KELD-UPDATE-003");

    for floor in [None, Some("not-semver")] {
        let error = verifier
            .admit(&observation(
                expected_identity(),
                InstallOwner::Direct,
                floor,
            ))
            .unwrap_err();
        assert_code(&error, "KELD-UPDATE-007");
    }

    let below = verifier
        .admit(&observation(
            expected_identity(),
            InstallOwner::Direct,
            Some("0.9.9"),
        ))
        .unwrap_err();
    assert_code(&below, "KELD-UPDATE-003");
    assert_eq!(admitted_at("1.2.0").version_floor(), "1.2.0");
}

#[test]
fn signature_authentication_precedes_json_meaning() {
    let invalid_json = br#"{"schema":1,"schema":1"#;
    let wrong_signature = sign(b"different bytes");
    let auth_error = admitted_at("1.0.0")
        .verify_manifest(invalid_json, &wrong_signature)
        .unwrap_err();
    assert_code(&auth_error, "KELD-UPDATE-004");

    let parse_error = admitted_at("1.0.0")
        .verify_manifest(invalid_json, &sign(invalid_json))
        .unwrap_err();
    assert_code(&parse_error, "KELD-UPDATE-005");
}

#[test]
fn signature_file_is_one_canonical_base64_line() {
    let release = release_json("2.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, "");
    let manifest = manifest_json(&release);
    let mut with_lf = sign(&manifest);
    with_lf.push(b'\n');
    assert!(
        admitted_at("1.0.0")
            .verify_manifest(&manifest, &with_lf)
            .is_ok()
    );

    let mut with_crlf = sign(&manifest);
    with_crlf.extend_from_slice(b"\r\n");
    let error = admitted_at("1.0.0")
        .verify_manifest(&manifest, &with_crlf)
        .unwrap_err();
    assert_code(&error, "KELD-UPDATE-004");
}

#[test]
fn manifest_selects_highest_precedence_and_ignores_present_delta() {
    let delta = format!(
        r#"{{"fromVersion":"1.0.0","url":"2.0.0/from-1.0.0.delta.zst","size":1,"blake3":"{ZERO_DIGEST}"}}"#
    );
    let releases = [
        release_json("1.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, ""),
        release_json("2.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, &delta),
        release_json("1.5.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, ""),
    ]
    .join(",");
    let manifest = manifest_json(&releases);
    let decision = verify_manifest(&manifest, "1.0.0").expect("valid manifest");
    let ManifestDecision::Update(selected) = decision else {
        panic!("2.0.0 is eligible");
    };
    assert_eq!(selected.identity().version, "2.0.0");
    assert_eq!(selected.url(), "2.0.0/full.zst");
}

#[test]
fn empty_eligible_set_is_successful_no_update() {
    let releases = [
        release_json("1.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, ""),
        release_json("0.9.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, ""),
    ]
    .join(",");
    let manifest = manifest_json(&releases);
    assert_eq!(
        verify_manifest(&manifest, "1.0.0"),
        Ok(ManifestDecision::NoUpdate)
    );
}

#[test]
fn duplicate_members_fail_at_every_typed_object_depth() {
    let release = release_json("2.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, "");
    let valid = String::from_utf8(manifest_json(&release)).expect("utf8 fixture");
    let cases = [
        valid.replacen(r#""schema":1"#, r#""schema":1,"schema":1"#, 1),
        valid.replacen(
            &format!(r#""id":"{APP_ID}""#),
            &format!(r#""id":"{APP_ID}","id":"{APP_ID}""#),
            1,
        ),
        valid.replacen(r#""size":1"#, r#""size":1,"size":1"#, 1),
    ];
    for bytes in cases {
        let error = verify_manifest(bytes.as_bytes(), "1.0.0").unwrap_err();
        assert_code(&error, "KELD-UPDATE-005");
        assert!(error.to_string().contains("duplicate field"), "{error}");
    }
}

#[test]
fn equal_precedence_releases_and_duplicate_delta_bases_are_invalid() {
    let equal = [
        release_json("2.0.0+host", "1", ZERO_DIGEST, "1", ZERO_DIGEST, ""),
        release_json("2.0.0+vendor", "1", ZERO_DIGEST, "1", ZERO_DIGEST, ""),
    ]
    .join(",");
    let equal_error = verify_manifest(&manifest_json(&equal), "1.0.0").unwrap_err();
    assert_code(&equal_error, "KELD-UPDATE-005");
    assert!(equal_error.to_string().contains("equal SemVer precedence"));

    let delta =
        format!(r#"{{"fromVersion":"1.0.0","url":"delta.zst","size":1,"blake3":"{ZERO_DIGEST}"}}"#);
    let release = release_json(
        "2.0.0",
        "1",
        ZERO_DIGEST,
        "1",
        ZERO_DIGEST,
        &format!("{delta},{delta}"),
    );
    let delta_error = verify_manifest(&manifest_json(&release), "1.0.0").unwrap_err();
    assert_code(&delta_error, "KELD-UPDATE-005");
    assert!(delta_error.to_string().contains("duplicate delta"));
}

#[test]
fn canonical_size_boundaries_reject_non_integer_forms() {
    for invalid_size in ["0", "-1", "1.0", "1e0", "9007199254740992"] {
        let release = release_json("2.0.0", invalid_size, ZERO_DIGEST, "1", ZERO_DIGEST, "");
        let error = verify_manifest(&manifest_json(&release), "1.0.0").unwrap_err();
        assert_code(&error, "KELD-UPDATE-005");
    }
    let maximum = release_json(
        "2.0.0",
        "9007199254740991",
        ZERO_DIGEST,
        "9007199254740991",
        ZERO_DIGEST,
        "",
    );
    assert!(verify_manifest(&manifest_json(&maximum), "1.0.0").is_ok());
}

#[test]
fn every_release_is_shape_validated_before_floor_filtering() {
    let valid_release = release_json("2.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, "");
    let old_invalid_version = release_json("01.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, "");
    let bad_version_manifest = manifest_json(&format!("{old_invalid_version},{valid_release}"));
    let version_error = verify_manifest(&bad_version_manifest, "1.0.0").unwrap_err();
    assert_code(&version_error, "KELD-UPDATE-005");

    let bad_digest = release_json("2.0.0", "1", "xyz", "1", ZERO_DIGEST, "");
    let digest_error = verify_manifest(&manifest_json(&bad_digest), "1.0.0").unwrap_err();
    assert_code(&digest_error, "KELD-UPDATE-005");

    let bad_content_size = release_json("2.0.0", "1", ZERO_DIGEST, "1e0", ZERO_DIGEST, "");
    let size_error = verify_manifest(&manifest_json(&bad_content_size), "1.0.0").unwrap_err();
    assert_code(&size_error, "KELD-UPDATE-005");

    let unknown = String::from_utf8(manifest_json(&valid_release))
        .expect("utf8")
        .replacen(r#""schema":1"#, r#""schema":1,"future":true"#, 1);
    let unknown_error = verify_manifest(unknown.as_bytes(), "1.0.0").unwrap_err();
    assert_code(&unknown_error, "KELD-UPDATE-005");

    let missing_full = format!(
        r#"{{"schema":1,"channel":"stable","target":"{TARGET}","app":{{"id":"{APP_ID}"}},"releases":[{{"version":"2.0.0","publishedAt":"2026-09-20T00:00:00Z"}}]}}"#
    );
    let missing_error = verify_manifest(missing_full.as_bytes(), "1.0.0").unwrap_err();
    assert_code(&missing_error, "KELD-UPDATE-005");
}

#[test]
fn signed_identity_mismatch_is_not_a_schema_or_signature_error() {
    let release = release_json("2.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, "");
    let manifest = String::from_utf8(manifest_json(&release))
        .expect("utf8")
        .replace(TARGET, "linux-x64")
        .into_bytes();
    let error = verify_manifest(&manifest, "1.0.0").unwrap_err();
    assert_code(&error, "KELD-UPDATE-006");
    assert!(matches!(
        error,
        UpdateError::ManifestIdentityMismatch {
            field: ManifestIdentityField::Target,
            ..
        }
    ));
}

#[test]
fn full_verifier_streams_valid_bytes_and_returns_bound_identity() {
    let content = b"canonical tar fixture bytes";
    let compressed = zstd::stream::encode_all(Cursor::new(content), 3).expect("compress");
    let selected = valid_selected(&compressed, content);
    let mut output = Vec::new();
    let receipt = selected
        .verify_full(&mut Cursor::new(&compressed), &mut output)
        .expect("verified full");
    assert_eq!(output, content);
    assert_eq!(receipt.identity().version, "2.0.0");
    assert_eq!(receipt.content_size(), content.len() as u64);
    assert_eq!(receipt.content_blake3(), blake3::hash(content).as_bytes());
}

#[test]
fn full_verifier_rejects_short_and_long_compressed_streams() {
    let content = b"canonical tar fixture bytes";
    let compressed = zstd::stream::encode_all(Cursor::new(content), 3).expect("compress");
    let selected = valid_selected(&compressed, content);

    let mut short = compressed.clone();
    short.pop();
    let short_error = selected
        .verify_full(&mut Cursor::new(short), &mut Vec::new())
        .unwrap_err();
    assert_code(&short_error, "KELD-UPDATE-008");

    let mut long = compressed.clone();
    long.push(0);
    let long_error = selected
        .verify_full(&mut Cursor::new(long), &mut Vec::new())
        .unwrap_err();
    assert_code(&long_error, "KELD-UPDATE-008");
}

#[test]
fn transport_digest_is_checked_before_decompression() {
    let content = b"canonical tar fixture bytes";
    let mut compressed = zstd::stream::encode_all(Cursor::new(content), 3).expect("compress");
    let selected = valid_selected(&compressed, content);
    compressed[0] ^= 1;
    let error = selected
        .verify_full(&mut Cursor::new(compressed), &mut Vec::new())
        .unwrap_err();
    assert_code(&error, "KELD-UPDATE-009");
    assert!(matches!(
        error,
        UpdateError::ArtifactDigestMismatch {
            domain: ArtifactDomain::Compressed,
            ..
        }
    ));
}

#[test]
fn content_size_and_digest_are_independent_postconditions() {
    let content = b"canonical tar fixture bytes";
    let compressed = zstd::stream::encode_all(Cursor::new(content), 3).expect("compress");
    for declared in [content.len() as u64 - 1, content.len() as u64 + 1] {
        let selected = selected_for(
            compressed.len() as u64,
            declared,
            &digest_hex(&compressed),
            &digest_hex(content),
        );
        let error = selected
            .verify_full(&mut Cursor::new(&compressed), &mut Vec::new())
            .unwrap_err();
        assert_code(&error, "KELD-UPDATE-008");
        assert!(matches!(
            error,
            UpdateError::ArtifactSizeMismatch {
                domain: ArtifactDomain::Content,
                ..
            }
        ));
    }

    let selected = selected_for(
        compressed.len() as u64,
        content.len() as u64,
        &digest_hex(&compressed),
        ZERO_DIGEST,
    );
    let error = selected
        .verify_full(&mut Cursor::new(&compressed), &mut Vec::new())
        .unwrap_err();
    assert_code(&error, "KELD-UPDATE-009");
    assert!(matches!(
        error,
        UpdateError::ArtifactDigestMismatch {
            domain: ArtifactDomain::Content,
            ..
        }
    ));
}

#[test]
fn corrupt_zstd_with_matching_transport_digest_is_processing_failure() {
    let compressed = b"not a zstd frame";
    let content = b"unused";
    let selected = selected_for(
        compressed.len() as u64,
        content.len() as u64,
        &digest_hex(compressed),
        &digest_hex(content),
    );
    let error = selected
        .verify_full(&mut Cursor::new(compressed), &mut Vec::new())
        .unwrap_err();
    assert_code(&error, "KELD-UPDATE-010");
}
