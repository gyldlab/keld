//! Independent negative controls for complete D-23 role-body references.
//! Synthetic records prove byte consistency only, never native protection.

use super::{
    read_unqualified_d23_historical_roles, verify_ready_revision_against_recorded_generation,
    verify_ready_revision_against_recorded_readers,
};
use crate::metadata_client::{MetadataClientRefusal, SignedRoleBytes, SignedRoleReaders, TufRole};
use crate::records::{
    CurrentRecord, GenerationPhase, GenerationRecord, MetadataRoleReference, RootRefresh,
    encode_metadata_generation,
};
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

fn canonical_current(generation: &GenerationRecord) -> (CurrentRecord, Vec<u8>) {
    let bytes = encode_metadata_generation(generation)
        .expect("the D-23 fixture must be an exact canonical generation");
    (
        CurrentRecord {
            repository_id: generation.repository_id,
            sequence: generation.sequence,
            generation_sha256: Sha256::digest(&bytes).into(),
        },
        bytes,
    )
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

#[test]
fn old_d23_historical_envelopes_preserve_exact_bytes_for_real_tuf_floor_checks() {
    let mut prior = generation();
    prior.root.version = 100;
    prior.timestamp.as_mut().unwrap().version = 100;
    prior.snapshot.as_mut().unwrap().version = 100;
    prior.targets.as_mut().unwrap().version = 100;
    prior.utc_floor_millis = 1_700_000_000_000;
    let (current, serialized) = canonical_current(&prior);

    let captured = read_unqualified_d23_historical_roles(
        &current,
        &serialized,
        &mut Cursor::new(TIMESTAMP),
        &mut Cursor::new(SNAPSHOT),
    )
    .expect("exact D-23 references copy old opaque signed envelopes as data");
    assert_eq!(captured.timestamp.as_slice(), TIMESTAMP);
    assert_eq!(captured.snapshot.as_slice(), SNAPSHOT);
    assert_eq!(captured.recorded_versions, [100; 4]);
    assert_eq!(captured.utc_floor_millis, 1_700_000_000_000);
    // A successful copy never authenticates an Ed25519 TUF signature, source
    // root, protected OS handle, D-90 lease or ReadyRevision. The real pinned
    // tough R5 test independently authenticates signed v100 -> v101 history.
}

#[test]
fn forged_current_digest_repository_or_sequence_refuses_before_old_role_reads() {
    let (current, serialized) = canonical_current(&generation());
    let mut mismatches = Vec::new();
    let mut wrong_hash = current.clone();
    wrong_hash.generation_sha256[0] ^= 1;
    mismatches.push(wrong_hash);
    let mut wrong_repository = current.clone();
    wrong_repository.repository_id[0] ^= 1;
    mismatches.push(wrong_repository);
    let mut wrong_sequence = current;
    wrong_sequence.sequence += 1;
    mismatches.push(wrong_sequence);

    for candidate in mismatches {
        let mut timestamp = FailOnRead;
        let mut snapshot = FailOnRead;
        assert_eq!(
            read_unqualified_d23_historical_roles(
                &candidate,
                &serialized,
                &mut timestamp,
                &mut snapshot
            )
            .unwrap_err(),
            MetadataClientRefusal::RecordedGenerationMismatch
        );
    }
    let (current, mut malformed) = canonical_current(&generation());
    malformed[0] ^= 1;
    let mut timestamp = FailOnRead;
    let mut snapshot = FailOnRead;
    assert_eq!(
        read_unqualified_d23_historical_roles(&current, &malformed, &mut timestamp, &mut snapshot)
            .unwrap_err(),
        MetadataClientRefusal::RecordedGenerationMismatch
    );
}

#[test]
fn d23_historical_timestamp_mismatch_never_reads_snapshot() {
    let (current, serialized) = canonical_current(&generation());
    let mut altered_timestamp = Cursor::new(&b"Timestamp-envelope"[..]);
    let mut forbidden_snapshot = FailOnRead;
    assert_eq!(
        read_unqualified_d23_historical_roles(
            &current,
            &serialized,
            &mut altered_timestamp,
            &mut forbidden_snapshot
        )
        .unwrap_err(),
        MetadataClientRefusal::RecordedRoleMismatch(TufRole::Timestamp)
    );
}

#[test]
fn d23_historical_snapshot_substitution_refuses_after_authentic_timestamp_bytes() {
    let (current, serialized) = canonical_current(&generation());
    assert_eq!(
        read_unqualified_d23_historical_roles(
            &current,
            &serialized,
            &mut Cursor::new(TIMESTAMP),
            &mut Cursor::new(&b"Snapshot-envelope"[..])
        )
        .unwrap_err(),
        MetadataClientRefusal::RecordedRoleMismatch(TufRole::Snapshot)
    );
}

#[test]
fn d23_nonready_history_refuses_before_old_role_reads() {
    let mut incomplete = generation();
    incomplete.phase = GenerationPhase::Roles;
    let (current, serialized) = canonical_current(&incomplete);
    let mut timestamp = FailOnRead;
    let mut snapshot = FailOnRead;
    assert_eq!(
        read_unqualified_d23_historical_roles(&current, &serialized, &mut timestamp, &mut snapshot)
            .unwrap_err(),
        MetadataClientRefusal::RecordedGenerationNotReady
    );
}

// Real scratch files, closed after writing and reopened for each observation.
// This exercises the existing canonical codec and bounded historical reader,
// not the protected native owner or a power-loss/restart guarantee. AC28's
// signed tough candidate (878a64e993) separately owns signature verification.
#[cfg(windows)]
#[test]
fn persisted_d23_history_reopen_detects_corruption_and_never_grants_authority() {
    use crate::records::{decode_metadata_current, encode_metadata_current};
    use std::fs::{self, File};
    use std::io::Write;

    let scratch = tempfile::tempdir().unwrap();
    let mut prior = generation();
    prior.utc_floor_millis = 1_700_000_000_000;
    let (current, generation_bytes) = canonical_current(&prior);
    let current_bytes = encode_metadata_current(&current).unwrap();
    let current_path = scratch.path().join("current.json");
    let generation_path = scratch.path().join("generation.json");
    let timestamp_path = scratch.path().join("timestamp.json");
    let snapshot_path = scratch.path().join("snapshot.json");
    for (path, bytes) in [
        (&current_path, current_bytes.as_slice()),
        (&generation_path, generation_bytes.as_slice()),
        (&timestamp_path, TIMESTAMP),
        (&snapshot_path, SNAPSHOT),
    ] {
        let mut file = File::create(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }
    let reopen = || {
        let current = decode_metadata_current(&fs::read(&current_path).unwrap())
            .map_err(|_| MetadataClientRefusal::RecordedGenerationMismatch)?;
        read_unqualified_d23_historical_roles(
            &current,
            &fs::read(&generation_path).unwrap(),
            &mut File::open(&timestamp_path).unwrap(),
            &mut File::open(&snapshot_path).unwrap(),
        )
    };
    let history = reopen().unwrap();
    assert_eq!(history.timestamp, TIMESTAMP);
    assert_eq!(history.snapshot, SNAPSHOT);
    assert_eq!(history.utc_floor_millis, prior.utc_floor_millis);
    // No same qualified D90 lease or authenticated TUF root exists in this
    // fixture. Even matching bytes must still refuse the actual closed intake.
    assert_eq!(
        verify_ready_revision_against_recorded_generation(roles(), &prior).unwrap_err(),
        MetadataClientRefusal::TufClientNotQualified
    );
    fs::write(&timestamp_path, b"Timestamp-envelope").unwrap();
    assert_eq!(
        reopen().unwrap_err(),
        MetadataClientRefusal::RecordedRoleMismatch(TufRole::Timestamp)
    );
    fs::write(&timestamp_path, TIMESTAMP).unwrap();
    fs::write(&snapshot_path, b"Snapshot-envelope").unwrap();
    assert_eq!(
        reopen().unwrap_err(),
        MetadataClientRefusal::RecordedRoleMismatch(TufRole::Snapshot)
    );
    fs::write(&snapshot_path, SNAPSHOT).unwrap();
    let mut altered = current.clone();
    altered.generation_sha256[0] ^= 1;
    fs::write(&current_path, encode_metadata_current(&altered).unwrap()).unwrap();
    assert_eq!(
        reopen().unwrap_err(),
        MetadataClientRefusal::RecordedGenerationMismatch
    );
    fs::write(&current_path, &current_bytes).unwrap();
    fs::write(
        &generation_path,
        &generation_bytes[..generation_bytes.len() - 1],
    )
    .unwrap();
    assert_eq!(
        reopen().unwrap_err(),
        MetadataClientRefusal::RecordedGenerationMismatch
    );
    fs::write(&generation_path, &generation_bytes).unwrap();
    assert_eq!(reopen().unwrap(), history);
}
