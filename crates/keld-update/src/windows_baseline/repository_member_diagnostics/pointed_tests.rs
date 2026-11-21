//! Real retained-directory negative controls for D-90's D-68 pointed-operation shape.
//! These checks never assert that an empty local scan establishes full membership.

use std::fs;
use std::sync::mpsc;
use std::thread;

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::require_only_pointed_repository_member;
use crate::UpdateError;
use crate::records::{PendingOperationKind, PendingRepositoryOperation, RepositoryAnchor};

fn owner() -> RepositoryAnchor {
    RepositoryAnchor {
        repository_id: [0x7f; 32],
        app_id: "com.gyldlab.d90-tests".to_owned(),
        owner_sid: "S-1-5-21-500-600-700-1001".to_owned(),
    }
}

fn pointer(owner: &RepositoryAnchor, kind: PendingOperationKind) -> PendingRepositoryOperation {
    PendingRepositoryOperation {
        repository_id: owner.repository_id,
        app_id: owner.app_id.clone(),
        owner_sid: owner.owner_sid.clone(),
        kind,
        attempt_id: [0x2a; 32],
    }
}

fn retained(path: &std::path::Path) -> Dir {
    Dir::open_ambient_dir(path, ambient_authority()).expect("retained real Windows parent")
}

fn fixture() -> (tempfile::TempDir, Dir, Dir, Dir) {
    let root = tempfile::tempdir().expect("isolated native repository fixture");
    for name in ["admissions", "recoveries", "transitions"] {
        fs::create_dir(root.path().join(name)).expect("create one operation namespace");
    }
    let admissions = retained(&root.path().join("admissions"));
    let recoveries = retained(&root.path().join("recoveries"));
    let transitions = retained(&root.path().join("transitions"));
    (root, admissions, recoveries, transitions)
}

fn refusal_step(
    admissions: &Dir,
    recoveries: &Dir,
    transitions: &Dir,
    owner: &RepositoryAnchor,
    pending: Option<&PendingRepositoryOperation>,
) -> &'static str {
    match require_only_pointed_repository_member(
        admissions,
        recoveries,
        transitions,
        owner,
        pending,
    )
    .expect_err("unaccounted operation membership must refuse")
    {
        UpdateError::Baseline { step, .. } => step,
        other => panic!("unexpected D-90 error: {other}"),
    }
}

#[test]
fn pointed_admission_must_be_the_only_local_operation_member() {
    let (root, admissions, recoveries, transitions) = fixture();
    let owner = owner();
    let pending = pointer(&owner, PendingOperationKind::InitialAdmission);
    let expected = root
        .path()
        .join("admissions")
        .join(format!("{}.json", "2a".repeat(32)));
    assert_eq!(
        refusal_step(
            &admissions,
            &recoveries,
            &transitions,
            &owner,
            Some(&pending)
        ),
        "repository pointed membership",
        "pointer with no native journal must refuse"
    );
    fs::write(&expected, b"pointed").expect("create pointed native leaf");
    require_only_pointed_repository_member(
        &admissions,
        &recoveries,
        &transitions,
        &owner,
        Some(&pending),
    )
    .expect("one pointed name is a local shape observation only");
    let orphan = root
        .path()
        .join("admissions")
        .join(format!("{}.json", "4d".repeat(32)));
    fs::write(&orphan, b"orphan-admitted-before-pointer")
        .expect("create independent unpointed attempt");
    assert_eq!(
        refusal_step(
            &admissions,
            &recoveries,
            &transitions,
            &owner,
            Some(&pending)
        ),
        "repository member namespace"
    );
    assert_eq!(fs::read(&expected).expect("pointed preserved"), b"pointed");
    assert_eq!(
        fs::read(&orphan).expect("unpointed preserved"),
        b"orphan-admitted-before-pointer"
    );
}

#[test]
fn cross_kind_or_partial_member_blocks_even_with_matching_pointer() {
    let (root, admissions, recoveries, transitions) = fixture();
    let owner = owner();
    let pending = pointer(&owner, PendingOperationKind::InitialAdmission);
    fs::write(
        root.path()
            .join("admissions")
            .join(format!("{}.json", "2a".repeat(32))),
        b"pointed",
    )
    .expect("create pointed admission");
    let foreign = root.path().join("recoveries").join("pending-unfinished");
    fs::write(&foreign, b"prepared").expect("create orphan recovery member");
    assert_eq!(
        refusal_step(
            &admissions,
            &recoveries,
            &transitions,
            &owner,
            Some(&pending)
        ),
        "repository member namespace"
    );
    assert_eq!(fs::read(&foreign).expect("foreign retained"), b"prepared");
}

#[test]
fn missing_pointer_and_foreign_identity_fail_before_any_recovery() {
    let (root, admissions, recoveries, transitions) = fixture();
    let owner = owner();
    require_only_pointed_repository_member(&admissions, &recoveries, &transitions, &owner, None)
        .expect("empty local directories remain an unqualified observation");
    let foreign_leaf = root.path().join("transitions").join("unknown.json");
    fs::write(&foreign_leaf, b"unpointed-transition").expect("create foreign unpointed transition");
    assert_eq!(
        refusal_step(&admissions, &recoveries, &transitions, &owner, None),
        "repository member namespace"
    );
    let mut pending = pointer(&owner, PendingOperationKind::PlannedTransition);
    pending.owner_sid.push_str("-another");
    assert_eq!(
        refusal_step(
            &admissions,
            &recoveries,
            &transitions,
            &owner,
            Some(&pending)
        ),
        "repository member identity",
        "a pointer for a foreign owner must never select a journal"
    );
}

#[test]
fn concurrent_sibling_after_unlocked_observation_refuses_on_recheck() {
    let (root, admissions, recoveries, transitions) = fixture();
    let owner = owner();
    let pending = pointer(&owner, PendingOperationKind::ManualRecovery);
    fs::write(
        root.path()
            .join("recoveries")
            .join(format!("{}.json", "2a".repeat(32))),
        b"pointed",
    )
    .expect("create pointed manual recovery");
    require_only_pointed_repository_member(
        &admissions,
        &recoveries,
        &transitions,
        &owner,
        Some(&pending),
    )
    .expect("initial local observation while no concurrent member exists");
    let sibling = root
        .path()
        .join("admissions")
        .join(format!("{}.json", "5c".repeat(32)));
    let writer_sibling = sibling.clone();
    let (release, begin) = mpsc::channel();
    let writer = thread::spawn(move || {
        begin.recv().expect("wait for observed boundary");
        fs::write(&writer_sibling, b"orphan").expect("race another admitted writer");
    });
    release.send(()).expect("start competing writer");
    writer.join().expect("competing writer finished");
    assert_eq!(
        refusal_step(
            &admissions,
            &recoveries,
            &transitions,
            &owner,
            Some(&pending)
        ),
        "repository member namespace"
    );
    assert!(
        sibling.is_file(),
        "the refuser must preserve other owner evidence"
    );
    // A local recheck detects this race but cannot stop it. Only the missing
    // KEL-53 repository-wide writer lease can serialize acquisition and launch.
}
