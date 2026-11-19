use super::{Election, acquire_at};
use crate::SingleInstanceDeclaration;
use crate::app_session::HostAppError;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::wrappers::SetNamedSecurityInfo;

pub(in crate::app_session) fn fixture() -> (TempDir, SingleInstanceDeclaration) {
    let root = tempfile::tempdir().expect("disposable election root");
    let descriptor = keld_guard::windows_owner_private_directory_security()
        .expect("existing current-user directory policy");
    SetNamedSecurityInfo(
        root.path(),
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        descriptor.dacl(),
        None,
    )
    .expect("apply the private DACL to the fresh fixture root");
    let declaration = serde_json::from_str(r#"{"id":"con.example"}"#).expect("canonical ID");
    (root, declaration)
}

pub(in crate::app_session) fn elect_at_for_test(
    root: &Path,
    declaration: &SingleInstanceDeclaration,
) -> Result<Election, HostAppError> {
    acquire_at(root, declaration)
}

#[test]
fn the_acquired_outcome_owns_the_same_lease_until_it_drops() {
    let (root, declaration) = fixture();
    let winner = acquire_at(root.path(), &declaration).expect("first election");
    let Election::Acquired(lease) = winner else {
        panic!("the fresh lease must be acquired");
    };
    lease
        .validate()
        .expect("winner retains the admitted object");

    let contender = acquire_at(root.path(), &declaration).expect("second election");
    assert!(matches!(contender, Election::Contended));
    let leaf = root
        .path()
        .join("Keld/InstanceLocks/v1/id-con.example/lease");
    assert_eq!(
        fs::rename(&leaf, leaf.with_extension("moved"))
            .expect_err("the retained winner prevents rename")
            .raw_os_error(),
        Some(32)
    );
    drop(lease);

    let successor = acquire_at(root.path(), &declaration).expect("election after release");
    assert!(matches!(successor, Election::Acquired(_)));
    assert!(
        leaf.is_file(),
        "the persistent leaf survives holder release"
    );
}

#[test]
fn invalid_lease_identity_fails_without_a_contention_verdict() {
    let (root, declaration) = fixture();
    drop(acquire_at(root.path(), &declaration).expect("create the real election leaf"));
    let leaf = root
        .path()
        .join("Keld/InstanceLocks/v1/id-con.example/lease");
    fs::hard_link(&leaf, root.path().join("alias")).expect("add a second link to the lease");

    let error = acquire_at(root.path(), &declaration).expect_err("an invalid leaf has no verdict");
    assert_eq!(error.code, "KELD-CORE-037");
    assert_eq!(error.phase, "single-instance lease admission");
    assert!(error.detail.contains("verify identity of lease"), "{error}");
}
