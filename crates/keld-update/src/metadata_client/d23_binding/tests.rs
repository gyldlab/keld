//! Independent negative controls for complete D-23 role-body references.
//! Synthetic records prove byte consistency only, never native protection.

use super::{
    verify_ready_revision_against_recorded_generation,
    verify_ready_revision_against_recorded_readers,
};
use crate::metadata_client::{MetadataClientRefusal, SignedRoleBytes, SignedRoleReaders, TufRole};
use crate::records::{GenerationPhase, GenerationRecord, MetadataRoleReference, RootRefresh};
use sha2::{Digest as _, Sha256};
use std::io::{self, Cursor, Read};

const ROOT: &[u8] = b"root-envelope";
const TIMESTAMP: &[u8] = b"timestamp-envelope";
const SNAPSHOT: &[u8] = b"snapshot-envelope";
const TARGETS: &[u8] = b"targets-envelope";

struct FailOnRead;

impl Read for FailOnRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("negative D-23 preflight reached a forbidden role reader")
    }
}

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

// A D-23 root mismatch must stop the reader boundary before consuming later
// untrusted sources. A per-role SHA-256 check after reading every role is too
// late to enforce this negative ordering contract.
#[test]
fn recorded_reader_root_mismatch_refuses_before_downstream_reads() {
    let mut wrong_root = Cursor::new(&b"Root-envelope"[..]);
    let mut timestamp = FailOnRead;
    let mut snapshot = FailOnRead;
    let mut targets = FailOnRead;
    assert_eq!(
        verify_ready_revision_against_recorded_readers(
            SignedRoleReaders {
                root: &mut wrong_root,
                timestamp: &mut timestamp,
                snapshot: &mut snapshot,
                targets: &mut targets,
            },
            &generation(),
        )
        .expect_err("different same-length root bytes must stop role intake"),
        MetadataClientRefusal::RecordedRoleMismatch(TufRole::Root)
    );
}

#[test]
fn incomplete_recorded_graph_refuses_before_any_role_read() {
    let mut record = generation();
    record.targets = None;
    let mut root = FailOnRead;
    let mut timestamp = FailOnRead;
    let mut snapshot = FailOnRead;
    let mut targets = FailOnRead;
    assert_eq!(
        verify_ready_revision_against_recorded_readers(
            SignedRoleReaders {
                root: &mut root,
                timestamp: &mut timestamp,
                snapshot: &mut snapshot,
                targets: &mut targets,
            },
            &record,
        )
        .expect_err("incomplete recorded graph must not trigger intake"),
        MetadataClientRefusal::RecordedRoleMissing(TufRole::Targets)
    );
}

#[test]
fn impossible_recorded_role_lengths_refuse_before_any_role_read() {
    // Independent D-21 caps, not derived from the implementation under test.
    // A fabricated zero or over-cap recorded length cannot cause even the
    // Root reader to run, regardless of which role carries the bad reference.
    for (role, max_bytes) in [
        (TufRole::Root, 65_536_u64),
        (TufRole::Timestamp, 16_384_u64),
        (TufRole::Snapshot, 262_144_u64),
        (TufRole::Targets, 1_048_576_u64),
    ] {
        for invalid_length in [0, max_bytes + 1, u64::MAX] {
            let mut record = generation();
            role_reference_mut(&mut record, role).length = invalid_length;
            let mut root = FailOnRead;
            let mut timestamp = FailOnRead;
            let mut snapshot = FailOnRead;
            let mut targets = FailOnRead;
            assert_eq!(
                verify_ready_revision_against_recorded_readers(
                    SignedRoleReaders {
                        root: &mut root,
                        timestamp: &mut timestamp,
                        snapshot: &mut snapshot,
                        targets: &mut targets,
                    },
                    &record,
                )
                .expect_err("impossible recorded length must refuse before any role I/O"),
                MetadataClientRefusal::RecordedRoleMismatch(role),
                "role {role:?} recorded length {invalid_length} reached an untrusted reader"
            );
        }
    }
}

#[test]
fn matching_recorded_readers_still_cannot_mint_ready_revision() {
    let mut root = Cursor::new(ROOT);
    let mut timestamp = Cursor::new(TIMESTAMP);
    let mut snapshot = Cursor::new(SNAPSHOT);
    let mut targets = Cursor::new(TARGETS);
    assert_eq!(
        verify_ready_revision_against_recorded_readers(
            SignedRoleReaders {
                root: &mut root,
                timestamp: &mut timestamp,
                snapshot: &mut snapshot,
                targets: &mut targets,
            },
            &generation(),
        )
        .expect_err("matching data remains unqualified without TUF and T-82"),
        MetadataClientRefusal::TufClientNotQualified
    );
}
