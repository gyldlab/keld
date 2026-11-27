//! Negative-only T-20/D-23 role-body consistency at the existing updater boundary.
//!
//! A D-23 generation is data until its retained native handles, original KEL-53
//! repository lease and D-90 census have been authenticated. A matching digest
//! is not TUF signature, expiry, version-floor, signed-content or source proof.
//! This module decodes canonical D-23 data only. It does no filesystem I/O,
//! TUF signature parsing, key selection or publication.

use sha2::{Digest as _, Sha256};

use super::{
    MetadataClientRefusal, ReadyRevision, SignedRoleBytes, SignedRoleReaders, TufRole,
    read_complete_role, require_role_bounds,
};
use crate::records::{
    CurrentRecord, GenerationPhase, GenerationRecord, MetadataRoleReference,
    decode_metadata_generation,
};

/// Data-only input for the pinned tough R5 historical-floor loader.
///
/// The two buffers preserve the original stored signed *envelopes*, including
/// their original signatures, even when a new root raises its threshold.
/// Recorded versions and UTC are expectations, NOT verified TUF role facts.
/// Only the future qualified client may compare them with authenticated roles.
/// These values prove neither native D-23 custody nor D-90 writer ownership.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct UnqualifiedD23HistoricalRoles {
    pub(super) timestamp: Vec<u8>,
    pub(super) snapshot: Vec<u8>,
    pub(super) recorded_versions: [u64; 4],
    pub(super) utc_floor_millis: u64,
}

/// Bind the exact stored D-23 current and canonical generation bytes before
/// copying old timestamp/snapshot envelopes for tough's historical floor checks.
///
/// The existing KEL-53 native owner must supply these from retained protected
/// handles while holding the SAME qualified D-90 repository lease. Caller
/// supplied bytes cannot establish that ownership. This method never calls
/// tough, opens a path, changes a record, or constructs `ReadyRevision`.
/// R5 must subsequently verify current-root role authority and signed versions;
/// an old envelope that fails the new threshold is floor-only.
pub(super) fn read_unqualified_d23_historical_roles(
    current: &CurrentRecord,
    generation_bytes: &[u8],
    timestamp_reader: &mut dyn std::io::Read,
    snapshot_reader: &mut dyn std::io::Read,
) -> Result<UnqualifiedD23HistoricalRoles, MetadataClientRefusal> {
    let generation = decode_metadata_generation(generation_bytes)
        .map_err(|_| MetadataClientRefusal::RecordedGenerationMismatch)?;
    if current.repository_id != generation.repository_id
        || current.sequence != generation.sequence
        || Sha256::digest(generation_bytes)[..] != current.generation_sha256[..]
    {
        return Err(MetadataClientRefusal::RecordedGenerationMismatch);
    }
    if generation.phase != GenerationPhase::Ready || generation.root_refresh.is_some() {
        return Err(MetadataClientRefusal::RecordedGenerationNotReady);
    }
    // A partial Ready graph or impossible reference must refuse before either
    // reader is touched. This makes a stored version floor independent of
    // caller-driven stream order, but is not signature authentication.
    let root = require_recorded_reference(TufRole::Root, Some(generation.root))?;
    let timestamp_ref = require_recorded_reference(TufRole::Timestamp, generation.timestamp)?;
    let snapshot_ref = require_recorded_reference(TufRole::Snapshot, generation.snapshot)?;
    let targets_ref = require_recorded_reference(TufRole::Targets, generation.targets)?;

    let timestamp = read_complete_role(TufRole::Timestamp, timestamp_reader)?;
    require_recorded_body(TufRole::Timestamp, &timestamp, Some(timestamp_ref))?;
    let snapshot = read_complete_role(TufRole::Snapshot, snapshot_reader)?;
    require_recorded_body(TufRole::Snapshot, &snapshot, Some(snapshot_ref))?;
    Ok(UnqualifiedD23HistoricalRoles {
        timestamp,
        snapshot,
        recorded_versions: [
            root.version,
            timestamp_ref.version,
            snapshot_ref.version,
            targets_ref.version,
        ],
        utc_floor_millis: generation.utc_floor_millis,
    })
}

