//! Canonical local bytes are data, never evidence of native protection.

use std::path::Path;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::error::hex_digest;
use crate::{
    ArtifactIdentity, Channel, DirectInstallationIdentity, InstallOwner, InstallProvenance,
    PrincipalModel, SigningKeyId, UpdateError,
};

pub(crate) const MAX_LOCAL_RECORD_BYTES: usize = 64 * 1024;
const PROVENANCE_SCHEMA: &str = "keld.install-provenance/v2";
const COMPLETE_SCHEMA: &str = "keld.complete/v1";
const FLOOR_SCHEMA: &str = "keld.version-floor/v1";
const ACTIVATION_JOURNAL_SCHEMA: &str = "keld.activation-journal/v1";
#[cfg(windows)]
const LIFECYCLE_INSTALLATION_BINDING_DOMAIN: &[u8] =
    b"keld.installation-binding/provenance-v2/v1\0";

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
    PreviousKnownGood,
}

impl PointerKind {
    fn schema(self) -> &'static str {
        match self {
            Self::Current => "keld.current/v1",
            Self::LastKnownGood => "keld.last-known-good/v1",
            Self::PreviousKnownGood => "keld.previous-known-good/v1",
        }
    }
}

/// Durable context shared by every phase of one activation attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActivationJournal {
    /// Host-minted, single-use attempt identity.
    pub(crate) attempt_id: [u8; 32],
    /// Exact signed candidate identity.
    pub(crate) candidate: ArtifactIdentity,
    /// Previously selected artifact that is eligible for this attempt's rollback.
    pub(crate) rollback_target: ArtifactIdentity,
    /// Exact trust floor before this attempt.
    pub(crate) prior_floor: String,
    /// Exact prior health-confirmed artifact.
    pub(crate) prior_last_known_good: ArtifactIdentity,
    /// Exact older health-confirmed artifact, absent only before the first update.
    pub(crate) prior_previous_known_good: Option<ArtifactIdentity>,
    /// Digest of the verified executable that owns protected publication.
    pub(crate) helper_image_blake3: [u8; 32],
    /// Fresh identity of the private attempt health channel.
    pub(crate) health_channel_id: [u8; 32],
    /// One-shot identity of the bounded lifecycle-keeper handoff channel.
    pub(crate) lifecycle_channel_id: [u8; 32],
    /// One explicit durable phase; phase recovery is never inferred from filenames.
    pub(crate) phase: ActivationPhase,
}

/// Persisted phase of the common updater transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActivationPhase {
    /// Journal is durable; floor and current may still have their prior values.
    PublishPending,
    /// Exact candidate is selected and awaiting its attempt-bound health result.
    AwaitingHealth,
    /// Exact attempt health was durably accepted; recovery must finish commit.
    HealthAccepted {
        /// Digest of the exact accepted health receipt.
        health_receipt_digest: [u8; 32],
    },
    /// A failure was durably recorded; recovery must finish the exact rollback.
    RollbackPending {
        /// Closed failure reason; arbitrary diagnostic text is not transaction input.
        failure: ActivationFailureClass,
    },
}

/// Closed failure category persisted when an attempt enters rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivationFailureClass {
    /// The candidate could not be started or exited before Ready.
    CandidateLaunch,
    /// The candidate returned a mismatched or rejected health receipt.
    HealthRejected,
    /// The exact health window expired.
    HealthTimeout,
    /// The recorded process family terminated unexpectedly.
    ProcessCrash,
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
    install_mode: String,
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireActivationJournal {
    schema: String,
    attempt_id: String,
    candidate: WireArtifact,
    rollback_target: WireArtifact,
    prior_floor: String,
    prior_last_known_good: WireArtifact,
    prior_previous_known_good: Option<WireArtifact>,
    helper_image_blake3: String,
    health_channel_id: String,
    lifecycle_channel_id: String,
    phase: WireActivationPhase,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "kebab-case", deny_unknown_fields)]
