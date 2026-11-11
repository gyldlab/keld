use std::cmp::Ordering;
use std::collections::HashSet;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::Deserialize;
use serde_json::Number;

use crate::error::{ManifestIdentityField, UpdateError};
use crate::provenance::{AdmittedInstallation, ArtifactIdentity};

const V0_SCHEMA: u64 = 1;
use keld_pack::MAX_ARTIFACT_BYTES as JSON_SAFE_INTEGER_MAX;

/// Result of authenticating, validating, and floor-filtering one v0 manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestDecision {
    /// The signed manifest is valid but contains no release above the protected floor.
    NoUpdate,
    /// The single highest eligible release and its required full artifact.
    Update(Box<SelectedFull>),
}

/// Signed full-artifact metadata selected from a completely validated manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFull {
    pub(crate) installation: crate::DirectInstallationIdentity,
    pub(crate) artifact: SignedFullArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SignedFullArtifact {
    pub(crate) identity: ArtifactIdentity,
    pub(crate) published_at: String,
    pub(crate) url: String,
    pub(crate) compressed_size: u64,
    pub(crate) compressed_blake3: [u8; 32],
    pub(crate) content_size: u64,
}

impl SelectedFull {
    /// Exact selected artifact identity, including the complete version string.
    #[must_use]
    pub const fn identity(&self) -> &ArtifactIdentity {
        &self.artifact.identity
    }

    /// Signed publication timestamp text.
    #[must_use]
    pub fn published_at(&self) -> &str {
        &self.artifact.published_at
    }

    /// Signed full-artifact URL relative to the selected feed.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.artifact.url
    }

    /// Exact signed compressed byte count.
    #[must_use]
    pub const fn compressed_size(&self) -> u64 {
        self.artifact.compressed_size
    }

    /// Signed BLAKE3 of the downloaded compressed bytes.
    #[must_use]
    pub const fn compressed_blake3(&self) -> &[u8; 32] {
        &self.artifact.compressed_blake3
    }

    /// Exact signed decompressed canonical-content byte count.
    #[must_use]
    pub const fn content_size(&self) -> u64 {
        self.artifact.content_size
    }
}

impl AdmittedInstallation {
    /// Authenticates and validates a v0 manifest, then selects only its full artifact.
    ///
    /// Detached signature verification over the literal `manifest_bytes` happens
    /// before JSON parsing. Every release and delta entry is shape-validated before
    /// floor filtering; delta entries never affect T2 selection.
    ///
    /// # Errors
    ///
    /// Returns a typed authentication, manifest, identity, or floor refusal. A valid
    /// manifest with no release above the admitted protected floor returns
    /// [`ManifestDecision::NoUpdate`].
    pub fn verify_manifest(
        &self,
        manifest_bytes: &[u8],
        signature_bytes: &[u8],
    ) -> Result<ManifestDecision, UpdateError> {
        let releases = authenticate_manifest(
            &self.identity,
            &self.verifying_key,
            manifest_bytes,
            signature_bytes,
        )?;
        Ok(select_release(self, releases))
    }
}

