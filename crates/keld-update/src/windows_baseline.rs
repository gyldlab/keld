//! One-shot machine baseline initialization and read-only provenance ownership.

use std::collections::BTreeSet;
use std::io::{self, Read};

use cap_fs_ext::{DirExt as _, FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt as _};
use cap_std::fs::{Dir, File, OpenOptions, OpenOptionsExt as _};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
};

use crate::windows_extraction::{ensure_directory, ensure_regular, open_ancestors};
use crate::windows_fs::{
    qualified_volume_root, require_volume_root_handle, validate_volume_locator,
};
use crate::{DirectInstallationIdentity, ProvenanceObservation, UpdateError};

mod initialize;
mod load;

pub use initialize::initialize_windows_baseline;
pub use load::load_windows_baseline;

/// Trusted deployment/host inputs, independent of the record being authenticated.
///
/// These values must come from trusted packaging/host configuration. An installer
/// must never accept lower-trust command-line, environment or feed assertions here.
#[derive(Debug, Clone)]
pub struct WindowsBaselineTrust {
    /// Complete identity expected by the host and authenticated baseline verifier.
    pub installation: DirectInstallationIdentity,
    /// Installer-asserted publisher digest, not independent Authenticode evidence.
    pub publisher_scope: [u8; 32],
    /// Expected canonical volume-GUID root, such as `\\?\Volume{GUID}\`.
    pub volume_guid: String,
}

/// Protected installed identity/floor with retained read handles and namespace pins.
///
/// This is not active-package selection, mutation authority or strict-role admission.
#[derive(Debug)]
pub struct LoadedWindowsBaseline {
    observation: ProvenanceObservation,
    floor: String,
    publisher_scope: [u8; 32],
    _roots: Roots,
    _records: Vec<File>,
}

impl LoadedWindowsBaseline {
    /// Protected identity and floor for ordinary update admission.
    #[must_use]
    pub const fn observation(&self) -> &ProvenanceObservation {
        &self.observation
    }

    /// Trusted expected installation, checked against the protected record.
    #[must_use]
    pub fn identity(&self) -> &DirectInstallationIdentity {
        &self._roots.trust.installation
    }

    /// Installer-asserted publisher digest; boot must independently verify its signer.
    #[must_use]
    pub const fn publisher_scope(&self) -> &[u8; 32] {
        &self.publisher_scope
    }

    /// Exact protected semantic-version floor, without activation selection.
    #[must_use]
    pub fn version_floor(&self) -> &str {
        &self.floor
    }
}

/// Successful baseline initialization after production reload and exact seed checks.
#[derive(Debug)]
pub struct WindowsBaselineReceipt {
    loaded: LoadedWindowsBaseline,
    _version: VersionPins,
}

impl WindowsBaselineReceipt {
    /// Read-only protected provenance retained by this initialization receipt.
    #[must_use]
    pub const fn loaded(&self) -> &LoadedWindowsBaseline {
        &self.loaded
    }
}

#[derive(Debug)]
struct Roots {
    trust: WindowsBaselineTrust,
    ancestors: Vec<Dir>,
    update: Dir,
    versions: Dir,
}

impl Roots {
    fn install(&self) -> io::Result<&Dir> {
        self.ancestors
            .last()
            .ok_or_else(|| io::Error::other("install root absent"))
    }
}

#[derive(Debug)]
struct VersionPins {
    _directories: Vec<Dir>,
    _files: Vec<File>,
}

fn error(step: &'static str, detail: impl std::fmt::Display) -> UpdateError {
    UpdateError::Baseline {
        step,
        detail: detail.to_string(),
    }
}

