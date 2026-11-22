//! Negative-only T-20/D-23 role-body consistency at the existing updater boundary.
//!
//! A D-23 generation is data until its retained native handles, original KEL-53
//! repository lease and D-90 census have been authenticated. A matching digest
//! is not TUF signature, expiry, version-floor, signed-content or source proof.
//! This module does no filesystem I/O, parsing, key selection or publication.

use sha2::{Digest as _, Sha256};

use super::{
    MetadataClientRefusal, ReadyRevision, SignedRoleBytes, SignedRoleReaders, TufRole,
    read_complete_role, require_role_bounds,
};
use crate::records::{GenerationPhase, GenerationRecord, MetadataRoleReference};

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
        let recorded = reference
            .as_ref()
            .ok_or(MetadataClientRefusal::RecordedRoleMissing(*role))?;
        let length = usize::try_from(recorded.length)
            .map_err(|_| MetadataClientRefusal::RecordedRoleMismatch(*role))?;
        if length == 0 || length > role.max_bytes() {
            return Err(MetadataClientRefusal::RecordedRoleMismatch(*role));
        }
    }
    for (role, reader, reference) in roles {
        let body = read_complete_role(role, reader)?;
        require_recorded_body(role, &body, reference)?;
    }
    Err(MetadataClientRefusal::TufClientNotQualified)
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
