//! Executable-located active-package selection (KEL-254 amendment A3 §4, task T2b).
//!
//! The executable path only locates candidate provenance; authenticated provenance
//! supplies authority. The trust anchor is the build-time [`ExpectedAppIdentity`], the
//! located roots' file identities and the recorded mode's OS protection profile; the
//! record's remaining fields are accepted only after those match.

use std::io::{self, Read as _};
use std::path::{Component, Path, PathBuf, Prefix};

use cap_fs_ext::{DirExt as _, FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt as _};
use cap_std::fs::{Dir, File, OpenOptions, OpenOptionsExt as _};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

use super::{ActivePackageSelection, WindowsBaselineTrust, admit_machine_file, exact_entries};
use crate::windows_extraction::{ensure_regular, open_ancestors};
use crate::windows_fs::qualified_volume_root;
use crate::{ExpectedAppIdentity, UpdateError, records};

const HOST: &str = "keld-host.exe";
const PROVENANCE: &str = "install-provenance";

/// Selects the active package of the installation that holds the running executable.
///
/// `locator` is the canonical current executable path and conveys no authority.
/// `executable` is the open handle whose image KEL-135 verified; its file identity, never
/// a reopen by path, binds the selection to the running process. The path must be
/// exactly `<install>\<update>\versions\<strict SemVer>\tree\keld-host.exe` (names
/// compare exactly, including case); the install root must hold exactly the update root
/// and `install-provenance`; every root the record names must be the located root by
/// volume serial number and file ID on the fixed NTFS volume whose GUID the record
/// carries; and the selected tree's `keld-host.exe` must be the executable itself.
///
/// The selection is journal-free: a pending activation journal refuses with
/// [`crate::ActivationEffect::JournalBoundRecoveryRequired`]. An invalid `current` is
/// repaired only when last-known-good is the located version, so a host started from
/// any other tree never causes a write.
///
/// # Errors
/// [`UpdateError::ExecutableBinding`] when the locator shape, executable identity, root
/// identity, volume or selected version does not bind; [`UpdateError::ProvenanceMismatch`]
/// when the record does not carry the expected app id, channel, target or signing key;
/// and every refusal of the protected record codec, protection profiles and
/// [`super::select_windows_active_package`].
pub fn select_active_package_for_executable(
    locator: &Path,
    executable: &std::fs::File,
    expected: &ExpectedAppIdentity,
) -> Result<ActivePackageSelection, UpdateError> {
    let layout = LocatedLayout::parse(locator)?;
    let executable_identity =
        ObjectIdentity::of_file(executable).map_err(|cause| binding("executable handle", cause))?;
    let volume =
        qualified_volume_root(executable).map_err(|cause| binding("executable volume", cause))?;
    let located = Located::open(&layout, &volume, &executable_identity)?;
    let (record_file, trust) = admit_record(located.install()?, expected, &volume)?;
    located.require_recorded_roots(&trust)?;

    let selection = super::load::select_with_repair_gate(&trust, Some(&layout.version))?;
    if selection.artifact.version != layout.version {
        return Err(binding(
            "selected version",
            format!(
                "the installation selects `{}`, not the located version `{}`",
                selection.artifact.version, layout.version
            ),
        ));
    }
    let selected_host =
        open_host(&selection.tree).map_err(|cause| binding("selected host", cause))?;
    if ObjectIdentity::of_cap_file(&selected_host)
        .map_err(|cause| binding("selected host", cause))?
        != executable_identity
    {
        return Err(binding(
            "selected host identity",
            "the executable is not the selected tree's keld-host.exe",
        ));
    }
    // The located pins and the record handle are held until the selection is proven.
    drop((selected_host, located, record_file));
    Ok(selection)
}

/// The located layout's directories and host, opened without following reparse points
/// and held until selection completes so none can be substituted meanwhile.
#[derive(Debug)]
struct Located {
    ancestors: Vec<Dir>,
    update: Dir,
    versions: Dir,
    _version: Dir,
    _tree: Dir,
    _host: File,
}

