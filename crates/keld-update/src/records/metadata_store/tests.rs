//! D-23 exact canonical-byte evidence, never protected-store authentication.
//!
//! Each family has independent byte oracles. No test opens a repository,
//! acquires an installation lease, or synthesizes a trusted TUF role.

use super::{
    AnchorRecord, CurrentRecord, EnrollmentState, GenerationPhase, GenerationRecord,
    MetadataRoleReference, RootRefresh, UpdateError,
};

mod anchor_current;
mod generation;

const APP: &str = "dev.keld.fixture";
const SID: &str = "S-1-5-21-100-200-300-1001";

fn anchor() -> AnchorRecord {
    AnchorRecord {
        repository_id: [0x11; 32],
        app_id: APP.to_owned(),
        owner_sid: SID.to_owned(),
        enrollment_state: EnrollmentState::Pending,
        initial_generation_sha256: [0x22; 32],
    }
}

fn current() -> CurrentRecord {
    CurrentRecord {
        repository_id: [0x11; 32],
        sequence: 1,
        generation_sha256: [0x33; 32],
    }
}

fn role(digest: u8, length: u64, version: u64) -> MetadataRoleReference {
    MetadataRoleReference {
        sha256: [digest; 32],
        length,
        version,
    }
}

fn generation() -> GenerationRecord {
    GenerationRecord {
        repository_id: [0x11; 32],
        sequence: 1,
        recovery_epoch: 0,
        previous_generation_sha256: None,
        policy_sha256: [0x44; 32],
        utc_floor_millis: 1_700_000_000_000,
        phase: GenerationPhase::Ready,
        root: role(0x55, 1_024, 1),
        timestamp: Some(role(0x66, 512, 2)),
        snapshot: Some(role(0x77, 4_096, 3)),
        targets: Some(role(0x88, 65_536, 4)),
        root_refresh: None,
    }
}

fn refusal<T: std::fmt::Debug>(result: Result<T, UpdateError>) -> String {
    match result.expect_err("closed D23 codec must refuse") {
        UpdateError::LocalRecordInvalid { detail } => detail,
        other => panic!("expected KELD-UPDATE-014, got {other:?}"),
    }
}

fn refresh() -> RootRefresh {
    RootRefresh {
        timestamp_keys_sha256: [0x91; 32],
        snapshot_keys_sha256: [0x92; 32],
    }
}