/// Check complete role bodies against the four exact D-23 recorded references.
///
/// Even if every length and SHA-256 match, unqualified T-20 cannot mint a
/// `ReadyRevision`. The actual retained D-23 owner must authenticate the
/// generation and historical floors and hold the same D-90 lease while the
/// reviewed pinned TUF client verifies signatures and current freshness.
pub(super) fn verify_ready_revision_against_recorded_generation(
    roles: SignedRoleBytes<'_>,
    generation: &GenerationRecord,
) -> Result<ReadyRevision, MetadataClientRefusal> {
    require_role_bounds(roles)?;
    if generation.phase != GenerationPhase::Ready || generation.root_refresh.is_some() {
        return Err(MetadataClientRefusal::RecordedGenerationNotReady);
    }
    for (role, body, reference) in [
        (TufRole::Root, roles.root, Some(generation.root)),
        (TufRole::Timestamp, roles.timestamp, generation.timestamp),
        (TufRole::Snapshot, roles.snapshot, generation.snapshot),
        (TufRole::Targets, roles.targets, generation.targets),
    ] {
        require_recorded_body(role, body, reference)?;
    }
    Err(MetadataClientRefusal::TufClientNotQualified)
}

/// Refuse an incomplete D-23 graph or impossible recorded role length before
/// reading any role, then bind each complete bounded body to its recorded
/// length and digest before reading the next role.
/// A reader may already own an acquired source; this only orders `Read` calls.
/// Even four matching bodies never create TUF or repository authority.
pub(super) fn verify_ready_revision_against_recorded_readers(
    readers: SignedRoleReaders<'_>,
    generation: &GenerationRecord,
) -> Result<ReadyRevision, MetadataClientRefusal> {
    if generation.phase != GenerationPhase::Ready || generation.root_refresh.is_some() {
        return Err(MetadataClientRefusal::RecordedGenerationNotReady);
    }
    let roles = [
        (TufRole::Root, readers.root, Some(generation.root)),
        (TufRole::Timestamp, readers.timestamp, generation.timestamp),
        (TufRole::Snapshot, readers.snapshot, generation.snapshot),
        (TufRole::Targets, readers.targets, generation.targets),
    ];
    for (role, _, reference) in &roles {
        require_recorded_reference(*role, *reference)?;
    }
    for (role, reader, reference) in roles {
        let body = read_complete_role(role, reader)?;
        require_recorded_body(role, &body, reference)?;
    }
    Err(MetadataClientRefusal::TufClientNotQualified)
}

fn require_recorded_reference(
    role: TufRole,
    reference: Option<MetadataRoleReference>,
) -> Result<MetadataRoleReference, MetadataClientRefusal> {
    let recorded = reference.ok_or(MetadataClientRefusal::RecordedRoleMissing(role))?;
    let length = usize::try_from(recorded.length)
        .map_err(|_| MetadataClientRefusal::RecordedRoleMismatch(role))?;
    if length == 0 || length > role.max_bytes() || recorded.version == 0 {
        return Err(MetadataClientRefusal::RecordedRoleMismatch(role));
    }
    Ok(recorded)
}

fn require_recorded_body(
    role: TufRole,
    body: &[u8],
    reference: Option<MetadataRoleReference>,
) -> Result<(), MetadataClientRefusal> {
    let reference = reference.ok_or(MetadataClientRefusal::RecordedRoleMissing(role))?;
    if u64::try_from(body.len()).ok() != Some(reference.length)
        || Sha256::digest(body)[..] != reference.sha256[..]
    {
        return Err(MetadataClientRefusal::RecordedRoleMismatch(role));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
