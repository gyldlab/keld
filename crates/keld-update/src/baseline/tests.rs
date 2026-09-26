use std::io::Cursor;

use crate::tests::{
    append_required_policy, digest_hex, expected_identity, finish_ustar, manifest_json,
    observation, release_json, sign, signing_key,
};
use crate::{BaselineVerifier, InstallOwner, ManifestDecision, UpdateError, UpdateVerifier};

fn fixture(version: &str, content: &[u8]) -> (BaselineVerifier, Vec<u8>, Vec<u8>) {
    let compressed =
        zstd::stream::encode_all(Cursor::new(content), 0).expect("fixture compression");
    let mut identity = expected_identity();
    identity.baseline.version = version.to_owned();
    identity.baseline.content_blake3 = *blake3::hash(content).as_bytes();
    let verifier = BaselineVerifier::new(identity, signing_key().verifying_key().to_bytes())
        .expect("fixture identity");
    let release = release_json(
        version,
        &compressed.len().to_string(),
        &digest_hex(&compressed),
        &content.len().to_string(),
        &digest_hex(content),
        "",
    );
    (verifier, manifest_json(&release), compressed)
}

#[test]
fn exact_baseline_is_selected_without_changing_update_floor_selection() {
    let content = b"baseline content";
    let (verifier, manifest, compressed) = fixture("1.0.0+installer", content);
    let mut json: serde_json::Value = serde_json::from_slice(&manifest).expect("fixture JSON");
    let mut newer = json["releases"][0].clone();
    newer["version"] = "3.0.0".into();
    json["releases"]
        .as_array_mut()
        .expect("release array")
        .push(newer);
    let bytes = serde_json::to_vec(&json).expect("fixture encoding");
    let selected = verifier
        .verify_manifest(&bytes, &sign(&bytes))
        .expect("exact baseline");
    assert_eq!(selected.identity().version, "1.0.0+installer");
    let mut output = Vec::new();
    let receipt = selected
        .verify_full(&mut Cursor::new(compressed), &mut output)
        .expect("verified baseline");
    assert_eq!(output, content);
    assert_eq!(receipt.identity().version, "1.0.0+installer");
    assert_eq!(receipt.content_size(), content.len() as u64);
    assert_eq!(receipt.content_blake3(), blake3::hash(content).as_bytes());
    let identity = verifier.configuration.expected.clone();
    let updater = UpdateVerifier::new(identity.clone(), signing_key().verifying_key().to_bytes())
        .expect("updater configuration");
    let admitted = updater
        .admit(&observation(
            identity.clone(),
            InstallOwner::Direct,
            Some("1.0.0+installer"),
        ))
        .expect("logical admission");
    let ManifestDecision::Update(update) = admitted
        .verify_manifest(&bytes, &sign(&bytes))
        .expect("valid update feed")
    else {
        panic!("newer release must be eligible")
    };
    assert_eq!(update.identity().version, "3.0.0");
    let at_latest = updater
        .admit(&observation(identity, InstallOwner::Direct, Some("3.0.0")))
        .expect("logical latest floor");
    assert_eq!(
        at_latest
            .verify_manifest(&bytes, &sign(&bytes))
            .expect("valid feed"),
        ManifestDecision::NoUpdate
    );
}

#[test]
fn baseline_rejects_version_metadata_digest_and_signed_schema_substitution() {
    let (verifier, original, _) = fixture("1.0.0+installer", b"baseline");
    for replacement in ["1.0.0+other", "2.0.0", "0.9.0"] {
        let mut json: serde_json::Value = serde_json::from_slice(&original).expect("fixture JSON");
        json["releases"][0]["version"] = replacement.into();
        let bytes = serde_json::to_vec(&json).expect("fixture encoding");
        assert!(
            matches!(
                verifier.verify_manifest(&bytes, &sign(&bytes)),
                Err(UpdateError::Baseline {
                    step: "baseline selection",
                    ..
                })
            ),
            "{replacement}"
        );
    }
    let mut json: serde_json::Value = serde_json::from_slice(&original).expect("fixture JSON");
    json["releases"][0]["full"]["contentBlake3"] = "00".repeat(32).into();
    let bytes = serde_json::to_vec(&json).expect("fixture encoding");
    assert!(matches!(
        verifier.verify_manifest(&bytes, &sign(&bytes)),
        Err(UpdateError::Baseline {
            step: "baseline identity",
            ..
        })
    ));
    let mut invalid = original.clone();
    invalid.push(b' ');
    assert!(matches!(
        verifier.verify_manifest(&invalid, &sign(&original)),
        Err(UpdateError::ManifestAuthentication { .. })
    ));
    let duplicate = String::from_utf8(original)
        .expect("UTF-8")
        .replacen("\"schema\":1", "\"schema\":1,\"schema\":1", 1)
        .into_bytes();
    assert!(matches!(
        verifier.verify_manifest(&duplicate, &sign(&duplicate)),
        Err(UpdateError::ManifestInvalid { .. })
    ));
}

