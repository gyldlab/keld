//! macOS dev-stage placement, separate from source selection and graph admission.

use std::fs::{self, File};
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use rustix::fs::{Mode, OFlags, mkdirat, open, openat};
use sha2::{Digest as _, Sha256};

use super::{BootCompileError, ensure_unix_metadata_owned_by};

const RESOLVER_INPUTS: [&str; 4] = [
    "node_modules",
    "package.json",
    "tsconfig.json",
    "jsconfig.json",
];

pub(super) struct StageNamespace {
    root: PathBuf,
    // Retain every selected parent object until the stage transaction finishes.
    parents: Vec<(PathBuf, File)>,
    invoking_uid: u32,
}

impl StageNamespace {
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn retain_launch_root(&self, path: &Path) -> Result<File, BootCompileError> {
        let nonce = path
            .file_name()
            .filter(|_| path.parent() == Some(self.root()))
            .ok_or_else(|| namespace_error(path, "launch root is not a direct namespace child"))?;
        let (_, parent) = self
            .parents
            .last()
            .ok_or_else(|| namespace_error(path, "selected dev directory was not retained"))?;
        let directory = File::from(
            openat(
                parent,
                nonce,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|source| namespace_error(path, source))?,
        );
        validate_private_directory(&directory, path, self.invoking_uid)?;
        Ok(directory)
    }

    /// Identity only: a late resolver input must not disable owned-nonce cleanup.
    pub(super) fn recheck_launch_root(
        &self,
        path: &Path,
        directory: &File,
    ) -> Result<(), BootCompileError> {
        self.recheck_identity()?;
        validate_private_directory(directory, path, self.invoking_uid)?;
        recheck_selected_directory(directory, path)
    }

    pub(super) fn recheck(&self) -> Result<(), BootCompileError> {
        self.recheck_identity()?;
        for ancestor in self.root.ancestors() {
            refuse_resolver_inputs(ancestor)?;
        }
        Ok(())
    }

    fn recheck_identity(&self) -> Result<(), BootCompileError> {
        for (path, directory) in &self.parents {
            validate_private_directory(directory, path, self.invoking_uid)?;
            recheck_selected_directory(directory, path)?;
        }
        Ok(())
    }
}

pub(super) fn prepare(project_root: &Path) -> Result<StageNamespace, BootCompileError> {
    prepare_in(
        project_root,
        &std::env::temp_dir(),
        rustix::process::geteuid().as_raw(),
    )
}

/// A non-authoritative locator. Only the separately checked directory objects
/// and existing executable-located stage validator confer any admission.
pub(super) fn project_namespace(
    project_root: &Path,
    temporary_root: &Path,
    invoking_uid: u32,
) -> PathBuf {
    let project = Sha256::digest(project_root.as_os_str().as_bytes());
    temporary_root
        .join(format!("keld-dev-{invoking_uid}"))
        .join(format!("{project:x}"))
}

fn prepare_in(
    project_root: &Path,
    temporary_root: &Path,
    invoking_uid: u32,
) -> Result<StageNamespace, BootCompileError> {
    let temporary_root = temporary_root
        .canonicalize()
        .map_err(|source| namespace_error(temporary_root, source))?;
    if temporary_root.starts_with(project_root) {
        return Err(namespace_error(
            &temporary_root,
            "the temporary directory is inside the project; select an owner-private temporary directory outside it",
        ));
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut parent = File::from(
        open(&temporary_root, flags, Mode::empty())
            .map_err(|source| namespace_error(&temporary_root, source))?,
    );
    validate_private_directory(&parent, &temporary_root, invoking_uid)?;
    for ancestor in temporary_root.ancestors() {
        refuse_resolver_inputs(ancestor)?;
    }

    let namespace = project_namespace(project_root, &temporary_root, invoking_uid);
    let relative = namespace
        .strip_prefix(&temporary_root)
        .map_err(|source| namespace_error(&namespace, source))?
        .join(".keld")
        .join("dev");
    let mut path = temporary_root;
    let mut parents = Vec::new();
    for name in relative.components() {
        let name = name.as_os_str();
        let parent_path = path.clone();
        path.push(name);
        match mkdirat(&parent, name, Mode::RWXU) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(source) => return Err(namespace_error(&path, source)),
        }
        let directory = File::from(
            openat(&parent, name, flags, Mode::empty())
                .map_err(|source| namespace_error(&path, source))?,
        );
        validate_private_directory(&directory, &path, invoking_uid)?;
        refuse_resolver_inputs(&path)?;
        parents.push((parent_path, parent));
        parent = directory;
    }
    parents.push((path.clone(), parent));
    Ok(StageNamespace {
        root: path,
        parents,
        invoking_uid,
    })
}

