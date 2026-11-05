//! Real kernel/filesystem checks in isolated owned temp roots only.
//! These do not prove shipping Host/Guardian/Bun descriptor inheritance or app behavior.
use super::{
    DirectoryKind, HostInstanceLease, SelectedLease, SingleInstanceVerdict, acquire, identity,
    validate_directory,
};
use crate::hello::SingleInstanceDeclaration;
use std::fs::{self, File};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const ID: &str = "org.keld.shared-parent-test";

fn declaration() -> Result<SingleInstanceDeclaration, serde_json::Error> {
    serde_json::from_str(r#"{"id":"org.keld.shared-parent-test"}"#)
}

fn create_directory(path: &Path, mode: u32) -> std::io::Result<()> {
    fs::create_dir(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

fn private_components(root: &Path) -> std::io::Result<PathBuf> {
    let mut path = root.join("Keld");
    create_directory(&path, 0o700)?;
    for name in ["InstanceLocks", "v1", ID] {
        path.push(name);
        create_directory(&path, 0o700)?;
    }
    Ok(path)
}

#[test]
fn shared_755_namespace_acquires_contends_and_reacquires_same_persistent_lease() -> TestResult {
    use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
    let root = tempfile::tempdir()?;
    let shared = root.path().join("Keld");
    create_directory(&shared, 0o755)?;
    let first = acquire(root.path(), &declaration()?)?;
    assert_eq!(first.verdict, SingleInstanceVerdict::Acquired);
    let owner = first
        .owner
        .expect("original Host owns its acquired descriptor");
    let lease_path = shared.join("InstanceLocks/v1").join(ID).join("lease");
    let selected = fs::symlink_metadata(&lease_path)?;
    let lease_identity = identity(&selected);
    assert_eq!(selected.permissions().mode() & 0o7777, 0o600);
    assert_eq!(selected.nlink(), 1);
    assert_eq!(
        fs::metadata(&shared)?.permissions().mode() & 0o7777,
        0o755,
        "existing shared namespace must not be chmodded"
    );
    let HostInstanceLease {
        _file: lease_file,
        _parents: retained_parents,
    } = &owner;
    assert!(
        FdFlag::from_bits_truncate(fcntl(lease_file, FcntlArg::F_GETFD)?)
            .contains(FdFlag::FD_CLOEXEC)
    );
    assert!(
        OFlag::from_bits_truncate(fcntl(lease_file, FcntlArg::F_GETFL)?)
            .contains(OFlag::O_NONBLOCK)
    );
    for directory in retained_parents {
        assert!(
            FdFlag::from_bits_truncate(fcntl(directory, FcntlArg::F_GETFD)?)
                .contains(FdFlag::FD_CLOEXEC)
        );
    }
    fs::write(&lease_path, b"persistent owned fixture contents")?;
    let second = acquire(root.path(), &declaration()?)?;
    assert_eq!(second.verdict, SingleInstanceVerdict::Contended);
    assert!(second.owner.is_none());
    assert_eq!(fs::read(&lease_path)?, b"persistent owned fixture contents");
    drop(owner);
    assert_eq!(
        identity(&fs::symlink_metadata(&lease_path)?),
        lease_identity,
        "Drop never unlinks the lease"
    );
    let third = acquire(root.path(), &declaration()?)?;
    assert_eq!(third.verdict, SingleInstanceVerdict::Acquired);
    let third_owner = third.owner.expect("new durable owner after actual release");
    let HostInstanceLease {
        _file: third_lease_file,
        ..
    } = &third_owner;
    assert_eq!(identity(&third_lease_file.metadata()?), lease_identity);
    assert_eq!(fs::read(&lease_path)?, b"persistent owned fixture contents");
    Ok(())
}

#[test]
fn writable_shared_namespace_and_private_755_components_are_refused() -> TestResult {
    for mode in [0o775, 0o757, 0o777] {
        let root = tempfile::tempdir()?;
        let shared = root.path().join("Keld");
        create_directory(&shared, mode)?;
        let Err(error) = acquire(root.path(), &declaration()?) else {
            return Err(
                std::io::Error::other("shared namespace allowed group/other writers").into(),
            );
        };
        assert_eq!(error.code(), "KELD-CORE-037");
        assert!(
            !shared.join("InstanceLocks").exists(),
            "refuse before creating private lock state"
        );
    }
    for relative in [
        "Keld/InstanceLocks",
        "Keld/InstanceLocks/v1",
        "Keld/InstanceLocks/v1/org.keld.shared-parent-test",
    ] {
        let root = tempfile::tempdir()?;
        let leaf = private_components(root.path())?;
        fs::set_permissions(
            root.path().join(relative),
            fs::Permissions::from_mode(0o755),
        )?;
        assert!(
            acquire(root.path(), &declaration()?).is_err(),
            "private component {relative} must remain exactly0700"
        );
        assert!(!leaf.join("lease").exists());
    }
    Ok(())
}

#[test]
fn shared_directory_wrong_uid_and_same_mode_substitution_are_refused() -> TestResult {
    let root = tempfile::tempdir()?;
    let shared = root.path().join("Keld");
    create_directory(&shared, 0o700)?;
    let retained = File::open(&shared)?;
    let uid = retained.metadata()?.uid();
    validate_directory(&retained, &shared, uid, DirectoryKind::SharedNamespace)?;
    assert!(
        validate_directory(
            &retained,
            &shared,
            uid.wrapping_add(1),
            DirectoryKind::SharedNamespace
        )
        .is_err(),
        "actual descriptor UID must match native owner"
    );
    fs::rename(&shared, root.path().join("old-keld"))?;
    create_directory(&shared, 0o700)?;
    assert_ne!(
        identity(&retained.metadata()?),
        identity(&fs::symlink_metadata(&shared)?)
    );
    assert!(
        validate_directory(&retained, &shared, uid, DirectoryKind::SharedNamespace).is_err(),
        "same uid/mode replacement is still a different directory"
    );
    Ok(())
}

#[test]
fn shared_mode_changes_after_selection_are_revalidated_before_verdict() -> TestResult {
    let root = tempfile::tempdir()?;
    let selected = SelectedLease::open(root.path(), &declaration()?)?;
    selected.validate()?;
    fs::set_permissions(root.path().join("Keld"), fs::Permissions::from_mode(0o775))?;
    assert!(
        selected.validate().is_err(),
        "retained shared mode must be checked again after open"
    );
    Ok(())
}

#[test]
fn every_selected_component_and_lease_refuses_symlinks() -> TestResult {
    for index in 0..4 {
        let root = tempfile::tempdir()?;
        let target = root.path().join("elsewhere");
        create_directory(&target, 0o700)?;
        let mut path = root.path().to_owned();
        for name in ["Keld", "InstanceLocks", "v1", ID].iter().take(index) {
            path.push(name);
            create_directory(&path, 0o700)?;
        }
        path.push(["Keld", "InstanceLocks", "v1", ID][index]);
        symlink(&target, &path)?;
        assert!(
            acquire(root.path(), &declaration()?).is_err(),
            "symlink component {index} cannot redirect custody"
        );
        assert!(!target.join("lease").exists());
    }
    let root = tempfile::tempdir()?;
    let parent = private_components(root.path())?;
    let target = root.path().join("other-lease");
    fs::write(&target, b"must remain unchanged")?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
    symlink(&target, parent.join("lease"))?;
    assert!(acquire(root.path(), &declaration()?).is_err());
    assert_eq!(fs::read(&target)?, b"must remain unchanged");
    Ok(())
}

#[test]
fn private_lease_mode_and_link_count_remain_independent_refusals() -> TestResult {
    let root = tempfile::tempdir()?;
    let selected = SelectedLease::open(root.path(), &declaration()?)?;
    selected.validate()?;
    fs::set_permissions(&selected.path, fs::Permissions::from_mode(0o644))?;
    assert!(selected.validate().is_err(), "private lease remains0600");
    fs::set_permissions(&selected.path, fs::Permissions::from_mode(0o600))?;
    fs::hard_link(&selected.path, root.path().join("second-lease-link"))?;
    assert_eq!(selected.file.metadata()?.nlink(), 2);
    assert!(
        selected.validate().is_err(),
        "shared ancestor never relaxes lease nlink1"
    );
    Ok(())
}

#[test]
fn native_root_must_still_be_absolute_nofollow_and_not_writable_by_others() -> TestResult {
    assert!(acquire(Path::new("relative-support"), &declaration()?).is_err());
    let root = tempfile::tempdir()?;
    let target = root.path().join("real-support");
    create_directory(&target, 0o700)?;
    let link = root.path().join("support-link");
    symlink(&target, &link)?;
    assert!(acquire(&link, &declaration()?).is_err());
    for mode in [0o775, 0o777] {
        let root = tempfile::tempdir()?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(mode))?;
        assert!(acquire(root.path(), &declaration()?).is_err());
        assert!(!root.path().join("Keld").exists());
    }
    Ok(())
}
