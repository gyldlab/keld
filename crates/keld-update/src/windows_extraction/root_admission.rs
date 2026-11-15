//! Windows extraction-root admission shared with the protected baseline reader.
//!
//! This owner validates the authenticated install/update tree, fixed NTFS
//! volume, exact directory profile and retained no-follow ancestor handles.

use std::fs::OpenOptions as StdOpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Component, Path, PathBuf, Prefix};

use cap_fs_ext::{DirExt as _, MetadataExt as _};
use cap_std::fs::Dir;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use super::{RootAuthority, WindowsExtractionRoot, ensure_directory, refusal};
use crate::windows_fs::qualify_volume;
use crate::{AdmittedInstallation, DirectInstallMode};

pub(super) fn open_root(admitted: &AdmittedInstallation) -> io::Result<WindowsExtractionRoot> {
    if admitted.identity.target != "windows-x64" {
        return Err(refusal("only the Windows x64 v0 package cell is supported"));
    }
    if admitted.identity.update_root.parent() != Some(admitted.identity.install_root.as_path())
        || admitted.identity.install_root.parent().is_none()
        || admitted
            .identity
            .update_root
            .file_name()
            .is_none_or(|name| name.eq_ignore_ascii_case("install-provenance"))
    {
        return Err(refusal("direct extraction root topology is invalid"));
    }
    match admitted.identity.install_mode {
        DirectInstallMode::PerUserDirect => open_owner_private_root(admitted),
        DirectInstallMode::MachineUacDirect => Err(refusal(
            "MachineUacDirect staging requires authenticated helper admission and an exclusive writer lease",
        )),
        DirectInstallMode::MachineSeamlessDirect => Err(refusal(
            "MachineSeamlessDirect staging requires the SYSTEM baseline loader",
        )),
    }
}

fn open_owner_private_root(admitted: &AdmittedInstallation) -> io::Result<WindowsExtractionRoot> {
    let path = &admitted.identity.update_root;
    let ancestors = open_ancestors(path)?;
    let last = ancestors
        .len()
        .checked_sub(1)
        .ok_or_else(|| refusal("missing staging root"))?;
    let install_index = last
        .checked_sub(1)
        .filter(|index| *index > 0)
        .ok_or_else(|| refusal("install root must be beneath the volume root"))?;
    let volume = ancestors[0].dir_metadata()?.dev();
    for (index, directory) in ancestors.iter().enumerate() {
        if directory.dir_metadata()?.dev() != volume {
            return Err(refusal("owner-private ancestors cross volumes"));
        }
        let object = directory.try_clone()?.into_std_file();
        if index == 0 {
            crate::windows_fs::require_volume_root_handle(&object)?;
            keld_guard::validate_windows_machine_volume_anchor(&object)?;
        } else if index == install_index || index == last {
            keld_guard::validate_windows_owner_private_directory(&object)?;
        }
    }
    let root = ancestors
        .last()
        .ok_or_else(|| refusal("missing staging root"))?;
    let versions = root.open_dir_nofollow("versions")?;
    let versions_metadata = versions.dir_metadata()?;
    ensure_directory(&versions_metadata)?;
    if versions_metadata.dev() != root.dir_metadata()?.dev() {
        return Err(refusal("versions crosses the admitted volume"));
    }
    let versions_file = versions.try_clone()?.into_std_file();
    keld_guard::validate_windows_owner_private_directory(&versions_file)?;
    qualify_volume(&versions_file)?;
    Ok(WindowsExtractionRoot {
        installation: admitted.identity.clone(),
        floor: admitted.floor.clone(),
        authority: RootAuthority::OwnerPrivate {
            _ancestors: ancestors,
        },
        versions,
    })
}

pub(crate) fn open_ancestors(path: &Path) -> io::Result<Vec<Dir>> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(refusal("staging root must be an absolute local drive path"));
    };
    if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        || components.next() != Some(Component::RootDir)
    {
        return Err(refusal("staging root must be an absolute local drive path"));
    }
    let mut anchor = PathBuf::from(prefix.as_os_str());
    anchor.push(Path::new(r"\"));
    let opened = StdOpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(anchor)?;
    let mut ancestors = vec![Dir::from_std_file(opened)];
    ensure_directory(&ancestors[0].dir_metadata()?)?;
    for component in components {
        let Component::Normal(name) = component else {
            return Err(refusal("staging root contains a non-normal component"));
        };
        let name = name
            .to_str()
            .ok_or_else(|| refusal("staging root is not UTF-8"))?;
        keld_guard::validate_fs_component(name).map_err(refusal)?;
        let parent = ancestors
            .last()
            .ok_or_else(|| refusal("missing root ancestor"))?;
        let child = parent.open_dir_nofollow(name)?;
        ensure_directory(&child.dir_metadata()?)?;
        ancestors.push(child);
    }
    Ok(ancestors)
}
