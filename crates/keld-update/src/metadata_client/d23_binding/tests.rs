//! Independent negative controls for complete D-23 role-body references.
//! Synthetic records prove byte consistency only, never native protection.

use super::verify_ready_revision_against_recorded_generation;
use crate::metadata_client::{MetadataClientRefusal, SignedRoleBytes, TufRole};
use crate::records::{GenerationPhase, GenerationRecord, MetadataRoleReference, RootRefresh};
use sha2::{Digest as _, Sha256};

const ROOT: &[u8] = b"root-envelope";
const TIMESTAMP: &[u8] = b"timestamp-envelope";
const SNAPSHOT: &[u8] = b"snapshot-envelope";
const TARGETS: &[u8] = b"targets-envelope";

fn roles() -> SignedRoleBytes<'static> {
    SignedRoleBytes {
        root: ROOT,
        timestamp: TIMESTAMP,
        snapshot: SNAPSHOT,
        targets: TARGETS,
    }
}

fn reference(bytes: &[u8]) -> MetadataRoleReference {
    MetadataRoleReference {
        sha256: Sha256::digest(bytes).into(),
        length: u64::try_from(bytes.len()).unwrap(),
        version: 1,
    }
}

fn generation() -> GenerationRecord {
    GenerationRecord {
        repository_id: [0x42; 32],
        sequence: 1,
        recovery_epoch: 0,
        previous_generation_sha256: None,
        policy_sha256: [0x21; 32],
        utc_floor_millis: 1,
        phase: GenerationPhase::Ready,
        root: reference(ROOT),
        timestamp: Some(reference(TIMESTAMP)),
        snapshot: Some(reference(SNAPSHOT)),
        targets: Some(reference(TARGETS)),
        root_refresh: None,
    }
}

fn role_reference_mut(
    generation: &mut GenerationRecord,
    role: TufRole,
) -> &mut MetadataRoleReference {
    match role {
        TufRole::Root => &mut generation.root,
        TufRole::Timestamp => generation.timestamp.as_mut().unwrap(),
        TufRole::Snapshot => generation.snapshot.as_mut().unwrap(),
        TufRole::Targets => generation.targets.as_mut().unwrap(),
    }
}

fn refusal(roles: SignedRoleBytes<'_>, record: &GenerationRecord) -> MetadataClientRefusal {
    verify_ready_revision_against_recorded_generation(roles, record)
        .expect_err("a data-only D-23 match can never grant ReadyRevision")
}

#[test]
fn matching_recorded_bodies_still_refuse_unqualified_tuf_and_lease() {
    assert_eq!(
        refusal(roles(), &generation()),
        MetadataClientRefusal::TufClientNotQualified
    );
}

#[test]
fn incomplete_or_armed_generation_cannot_select_role_bodies() {
    let mut incomplete = generation();
    incomplete.phase = GenerationPhase::Roles;
    assert_eq!(
        refusal(roles(), &incomplete),
        MetadataClientRefusal::RecordedGenerationNotReady
    );

    let mut armed = generation();
    armed.root_refresh = Some(RootRefresh {
        timestamp_keys_sha256: [0x11; 32],
        snapshot_keys_sha256: [0x22; 32],
    });
    assert_eq!(
        refusal(roles(), &armed),
        MetadataClientRefusal::RecordedGenerationNotReady
    );
}

#[test]
fn missing_recorded_role_refuses_even_with_complete_input_bytes() {
    for role in [TufRole::Timestamp, TufRole::Snapshot, TufRole::Targets] {
        let mut record = generation();
        match role {
            TufRole::Timestamp => record.timestamp = None,
            TufRole::Snapshot => record.snapshot = None,
            TufRole::Targets => record.targets = None,
            TufRole::Root => unreachable!(),
        }
        assert_eq!(
            refusal(roles(), &record),
            MetadataClientRefusal::RecordedRoleMissing(role)
        );
    }
}

#[test]
fn complete_length_or_digest_substitution_refuses_per_role() {
    for role in [
        TufRole::Root,
        TufRole::Timestamp,
        TufRole::Snapshot,
        TufRole::Targets,
    ] {
        let mut record = generation();
        role_reference_mut(&mut record, role).length += 1;
        assert_eq!(
            refusal(roles(), &record),
            MetadataClientRefusal::RecordedRoleMismatch(role)
        );

        let mut record = generation();
        role_reference_mut(&mut record, role).sha256[0] ^= 1;
        assert_eq!(
            refusal(roles(), &record),
            MetadataClientRefusal::RecordedRoleMismatch(role)
        );
    }
}

#[test]
fn substituted_same_length_role_bytes_refuse_without_reaching_tuf_grant() {
    for role in [
        TufRole::Root,
        TufRole::Timestamp,
        TufRole::Snapshot,
        TufRole::Targets,
    ] {
        let mut substituted = roles();
        let altered = match role {
            TufRole::Root => &b"Root-envelope"[..],
            TufRole::Timestamp => &b"Timestamp-envelope"[..],
            TufRole::Snapshot => &b"Snapshot-envelope"[..],
            TufRole::Targets => &b"Targets-envelope"[..],
        };
        let original = match role {
            TufRole::Root => ROOT,
            TufRole::Timestamp => TIMESTAMP,
            TufRole::Snapshot => SNAPSHOT,
            TufRole::Targets => TARGETS,
        };
        assert_eq!(altered.len(), original.len(), "hash-only substitution");
        match role {
            TufRole::Root => substituted.root = altered,
            TufRole::Timestamp => substituted.timestamp = altered,
            TufRole::Snapshot => substituted.snapshot = altered,
            TufRole::Targets => substituted.targets = altered,
        }
        assert_eq!(
            refusal(substituted, &generation()),
            MetadataClientRefusal::RecordedRoleMismatch(role)
        );
    }
}
