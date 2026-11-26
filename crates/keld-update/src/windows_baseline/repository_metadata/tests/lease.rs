//! Real Windows native source-file lease tests; no D90 issuer.

use super::super::{ObjectIdentity, RepositoryWriterLease};
use crate::UpdateError;
use crate::records::RepositoryAnchor;
use cap_std::fs::Dir;
use std::fs;

/// A real protected D-44-style directory. This test owns the provisioning:
/// no production code may create the proposed persistent repository lease.
fn lease_fixture() -> (tempfile::TempDir, Dir) {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let fixture = tempfile::tempdir().expect("real isolated Windows lease fixture");
    let parent = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(fixture.path())
        .expect("retain the real native test parent");
    let profile = keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;
    let repository =
        crate::windows_fs::create_directory_relative_with_profile(&parent, "repository", profile)
            .expect("create a protected test repository through the reviewed native owner");
    (fixture, Dir::from_std_file(repository))
}

fn lease_anchor() -> RepositoryAnchor {
    RepositoryAnchor {
        repository_id: [0x42; 32],
        app_id: "com.gyldlab.lease-negative".to_owned(),
        owner_sid: "S-1-5-21-123-456-789-1001".to_owned(),
    }
}

fn provision_test_lock(repository: &Dir) {
    let handle = crate::windows_fs::create_file_relative_with_profile(
        &repository
            .try_clone()
            .expect("clone native parent")
            .into_std_file(),
        "repository.lock",
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    )
    .expect("test fixture owns separate create-new before any lease");
    handle.sync_all().expect("flush the test-owned empty file");
}

#[test]
fn existing_repository_lease_is_one_real_file_handle_and_never_creates() {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
        FILE_WRITE_DATA,
    };

    let (root, retained) = lease_fixture();
    let identity = lease_anchor();
    let missing = RepositoryWriterLease::open_existing(&retained, &identity)
        .expect_err("an absent repository lock must not self-provision");
    assert!(matches!(
        missing,
        UpdateError::Baseline {
            step: "repository metadata lease",
            ..
        }
    ));
    assert!(
        !root.path().join("repository/repository.lock").exists(),
        "refusal leaves no new persistent writer object"
    );

    provision_test_lock(&retained);
    let lease = RepositoryWriterLease::open_existing(&retained, &identity)
        .expect("bind one real protected lock handle");
    let lock_path = root.path().join("repository/repository.lock");
    let refused_data_writer = fs::OpenOptions::new()
        .access_mode(FILE_WRITE_DATA)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(&lock_path)
        .expect_err("a held exclusive file lease must deny another data writer");
    assert_eq!(
        refused_data_writer.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION.cast_signed()),
        "the OS, not a synthetic mutex, must refuse data-write access"
    );
    // CreateFileW sharing does not cover FILE_WRITE_ATTRIBUTES. The current
    // owner cannot claim atomic reparse metadata immutability from share=0.
    let attributes_only = fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(&lock_path)
        .expect("attributes-only access remains possible despite a data lease");
    drop(attributes_only);
    let busy = RepositoryWriterLease::open_existing(&retained, &identity)
        .expect_err("second repository metadata writer must conflict on the same native object");
    assert!(matches!(
        busy,
        UpdateError::Baseline {
            step: "repository metadata lease",
            ..
        }
    ));
    lease
        .revalidate()
        .expect("live pinned handle remains admissible");
    drop(lease);
    RepositoryWriterLease::open_existing(&retained, &identity)
        .expect("closing the original owner releases the native lease");
    assert_eq!(
        fs::read(lock_path).expect("test-owned lock stays present"),
        [] as [u8; 0]
    );
}

#[test]
fn identical_repo_claims_on_different_physical_roots_cannot_prove_d90_coverage() {
    let (_first_root, first) = lease_fixture();
    let (_second_root, second) = lease_fixture();
    let identity = lease_anchor();
    provision_test_lock(&first);
    provision_test_lock(&second);

    let first_lease = RepositoryWriterLease::open_existing(&first, &identity)
        .expect("first actual directory owns one lease");
    let second_lease = RepositoryWriterLease::open_existing(&second, &identity)
        .expect("same claimed app and repo do not bind a foreign native parent");
    first_lease
        .revalidate()
        .expect("first native parent stays admitted");
    second_lease
        .revalidate()
        .expect("second native parent stays admitted");
    // The missing Guard/D44 root-minting proof MUST prevent these two handles
    // from ever both reaching the uninhabited RepositoryLedgerCensus.
}

#[test]
fn pinned_repository_and_lock_object_identity_mismatches_refuse_before_io() {
    let (root, retained) = lease_fixture();
    let (_foreign_root, foreign) = lease_fixture();
    let identity = lease_anchor();
    provision_test_lock(&retained);
    provision_test_lock(&foreign);
    let mut lease = RepositoryWriterLease::open_existing(&retained, &identity)
        .expect("native pinned repository lock");

    let actual_repository = lease.repository_object.clone();
    lease.repository_object =
        ObjectIdentity::of_dir(&foreign).expect("second physical directory identity");
    assert!(matches!(
        lease
            .revalidate()
            .expect_err("a different physical parent must refuse"),
        UpdateError::Baseline {
            step: "repository metadata lease root identity",
            ..
        }
    ));
    lease.repository_object = actual_repository;
    lease.revalidate().expect("restored native root identity");

    let alternate = foreign
        .open("repository.lock")
        .expect("second test-owned native lock file");
    let actual_lock = lease.lease_object.clone();
    lease.lease_object =
        ObjectIdentity::of_cap_file(&alternate).expect("second physical lock identity");
    assert!(matches!(
        lease
            .revalidate()
            .expect_err("a different pinned lock object must refuse"),
        UpdateError::Baseline {
            step: "repository metadata lease file identity",
            ..
        }
    ));
    lease.lease_object = actual_lock;
    lease.revalidate().expect("restored lock object identity");
    drop(alternate);
    drop(lease);

    // Independent actual-byte malformed lock: absence of body is an invariant
    // checked before any re-acquired owner may read or publish a record.
    let lock_path = root.path().join("repository/repository.lock");
    fs::write(&lock_path, b"corrupt durable lock").expect("mutate after releasing share-zero");
    assert!(matches!(
        RepositoryWriterLease::open_existing(&retained, &identity)
            .expect_err("a nonempty physical lease file cannot be adopted"),
        UpdateError::Baseline {
            step: "repository metadata lease identity",
            ..
        }
    ));
    assert_eq!(
        fs::read(lock_path).expect("refusal preserves the original evidence"),
        b"corrupt durable lock"
    );
}