impl Located {
    fn open(
        layout: &LocatedLayout,
        volume: &str,
        executable: &ObjectIdentity,
    ) -> Result<Self, UpdateError> {
        let ancestors = open_ancestors(&layout.install_root)
            .map_err(|cause| binding("located roots", cause))?;
        let install = ancestors
            .last()
            .ok_or_else(|| binding("located roots", "install root absent"))?;
        exact_entries(install, &[layout.update_name.as_str(), PROVENANCE])
            .map_err(|cause| binding("located install root", cause))?;
        require_volume(install, volume, "located install root")?;
        let update = install
            .open_dir_nofollow(&layout.update_name)
            .map_err(|cause| binding("located update root", cause))?;
        let versions = update
            .open_dir_nofollow("versions")
            .map_err(|cause| binding("located versions root", cause))?;
        let version = versions
            .open_dir_nofollow(&layout.version)
            .map_err(|cause| binding("located version", cause))?;
        let tree = version
            .open_dir_nofollow("tree")
            .map_err(|cause| binding("located tree", cause))?;
        let host = open_host(&tree).map_err(|cause| binding("located host", cause))?;
        if ObjectIdentity::of_cap_file(&host).map_err(|cause| binding("located host", cause))?
            != *executable
        {
            return Err(binding(
                "executable identity",
                "the executable is not the located tree's keld-host.exe",
            ));
        }
        Ok(Self {
            ancestors,
            update,
            versions,
            _version: version,
            _tree: tree,
            _host: host,
        })
    }

    fn install(&self) -> Result<&Dir, UpdateError> {
        self.ancestors
            .last()
            .ok_or_else(|| binding("located roots", "install root absent"))
    }

    /// Every root the record names must be the located root by volume serial and file
    /// ID; the recorded mode's profiles and volume are enforced by `open_roots`.
    fn require_recorded_roots(&self, trust: &WindowsBaselineTrust) -> Result<(), UpdateError> {
        let roots =
            super::open_roots(trust, false).map_err(|cause| binding("recorded roots", cause))?;
        let recorded_install = roots
            .ancestors
            .last()
            .ok_or_else(|| binding("recorded roots", "install root absent"))?;
        for (step, recorded, located) in [
            ("install root identity", recorded_install, self.install()?),
            ("update root identity", &roots.update, &self.update),
            ("versions root identity", &roots.versions, &self.versions),
        ] {
            if ObjectIdentity::of_dir(recorded).map_err(|cause| binding(step, cause))?
                != ObjectIdentity::of_dir(located).map_err(|cause| binding(step, cause))?
            {
                return Err(binding(
                    step,
                    "the record names a root that is not the located root",
                ));
            }
        }
        Ok(())
    }
}

/// Reads, decodes and matches the located record, then admits the same handle against
/// the recorded mode's profile before building trust from it.
fn admit_record(
    install: &Dir,
    expected: &ExpectedAppIdentity,
    volume: &str,
) -> Result<(File, WindowsBaselineTrust), UpdateError> {
    let (record_file, record_bytes) = read_unadmitted_record(install)?;
    let record = records::decode_provenance(&record_bytes)?;
    expected.require_matches(&record.provenance.identity)?;
    let profile = record.provenance.identity.install_mode.protection_profile();
    admit_machine_file(
        install,
        &cap_std::io_lifetimes::AsFilelike::as_filelike_view::<std::fs::File>(&record_file),
        profile,
    )
    .map_err(|cause| binding("protected record profile", cause))?;
    if !record.volume_guid.eq_ignore_ascii_case(volume) {
        return Err(binding(
            "record volume",
            "the record's volume GUID is not the executable's volume",
        ));
    }
    let trust = WindowsBaselineTrust {
        installation: record.provenance.identity,
        owner: record.provenance.owner,
        publisher_scope: record.publisher_scope,
        volume_guid: record.volume_guid,
    };
    trust.require_direct_owner()?;
    Ok((record_file, trust))
}

/// The installation layout a locator names; every component is checked lexically.
#[derive(Debug)]
struct LocatedLayout {
    install_root: PathBuf,
    update_name: String,
    version: String,
}