pub(crate) fn authenticate_manifest(
    identity: &crate::DirectInstallationIdentity,
    key: &VerifyingKey,
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
) -> Result<Vec<ValidRelease>, UpdateError> {
    let signature = parse_signature(signature_bytes)?;
    key.verify_strict(manifest_bytes, &signature)
        .map_err(|error| UpdateError::ManifestAuthentication {
            detail: format!("signature does not match literal manifest bytes: {error}"),
        })?;
    let manifest: WireManifest =
        serde_json::from_slice(manifest_bytes).map_err(|error| UpdateError::ManifestInvalid {
            detail: error.to_string(),
        })?;
    validate_schema(&manifest.schema)?;
    compare_identity(
        ManifestIdentityField::AppId,
        &identity.app_id,
        &manifest.app.id,
    )?;
    compare_identity(
        ManifestIdentityField::Channel,
        identity.channel.as_str(),
        &manifest.channel,
    )?;
    compare_identity(
        ManifestIdentityField::Target,
        &identity.target,
        &manifest.target,
    )?;
    validate_releases(manifest.releases)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireManifest {
    schema: Number,
    channel: String,
    target: String,
    app: WireApp,
    releases: Vec<WireRelease>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireApp {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRelease {
    version: String,
    #[serde(rename = "publishedAt")]
    published_at: String,
    full: WireFull,
    #[serde(default)]
    deltas: Vec<WireDelta>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFull {
    url: String,
    size: Number,
    blake3: String,
    #[serde(rename = "contentSize")]
    content_size: Number,
    #[serde(rename = "contentBlake3")]
    content_blake3: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireDelta {
    #[serde(rename = "fromVersion")]
    from_version: String,
    url: String,
    size: Number,
    blake3: String,
}

#[derive(Debug)]
pub(crate) struct ValidRelease {
    version_text: String,
    version: Version,
    published_at: String,
    full: ValidFull,
}

#[derive(Debug)]
struct ValidFull {
    url: String,
    size: u64,
    blake3: [u8; 32],
    content_size: u64,
    content_blake3: [u8; 32],
}

/// Verifies only the detached signature of these literal bytes under this key.
///
/// This does not parse a manifest or establish app identity, authorization,
/// provenance, qualification, installation or launch readiness. The caller must
/// obtain its expected key from its existing trusted context; a key from the
/// signed document is not authority. Installed paths still use their semantic
/// manifest verifier. No private key is accepted or generated here.
///
/// # Errors
/// Refuses invalid/weak keys, noncanonical signatures, oversized inputs or a
/// signature that does not authenticate the exact bytes.
pub fn verify_literal_release_signature(
    public_key: [u8; 32], bytes: &[u8], signature: &[u8],
) -> Result<(), UpdateError> {
    if bytes.is_empty() || bytes.len() > 1024 * 1024 || signature.len() > 89 {
        return Err(UpdateError::ManifestAuthentication {detail:"literal signature input exceeds bounds".to_owned()});
    }
    let key=VerifyingKey::from_bytes(&public_key).map_err(|error|UpdateError::ManifestAuthentication {detail:format!("invalid literal signature key: {error}")})?;
    if key.is_weak() {return Err(UpdateError::ManifestAuthentication {detail:"weak literal signature key".to_owned()});}
    verify_literal_with_key(&key,bytes,signature)
}
fn verify_literal_with_key(key:&VerifyingKey,bytes:&[u8],signature:&[u8])->Result<(),UpdateError> {
    let signature=parse_signature(signature)?;
    key.verify_strict(bytes,&signature).map_err(|error|UpdateError::ManifestAuthentication {detail:format!("signature does not match literal manifest bytes: {error}")})
}

fn parse_signature(bytes: &[u8]) -> Result<Signature, UpdateError> {
    let encoded = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if encoded.is_empty()
        || encoded.iter().any(|byte| matches!(byte, b'\r' | b'\n'))
        || bytes
            .strip_suffix(b"\n")
            .is_some_and(|line| line.ends_with(b"\n"))
    {
        return Err(UpdateError::ManifestAuthentication {
            detail: "updates.json.sig must contain one base64 line".to_owned(),
        });
    }
    let mut decoded = [0_u8; 64];
    let written = BASE64_STANDARD
        .decode_slice(encoded, &mut decoded)
        .map_err(|error| UpdateError::ManifestAuthentication {
            detail: format!("updates.json.sig is not canonical standard base64: {error}"),
        })?;
    if written != decoded.len() {
        return Err(UpdateError::ManifestAuthentication {
            detail: format!(
                "updates.json.sig decoded to {written} bytes instead of {}",
                decoded.len()
            ),
        });
    }
    Ok(Signature::from_bytes(&decoded))
}

fn validate_schema(schema: &Number) -> Result<(), UpdateError> {
    if schema.as_u64() == Some(V0_SCHEMA) {
        Ok(())
    } else {
        Err(invalid(format!(
            "schema must be the integer {V0_SCHEMA}, found {schema}"
        )))
    }
}

fn compare_identity(
    field: ManifestIdentityField,
    expected: &str,
    found: &str,
) -> Result<(), UpdateError> {
    if expected == found {
        Ok(())
    } else {
        Err(UpdateError::ManifestIdentityMismatch {
            field,
            expected: expected.to_owned(),
            found: found.to_owned(),
        })
    }
}

fn validate_releases(releases: Vec<WireRelease>) -> Result<Vec<ValidRelease>, UpdateError> {
    let mut validated = Vec::with_capacity(releases.len());
    for release in releases {
        let version = parse_version("release.version", &release.version)?;
        let full = ValidFull {
            url: release.full.url,
            size: parse_size("release.full.size", &release.full.size)?,
            blake3: parse_digest("release.full.blake3", &release.full.blake3)?,
            content_size: parse_size("release.full.contentSize", &release.full.content_size)?,
            content_blake3: parse_digest(
                "release.full.contentBlake3",
                &release.full.content_blake3,
            )?,
        };
        validate_deltas(&release.deltas)?;
        validated.push(ValidRelease {
            version_text: release.version,
            version,
            published_at: release.published_at,
            full,
        });
    }

    let mut precedence_order: Vec<usize> = (0..validated.len()).collect();
    precedence_order.sort_by(|left, right| {
        validated[*left]
            .version
            .cmp_precedence(&validated[*right].version)
    });
    for pair in precedence_order.windows(2) {
        let left = &validated[pair[0]];
        let right = &validated[pair[1]];
        if left.version.cmp_precedence(&right.version) == Ordering::Equal {
            return Err(invalid(format!(
                "release versions `{}` and `{}` have equal SemVer precedence",
                left.version_text, right.version_text
            )));
        }
    }
    Ok(validated)
}

fn validate_deltas(deltas: &[WireDelta]) -> Result<(), UpdateError> {
    let mut exact_from_versions = HashSet::with_capacity(deltas.len());
    for delta in deltas {
        parse_version("release.deltas[].fromVersion", &delta.from_version)?;
        if !exact_from_versions.insert(delta.from_version.as_str()) {
            return Err(invalid(format!(
                "duplicate delta fromVersion `{}`",
                delta.from_version
            )));
        }
        parse_size("release.deltas[].size", &delta.size)?;
        parse_digest("release.deltas[].blake3", &delta.blake3)?;
        let _ = &delta.url;
    }
    Ok(())
}

fn select_release(
    admitted: &AdmittedInstallation,
    releases: Vec<ValidRelease>,
) -> ManifestDecision {
    let selected = releases
        .into_iter()
        .filter(|release| release.version.cmp_precedence(&admitted.floor) == Ordering::Greater)
        .max_by(|left, right| left.version.cmp_precedence(&right.version));
    let Some(release) = selected else {
        return ManifestDecision::NoUpdate;
    };
    ManifestDecision::Update(Box::new(SelectedFull {
        installation: admitted.identity.clone(),
        artifact: release.into_artifact(&admitted.identity),
    }))
}

pub(crate) fn select_baseline(
    identity: &crate::DirectInstallationIdentity,
    releases: Vec<ValidRelease>,
) -> Result<SignedFullArtifact, UpdateError> {
    let release = releases
        .into_iter()
        .find(|release| release.version_text == identity.baseline.version)
        .ok_or_else(|| UpdateError::Baseline {
            step: "baseline selection",
            detail: "authenticated manifest does not contain the exact configured baseline version"
                .to_owned(),
        })?;
    let artifact = release.into_artifact(identity);
    if artifact.identity != identity.baseline {
        return Err(UpdateError::Baseline {
            step: "baseline identity",
            detail: "authenticated full artifact differs from the configured baseline".to_owned(),
        });
    }
    Ok(artifact)
}

impl ValidRelease {
    fn into_artifact(self, installation: &crate::DirectInstallationIdentity) -> SignedFullArtifact {
        SignedFullArtifact {
            identity: ArtifactIdentity {
                app_id: installation.app_id.clone(),
                channel: installation.channel,
                target: installation.target.clone(),
                version: self.version_text,
                content_blake3: self.full.content_blake3,
            },
            published_at: self.published_at,
            url: self.full.url,
            compressed_size: self.full.size,
            compressed_blake3: self.full.blake3,
            content_size: self.full.content_size,
        }
    }
}

fn parse_version(field: &str, text: &str) -> Result<Version, UpdateError> {
    Version::parse(text)
        .map_err(|error| invalid(format!("{field} `{text}` is not strict SemVer: {error}")))
}

fn parse_size(field: &str, number: &Number) -> Result<u64, UpdateError> {
    let Some(value) = number.as_u64() else {
        return Err(invalid(format!(
            "{field} must be a positive base-10 JSON integer"
        )));
    };
    if !(1..=JSON_SAFE_INTEGER_MAX).contains(&value) {
        return Err(invalid(format!(
            "{field} must be in 1..={JSON_SAFE_INTEGER_MAX}, found {value}"
        )));
    }
    Ok(value)
}

pub(crate) fn parse_digest(field: &str, text: &str) -> Result<[u8; 32], UpdateError> {
    if text.len() != 64 || !text.is_ascii() {
        return Err(invalid(format!(
            "{field} must contain exactly 64 hexadecimal characters"
        )));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let high = hex_nibble(pair[0])
            .ok_or_else(|| invalid(format!("{field} contains a non-hexadecimal character")))?;
        let low = hex_nibble(pair[1])
            .ok_or_else(|| invalid(format!("{field} contains a non-hexadecimal character")))?;
        digest[index] = (high << 4) | low;
    }
    Ok(digest)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn invalid(detail: String) -> UpdateError {
    UpdateError::ManifestInvalid { detail }
}

#[cfg(test)]
mod literal_signature_tests {
    use super::*;
    const PUBLIC:[u8;32]=[234,74,108,99,226,156,82,10,190,245,80,123,19,46,197,249,149,71,118,174,190,190,123,146,66,30,234,105,20,70,210,44];
    const BYTES:&[u8]=b"{\"schema\":1,\"channel\":\"stable\",\"target\":\"windows-x64\",\"app\":{\"id\":\"dev.keld.fixture\"},\"releases\":[{\"version\":\"1.0.0\",\"publishedAt\":\"2026-09-20T00:00:00Z\",\"full\":{\"url\":\"1.0.0/full.zst\",\"size\":1,\"blake3\":\"0101010101010101010101010101010101010101010101010101010101010101\",\"contentSize\":1024,\"contentBlake3\":\"0101010101010101010101010101010101010101010101010101010101010101\"},\"deltas\":[]}]}";
    const SIGNATURE:&[u8]=b"Wo6PAwIhBqPDI5wAW5JKv0v8BnxOIZrOKKo2JQODnZe3M5/BQZax2UVCM2SNqOxVNkBYoA1W4bzaWXbvDJ/LBw==";
    #[test] fn literal_signature_reuses_exact_public_fixture_and_refuses_changes() {
        verify_literal_release_signature(PUBLIC,BYTES,SIGNATURE).unwrap();
        let mut changed=BYTES.to_vec();changed[0]^=1;
        assert!(verify_literal_release_signature(PUBLIC,&changed,SIGNATURE).is_err());
        let mut wrong=PUBLIC;wrong[0]^=1;
        assert!(verify_literal_release_signature(wrong,BYTES,SIGNATURE).is_err());
        assert!(verify_literal_release_signature([0;32],BYTES,SIGNATURE).is_err());
        assert!(verify_literal_release_signature(PUBLIC,BYTES,b"not base64").is_err());
        let mut extra=SIGNATURE.to_vec();extra.push(b'\n');
        verify_literal_release_signature(PUBLIC,BYTES,&extra).unwrap();
        extra.push(b'\n');assert!(verify_literal_release_signature(PUBLIC,BYTES,&extra).is_err());
        let mut crlf=SIGNATURE.to_vec();crlf.extend_from_slice(b"\r\n");assert!(verify_literal_release_signature(PUBLIC,BYTES,&crlf).is_err());
        assert!(verify_literal_release_signature(PUBLIC,&vec![0;1024*1024+1],SIGNATURE).is_err());
    }
}
