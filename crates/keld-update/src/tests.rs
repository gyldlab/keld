use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use ed25519_dalek::{Signer, SigningKey};
use keld_guard::ProfileDigest;

use super::*;

const APP_ID: &str = "dev.keld.fixture";
const TARGET: &str = "windows-x64";
const ZERO_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
type IdentitySubstitution = (ProvenanceField, fn(&mut DirectInstallationIdentity));

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

fn archive_receipt(content: &[u8]) -> VerifiedFull {
    let compressed = zstd::stream::encode_all(Cursor::new(content), 0).expect("fixture zstd");
    valid_selected(&compressed, content)
        .verify_full(&mut Cursor::new(compressed), &mut Vec::new())
        .expect("verified fixture archive")
}

fn append_ustar_entry(archive: &mut Vec<u8>, name: &str, kind: u8, data: &[u8]) {
    let mut header = [0_u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..108].copy_from_slice(if kind == b'5' {
        b"0000755\0"
    } else {
        b"0000644\0"
    });
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size = if kind == b'5' { 0 } else { data.len() as u64 };
    let size_field = format!("{size:011o}\0");
    header[124..136].copy_from_slice(size_field.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].fill(b' ');
    header[156] = kind;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[329..337].copy_from_slice(b"0000000\0");
    header[337..345].copy_from_slice(b"0000000\0");
    refresh_ustar_checksum(&mut header);
    archive.extend_from_slice(&header);
    archive.extend_from_slice(data);
    let padding = (512 - (data.len() % 512)) % 512;
    archive.resize(archive.len() + padding, 0);
}

fn finish_ustar(archive: &mut Vec<u8>) {
    archive.resize(archive.len() + 1024, 0);
}

fn append_required_policy(archive: &mut Vec<u8>) {
    // Literal wire oracle, deliberately independent of the producer's constants.
    append_ustar_entry(archive, ".keld", b'5', &[]);
    append_ustar_entry(
        archive,
        ".keld/update-policy.v1",
        b'0',
        b"{\"schema\":1,\"dataMigration\":\"none\"}\n",
    );
}

fn refresh_ustar_checksum(header: &mut [u8; 512]) {
    header[148..156].fill(b' ');
    let sum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    let checksum = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(checksum.as_bytes());
}

fn lexical_windows_package_names(paths: &[&str]) -> Result<(), String> {
    for path in paths {
        for component in path.split('/') {
            keld_guard::validate_windows_package_component(component)?;
        }
    }
    Ok(())
}

