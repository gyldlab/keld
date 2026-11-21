//! KEL-270 D-23 canonical metadata-store *data* records, with no native authority.
//!
//! An encoded digest or decoded record does not authenticate a signature,
//! retained OS handle, repository lease, generation history or complete ledger.
//! T20 remains the TUF metadata client and KEL-53 T82 remains the native writer.
//! This module performs no I/O and never grants metadata or installer admission.

use serde::{Deserialize, Serialize};

use super::{decode, digest, encode, hex_digest, initial_admission, invalid, schema};
use crate::UpdateError;

const ANCHOR_SCHEMA: &str = "keld.metadata-anchor/v1";
const CURRENT_SCHEMA: &str = "keld.metadata-current/v1";
const GENERATION_SCHEMA: &str = "keld.metadata-generation/v1";
const ANCHOR_MAX: usize = 2_048;
const CURRENT_MAX: usize = 512;
const GENERATION_MAX: usize = 4_096;

// D-21's role-body caps; these are independent of D-23's generation JSON cap.
const ROOT_CAP: u64 = 65_536;
const TIMESTAMP_CAP: u64 = 16_384;
const SNAPSHOT_CAP: u64 = 262_144;
const TARGETS_CAP: u64 = 1_048_576;

/// D-23 enrollment anchor state, not a native enrollment grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EnrollmentState {
    Pending,
    Established,
}

/// The complete typed D-23 enrollment-anchor byte record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AnchorRecord {
    pub(crate) repository_id: [u8; 32],
    pub(crate) app_id: String,
    pub(crate) owner_sid: String,
    pub(crate) enrollment_state: EnrollmentState,
    pub(crate) initial_generation_sha256: [u8; 32],
}

/// The replaceable D-23 current record as data, not an authenticated head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CurrentRecord {
    pub(crate) repository_id: [u8; 32],
    pub(crate) sequence: u64,
    pub(crate) generation_sha256: [u8; 32],
}

/// D-23 metadata generation phase, never a `ReadyRevision` capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum GenerationPhase {
    RootRefresh,
    Roles,
    Ready,
}

/// One role-blob reference. Its bytes, signatures and retained handle are not read here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MetadataRoleReference {
    pub(crate) sha256: [u8; 32],
    pub(crate) length: u64,
    pub(crate) version: u64,
}

/// Starting trusted-root role-key fingerprints; T20 verifies their TUF meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RootRefresh {
    pub(crate) timestamp_keys_sha256: [u8; 32],
    pub(crate) snapshot_keys_sha256: [u8; 32],
}