fn validate_private_directory(
    directory: &File,
    path: &Path,
    invoking_uid: u32,
) -> Result<(), BootCompileError> {
    let metadata = directory
        .metadata()
        .map_err(|source| namespace_error(path, source))?;
    ensure_unix_metadata_owned_by(path, &metadata, invoking_uid)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o7777 != 0o700 {
        return Err(namespace_error(
            path,
            "the selected directory must already be invoking-owner private with mode 0700; existing permissions are not changed",
        ));
    }
    Ok(())
}

fn recheck_selected_directory(directory: &File, path: &Path) -> Result<(), BootCompileError> {
    let retained = directory
        .metadata()
        .map_err(|source| namespace_error(path, source))?;
    let current = fs::symlink_metadata(path).map_err(|source| namespace_error(path, source))?;
    if !current.is_dir() || current.dev() != retained.dev() || current.ino() != retained.ino() {
        return Err(namespace_error(
            path,
            "the selected namespace directory changed during source capture",
        ));
    }
    Ok(())
}

fn refuse_resolver_inputs(directory: &Path) -> Result<(), BootCompileError> {
    for name in RESOLVER_INPUTS {
        let path = directory.join(name);
        match fs::symlink_metadata(&path) {
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(namespace_error(&path, source)),
            Ok(_) => {
                return Err(namespace_error(
                    &path,
                    "a stage ancestor contains a module-resolution input; select a clean owner-private temporary directory",
                ));
            }
        }
    }
    Ok(())
}

