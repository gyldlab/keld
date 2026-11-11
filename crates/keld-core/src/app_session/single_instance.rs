//! GH453 cooperative macOS dev single-instance resource owner.
//! Approved bounded scope; native qualification remains independently required.
//! The original durable Host owns this descriptor; it is never passed to Guardian/Bun.

use std::fs::{self, File, Metadata, TryLockError};
use std::os::fd::AsFd as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use nix::fcntl::{OFlag, openat};
use nix::sys::stat::{Mode, mkdirat};
use serde::{Deserialize, Serialize};

use super::HostAppError;

/// Data only: the Host owns the resource and Guardian copies only this outcome.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum SingleInstanceVerdict {
    Acquired,
    Contended,
}

/// Non-Clone owner. Drop closes the file; it never unlinks a persistent lease.
#[derive(Debug)]
pub(super) struct HostInstanceLease {
    _file: File,
    _parents: Vec<File>,
}

pub(super) struct SingleInstanceAttempt {
    pub(super) verdict: SingleInstanceVerdict,
    pub(super) owner: Option<HostInstanceLease>,
}

fn failure(detail: impl std::fmt::Display) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-037",
        "cooperative single-instance admission",
        detail.to_string(),
        "Use the reviewed singleInstance declaration and native user Application Support namespace; preserve its persistent lock file. Do not delete a possibly live lock or infer success from an I/O error.",
    )
}

fn identity(metadata: &Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}

#[derive(Clone, Copy)]
enum DirectoryKind {
    SharedNamespace,
    PrivateLock,
}

fn validate_directory(
    file: &File,
    path: &Path,
    user_owner: u32,
    kind: DirectoryKind,
) -> Result<(), HostAppError> {
    let held = file.metadata().map_err(failure)?;
    let selected = fs::symlink_metadata(path).map_err(failure)?;
    let mode = held.permissions().mode() & 0o7777;
    let valid_mode = match kind {
        // Keld is shared with existing app services; preserve their read/search
        // access while refusing group/other writers.
        DirectoryKind::SharedNamespace => mode & 0o022 == 0,
        DirectoryKind::PrivateLock => mode == 0o700,
    };
    if !held.is_dir()
        || !selected.is_dir()
        || held.uid() != user_owner
        || !valid_mode
        || identity(&held) != identity(&selected)
    {
        return Err(failure(
            "persistent service directory identity, owner or mode mismatch",
        ));
    }
    Ok(())
}

/// Owned selection is revalidated before and after attempting the kernel lock.
struct SelectedLease {
    file: File,
    parents: Vec<File>,
    selected: Vec<(PathBuf, DirectoryKind)>,
    native_root: PathBuf,
    root_metadata: Metadata,
    user_owner: u32,
    path: PathBuf,
}

impl SelectedLease {
    fn open(
        native_root: &Path,
        declaration: &crate::hello::SingleInstanceDeclaration,
    ) -> Result<Self, HostAppError> {
        if !native_root.is_absolute() {
            return Err(failure(
                "native Application Support locator must be absolute",
            ));
        }
        let root = File::from(super::open_root(native_root)?);
        let root_metadata = root.metadata().map_err(failure)?;
        let named = fs::symlink_metadata(native_root).map_err(failure)?;
        if !root_metadata.is_dir()
            || !named.is_dir()
            || identity(&root_metadata) != identity(&named)
            || root_metadata.permissions().mode() & 0o022 != 0
        {
            return Err(failure(
                "native user directory was substituted or is group/other writable",
            ));
        }
        let user_owner = root_metadata.uid();
        let mut parents = vec![root];
        let mut path = native_root.to_owned();
        let mut selected = Vec::new();
        for (name, kind) in [
            ("Keld", DirectoryKind::SharedNamespace),
            ("InstanceLocks", DirectoryKind::PrivateLock),
            ("v1", DirectoryKind::PrivateLock),
            (declaration.id(), DirectoryKind::PrivateLock),
        ] {
            let parent = parents
                .last()
                .ok_or_else(|| failure("retained native directory unavailable"))?;
            match mkdirat(parent.as_fd(), name, Mode::S_IRWXU) {
                Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
                Err(error) => return Err(failure(error)),
            }
            let directory = File::from(
                openat(
                    parent.as_fd(),
                    name,
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(failure)?,
            );
            path.push(name);
            validate_directory(&directory, &path, user_owner, kind)?;
            selected.push((path.clone(), kind));
            parents.push(directory);
        }
        let parent = parents
            .last()
            .ok_or_else(|| failure("persistent lease parent unavailable"))?;
        let file = File::from(
            openat(
                parent.as_fd(),
                "lease",
                OFlag::O_RDWR
                    | OFlag::O_CREAT
                    | OFlag::O_NOFOLLOW
                    | OFlag::O_CLOEXEC
                    | OFlag::O_NONBLOCK,
                Mode::S_IRUSR | Mode::S_IWUSR,
            )
            .map_err(failure)?,
        );
        path.push("lease");
        Ok(Self {
            file,
            parents,
            selected,
            native_root: native_root.to_owned(),
            root_metadata,
            user_owner,
            path,
        })
    }

    fn validate(&self) -> Result<(), HostAppError> {
        let held = self.file.metadata().map_err(failure)?;
        let named = fs::symlink_metadata(&self.path).map_err(failure)?;
        if !held.is_file()
            || !named.is_file()
            || held.nlink() != 1
            || held.uid() != self.user_owner
            || held.permissions().mode() & 0o7777 != 0o600
            || identity(&held) != identity(&named)
        {
            return Err(failure(
                "persistent lease identity, kind, owner, link count or mode mismatch",
            ));
        }
        let named_root = fs::symlink_metadata(&self.native_root).map_err(failure)?;
        if !named_root.is_dir() || identity(&named_root) != identity(&self.root_metadata) {
            return Err(failure(
                "native user directory changed during lock admission",
            ));
        }
        for (directory, (path, kind)) in self.parents.iter().skip(1).zip(&self.selected) {
            validate_directory(directory, path, self.user_owner, *kind)?;
        }
        Ok(())
    }
}

/// One cold attempt. The caller retains the resulting owner on the original
/// durable Host stack; only the data verdict crosses Guardian/Bun. No lock fd
/// is cloned, explicitly inherited, truncated, renamed or unlinked here.
/// Retained stat checks detect observable substitution but do not prevent a
/// later hostile same-user replacement; the Legacy cooperative scope is explicit.
pub(super) fn acquire(
    native_root: &Path,
    declaration: &crate::hello::SingleInstanceDeclaration,
) -> Result<SingleInstanceAttempt, HostAppError> {
    let selected = SelectedLease::open(native_root, declaration)?;
    selected.validate()?;
    let acquired = match selected.file.try_lock() {
        Ok(()) => true,
        Err(TryLockError::WouldBlock) => false,
        Err(TryLockError::Error(error)) => return Err(failure(error)),
    };
    selected.validate()?;
    if acquired {
        Ok(SingleInstanceAttempt {
            verdict: SingleInstanceVerdict::Acquired,
            owner: Some(HostInstanceLease {
                _file: selected.file,
                _parents: selected.parents,
            }),
        })
    } else {
        Ok(SingleInstanceAttempt {
            verdict: SingleInstanceVerdict::Contended,
            owner: None,
        })
    }
}

#[cfg(all(test, not(keld_core_native_renderer_main)))]
mod tests;
