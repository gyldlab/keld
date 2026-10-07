//! Executable-located active-package selection (KEL-254 amendment A3 §4, task T2b).
//!
//! The executable path only locates candidate provenance; authenticated provenance
//! supplies authority. The trust anchor is the build-time [`ExpectedAppIdentity`], the
//! located roots' file identities and the recorded mode's OS protection profile; the
//! record's remaining fields are accepted only after those match. The image that locates
//! its installation is a closed [`WindowsLocatedImage`] choice (KEL-53 "Helper launch and
//! self-anchor"), never a free-form name.

use std::io;
use std::path::{Component, Path, PathBuf, Prefix};

use cap_fs_ext::{DirExt as _, MetadataExt as _};
use cap_std::fs::{Dir, File};

use super::{
    ActivePackageSelection, WindowsBaselineTrust, admit_machine_file, error, exact_entries,
    open_pinned_leaf, read_open_record,
};
use crate::windows_extraction::{ensure_regular, open_ancestors};
use crate::windows_fs::qualified_volume_root;
use crate::{ExpectedAppIdentity, UpdateError, WindowsLocatedImage, records};

const PROVENANCE: &str = "install-provenance";

/// Selects the active package of the installation that holds the running executable.
///
/// `image` names which executable is running, `keld-host.exe` or
/// `keld-updater-helper.exe`; the one locator serves both. `locator` is the canonical
/// current executable path and conveys no authority. `executable` is the open handle
/// whose image KEL-135 verified; its file identity, never a reopen by path, binds the
/// selection to the running process. The path must be exactly
/// `<install>\<update>\versions\<strict SemVer>\tree\<image file name>` (names compare
/// exactly, including case); the install root must hold exactly the update root and
/// `install-provenance`; every root the record names must be the located root by volume
/// serial number and file ID on the fixed NTFS volume whose GUID the record carries; and
/// the selected tree's file of that image must be the executable itself.
///
/// `publisher_scope` and `app_id` are what the single `keld-guard` Authenticode owner
/// verified for `executable`'s image. The admitted record must name exactly them, the
/// one rule both images share, before the snapshot lease: a signer that the record does
/// not name reads no activation state and writes nothing.
///
/// The selection is journal-free: a pending activation journal refuses with
/// [`crate::ActivationEffect::JournalBoundRecoveryRequired`]. An invalid `current` is
/// repaired only when last-known-good is the located version, so an executable started
/// from any other tree never causes a write. In `MachineUacDirect` the ordinary process
/// never repairs or recovers: a pending journal or an invalid `current` refuses with the
/// typed [`crate::ActivationEffect::MachineRecoveryRequired`] and writes nothing.
///
/// # Errors
/// [`UpdateError::ExecutableBinding`] when the locator shape, executable identity, located
/// layout or volume, recorded-root identity, record volume or selected version does not
/// bind; [`UpdateError::ProvenanceMismatch`] when the record does not carry the expected
/// app id, channel, target or signing key; [`UpdateError::RecordedSignerMismatch`] when
/// it names another publisher or app than the verified signer; and, typed as
/// [`super::select_windows_active_package`] types them, every refusal to read, decode
/// or admit the protected record against the recorded mode's profile and every
/// `open_roots` refusal of the recorded roots ([`UpdateError::Baseline`], step
/// `recorded roots`).
pub fn select_active_package_for_executable(
    image: WindowsLocatedImage,
    locator: &Path,
    executable: &std::fs::File,
    expected: &ExpectedAppIdentity,
    publisher_scope: &[u8; 32],
    app_id: &str,
) -> Result<ActivePackageSelection, UpdateError> {
    let installation = locate(image, locator, executable, expected)?;
    installation
        .trust
        .require_verified_signer(publisher_scope, app_id)?;
    installation.require_recorded_roots()?;

    let selection =
        super::load::select_with_repair_gate(&installation.trust, Some(installation.located()))?;
    if selection.artifact.version != installation.layout.version {
        return Err(binding(
            image,
            "selected version",
            format!(
                "the installation selects `{}`, not the located version `{}`",
                selection.artifact.version, installation.layout.version
            ),
        ));
    }
    // Defence in depth that A3 §4 requires literally: every located component is held
    // open without delete sharing, so none can be renamed or replaced, and the
    // located-image identity with the version equality above already implies this check.
    let selected = open_image(&selection.tree, image)
        .map_err(|cause| binding(image, "selected image", cause))?;
    if ObjectIdentity::of_cap_file(&selected)
        .map_err(|cause| binding(image, "selected image", cause))?
        != installation.executable
    {
        return Err(binding(
            image,
            "selected image identity",
            format!(
                "the executable is not the selected tree's {}",
                image.file_name()
            ),
        ));
    }
    // The located pins and the record handle are held until the selection is proven.
    drop((selected, installation));
    Ok(selection)
}