enum WireActivationPhase {
    PublishPending {},
    AwaitingHealth {},
    HealthAccepted { health_receipt_digest: String },
    RollbackPending { failure: String },
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
        protection_profile: identity
            .install_mode
            .protection_profile()
            .as_str()
            .to_owned(),
        identity: WireIdentity {
            install_mode: identity.install_mode.as_str().to_owned(),
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

/// Derives the immutable lifecycle installation ID from canonical protected provenance.
///
/// This contract is deliberately versioned independently from the record schema. Any future
/// provenance version must preserve this v2 projection or introduce a separately versioned
/// lifecycle-binding contract; callers must not substitute path hashing or caller-selected IDs.
#[cfg(windows)]
pub(crate) fn lifecycle_installation_id(
    provenance: &InstallProvenance,
    publisher_scope: &[u8; 32],
    volume_guid: &str,
) -> Result<[u8; 32], UpdateError> {
    let canonical = encode_provenance(provenance, publisher_scope, volume_guid)?;
    let length = u64::try_from(canonical.len())
        .map_err(|_| invalid("canonical provenance length does not fit u64"))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(LIFECYCLE_INSTALLATION_BINDING_DOMAIN);
    hasher.update(&length.to_le_bytes());
    hasher.update(&canonical);
    let installation_id = *hasher.finalize().as_bytes();
    if installation_id == [0; 32] {
        return Err(invalid("derived lifecycle installation ID is empty"));
    }
    Ok(installation_id)
}

pub(crate) fn decode_provenance(bytes: &[u8]) -> Result<ProvenanceRecord, UpdateError> {
    let wire: WireProvenance = decode(bytes)?;
    schema(&wire.schema, PROVENANCE_SCHEMA)?;
    if wire.owner != "direct" || wire.identity.principal_model != "strict-distinct-os-principals" {
        return Err(invalid("unsupported owner or principal model"));
    }
    let install_mode = crate::DirectInstallMode::parse(&wire.identity.install_mode)?;
    if wire.protection_profile != install_mode.protection_profile().as_str() {
        return Err(invalid("protection profile does not match install mode"));
    }
    nonempty(&wire.volume_guid)?;
    path_text(Path::new(&wire.identity.install_root))?;
    path_text(Path::new(&wire.identity.update_root))?;
    let identity = DirectInstallationIdentity {
        install_mode,
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

#[cfg(test)]
pub(crate) fn encode_activation_journal(
    journal: &ActivationJournal,
) -> Result<Vec<u8>, UpdateError> {
    validate_activation_journal(journal)?;
    encode(&WireActivationJournal {
        schema: ACTIVATION_JOURNAL_SCHEMA.to_owned(),
        attempt_id: hex_digest(&journal.attempt_id),
        candidate: wire_artifact(&journal.candidate)?,
        rollback_target: wire_artifact(&journal.rollback_target)?,
        prior_floor: journal.prior_floor.clone(),
        prior_last_known_good: wire_artifact(&journal.prior_last_known_good)?,
        prior_previous_known_good: journal
            .prior_previous_known_good
            .as_ref()
            .map(wire_artifact)
            .transpose()?,
        helper_image_blake3: hex_digest(&journal.helper_image_blake3),
        health_channel_id: hex_digest(&journal.health_channel_id),
        lifecycle_channel_id: hex_digest(&journal.lifecycle_channel_id),
        phase: wire_activation_phase(&journal.phase),
    })
}

pub(crate) fn decode_activation_journal(bytes: &[u8]) -> Result<ActivationJournal, UpdateError> {
    let wire: WireActivationJournal = decode(bytes)?;
    schema(&wire.schema, ACTIVATION_JOURNAL_SCHEMA)?;
    let journal = ActivationJournal {
        attempt_id: digest(&wire.attempt_id)?,
        candidate: artifact(wire.candidate)?,
        rollback_target: artifact(wire.rollback_target)?,
        prior_floor: wire.prior_floor,
        prior_last_known_good: artifact(wire.prior_last_known_good)?,
        prior_previous_known_good: wire.prior_previous_known_good.map(artifact).transpose()?,
        helper_image_blake3: digest(&wire.helper_image_blake3)?,
        health_channel_id: digest(&wire.health_channel_id)?,
        lifecycle_channel_id: digest(&wire.lifecycle_channel_id)?,
        phase: activation_phase(wire.phase)?,
    };
    validate_activation_journal(&journal)?;
    Ok(journal)
}

/// Raw-byte fuzzer hook for the canonical activation-journal decoder.
///
/// This entry point exists only when the non-product `fuzzing` feature is enabled.
/// It exercises the production decoder and returns whether the input was admitted.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub fn fuzz_activation_journal(bytes: &[u8]) -> bool {
    decode_activation_journal(bytes).is_ok()
}

pub(crate) fn validate_activation_journal(journal: &ActivationJournal) -> Result<(), UpdateError> {
    if journal.lifecycle_channel_id == journal.attempt_id
        || journal.lifecycle_channel_id == journal.health_channel_id
    {
        return Err(invalid(
            "lifecycle keeper channel must be fresh and distinct from attempt and health identities",
        ));
    }
    strict_version(&journal.prior_floor)?;
    let prior_floor = semver::Version::parse(&journal.prior_floor)
        .map_err(|_| invalid("prior floor must be strict SemVer"))?;
    let candidate_version = semver::Version::parse(&journal.candidate.version)
        .map_err(|_| invalid("candidate version must be strict SemVer"))?;
    if candidate_version.cmp_precedence(&prior_floor) != std::cmp::Ordering::Greater {
        return Err(invalid(
            "candidate must advance the exact prior trust floor",
        ));
    }
    for identity in [&journal.rollback_target, &journal.prior_last_known_good]
        .into_iter()
        .chain(journal.prior_previous_known_good.iter())
    {
        let version = semver::Version::parse(&identity.version)
            .map_err(|_| invalid("known-good artifacts must carry strict SemVer"))?;
        if version.cmp_precedence(&prior_floor) == std::cmp::Ordering::Greater {
            return Err(invalid(
                "prior floor must cover every recorded known-good artifact",
            ));
        }
    }
    for identity in [
        &journal.candidate,
        &journal.rollback_target,
        &journal.prior_last_known_good,
    ] {
        wire_artifact(identity)?;
        if !same_artifact_scope(&journal.candidate, identity) {
            return Err(invalid(
                "journal artifacts differ in app/channel/target scope",
            ));
        }
    }
    if let Some(previous) = &journal.prior_previous_known_good {
        wire_artifact(previous)?;
        if !same_artifact_scope(&journal.candidate, previous)
            || previous == &journal.prior_last_known_good
        {
            return Err(invalid(
                "previous known-good artifact has mixed or duplicate scope",
            ));
        }
        let previous_version = semver::Version::parse(&previous.version)
            .map_err(|_| invalid("previous known-good artifact must carry strict SemVer"))?;
        let last_known_good_version =
            semver::Version::parse(&journal.prior_last_known_good.version)
                .map_err(|_| invalid("last-known-good artifact must carry strict SemVer"))?;
        if previous_version.cmp_precedence(&last_known_good_version) != std::cmp::Ordering::Less {
            return Err(invalid(
                "previous known-good artifact must be older than prior last-known-good",
            ));
        }
    }
    if journal.rollback_target != journal.prior_last_known_good
        && journal.prior_previous_known_good.as_ref() != Some(&journal.rollback_target)
    {
        return Err(invalid(
            "rollback target is not a recorded known-good artifact",
        ));
    }
    Ok(())
}

fn same_artifact_scope(left: &ArtifactIdentity, right: &ArtifactIdentity) -> bool {
    left.app_id == right.app_id && left.channel == right.channel && left.target == right.target
}

#[cfg(test)]
#[cfg(test)]
fn wire_activation_phase(phase: &ActivationPhase) -> WireActivationPhase {
    match phase {
        ActivationPhase::PublishPending => WireActivationPhase::PublishPending {},
        ActivationPhase::AwaitingHealth => WireActivationPhase::AwaitingHealth {},
        ActivationPhase::HealthAccepted {
            health_receipt_digest,
        } => WireActivationPhase::HealthAccepted {
            health_receipt_digest: hex_digest(health_receipt_digest),
        },
        ActivationPhase::RollbackPending { failure } => WireActivationPhase::RollbackPending {
            failure: failure.as_str().to_owned(),
        },
    }
}

fn activation_phase(phase: WireActivationPhase) -> Result<ActivationPhase, UpdateError> {
    match phase {
        WireActivationPhase::PublishPending {} => Ok(ActivationPhase::PublishPending),
        WireActivationPhase::AwaitingHealth {} => Ok(ActivationPhase::AwaitingHealth),
        WireActivationPhase::HealthAccepted {
            health_receipt_digest,
        } => Ok(ActivationPhase::HealthAccepted {
            health_receipt_digest: digest(&health_receipt_digest)?,
        }),
        WireActivationPhase::RollbackPending { failure } => Ok(ActivationPhase::RollbackPending {
            failure: ActivationFailureClass::parse(&failure)?,
        }),
    }
}

impl ActivationFailureClass {
    #[cfg(test)]
    const fn as_str(self) -> &'static str {
        match self {
            Self::CandidateLaunch => "candidate-launch",
            Self::HealthRejected => "health-rejected",
            Self::HealthTimeout => "health-timeout",
            Self::ProcessCrash => "process-crash",
        }
    }

    fn parse(value: &str) -> Result<Self, UpdateError> {
        match value {
            "candidate-launch" => Ok(Self::CandidateLaunch),
            "health-rejected" => Ok(Self::HealthRejected),
            "health-timeout" => Ok(Self::HealthTimeout),
            "process-crash" => Ok(Self::ProcessCrash),
            _ => Err(invalid("unsupported activation failure class")),
        }
    }
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