/// D-23 immutable generation record. Field values alone grant no trust.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GenerationRecord {
    pub(crate) repository_id: [u8; 32],
    pub(crate) sequence: u64,
    pub(crate) recovery_epoch: u64,
    pub(crate) previous_generation_sha256: Option<[u8; 32]>,
    pub(crate) policy_sha256: [u8; 32],
    pub(crate) utc_floor_millis: u64,
    pub(crate) phase: GenerationPhase,
    pub(crate) root: MetadataRoleReference,
    pub(crate) timestamp: Option<MetadataRoleReference>,
    pub(crate) snapshot: Option<MetadataRoleReference>,
    pub(crate) targets: Option<MetadataRoleReference>,
    pub(crate) root_refresh: Option<RootRefresh>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireAnchor {
    schema: String,
    repository_id: String,
    app_id: String,
    owner_sid: String,
    enrollment_state: EnrollmentState,
    initial_generation_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireCurrent {
    schema: String,
    repository_id: String,
    sequence: u64,
    generation_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireRoleReference {
    sha256: String,
    length: u64,
    version: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireRootRefresh {
    timestamp_keys_sha256: String,
    snapshot_keys_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireGeneration {
    schema: String,
    repository_id: String,
    sequence: u64,
    recovery_epoch: u64,
    previous_generation_sha256: Option<String>,
    policy_sha256: String,
    utc_floor_millis: u64,
    phase: GenerationPhase,
    root: WireRoleReference,
    timestamp: Option<WireRoleReference>,
    snapshot: Option<WireRoleReference>,
    targets: Option<WireRoleReference>,
    root_refresh: Option<WireRootRefresh>,
}

/// Encodes only the fixed-field D-23 Anchor JSON; no enrollment is performed.
pub(crate) fn encode_metadata_anchor(record: &AnchorRecord) -> Result<Vec<u8>, UpdateError> {
    validate_anchor(record)?;
    encode_bounded(
        &WireAnchor {
            schema: ANCHOR_SCHEMA.to_owned(),
            repository_id: hex_digest(&record.repository_id),
            app_id: record.app_id.clone(),
            owner_sid: record.owner_sid.clone(),
            enrollment_state: record.enrollment_state,
            initial_generation_sha256: hex_digest(&record.initial_generation_sha256),
        },
        ANCHOR_MAX,
    )
}

/// Parses an exact canonical D-23 Anchor; the native owner must authenticate its handle.
pub(crate) fn decode_metadata_anchor(bytes: &[u8]) -> Result<AnchorRecord, UpdateError> {
    let wire: WireAnchor = decode_bounded(bytes, ANCHOR_MAX)?;
    schema(&wire.schema, ANCHOR_SCHEMA)?;
    let record = AnchorRecord {
        repository_id: digest(&wire.repository_id)?,
        app_id: wire.app_id,
        owner_sid: wire.owner_sid,
        enrollment_state: wire.enrollment_state,
        initial_generation_sha256: digest(&wire.initial_generation_sha256)?,
    };
    validate_anchor(&record)?;
    Ok(record)
}

/// Encodes the replaceable D-23 Current JSON bytes without replacing a record.
pub(crate) fn encode_metadata_current(record: &CurrentRecord) -> Result<Vec<u8>, UpdateError> {
    validate_current(record)?;
    encode_bounded(
        &WireCurrent {
            schema: CURRENT_SCHEMA.to_owned(),
            repository_id: hex_digest(&record.repository_id),
            sequence: record.sequence,
            generation_sha256: hex_digest(&record.generation_sha256),
        },
        CURRENT_MAX,
    )
}

/// Parses a canonical D-23 Current without trusting its generation selection.
pub(crate) fn decode_metadata_current(bytes: &[u8]) -> Result<CurrentRecord, UpdateError> {
    let wire: WireCurrent = decode_bounded(bytes, CURRENT_MAX)?;
    schema(&wire.schema, CURRENT_SCHEMA)?;
    let record = CurrentRecord {
        repository_id: digest(&wire.repository_id)?,
        sequence: wire.sequence,
        generation_sha256: digest(&wire.generation_sha256)?,
    };
    validate_current(&record)?;
    Ok(record)
}

/// Encodes D-23 generation data, never signs, publishes or admits a revision.
pub(crate) fn encode_metadata_generation(
    record: &GenerationRecord,
) -> Result<Vec<u8>, UpdateError> {
    validate_generation(record)?;
    let wire = WireGeneration {
        schema: GENERATION_SCHEMA.to_owned(),
        repository_id: hex_digest(&record.repository_id),
        sequence: record.sequence,
        recovery_epoch: record.recovery_epoch,
        previous_generation_sha256: record
            .previous_generation_sha256
            .map(|value| hex_digest(&value)),
        policy_sha256: hex_digest(&record.policy_sha256),
        utc_floor_millis: record.utc_floor_millis,
        phase: record.phase,
        root: wire_role(record.root),
        timestamp: record.timestamp.map(wire_role),
        snapshot: record.snapshot.map(wire_role),
        targets: record.targets.map(wire_role),
        root_refresh: record.root_refresh.map(wire_refresh),
    };
    encode_bounded(&wire, GENERATION_MAX)
}

/// Parses exact D-23 generation data; T20 and the native writer remain separate.
pub(crate) fn decode_metadata_generation(bytes: &[u8]) -> Result<GenerationRecord, UpdateError> {
    let wire: WireGeneration = decode_bounded(bytes, GENERATION_MAX)?;
    schema(&wire.schema, GENERATION_SCHEMA)?;
    let record = GenerationRecord {
        repository_id: digest(&wire.repository_id)?,
        sequence: wire.sequence,
        recovery_epoch: wire.recovery_epoch,
        previous_generation_sha256: wire
            .previous_generation_sha256
            .as_deref()
            .map(digest)
            .transpose()?,
        policy_sha256: digest(&wire.policy_sha256)?,
        utc_floor_millis: wire.utc_floor_millis,
        phase: wire.phase,
        root: from_wire_role(&wire.root)?,
        timestamp: wire.timestamp.as_ref().map(from_wire_role).transpose()?,
        snapshot: wire.snapshot.as_ref().map(from_wire_role).transpose()?,
        targets: wire.targets.as_ref().map(from_wire_role).transpose()?,
        root_refresh: wire
            .root_refresh
            .as_ref()
            .map(from_wire_refresh)
            .transpose()?,
    };
    validate_generation(&record)?;
    Ok(record)
}

fn validate_anchor(record: &AnchorRecord) -> Result<(), UpdateError> {
    initial_admission::validate_app(&record.app_id)?;
    initial_admission::validate_sid(&record.owner_sid)?;
    Ok(())
}

fn validate_current(record: &CurrentRecord) -> Result<(), UpdateError> {
    if record.sequence == 0 {
        return Err(invalid("D-23 current sequence must be positive"));
    }
    Ok(())
}

fn validate_role(role: MetadataRoleReference, cap: u64) -> Result<(), UpdateError> {
    if !(1..=cap).contains(&role.length) || role.version == 0 {
        return Err(invalid(
            "D-23 role length or version is outside the D-21 profile",
        ));
    }
    Ok(())
}

fn validate_generation(record: &GenerationRecord) -> Result<(), UpdateError> {
    if record.sequence == 0 || (record.sequence == 1) != record.previous_generation_sha256.is_none()
    {
        return Err(invalid(
            "D-23 generation sequence and previous digest are inconsistent",
        ));
    }
    // Enrollment creates sequence one with recovery epoch zero. Only a later,
    // independently authorized recovery can advance that epoch.
    if record.sequence == 1 && record.recovery_epoch != 0 {
        return Err(invalid("D-23 genesis recovery epoch must be zero"));
    }
    // The data-only codec constrains the *integer representation*. The real
    // OS clock owner must still verify its supported UTC range before use.
    if record.utc_floor_millis > i64::MAX as u64 {
        return Err(invalid("D-23 UTC milliseconds exceed signed storage range"));
    }
    validate_role(record.root, ROOT_CAP)?;
    if let Some(role) = record.timestamp {
        validate_role(role, TIMESTAMP_CAP)?;
    }
    if let Some(role) = record.snapshot {
        validate_role(role, SNAPSHOT_CAP)?;
    }
    if let Some(role) = record.targets {
        validate_role(role, TARGETS_CAP)?;
    }
    match record.phase {
        GenerationPhase::RootRefresh if record.root_refresh.is_none() => {
            return Err(invalid("D-23 root-refresh phase needs key fingerprints"));
        }
        GenerationPhase::Roles | GenerationPhase::Ready if record.root_refresh.is_some() => {
            return Err(invalid(
                "D-23 rootRefresh belongs only to root-refresh phase",
            ));
        }
        GenerationPhase::Roles
            if (record.snapshot.is_some() && record.timestamp.is_none())
                || (record.targets.is_some() && record.snapshot.is_none()) =>
        {
            return Err(invalid(
                "D-23 roles phase requires ordered timestamp, snapshot and targets",
            ));
        }
        GenerationPhase::Ready
            if record.timestamp.is_none()
                || record.snapshot.is_none()
                || record.targets.is_none() =>
        {
            return Err(invalid("D-23 ready phase needs all four role references"));
        }
        GenerationPhase::RootRefresh | GenerationPhase::Roles | GenerationPhase::Ready => {}
    }
    Ok(())
}

fn wire_role(role: MetadataRoleReference) -> WireRoleReference {
    WireRoleReference {
        sha256: hex_digest(&role.sha256),
        length: role.length,
        version: role.version,
    }
}

fn from_wire_role(wire: &WireRoleReference) -> Result<MetadataRoleReference, UpdateError> {
    Ok(MetadataRoleReference {
        sha256: digest(&wire.sha256)?,
        length: wire.length,
        version: wire.version,
    })
}

fn wire_refresh(refresh: RootRefresh) -> WireRootRefresh {
    WireRootRefresh {
        timestamp_keys_sha256: hex_digest(&refresh.timestamp_keys_sha256),
        snapshot_keys_sha256: hex_digest(&refresh.snapshot_keys_sha256),
    }
}

fn from_wire_refresh(wire: &WireRootRefresh) -> Result<RootRefresh, UpdateError> {
    Ok(RootRefresh {
        timestamp_keys_sha256: digest(&wire.timestamp_keys_sha256)?,
        snapshot_keys_sha256: digest(&wire.snapshot_keys_sha256)?,
    })
}

fn encode_bounded<T: Serialize>(wire: &T, max: usize) -> Result<Vec<u8>, UpdateError> {
    let bytes = encode(wire)?;
    if bytes.len() > max {
        return Err(invalid("D-23 metadata record exceeds its fixed byte cap"));
    }
    Ok(bytes)
}

fn decode_bounded<T: serde::de::DeserializeOwned + Serialize>(
    bytes: &[u8],
    max: usize,
) -> Result<T, UpdateError> {
    if bytes.is_empty() || bytes.len() > max {
        return Err(invalid(
            "D-23 metadata record is empty or exceeds its fixed byte cap",
        ));
    }
    decode(bytes)
}

#[cfg(test)]
mod tests;