/// The installation that one executable image locates, with its record admitted against
/// the recorded mode's profile and every located component held open; nothing beneath
/// the update root has been read and no lease is held.
#[derive(Debug)]
pub(super) struct LocatedInstallation {
    image: WindowsLocatedImage,
    layout: LocatedLayout,
    executable: ObjectIdentity,
    located: Located,
    _record: File,
    /// The located record's claims, accepted only after the expectation, the located
    /// roots' identities and the recorded mode's protection profile matched.
    pub(super) trust: WindowsBaselineTrust,
}

impl LocatedInstallation {
    /// The image and the version whose tree holds it.
    pub(super) fn located(&self) -> super::load::LocatedVersion<'_> {
        super::load::LocatedVersion {
            image: self.image,
            version: &self.layout.version,
        }
    }

    /// Every root the record names must be the located root by volume serial and file
    /// ID; the recorded mode's profiles and volume are enforced by `open_roots`, whose
    /// refusals are typed as the installed-root selector types them.
    pub(super) fn require_recorded_roots(&self) -> Result<(), UpdateError> {
        let roots = super::open_roots(&self.trust, false)
            .map_err(|cause| error("recorded roots", cause))?;
        let recorded_install = roots
            .ancestors
            .last()
            .ok_or_else(|| error("recorded roots", "install root absent"))?;
        for (step, recorded, located) in [
            (
                "install root identity",
                recorded_install,
                self.located.install()?,
            ),
            ("update root identity", &roots.update, &self.located.update),
            (
                "versions root identity",
                &roots.versions,
                &self.located.versions,
            ),
        ] {
            if ObjectIdentity::of_dir(recorded).map_err(|cause| binding(self.image, step, cause))?
                != ObjectIdentity::of_dir(located)
                    .map_err(|cause| binding(self.image, step, cause))?
            {
                return Err(binding(
                    self.image,
                    step,
                    "the record names a root that is not the located root",
                ));
            }
        }
        Ok(())
    }
}

/// Locates `image`'s installation from `locator` and the verified `executable` handle,
/// then reads, decodes and admits its record against `expected` and the recorded mode's
/// protection profile. Nothing beneath the update root is read.
pub(super) fn locate(
    image: WindowsLocatedImage,
    locator: &Path,
    executable: &std::fs::File,
    expected: &ExpectedAppIdentity,
) -> Result<LocatedInstallation, UpdateError> {
    let layout = LocatedLayout::parse(image, locator)?;
    let executable_identity = ObjectIdentity::of_file(executable)
        .map_err(|cause| binding(image, "executable handle", cause))?;
    let volume = qualified_volume_root(executable)
        .map_err(|cause| binding(image, "executable volume", cause))?;
    let located = Located::open(image, &layout, &volume, &executable_identity)?;
    let (record, trust) = admit_record(image, located.install()?, expected, &volume)?;
    Ok(LocatedInstallation {
        image,
        layout,
        executable: executable_identity,
        located,
        _record: record,
        trust,
    })
}

/// The located layout's directories and image, opened without following reparse points
/// and held until selection completes so none can be substituted meanwhile.
#[derive(Debug)]
struct Located {
    image: WindowsLocatedImage,
    ancestors: Vec<Dir>,
    update: Dir,
    versions: Dir,
    _version: Dir,
    _tree: Dir,
    _image: File,
}

impl Located {
    fn open(
        image: WindowsLocatedImage,
        layout: &LocatedLayout,
        volume: &str,
        executable: &ObjectIdentity,
    ) -> Result<Self, UpdateError> {
        let ancestors = open_ancestors(&layout.install_root)
            .map_err(|cause| binding(image, "located roots", cause))?;
        let install = ancestors
            .last()
            .ok_or_else(|| binding(image, "located roots", "install root absent"))?;
        exact_entries(install, &[layout.update_name.as_str(), PROVENANCE])
            .map_err(|cause| binding(image, "located install root", cause))?;
        require_volume(image, install, volume, "located install root")?;
        let update = install
            .open_dir_nofollow(&layout.update_name)
            .map_err(|cause| binding(image, "located update root", cause))?;
        let versions = update
            .open_dir_nofollow("versions")
            .map_err(|cause| binding(image, "located versions root", cause))?;
        let version = versions
            .open_dir_nofollow(&layout.version)
            .map_err(|cause| binding(image, "located version", cause))?;
        let tree = version
            .open_dir_nofollow("tree")
            .map_err(|cause| binding(image, "located tree", cause))?;
        let file =
            open_image(&tree, image).map_err(|cause| binding(image, "located image", cause))?;
        if ObjectIdentity::of_cap_file(&file)
            .map_err(|cause| binding(image, "located image", cause))?
            != *executable
        {
            return Err(binding(
                image,
                "executable identity",
                format!(
                    "the executable is not the located tree's {}",
                    image.file_name()
                ),
            ));
        }
        Ok(Self {
            image,
            ancestors,
            update,
            versions,
            _version: version,
            _tree: tree,
            _image: file,
        })
    }