fn parse_test_archive(bytes: &[u8]) -> Result<ValidatedArchive, UpdateError> {
    let receipt = archive_receipt(bytes);
    crate::archive::parse_canonical_ustar(
        &receipt,
        &mut Cursor::new(bytes),
        lexical_windows_package_names,
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
    let substitutions: [IdentitySubstitution; 9] = [
        (ProvenanceField::AppId, |identity| {
            identity.app_id.push_str(".other");
        }),
        (ProvenanceField::Channel, |identity| {
            identity.channel = Channel::Beta;
        }),
        (ProvenanceField::Target, |identity| {
            identity.target = "linux-x64".to_owned();
        }),
        (ProvenanceField::InstallRoot, |identity| {
            identity.install_root.push("other");
        }),
        (ProvenanceField::UpdateRoot, |identity| {
            identity.update_root.push("other");
        }),
        (ProvenanceField::SigningKey, |identity| {
            identity.signing_key_id = SigningKeyId::from_public_key(&[8_u8; 32]);
        }),
        (ProvenanceField::Baseline, |identity| {
            identity.baseline.content_blake3[0] ^= 1;
        }),
        (ProvenanceField::Profile, |identity| {
            identity.profile_digest = ProfileDigest([3_u8; 32]);
        }),
        (ProvenanceField::PrincipalModel, |identity| {
            identity.principal_model = PrincipalModel::LegacySameUser;
        }),
    ];
    for (expected_field, substitute) in substitutions {
        let mut identity = expected_identity();
        substitute(&mut identity);
        let error = verifier
            .admit(&observation(identity, InstallOwner::Direct, Some("1.0.0")))
            .unwrap_err();
        assert_code(&error, "KELD-UPDATE-003");
        assert!(
            matches!(
                &error,
                UpdateError::ProvenanceMismatch { field, .. } if *field == expected_field
            ),
            "each identity substitution must report {expected_field:?}, got {error:?}"
        );
    }

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
    let equal_precedence_with_build_metadata = verifier
        .admit(&observation(
            expected_identity(),
            InstallOwner::Direct,
            Some("1.0.0+repacked"),
        ))
        .unwrap_err();
    assert_code(&equal_precedence_with_build_metadata, "KELD-UPDATE-003");
    assert!(matches!(
        equal_precedence_with_build_metadata,
        UpdateError::ProvenanceMismatch {
            field: ProvenanceField::VersionFloor,
            ..
        }
    ));
    assert_eq!(admitted_at("1.2.0").version_floor(), "1.2.0");
}

#[test]
fn verifier_rejects_weak_or_mismatched_configured_signing_key() {
    let weak_public_key = [
        1_u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0,
    ];
    let mut weak_identity = expected_identity();
    weak_identity.signing_key_id = SigningKeyId::from_public_key(&weak_public_key);
    let weak_error = UpdateVerifier::new(weak_identity, weak_public_key).unwrap_err();
    assert_code(&weak_error, "KELD-UPDATE-004");
    assert!(matches!(
        weak_error,
        UpdateError::ManifestAuthentication { detail } if detail.contains("weak")
    ));

    let mut mismatched = expected_identity();
    mismatched.signing_key_id = SigningKeyId::from_public_key(&[8_u8; 32]);
    let key_error =
        UpdateVerifier::new(mismatched, signing_key().verifying_key().to_bytes()).unwrap_err();
    assert_code(&key_error, "KELD-UPDATE-003");
    assert!(matches!(
        key_error,
        UpdateError::ProvenanceMismatch {
            field: ProvenanceField::SigningKey,
            ..
        }
    ));
}

#[cfg(windows)]
#[test]
fn provenance_path_identity_does_not_compare_lossy_display_text() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let mut expected = expected_identity();
    expected.update_root = PathBuf::from(OsString::from_wide(&[0xd800]));
    let expected_display = expected.update_root.display().to_string();
    let verifier = UpdateVerifier::new(expected.clone(), signing_key().verifying_key().to_bytes())
        .expect("fixture verifier");
    let mut observed = expected;
    observed.update_root = PathBuf::from(OsString::from_wide(&[0xd801]));
    assert_eq!(
        expected_display,
        observed.update_root.display().to_string(),
        "control: lossy display collapses the distinct paths"
    );

    let error = verifier
        .admit(&observation(observed, InstallOwner::Direct, Some("1.0.0")))
        .unwrap_err();
    assert_code(&error, "KELD-UPDATE-003");
    assert!(matches!(
        error,
        UpdateError::ProvenanceMismatch {
            field: ProvenanceField::UpdateRoot,
            ..
        }
    ));
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
fn signed_identity_mismatches_report_the_exact_manifest_field() {
    let release = release_json("2.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, "");
    let substitutions = [
        (APP_ID, "dev.keld.other", ManifestIdentityField::AppId),
        (
            "\"channel\":\"stable\"",
            "\"channel\":\"beta\"",
            ManifestIdentityField::Channel,
        ),
        (TARGET, "linux-x64", ManifestIdentityField::Target),
    ];
    for (expected, replacement, expected_field) in substitutions {
        let manifest = String::from_utf8(manifest_json(&release))
            .expect("utf8")
            .replace(expected, replacement)
            .into_bytes();
        let error = verify_manifest(&manifest, "1.0.0").unwrap_err();
        assert_code(&error, "KELD-UPDATE-006");
        assert!(
            matches!(
                &error,
                UpdateError::ManifestIdentityMismatch { field, .. } if *field == expected_field
            ),
            "signed identity substitution must report {expected_field:?}, got {error:?}"
        );
    }
}

#[test]
fn signed_unknown_schema_is_rejected_after_authentication() {
    let release = release_json("2.0.0", "1", ZERO_DIGEST, "1", ZERO_DIGEST, "");
    let manifest = String::from_utf8(manifest_json(&release))
        .expect("utf8")
        .replacen(r#""schema":1"#, r#""schema":2"#, 1)
        .into_bytes();
    let error = verify_manifest(&manifest, "1.0.0").unwrap_err();
    assert_code(&error, "KELD-UPDATE-005");
    assert!(matches!(
        error,
        UpdateError::ManifestInvalid { detail } if detail.contains("schema must be the integer 1")
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

struct RewritesOnRewind {
    signed: Cursor<Vec<u8>>,
    replacement: Cursor<Vec<u8>>,
    replacement_pass: bool,
    pause_at: Option<u64>,
    paused_once: bool,
}

impl Read for RewritesOnRewind {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.replacement_pass {
            if let Some(pause_at) = self.pause_at {
                let position = self.replacement.position();
                if position >= pause_at && !self.paused_once {
                    self.paused_once = true;
                    return Ok(0);
                }
                let remaining = pause_at.saturating_sub(position);
                if remaining != 0 {
                    let limit = usize::try_from(remaining.min(bytes.len() as u64))
                        .expect("bounded by the output buffer length");
                    return self.replacement.read(&mut bytes[..limit]);
                }
            }
            self.replacement.read(bytes)
        } else {
            self.signed.read(bytes)
        }
    }
}

impl Seek for RewritesOnRewind {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        if !self.replacement_pass
            && matches!(position, SeekFrom::Start(0))
            && self.signed.position() == self.signed.get_ref().len() as u64
        {
            self.replacement_pass = true;
        }
        if self.replacement_pass {
            self.replacement.seek(position)
        } else {
            self.signed.seek(position)
        }
    }
}

#[test]
fn full_verifier_rejects_compressed_source_change_between_hash_and_decode() {
    let content = b"canonical tar fixture bytes";
    let frame = zstd::stream::encode_all(Cursor::new(content), 3).expect("compress content");
    let skippable = |payload: u8| {
        let mut bytes = frame.clone();
        bytes.extend_from_slice(&0x184D_2A50_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.push(payload);
        bytes
    };
    let signed = skippable(b'A');
    let replacement = skippable(b'B');
    assert_eq!(signed.len(), replacement.len());
    assert_ne!(signed, replacement);
    assert_eq!(
        zstd::stream::decode_all(Cursor::new(&replacement)).expect("decode replacement"),
        content,
        "zstd skippable frame must leave canonical content unchanged"
    );

    let selected = valid_selected(&signed, content);
    let mut source = RewritesOnRewind {
        signed: Cursor::new(signed),
        replacement: Cursor::new(replacement),
        replacement_pass: false,
        pause_at: None,
        paused_once: false,
    };
    let error = selected
        .verify_full(&mut source, &mut Vec::new())
        .expect_err("the decode pass must be bound to the signed transport bytes");
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
fn full_verifier_rejects_source_bytes_after_second_pass_eof() {
    let content = b"canonical tar fixture bytes";
    let frame = zstd::stream::encode_all(Cursor::new(content), 3).expect("compress content");
    let mut signed = frame.clone();
    signed.extend_from_slice(b"signed trailing bytes");
    let selected = valid_selected(&signed, content);
    let mut source = RewritesOnRewind {
        signed: Cursor::new(signed.clone()),
        replacement: Cursor::new(signed),
        replacement_pass: false,
        pause_at: Some(frame.len() as u64),
        paused_once: false,
    };

    let error = selected
        .verify_full(&mut source, &mut Vec::new())
        .expect_err("data after a second-pass EOF must not be raw-drained into a receipt");
    assert_code(&error, "KELD-UPDATE-008");
    assert!(matches!(
        error,
        UpdateError::ArtifactSizeMismatch {
            domain: ArtifactDomain::Compressed,
            ..
        }
    ));
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

#[test]
fn archive_invalid_error_has_stable_code_and_repair_guidance() {
    let error = UpdateError::ArchiveInvalid {
        detail: "header checksum mismatch",
    };
    assert_code(&error, "KELD-UPDATE-011");
    assert!(error.to_string().contains("publish a canonical package"));
}

#[test]
fn canonical_archive_preflight_accepts_policy_only_and_nested_file_trees() {
    let mut empty = Vec::new();
    append_required_policy(&mut empty);
    finish_ustar(&mut empty);
    let validated = parse_test_archive(&empty).expect("policy-only package");
    assert_eq!(validated.entries().len(), 2);
    assert_eq!(validated.content_size(), 2560);
    assert_eq!(validated.content_blake3(), blake3::hash(&empty).as_bytes());

    let mut archive = Vec::new();
    append_required_policy(&mut archive);
    append_ustar_entry(&mut archive, "assets", b'5', &[]);
    append_ustar_entry(&mut archive, "assets/a.txt", b'0', b"x");
    append_ustar_entry(&mut archive, "assets/b.bin", b'0', &vec![0x5a; 513]);
    append_ustar_entry(&mut archive, "empty", b'5', &[]);
    finish_ustar(&mut archive);
    let validated = parse_test_archive(&archive).expect("canonical nested package tree");
    assert_eq!(
        validated
            .entries()
            .iter()
            .map(ArchiveEntry::name)
            .collect::<Vec<_>>(),
        [
            ".keld",
            ".keld/update-policy.v1",
            "assets",
            "assets/a.txt",
            "assets/b.bin",
            "empty"
        ]
    );
    assert_eq!(validated.entries()[2].kind(), ArchiveEntryKind::Directory);
    assert_eq!(validated.entries()[3].size(), 1);
    assert_eq!(validated.entries()[4].size(), 513);
    assert_eq!(
        validated.content_blake3(),
        blake3::hash(&archive).as_bytes()
    );
}

#[test]
fn canonical_archive_preflight_rejects_a_signed_package_missing_update_policy() {
    // `parse_test_archive` signs and verifies this otherwise canonical byte stream before
    // archive admission. This isolates the policy predicate from manifest/auth failures.
    let error = parse_test_archive(&[0_u8; 1024])
        .expect_err("a signed v0 package without update-policy.v1 must be refused");
    assert_code(&error, "KELD-UPDATE-011");
    assert!(matches!(
        error,
        UpdateError::ArchiveInvalid {
            detail: "required no-migration policy is missing"
        }
    ));
}

#[test]
fn canonical_archive_preflight_requires_exact_policy_bytes_and_file_kind() {
    let correct = b"{\"schema\":1,\"dataMigration\":\"none\"}\n";
    for policy in [
        b"{\"schema\":1,\"dataMigration\":\"required\"}\n".as_slice(),
        b"{\"dataMigration\":\"none\",\"schema\":1}\n".as_slice(),
        b"{\"schema\":1,\"dataMigration\":\"none\"}\r\n".as_slice(),
        &correct[..correct.len() - 1],
        b"".as_slice(),
    ] {
        let mut archive = Vec::new();
        append_ustar_entry(&mut archive, ".keld", b'5', &[]);
        append_ustar_entry(&mut archive, ".keld/update-policy.v1", b'0', policy);
        finish_ustar(&mut archive);
        let error = parse_test_archive(&archive).expect_err("signed different policy");
        assert!(matches!(
            error,
            UpdateError::ArchiveInvalid {
                detail: "no-migration policy is not the exact required regular file"
            }
        ));
    }
    let mut directory = Vec::new();
    append_ustar_entry(&mut directory, ".keld", b'5', &[]);
    append_ustar_entry(&mut directory, ".keld/update-policy.v1", b'5', &[]);
    finish_ustar(&mut directory);
    assert!(matches!(
        parse_test_archive(&directory),
        Err(UpdateError::ArchiveInvalid {
            detail: "no-migration policy is not the exact required regular file"
        })
    ));

    let mut duplicate = Vec::new();
    append_required_policy(&mut duplicate);
    append_ustar_entry(&mut duplicate, ".keld/update-policy.v1", b'0', correct);
    finish_ustar(&mut duplicate);
    assert!(matches!(
        parse_test_archive(&duplicate),
        Err(UpdateError::ArchiveInvalid {
            detail: "entry names are not strictly byte-sorted"
        })
    ));
}

#[cfg(windows)]
#[test]
fn produced_package_metadata_drives_signed_full_verification_and_policy_admission() {
    let mut source = Cursor::new(b"application fixture");
    let mut entries = [keld_pack::PackageEntry::File {
        name: "app.bin",
        size: 19,
        input: &mut source,
    }];
    let mut compressed = Vec::new();
    let produced =
        keld_pack::produce_windows_v0(&mut entries, &mut compressed).expect("native producer");
    let selected = selected_for(
        produced.compressed_size(),
        produced.content_size(),
        &crate::error::hex_digest(produced.compressed_blake3()),
        &crate::error::hex_digest(produced.content_blake3()),
    );
    let mut content = Vec::new();
    let verified = selected
        .verify_full(&mut Cursor::new(compressed), &mut content)
        .expect("signed producer metadata");
    let admitted = verified
        .validate_windows_archive(&mut Cursor::new(&content))
        .expect("native package admission");
    assert_eq!(
        admitted
            .entries()
            .iter()
            .map(ArchiveEntry::name)
            .collect::<Vec<_>>(),
        [".keld", ".keld/update-policy.v1", "app.bin"]
    );
    let policy = &admitted.entries()[1];
    let offset = usize::try_from(policy.data_offset()).expect("small fixture");
    let length = usize::try_from(policy.size()).expect("small policy");
    assert_eq!(
        &content[offset..offset + length],
        b"{\"schema\":1,\"dataMigration\":\"none\"}\n"
    );
}

#[test]
fn canonical_archive_preflight_rejects_noncanonical_and_conflicting_trees() {
    let mut bad_checksum = Vec::new();
    append_ustar_entry(&mut bad_checksum, "file", b'0', b"x");
    finish_ustar(&mut bad_checksum);
    bad_checksum[148] = if bad_checksum[148] == b'7' {
        b'6'
    } else {
        b'7'
    };

    let mut bad_padding = Vec::new();
    append_ustar_entry(&mut bad_padding, "file", b'0', b"x");
    finish_ustar(&mut bad_padding);
    bad_padding[512 + 1] = 1;

    let mut unsupported_link = Vec::new();
    append_ustar_entry(&mut unsupported_link, "link", b'2', &[]);
    finish_ustar(&mut unsupported_link);

    let mut missing_parent = Vec::new();
    append_ustar_entry(&mut missing_parent, "a/b", b'0', b"x");
    finish_ustar(&mut missing_parent);

    let mut file_ancestor = Vec::new();
    append_ustar_entry(&mut file_ancestor, "a", b'0', b"x");
    append_ustar_entry(&mut file_ancestor, "a/b", b'0', b"y");
    finish_ustar(&mut file_ancestor);

    let mut trailing_bytes = vec![0_u8; 1024];
    trailing_bytes.push(0);

    for (case, bytes, expected_detail) in [
        ("checksum", bad_checksum, "header checksum is not canonical"),
        ("padding", bad_padding, "entry data padding is nonzero"),
        (
            "unsupported link",
            unsupported_link,
            "entry type is not a regular file or directory",
        ),
        (
            "missing parent",
            missing_parent,
            "entry is missing an explicit parent directory",
        ),
        (
            "file ancestor",
            file_ancestor,
            "a file is an ancestor of another entry",
        ),
        (
            "trailing data",
            trailing_bytes,
            "archive has trailing bytes",
        ),
    ] {
        let error = parse_test_archive(&bytes).expect_err(case);
        assert_code(&error, "KELD-UPDATE-011");
        assert!(
            matches!(error, UpdateError::ArchiveInvalid { detail } if detail == expected_detail),
            "{case} must fail at its own parser predicate, got {error}"
        );
    }
}

#[test]
fn canonical_archive_preflight_rejects_changed_content_and_size() {
    let mut signed_bytes = Vec::new();
    append_ustar_entry(&mut signed_bytes, "file", b'0', b"x");
    finish_ustar(&mut signed_bytes);
    let receipt = archive_receipt(&signed_bytes);

    let mut changed_bytes = Vec::new();
    append_ustar_entry(&mut changed_bytes, "file", b'0', b"y");
    finish_ustar(&mut changed_bytes);
    let error = crate::archive::parse_canonical_ustar(
        &receipt,
        &mut Cursor::new(&changed_bytes),
        lexical_windows_package_names,
    )
    .unwrap_err();
    assert_code(&error, "KELD-UPDATE-009");

    let error = crate::archive::parse_canonical_ustar(
        &receipt,
        &mut Cursor::new(&signed_bytes[..signed_bytes.len() - 1]),
        lexical_windows_package_names,
    )
    .unwrap_err();
    assert_code(&error, "KELD-UPDATE-008");
}

#[cfg(windows)]
#[test]
fn windows_archive_preflight_rejects_case_aliases_and_non_nfc_names() {
    let mut aliases = Vec::new();
    append_required_policy(&mut aliases);
    append_ustar_entry(&mut aliases, "README", b'0', b"a");
    append_ustar_entry(&mut aliases, "Readme", b'0', b"b");
    finish_ustar(&mut aliases);
    let receipt = archive_receipt(&aliases);
    let error = receipt
        .validate_windows_archive(&mut Cursor::new(&aliases))
        .unwrap_err();
    assert_code(&error, "KELD-UPDATE-011");

    let mut decomposed = Vec::new();
    append_required_policy(&mut decomposed);
    append_ustar_entry(&mut decomposed, "cafe\u{0301}.txt", b'0', b"a");
    finish_ustar(&mut decomposed);
    let receipt = archive_receipt(&decomposed);
    let error = receipt
        .validate_windows_archive(&mut Cursor::new(&decomposed))
        .unwrap_err();
    assert_code(&error, "KELD-UPDATE-011");
}
