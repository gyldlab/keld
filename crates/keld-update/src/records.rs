//! Canonical local bytes are data, never evidence of native protection.

use std::path::Path;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::error::hex_digest;
use crate::{
    ArtifactIdentity, Channel, DirectInstallationIdentity, InstallOwner, InstallProvenance,
    PrincipalModel, SigningKeyId, UpdateError,
};

pub(crate) const MAX_LOCAL_RECORD_BYTES: usize = 64 * 1024;
const PROVENANCE_SCHEMA: &str = "keld.install-provenance/v1";
const COMPLETE_SCHEMA: &str = "keld.complete/v1";
const FLOOR_SCHEMA: &str = "keld.version-floor/v1";
const PROTECTION: &str = "windows-system-users-rx-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvenanceRecord {
    pub(crate) provenance: InstallProvenance,
    pub(crate) publisher_scope: [u8; 32],
    pub(crate) volume_guid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompleteRecord {
    pub(crate) artifact: ArtifactIdentity,
    pub(crate) content_size: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PointerKind {
    Current,
    LastKnownGood,
}

impl PointerKind {
    fn schema(self) -> &'static str {
        match self {
            Self::Current => "keld.current/v1",
            Self::LastKnownGood => "keld.last-known-good/v1",
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireArtifact {
    app_id: String,
    channel: String,
    target: String,
    version: String,
    content_blake3: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireIdentity {
    app_id: String,
    channel: String,
    target: String,
    install_root: String,
    update_root: String,
    signing_key_id: String,
    baseline: WireArtifact,
    profile_digest: String,
    principal_model: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProvenance {
    schema: String,
    owner: String,
    protection_profile: String,
    identity: WireIdentity,
    publisher_scope: String,
    volume_guid: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireComplete {
    schema: String,
    artifact: WireArtifact,
    content_size: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFloor {
    schema: String,
    version: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePointer {
    schema: String,
    artifact: WireArtifact,
}

pub(crate) fn encode_provenance(
    record: &InstallProvenance,
    publisher_scope: &[u8; 32],
    volume_guid: &str,
) -> Result<Vec<u8>, UpdateError> {
    if record.owner != InstallOwner::Direct {
        return Err(invalid("only direct provenance is supported"));
    }
    crate::provenance::validate_expected(&record.identity)?;
    nonempty(volume_guid)?;
    let identity = &record.identity;
    encode(&WireProvenance {
        schema: PROVENANCE_SCHEMA.to_owned(),
        owner: "direct".to_owned(),
        protection_profile: PROTECTION.to_owned(),
        identity: WireIdentity {
            app_id: identity.app_id.clone(),
            channel: identity.channel.as_str().to_owned(),
            target: identity.target.clone(),
            install_root: path_text(&identity.install_root)?.to_owned(),
            update_root: path_text(&identity.update_root)?.to_owned(),
            signing_key_id: hex_digest(identity.signing_key_id.as_bytes()),
            baseline: wire_artifact(&identity.baseline)?,
            profile_digest: hex_digest(&identity.profile_digest.0),
            principal_model: "strict-distinct-os-principals".to_owned(),
        },
        publisher_scope: hex_digest(publisher_scope),
        volume_guid: volume_guid.to_owned(),
    })
}

pub(crate) fn decode_provenance(bytes: &[u8]) -> Result<ProvenanceRecord, UpdateError> {
    let wire: WireProvenance = decode(bytes)?;
    schema(&wire.schema, PROVENANCE_SCHEMA)?;
    if wire.owner != "direct"
        || wire.protection_profile != PROTECTION
        || wire.identity.principal_model != "strict-distinct-os-principals"
    {
        return Err(invalid(
            "unsupported owner, protection profile or principal model",
        ));
    }
    nonempty(&wire.volume_guid)?;
    path_text(Path::new(&wire.identity.install_root))?;
    path_text(Path::new(&wire.identity.update_root))?;
    let identity = DirectInstallationIdentity {
        app_id: wire.identity.app_id,
        channel: channel(&wire.identity.channel)?,
        target: wire.identity.target,
        install_root: wire.identity.install_root.into(),
        update_root: wire.identity.update_root.into(),
        signing_key_id: SigningKeyId::from_digest(digest(&wire.identity.signing_key_id)?),
        baseline: artifact(wire.identity.baseline)?,
        profile_digest: keld_guard::ProfileDigest(digest(&wire.identity.profile_digest)?),
        principal_model: PrincipalModel::StrictDistinctOsPrincipals,
    };
    crate::provenance::validate_expected(&identity)?;
    Ok(ProvenanceRecord {
        provenance: InstallProvenance {
            identity,
            owner: InstallOwner::Direct,
        },
        publisher_scope: digest(&wire.publisher_scope)?,
        volume_guid: wire.volume_guid,
    })
}

pub(crate) fn encode_complete(
    artifact: &ArtifactIdentity,
    content_size: u64,
) -> Result<Vec<u8>, UpdateError> {
    size(content_size)?;
    encode(&WireComplete {
        schema: COMPLETE_SCHEMA.to_owned(),
        artifact: wire_artifact(artifact)?,
        content_size,
    })
}

pub(crate) fn decode_complete(bytes: &[u8]) -> Result<CompleteRecord, UpdateError> {
    let wire: WireComplete = decode(bytes)?;
    schema(&wire.schema, COMPLETE_SCHEMA)?;
    size(wire.content_size)?;
    Ok(CompleteRecord {
        artifact: artifact(wire.artifact)?,
        content_size: wire.content_size,
    })
}

pub(crate) fn encode_floor(version: &str) -> Result<Vec<u8>, UpdateError> {
    strict_version(version)?;
    encode(&WireFloor {
        schema: FLOOR_SCHEMA.to_owned(),
        version: version.to_owned(),
    })
}

pub(crate) fn decode_floor(bytes: &[u8]) -> Result<String, UpdateError> {
    let wire: WireFloor = decode(bytes)?;
    schema(&wire.schema, FLOOR_SCHEMA)?;
    strict_version(&wire.version)?;
    Ok(wire.version)
}

pub(crate) fn encode_pointer(
    kind: PointerKind,
    identity: &ArtifactIdentity,
) -> Result<Vec<u8>, UpdateError> {
    encode(&WirePointer {
        schema: kind.schema().to_owned(),
        artifact: wire_artifact(identity)?,
    })
}

pub(crate) fn decode_pointer(
    kind: PointerKind,
    bytes: &[u8],
) -> Result<ArtifactIdentity, UpdateError> {
    let wire: WirePointer = decode(bytes)?;
    schema(&wire.schema, kind.schema())?;
    artifact(wire.artifact)
}

fn encode<T: Serialize>(wire: &T) -> Result<Vec<u8>, UpdateError> {
    let bytes = serde_json::to_vec(wire).map_err(|error| invalid(error.to_string()))?;
    if bytes.len() > MAX_LOCAL_RECORD_BYTES {
        return Err(invalid("record exceeds 64 KiB"));
    }
    Ok(bytes)
}

fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, UpdateError> {
    if bytes.is_empty() || bytes.len() > MAX_LOCAL_RECORD_BYTES {
        return Err(invalid("record must contain 1..=65536 bytes"));
    }
    let wire: T = serde_json::from_slice(bytes).map_err(|error| invalid(error.to_string()))?;
    if encode(&wire)? != bytes {
        return Err(invalid("record differs from its canonical typed encoding"));
    }
    Ok(wire)
}

fn wire_artifact(identity: &ArtifactIdentity) -> Result<WireArtifact, UpdateError> {
    nonempty(&identity.app_id)?;
    nonempty(&identity.target)?;
    strict_version(&identity.version)?;
    Ok(WireArtifact {
        app_id: identity.app_id.clone(),
        channel: identity.channel.as_str().to_owned(),
        target: identity.target.clone(),
        version: identity.version.clone(),
        content_blake3: hex_digest(&identity.content_blake3),
    })
}

fn artifact(wire: WireArtifact) -> Result<ArtifactIdentity, UpdateError> {
    nonempty(&wire.app_id)?;
    nonempty(&wire.target)?;
    strict_version(&wire.version)?;
    Ok(ArtifactIdentity {
        app_id: wire.app_id,
        channel: channel(&wire.channel)?,
        target: wire.target,
        version: wire.version,
        content_blake3: digest(&wire.content_blake3)?,
    })
}

fn digest(text: &str) -> Result<[u8; 32], UpdateError> {
    if text
        .bytes()
        .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("digest must use lowercase hexadecimal"));
    }
    crate::manifest::parse_digest("local digest", text)
        .map_err(|_| invalid("digest must contain exactly 64 lowercase hexadecimal characters"))
}

fn channel(text: &str) -> Result<Channel, UpdateError> {
    match text {
        "stable" => Ok(Channel::Stable),
        "beta" => Ok(Channel::Beta),
        "canary" => Ok(Channel::Canary),
        _ => Err(invalid("unsupported channel")),
    }
}

fn strict_version(text: &str) -> Result<(), UpdateError> {
    semver::Version::parse(text)
        .map(|_| ())
        .map_err(|_| invalid("version must be strict SemVer"))
}

fn nonempty(text: &str) -> Result<(), UpdateError> {
    if text.is_empty() || text.chars().any(char::is_control) {
        Err(invalid(
            "identity text must be nonempty and contain no control characters",
        ))
    } else {
        Ok(())
    }
}

fn schema(found: &str, expected: &str) -> Result<(), UpdateError> {
    if found == expected {
        Ok(())
    } else {
        Err(invalid("unsupported record schema"))
    }
}

fn size(value: u64) -> Result<(), UpdateError> {
    if (1..=keld_pack::MAX_ARTIFACT_BYTES).contains(&value) {
        Ok(())
    } else {
        Err(invalid(
            "content size is outside the canonical package bound",
        ))
    }
}

pub(crate) fn path_text(path: &Path) -> Result<&str, UpdateError> {
    let text = path
        .to_str()
        .ok_or_else(|| invalid("root is not lossless UTF-8"))?;
    let drive = text.strip_prefix(r"\\?\").unwrap_or(text);
    let bytes = drive.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1..3] != *b":\\"
        || drive.contains('/')
    {
        return Err(invalid("root must be an absolute drive path"));
    }
    if bytes.len() > 3 {
        for component in drive[3..].split('\\') {
            keld_guard::validate_fs_component(component).map_err(invalid)?;
        }
    }
    Ok(text)
}

fn invalid(detail: impl Into<String>) -> UpdateError {
    UpdateError::LocalRecordInvalid {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests;