    fn install(&self) -> Result<&Dir, UpdateError> {
        self.ancestors
            .last()
            .ok_or_else(|| binding(self.image, "located roots", "install root absent"))
    }
}

/// Reads, decodes and matches the located record before its mode is known, then admits
/// the same handle against the recorded mode's profile before building trust from it.
fn admit_record(
    image: WindowsLocatedImage,
    install: &Dir,
    expected: &ExpectedAppIdentity,
    volume: &str,
) -> Result<(File, WindowsBaselineTrust), UpdateError> {
    let record_file = open_pinned_leaf(install, PROVENANCE)
        .map_err(|cause| error("protected record open", cause))?;
    let (record_file, record_bytes) = read_open_record(record_file)?;
    let record = records::decode_provenance(&record_bytes)?;
    expected.require_matches(&record.provenance.identity)?;
    let profile = record.provenance.identity.install_mode.protection_profile();
    admit_machine_file(
        install,
        &cap_std::io_lifetimes::AsFilelike::as_filelike_view::<std::fs::File>(&record_file),
        profile,
    )
    .map_err(|cause| error("protected record profile", cause))?;
    if !record.volume_guid.eq_ignore_ascii_case(volume) {
        return Err(binding(
            image,
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
    fn parse(image: WindowsLocatedImage, locator: &Path) -> Result<Self, UpdateError> {
        let mut components = locator.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return Err(binding(
                image,
                "locator",
                "not an absolute local drive path",
            ));
        };
        if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
            || components.next() != Some(Component::RootDir)
        {
            return Err(binding(
                image,
                "locator",
                "not an absolute local drive path",
            ));
        }
        let mut names = Vec::new();
        for component in components {
            let Component::Normal(name) = component else {
                return Err(binding(image, "locator", "contains a non-normal component"));
            };
            names.push(
                name.to_str()
                    .ok_or_else(|| binding(image, "locator", "not UTF-8"))?
                    .to_owned(),
            );
        }
        let [install @ .., update, versions, version, tree, file] = names.as_slice() else {
            return Err(binding(image, "locator", "too few components"));
        };
        if install.is_empty() {
            return Err(binding(image, "locator", "install root is a volume root"));
        }
        if file != image.file_name() || tree != "tree" || versions != "versions" {
            return Err(binding(
                image,
                "locator",
                format!(
                    "path must end in exactly versions\\<version>\\tree\\{}",
                    image.file_name()
                ),
            ));
        }
        if update.eq_ignore_ascii_case(PROVENANCE) {
            return Err(binding(
                image,
                "locator",
                "update root conflicts with provenance",
            ));
        }
        records::strict_version(version)
            .map_err(|_| binding(image, "locator", "version directory is not strict SemVer"))?;
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

/// Opens `image`'s file in `tree` as a pinned leaf: no reparse point is followed and
/// nothing but reads is shared.
fn open_image(tree: &Dir, image: WindowsLocatedImage) -> io::Result<File> {
    open_pinned_leaf(tree, image.file_name())
}

fn require_volume(
    image: WindowsLocatedImage,
    directory: &Dir,
    volume: &str,
    step: &'static str,
) -> Result<(), UpdateError> {
    let actual = qualified_volume_root(
        &directory
            .try_clone()
            .map_err(|cause| binding(image, step, cause))?
            .into_std_file(),
    )
    .map_err(|cause| binding(image, step, cause))?;
    if actual.eq_ignore_ascii_case(volume) {
        Ok(())
    } else {
        Err(binding(
            image,
            step,
            "located root is not on the executable's volume",
        ))
    }
}

fn binding(
    image: WindowsLocatedImage,
    step: &'static str,
    detail: impl std::fmt::Display,
) -> UpdateError {
    UpdateError::ExecutableBinding {
        image,
        step,
        detail: detail.to_string(),
    }
}