#[test]
fn baseline_validates_nonselected_releases_before_selection() {
    let (verifier, original, _) = fixture("1.0.0", b"baseline");
    let mut json: serde_json::Value = serde_json::from_slice(&original).expect("fixture JSON");
    let mut malformed = json["releases"][0].clone();
    malformed["version"] = "9.0.0".into();
    malformed["full"]["size"] = 0.into();
    json["releases"]
        .as_array_mut()
        .expect("array")
        .push(malformed);
    let bytes = serde_json::to_vec(&json).expect("encoding");
    assert!(matches!(
        verifier.verify_manifest(&bytes, &sign(&bytes)),
        Err(UpdateError::ManifestInvalid { .. })
    ));
}

#[test]
fn baseline_full_verifier_enforces_signed_compressed_and_content_domains() {
    let (verifier, manifest, compressed) = fixture("1.0.0", b"baseline content");
    let selected = verifier
        .verify_manifest(&manifest, &sign(&manifest))
        .expect("selection");
    let mut short = compressed.clone();
    short.pop();
    assert!(matches!(
        selected.verify_full(&mut Cursor::new(short), &mut Vec::new()),
        Err(UpdateError::ArtifactSizeMismatch {
            domain: crate::ArtifactDomain::Compressed,
            ..
        })
    ));
    let mut wrong = compressed.clone();
    wrong[0] ^= 1;
    assert!(matches!(
        selected.verify_full(&mut Cursor::new(wrong), &mut Vec::new()),
        Err(UpdateError::ArtifactDigestMismatch {
            domain: crate::ArtifactDomain::Compressed,
            ..
        })
    ));
    let different = zstd::stream::encode_all(Cursor::new(b"baseline contenU"), 0)
        .expect("different fixture content");
    let mut alternate: serde_json::Value = serde_json::from_slice(&manifest).expect("JSON");
    alternate["releases"][0]["full"]["size"] = (different.len() as u64).into();
    alternate["releases"][0]["full"]["blake3"] = digest_hex(&different).into();
    let alternate_bytes = serde_json::to_vec(&alternate).expect("encoding");
    let alternate_selected = verifier
        .verify_manifest(&alternate_bytes, &sign(&alternate_bytes))
        .expect("signed transport with conflicting content");
    assert!(matches!(
        alternate_selected.verify_full(&mut Cursor::new(different), &mut Vec::new()),
        Err(UpdateError::ArtifactDigestMismatch {
            domain: crate::ArtifactDomain::Content,
            ..
        })
    ));
    let mut json: serde_json::Value = serde_json::from_slice(&manifest).expect("JSON");
    json["releases"][0]["full"]["contentSize"] = 1.into();
    let bytes = serde_json::to_vec(&json).expect("encoding");
    let selected = verifier
        .verify_manifest(&bytes, &sign(&bytes))
        .expect("signed lower content bound");
    assert!(matches!(
        selected.verify_full(&mut Cursor::new(compressed), &mut Vec::new()),
        Err(UpdateError::ArtifactSizeMismatch {
            domain: crate::ArtifactDomain::Content,
            ..
        })
    ));
}

#[test]
fn baseline_uses_shared_canonical_archive_and_policy_validation() {
    let mut canonical = Vec::new();
    append_required_policy(&mut canonical);
    finish_ustar(&mut canonical);
    for content in [&canonical[..], &[0_u8; 1024][..]] {
        let (verifier, manifest, compressed) = fixture("1.0.0", content);
        let selected = verifier
            .verify_manifest(&manifest, &sign(&manifest))
            .expect("selection");
        let receipt = selected
            .verify_full(&mut Cursor::new(compressed), &mut Vec::new())
            .expect("signed bytes");
        let result = crate::archive::parse_content_archive(
            &receipt.content,
            &mut Cursor::new(content),
            |_| Ok(()),
        );
        if content == canonical {
            assert_eq!(
                result.expect("canonical policy").identity(),
                receipt.identity()
            );
        } else {
            assert!(matches!(
                result,
                Err(UpdateError::ArchiveInvalid {
                    detail: "required no-migration policy is missing"
                })
            ));
        }
    }
}