fn open_roots(trust: &WindowsBaselineTrust, private: bool) -> io::Result<Roots> {
    let identity = &trust.installation;
    if identity.target != "windows-x64" {
        return Err(io::Error::other("baseline requires windows-x64"));
    }
    crate::records::path_text(&identity.install_root).map_err(io::Error::other)?;
    crate::records::path_text(&identity.update_root).map_err(io::Error::other)?;
    validate_volume_locator(&trust.volume_guid)?;
    if identity.update_root.parent() != Some(identity.install_root.as_path()) {
        return Err(io::Error::other(
            "update root must be one direct child of install root",
        ));
    }
    let update_name = identity
        .update_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("update component absent"))?;
    if update_name.eq_ignore_ascii_case("install-provenance") {
        return Err(io::Error::other(
            "update component conflicts with provenance",
        ));
    }
    let ancestors = open_ancestors(&identity.install_root)?;
    let last = ancestors
        .len()
        .checked_sub(1)
        .ok_or_else(|| io::Error::other("install root absent"))?;
    let volume = ancestors[0].dir_metadata()?.dev();
    for (index, directory) in ancestors.iter().enumerate() {
        if directory.dir_metadata()?.dev() != volume {
            return Err(io::Error::other("ancestor crosses the expected volume"));
        }
        let file = directory.try_clone()?.into_std_file();
        if index == 0 {
            require_volume_root_handle(&file)?;
            keld_guard::validate_windows_machine_volume_anchor(&file)?;
        } else if index == last && private {
            keld_guard::validate_windows_owner_private_directory(&file)?;
        } else {
            keld_guard::validate_windows_machine_directory(&file)?;
        }
        let actual = qualified_volume_root(&file)?;
        if !actual.eq_ignore_ascii_case(&trust.volume_guid) {
            return Err(io::Error::other(
                "actual volume differs from trusted expected volume",
            ));
        }
    }
    let update = ancestors[last].open_dir_nofollow(update_name)?;
    let versions = update.open_dir_nofollow("versions")?;
    for directory in [&update, &versions] {
        ensure_directory(&directory.dir_metadata()?)?;
        if directory.dir_metadata()?.dev() != volume {
            return Err(io::Error::other(
                "baseline scaffold crosses the expected volume",
            ));
        }
        let file = directory.try_clone()?.into_std_file();
        if private {
            keld_guard::validate_windows_owner_private_directory(&file)?;
        } else {
            keld_guard::validate_windows_machine_directory(&file)?;
        }
    }
    Ok(Roots {
        trust: trust.clone(),
        ancestors,
        update,
        versions,
    })
}

fn exact_entries(directory: &Dir, expected: &[&str]) -> io::Result<()> {
    let mut names = BTreeSet::new();
    for entry in directory.entries()? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| io::Error::other("non-UTF-8 directory entry"))?;
        names.insert(name);
    }
    let wanted: BTreeSet<String> = expected.iter().map(|name| (*name).to_owned()).collect();
    if names != wanted {
        return Err(io::Error::other(format!(
            "unexpected state: wanted {wanted:?}, found {names:?}"
        )));
    }
    Ok(())
}

fn open_machine_file(parent: &Dir, leaf: &str) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .follow(FollowSymlinks::No);
    let file = parent.open_with(leaf, &options)?;
    ensure_regular(&file.metadata()?)?;
    if file.metadata()?.dev() != parent.dir_metadata()?.dev() {
        return Err(io::Error::other("record crosses parent volume"));
    }
    keld_guard::validate_windows_machine_file(&file.try_clone()?.into_std())?;
    Ok(file)
}

fn read_record(parent: &Dir, leaf: &str) -> Result<(File, Vec<u8>), UpdateError> {
    let mut file =
        open_machine_file(parent, leaf).map_err(|cause| error("protected record open", cause))?;
    let length = file
        .metadata()
        .map_err(|cause| error("protected record size", cause))?
        .len();
    if length > crate::records::MAX_LOCAL_RECORD_BYTES as u64 {
        return Err(error("protected record size", "record exceeds 64 KiB"));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|cause| error("protected record read", cause))?;
    Ok((file, bytes))
}

fn seal_child(parent: &Dir, leaf: &str, directory: bool) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options
        .access_mode(READ_CONTROL | WRITE_DAC)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .follow(FollowSymlinks::No);
    if directory {
        options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }
    let object = parent.open_with(leaf, &options)?;
    if directory {
        ensure_directory(&object.metadata()?)?;
    } else {
        ensure_regular(&object.metadata()?)?;
    }
    let mut object = object.into_std();
    if directory {
        keld_guard::seal_windows_machine_directory(&mut object)
    } else {
        keld_guard::seal_windows_machine_file(&mut object)
    }
}

#[cfg(test)]
mod tests;