impl LocatedLayout {
    fn parse(locator: &Path) -> Result<Self, UpdateError> {
        let mut components = locator.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return Err(binding("locator", "not an absolute local drive path"));
        };
        if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
            || components.next() != Some(Component::RootDir)
        {
            return Err(binding("locator", "not an absolute local drive path"));
        }
        let mut names = Vec::new();
        for component in components {
            let Component::Normal(name) = component else {
                return Err(binding("locator", "contains a non-normal component"));
            };
            names.push(
                name.to_str()
                    .ok_or_else(|| binding("locator", "not UTF-8"))?
                    .to_owned(),
            );
        }
        let [install @ .., update, versions, version, tree, host] = names.as_slice() else {
            return Err(binding("locator", "too few components"));
        };
        if install.is_empty() {
            return Err(binding("locator", "install root is a volume root"));
        }
        if host != HOST || tree != "tree" || versions != "versions" {
            return Err(binding(
                "locator",
                "path must end in exactly versions\\<version>\\tree\\keld-host.exe",
            ));
        }
        if update.eq_ignore_ascii_case(PROVENANCE) {
            return Err(binding("locator", "update root conflicts with provenance"));
        }
        records::strict_version(version)
            .map_err(|_| binding("locator", "version directory is not strict SemVer"))?;
        let mut install_root = PathBuf::from(prefix.as_os_str());
        install_root.push(Path::new(r"\"));
        for name in install {
            install_root.push(name);
        }
        Ok(Self {
            install_root,
            update_name: update.clone(),
            version: version.clone(),
        })
    }
}

/// Volume serial number and file ID of an open object.
#[derive(Debug, PartialEq, Eq)]
struct ObjectIdentity {
    volume: u64,
    file: u64,
}

impl ObjectIdentity {
    fn of_file(file: &std::fs::File) -> io::Result<Self> {
        let metadata = cap_std::fs::Metadata::from_file(file)?;
        ensure_regular(&metadata)?;
        Ok(Self {
            volume: metadata.dev(),
            file: metadata.ino(),
        })
    }

    fn of_cap_file(file: &File) -> io::Result<Self> {
        Self::of_file(&cap_std::io_lifetimes::AsFilelike::as_filelike_view::<
            std::fs::File,
        >(file))
    }

    fn of_dir(directory: &Dir) -> io::Result<Self> {
        let metadata = directory.dir_metadata()?;
        Ok(Self {
            volume: metadata.dev(),
            file: metadata.ino(),
        })
    }
}

fn open_host(tree: &Dir) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .follow(FollowSymlinks::No);
    tree.open_with(HOST, &options)
}

/// Reads the located provenance record before its mode is known; the same handle is
/// admitted against the recorded mode's profile before any field is trusted.
fn read_unadmitted_record(install: &Dir) -> Result<(File, Vec<u8>), UpdateError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .follow(FollowSymlinks::No);
    let mut file = install
        .open_with(PROVENANCE, &options)
        .map_err(|cause| binding("protected record open", cause))?;
    let metadata = file
        .metadata()
        .map_err(|cause| binding("protected record size", cause))?;
    let limit = u64::try_from(records::MAX_LOCAL_RECORD_BYTES)
        .map_err(|cause| binding("protected record size", cause))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(binding(
            "protected record size",
            "record is not a regular file of at most 64 KiB",
        ));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|cause| binding("protected record read", cause))?;
    Ok((file, bytes))
}

fn require_volume(directory: &Dir, volume: &str, step: &'static str) -> Result<(), UpdateError> {
    let actual = qualified_volume_root(
        &directory
            .try_clone()
            .map_err(|cause| binding(step, cause))?
            .into_std_file(),
    )
    .map_err(|cause| binding(step, cause))?;
    if actual.eq_ignore_ascii_case(volume) {
        Ok(())
    } else {
        Err(binding(
            step,
            "located root is not on the executable's volume",
        ))
    }
}

fn binding(step: &'static str, detail: impl std::fmt::Display) -> UpdateError {
    UpdateError::ExecutableBinding {
        step,
        detail: detail.to_string(),
    }
}