fn namespace_error(path: &Path, detail: impl std::fmt::Display) -> BootCompileError {
    BootCompileError::new(
        "macOS dev namespace",
        format!("{}: {detail}", path.display()),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)] // assertions describe owned native filesystem fixtures
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().expect("owned temporary root");
        let root_path = root.path().canonicalize().expect("canonical temp root");
        let project = root_path.join("project");
        let temporary = root_path.join("isolated");
        fs::create_dir(&project).expect("project");
        fs::create_dir(&temporary).expect("private temporary base");
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o700))
            .expect("fixture-only private mode");
        (root, project, temporary)
    }

    #[test]
    fn isolated_namespace_is_private_stable_and_distinct_from_source() {
        let (_root, project, temporary) = fixture();
        let uid = rustix::process::geteuid().as_raw();
        let first = prepare_in(&project, &temporary, uid).expect("isolated namespace");
        let second = prepare_in(&project, &temporary, uid).expect("same project namespace");
        assert_eq!(first.root(), second.root());
        assert!(!first.root().starts_with(&project));
        assert!(first.root().ends_with(".keld/dev"));
        assert_eq!(
            fs::metadata(first.root())
                .expect("mode")
                .permissions()
                .mode()
                & 0o7777,
            0o700
        );
        let other = temporary.join("other-project");
        fs::create_dir(&other).expect("second project");
        let third = prepare_in(&other, &temporary, uid).expect("distinct project namespace");
        assert_ne!(first.root(), third.root());
    }

    #[test]
    fn project_contained_temp_refuses_before_namespace_creation() {
        let (_root, project, _temporary) = fixture();
        let temporary = project.join("temporary");
        fs::create_dir(&temporary).expect("nested temp");
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o700))
            .expect("fixture private mode");
        let error = prepare_in(&project, &temporary, rustix::process::geteuid().as_raw())
            .err()
            .expect("project ancestry refuses");
        assert!(error.to_string().contains("inside the project"), "{error}");
        assert_eq!(fs::read_dir(temporary).expect("unchanged temp").count(), 0);
    }

    #[test]
    fn resolver_bearing_temp_ancestor_refuses_before_private_namespace() {
        for name in RESOLVER_INPUTS {
            let (root, project, temporary) = fixture();
            let input = root.path().join(name);
            fs::write(&input, b"ancestor input").expect("ancestor resolver fixture");
            let error = prepare_in(&project, &temporary, rustix::process::geteuid().as_raw())
                .err()
                .expect("ancestor input refuses");
            assert!(error.to_string().contains(name), "{error}");
            assert!(error.to_string().contains("KELD-CLI-047"), "{error}");
            assert_eq!(
                fs::read(&input).expect("source preserved"),
                b"ancestor input"
            );
            assert_eq!(fs::read_dir(&temporary).expect("unchanged temp").count(), 0);
        }
    }

    #[test]
    fn symlink_namespace_is_not_followed_or_removed() {
        let (root, project, temporary) = fixture();
        let uid = rustix::process::geteuid().as_raw();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).expect("outside directory");
        fs::write(outside.join("sentinel"), b"preserve").expect("outside sentinel");
        let link = temporary.join(format!("keld-dev-{uid}"));
        symlink(&outside, &link).expect("namespace symlink");
        let error = prepare_in(&project, &temporary, uid)
            .err()
            .expect("namespace symlink refuses");
        assert!(error.to_string().contains("KELD-CLI-047"), "{error}");
        assert!(
            fs::symlink_metadata(link)
                .expect("link retained")
                .is_symlink()
        );
        assert_eq!(
            fs::read(outside.join("sentinel")).expect("outside preserved"),
            b"preserve"
        );
        assert_eq!(fs::read_dir(outside).expect("outside census").count(), 1);
    }

    #[test]
    fn existing_namespace_mode_is_refused_without_repair() {
        let (_root, project, temporary) = fixture();
        let uid = rustix::process::geteuid().as_raw();
        let namespace = prepare_in(&project, &temporary, uid).expect("initial namespace");
        let root = namespace.root().to_owned();
        drop(namespace);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("fixture mutation");
        let error = prepare_in(&project, &temporary, uid)
            .err()
            .expect("wrong mode refuses");
        assert!(error.to_string().contains("mode 0700"), "{error}");
        assert_eq!(
            fs::metadata(root)
                .expect("unmodified mode")
                .permissions()
                .mode()
                & 0o7777,
            0o755
        );
    }

    #[test]
    fn replaced_parent_is_not_mistaken_for_the_retained_directory() {
        let (root, project, temporary) = fixture();
        let namespace = prepare_in(&project, &temporary, rustix::process::geteuid().as_raw())
            .expect("captured namespace");
        let parent = namespace.root().parent().expect("private parent");
        let retained = root.path().join("retained-parent");
        fs::rename(parent, &retained).expect("replace selected parent pathname");
        fs::create_dir(parent).expect("replacement directory");
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .expect("replacement also has valid owner and mode");
        fs::write(parent.join("sentinel"), b"replacement").expect("replacement sentinel");
        let error = namespace
            .recheck()
            .expect_err("identity substitution refuses");
        assert!(
            error.to_string().contains("changed during source capture"),
            "{error}"
        );
        assert_eq!(
            fs::read(parent.join("sentinel")).expect("untouched replacement"),
            b"replacement"
        );
        assert!(retained.join("dev").is_dir(), "retained object survives");
    }

    #[test]
    fn late_ancestor_input_refuses_at_capture_finalization() {
        let (root, project, temporary) = fixture();
        let namespace = prepare_in(&project, &temporary, rustix::process::geteuid().as_raw())
            .expect("initially clean namespace");
        let input = root.path().join("package.json");
        fs::write(&input, b"{}").expect("late ancestor input");
        let error = namespace.recheck().expect_err("late input refuses");
        assert!(error.to_string().contains("package.json"), "{error}");
        assert_eq!(fs::read(input).expect("input unchanged"), b"{}");
    }
}
