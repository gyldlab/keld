//! Unpublished Windows staging bound to verified bytes and retained OS objects.

use std::collections::BTreeMap;
use std::fs::{File as StdFile, OpenOptions as StdOpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Component, Path, PathBuf, Prefix};

use cap_fs_ext::{
    DirExt as _, FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt as _, OsMetadataExt as _,
};
use cap_std::fs::{Dir, File, Metadata, OpenOptions, OpenOptionsExt as _};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE, WRITE_DAC,
};

use crate::provenance::match_identity;
use crate::windows_fs::{
    create_directory_relative, create_directory_relative_with_profile,
    create_file_relative_with_profile, qualify_volume,
};
use crate::{
    AdmittedInstallation, ArchiveEntryKind, ArtifactDomain, ArtifactIdentity, DirectInstallMode,
    DirectInstallationIdentity, LoadedWindowsBaseline, ProvenanceField, UpdateError,
    ValidatedArchive, VerifiedFull, VersionPublicationOutcome,
};

const COPY_BYTES: usize = 16 * 1024;

/// Protected fixed-NTFS staging directories retained by the host.
///
/// Owner-private roots prove filesystem protection only. Machine roots retain their
/// real loader lease and require SYSTEM for staging. Neither creates installation
/// scaffolding nor proves live strict-profile admission.
#[derive(Debug)]
pub struct WindowsExtractionRoot {
    installation: DirectInstallationIdentity,
    floor: semver::Version,
    authority: RootAuthority,
    versions: Dir,
}

#[derive(Debug)]
enum RootAuthority {
    OwnerPrivate {
        _ancestors: Vec<Dir>,
    },
    Machine {
        _loaded: Box<LoadedWindowsBaseline>,
    },
    ActivationWriter {
        snapshot: Box<crate::windows_baseline::WindowsActivationWriteSnapshot>,
    },
}

impl RootAuthority {
    fn validate_parent(&self, directory: &StdFile) -> io::Result<()> {
        match self {
            Self::OwnerPrivate { .. } => {
                keld_guard::validate_windows_owner_private_directory(directory)
            }
            Self::Machine { .. } => {
                keld_guard::require_windows_system_token()?;
                keld_guard::validate_windows_machine_directory(directory)
            }
            Self::ActivationWriter { snapshot } => {
                keld_guard::validate_windows_install_directory(directory, snapshot.profile())
            }
        }
    }
}

/// Flushed and read-back bytes in an unpublished, incomplete Windows stage.
///
/// Retains all directory and read handles, and exclusively borrows its root.
/// It is neither a runnable version nor a durable publication receipt. Dropping
/// it releases handles and leaves the named incomplete stage for diagnosis.
#[derive(Debug)]
pub struct ExtractedWindowsStage<'root> {
    root: &'root mut WindowsExtractionRoot,
    name: String,
    identity: ArtifactIdentity,
    content_size: u64,
    directories: Vec<Dir>,
    files: Vec<File>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VersionPublicationBoundary {
    CompleteMarkerPublished,
    BeforeVersionRename,
    VersionDirectoryPublished,
}

impl ExtractedWindowsStage<'_> {
    /// Signed identity of the staged bytes, without activation authority.
    #[must_use]
    pub const fn identity(&self) -> &ArtifactIdentity {
        &self.identity
    }

    /// Single diagnostic directory name beneath the admitted root's `versions`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Publishes this verified stage as one complete, immutable version directory.
    ///
    /// This does not change the version floor, active pointer, known-good slots or
    /// activation journal. The stage is consumable only when its root retains the
    /// installation-wide activation writer lease.
    ///
    /// # Errors
    /// Refuses a stage without the writer lease, any stage or destination substitution,
    /// profile/readback mismatch, and any failure at the completion-marker or
    /// absent-target version-publication boundary. A post-rename readback failure
    /// reports that the destination may exist but remains unselected.
    pub fn publish_version(self) -> Result<ArtifactIdentity, UpdateError> {
        self.publish_version_inner(|_, _| Ok(()))
    }

    #[cfg(test)]
    pub(crate) fn publish_version_with_observer(
        self,
        observe: impl FnMut(VersionPublicationBoundary, &str) -> io::Result<()>,
    ) -> Result<ArtifactIdentity, UpdateError> {
        self.publish_version_inner(observe)
    }

    fn publish_version_inner(
        self,
        mut observe: impl FnMut(VersionPublicationBoundary, &str) -> io::Result<()>,
    ) -> Result<ArtifactIdentity, UpdateError> {
        let stage_name = self.name.clone();
        let candidate = self.identity.clone();
        let content_size = self.content_size;
        let root = &*self.root;
        let snapshot = match &root.authority {
            RootAuthority::ActivationWriter { snapshot } => snapshot.as_ref(),
            RootAuthority::OwnerPrivate { .. } | RootAuthority::Machine { .. } => {
                return Err(version_publication_error(
                    &candidate,
                    &stage_name,
                    VersionPublicationOutcome::StageRetained,
                    "publishing a version requires the retained exclusive activation-writer lease",
                ));
            }
        };
        let stage = self.directories.first().ok_or_else(|| {
            version_publication_error(
                &candidate,
                &stage_name,
                VersionPublicationOutcome::StageRetained,
                "retained stage directory is absent",
            )
        })?;
        prepare_complete_stage(
            root,
            stage,
            snapshot.profile(),
            &candidate,
            &stage_name,
            content_size,
            &mut observe,
        )?;

        // This cloned directory handle has no delete sharing. Release it with the
        // retained tree/file handles before the same-volume directory rename.
        drop(self.files);
        drop(self.directories);
        publish_complete_stage(
            root,
            snapshot,
            &candidate,
            &stage_name,
            content_size,
            &mut observe,
        )?;
        Ok(candidate)
    }
}

fn prepare_complete_stage(
    root: &WindowsExtractionRoot,
    stage: &Dir,
    profile: keld_guard::WindowsInstallProtectionProfile,
    candidate: &ArtifactIdentity,
    stage_name: &str,
    content_size: u64,
    observe: &mut impl FnMut(VersionPublicationBoundary, &str) -> io::Result<()>,
) -> Result<(), UpdateError> {
    let stage_file = stage
        .try_clone()
        .map_err(|cause| {
            version_publication_error(
                candidate,
                stage_name,
                VersionPublicationOutcome::StageRetained,
                format!("retained stage directory clone failed: {cause}"),
            )
        })?
        .into_std_file();
    root.authority
        .validate_parent(&stage_file)
        .map_err(|cause| {
            version_publication_error(
                candidate,
                stage_name,
                VersionPublicationOutcome::StageRetained,
                format!("staging profile changed before completion: {cause}"),
            )
        })?;
    drop(stage_file);
    crate::windows_baseline::exact_entries(stage, &["content.tar", "tree"]).map_err(|cause| {
        version_publication_error(
            candidate,
            stage_name,
            VersionPublicationOutcome::StageRetained,
            format!("incomplete stage contents are not exact: {cause}"),
        )
    })?;
    let collision =
        version_target_collides(&root.versions, &candidate.version).map_err(|cause| {
            version_publication_error(
                candidate,
                stage_name,
                VersionPublicationOutcome::StageRetained,
                format!("version target census failed before completion: {cause}"),
            )
        })?;
    if collision {
        return Err(version_publication_error(
            candidate,
            stage_name,
            VersionPublicationOutcome::StageRetained,
            "candidate version collides with an existing Windows case-insensitive name",
        ));
    }
    let complete = crate::records::encode_complete(candidate, content_size).map_err(|cause| {
        version_publication_error(
            candidate,
            stage_name,
            VersionPublicationOutcome::StageRetained,
            format!("completion record encoding failed: {cause}"),
        )
    })?;
    crate::windows_baseline::publish_new_record(stage, ".complete", &complete, profile).map_err(
        |cause| {
            version_publication_error(
                candidate,
                stage_name,
                VersionPublicationOutcome::StageRetained,
                format!("completion record was not published: {cause}"),
            )
        },
    )?;
    observe(
        VersionPublicationBoundary::CompleteMarkerPublished,
        stage_name,
    )
    .map_err(|cause| {
        version_publication_error(
            candidate,
            stage_name,
            VersionPublicationOutcome::StageRetained,
            format!("failure after completion-marker publication: {cause}"),
        )
    })?;
    crate::windows_baseline::exact_entries(stage, &[".complete", "content.tar", "tree"]).map_err(
        |cause| {
            version_publication_error(
                candidate,
                stage_name,
                VersionPublicationOutcome::StageRetained,
                format!("completed stage contents are not exact: {cause}"),
            )
        },
    )
}

fn publish_complete_stage(
    root: &WindowsExtractionRoot,
    snapshot: &crate::WindowsActivationWriteSnapshot,
    candidate: &ArtifactIdentity,
    stage_name: &str,
    content_size: u64,
    observe: &mut impl FnMut(VersionPublicationBoundary, &str) -> io::Result<()>,
) -> Result<(), UpdateError> {
    let versions = root
        .versions
        .try_clone()
        .map_err(|cause| {
            version_publication_error(
                candidate,
                stage_name,
                VersionPublicationOutcome::StageRetained,
                format!("retained versions directory clone failed: {cause}"),
            )
        })?
        .into_std_file();
    root.authority.validate_parent(&versions).map_err(|cause| {
        version_publication_error(
            candidate,
            stage_name,
            VersionPublicationOutcome::StageRetained,
            format!("versions profile changed before rename: {cause}"),
        )
    })?;
    observe(VersionPublicationBoundary::BeforeVersionRename, stage_name).map_err(|cause| {
        version_publication_error(
            candidate,
            stage_name,
            VersionPublicationOutcome::StageRetained,
            format!("failure before version-directory rename: {cause}"),
        )
    })?;
    crate::windows_fs::publish_new(&versions, stage_name, &candidate.version).map_err(|cause| {
        version_publication_error(
            candidate,
            stage_name,
            VersionPublicationOutcome::DestinationUnconfirmed,
            format!("absent-target write-through rename did not confirm its effect: {cause}"),
        )
    })?;
    observe(
        VersionPublicationBoundary::VersionDirectoryPublished,
        stage_name,
    )
    .map_err(|cause| {
        version_publication_error(
            candidate,
            stage_name,
            VersionPublicationOutcome::DestinationUnconfirmed,
            format!("failure after version-directory rename: {cause}"),
        )
    })?;
    snapshot
        .verify_published_version(candidate, content_size)
        .map_err(|cause| {
            version_publication_error(
                candidate,
                stage_name,
                VersionPublicationOutcome::DestinationUnconfirmed,
                format!("final version readback failed: {cause}"),
            )
        })
}

fn version_publication_error(
    candidate: &ArtifactIdentity,
    stage_name: &str,
    outcome: VersionPublicationOutcome,
    detail: impl std::fmt::Display,
) -> UpdateError {
    UpdateError::VersionPublication {
        version: candidate.version.clone(),
        stage_name: stage_name.to_owned(),
        outcome,
        detail: detail.to_string(),
    }
}

fn version_target_collides(versions: &Dir, candidate: &str) -> io::Result<bool> {
    for entry in versions.entries()? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| io::Error::other("non-UTF-8 entry in versions directory"))?;
        if keld_guard::validate_windows_package_paths(&[candidate, &name]).is_err() {
            return Ok(true);
        }
    }
    Ok(false)
}

impl AdmittedInstallation {
    /// Opens and validates the existing protected Windows staging scaffold.
    ///
    /// # Errors
    /// Refuses unsupported paths/volumes, reparses, missing `versions`, or any
    /// root descriptor outside the exact provenance-selected protection profile.
    pub fn open_windows_extraction_root(&self) -> Result<WindowsExtractionRoot, UpdateError> {
        open_root(self).map_err(|error| extraction_error(None, "root admission", error))
    }
}

fn open_root(admitted: &AdmittedInstallation) -> io::Result<WindowsExtractionRoot> {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExtractionEvent {
    PreCreate,
    AfterStageCreate,
    BeforePayloadWrite,
    BeforeMember,
    BeforeFileFlush,
    BeforeReadback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StageProtection {
    OwnerPrivate,
    Machine,
    MachineUac,
}

impl StageProtection {
    const fn install_profile(self) -> keld_guard::WindowsInstallProtectionProfile {
        match self {
            Self::OwnerPrivate => keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
            Self::Machine => keld_guard::WindowsInstallProtectionProfile::MachineSystem,
            Self::MachineUac => keld_guard::WindowsInstallProtectionProfile::MachineUac,
        }
    }

    pub(crate) fn create_directory(self, parent: &StdFile, component: &str) -> io::Result<StdFile> {
        match self {
            Self::MachineUac => {
                create_directory_relative_with_profile(parent, component, self.install_profile())
            }
            Self::OwnerPrivate | Self::Machine => create_directory_relative(parent, component),
        }
    }
}

#[derive(Clone, Copy)]
struct CopyRange {
    offset: u64,
    size: u64,
}

impl WindowsExtractionRoot {
    pub(crate) fn from_activation_write_snapshot(
        snapshot: crate::windows_baseline::WindowsActivationWriteSnapshot,
    ) -> Result<Self, UpdateError> {
        let installation = snapshot.identity().clone();
        match installation.install_mode {
            DirectInstallMode::PerUserDirect => {}
            #[cfg(test)]
            DirectInstallMode::MachineUacDirect => {}
            #[cfg(not(test))]
            DirectInstallMode::MachineUacDirect => {
                return Err(extraction_error(
                    None,
                    "machine-uac activation authority",
                    refusal("production UAC staging requires authenticated helper admission"),
                ));
            }
            DirectInstallMode::MachineSeamlessDirect => {
                return Err(extraction_error(
                    None,
                    "machine-seamless writer mechanism",
                    refusal("MachineSeamlessDirect remains gated on its privileged coordinator"),
                ));
            }
        }
        let versions = snapshot
            .retained_versions()
            .map_err(|error| extraction_error(None, "writer versions handle", error))?;
        let parent = versions
            .try_clone()
            .map_err(|error| extraction_error(None, "writer versions handle", error))?
            .into_std_file();
        keld_guard::validate_windows_install_directory(&parent, snapshot.profile())
            .map_err(|error| extraction_error(None, "writer versions profile", error))?;
        let floor = snapshot.floor().clone();
        Ok(Self {
            installation,
            floor,
            authority: RootAuthority::ActivationWriter {
                snapshot: Box::new(snapshot),
            },
            versions,
        })
    }

    /// Starts the common journaled activation for a version published under this root.
    ///
    /// The root must retain the exclusive activation-writer lease that published the
    /// candidate. Under that same lease the transaction re-verifies and pins the
    /// complete candidate and every referenced version, mints fresh attempt, health and
    /// lifecycle identities, durably journals `PublishPending`, advances the floor,
    /// selects `current` and journals `AwaitingHealth`. The returned attempt keeps the
    /// lease until health commits or rolls it back.
    ///
    /// # Errors
    /// Refuses a root without the writer lease, a zero coordinator digest, a candidate
    /// outside the installation scope or not above the floor, any unverified or
    /// unreferenced version, or a failed durable step. Effects are reported by
    /// [`crate::ActivationEffect`].
    pub fn begin_activation(
        self,
        candidate: &ArtifactIdentity,
        coordinator_image_blake3: [u8; 32],
    ) -> Result<crate::WindowsActivationAttempt, UpdateError> {
        let Self {
            authority,
            versions,
            ..
        } = self;
        drop(versions);
        match authority {
            RootAuthority::ActivationWriter { snapshot } => {
                snapshot.begin_activation(candidate, coordinator_image_blake3)
            }
            RootAuthority::OwnerPrivate { .. } | RootAuthority::Machine { .. } => {
                Err(UpdateError::Activation {
                    step: "activation writer authority",
                    effect: crate::ActivationEffect::ProtectedStateUnchanged,
                    detail: "activation requires the retained exclusive activation-writer lease"
                        .to_owned(),
                })
            }
        }
    }

    pub(crate) fn from_loaded(loaded: LoadedWindowsBaseline) -> Result<Self, UpdateError> {
        let mode = loaded.identity().install_mode;
        match mode {
            DirectInstallMode::MachineSeamlessDirect => {
                keld_guard::require_windows_system_token()
                    .map_err(|error| extraction_error(None, "machine staging authority", error))?;
            }
            DirectInstallMode::MachineUacDirect => {
                return Err(extraction_error(
                    None,
                    "machine-uac activation authority",
                    refusal(
                        "a read-only baseline snapshot does not carry the authenticated exclusive writer lease",
                    ),
                ));
            }
            DirectInstallMode::PerUserDirect => {
                return Err(extraction_error(
                    None,
                    "machine staging mode",
                    refusal("machine staging requires a machine-wide direct mode"),
                ));
            }
        }
        let versions = loaded
            .retained_versions()
            .map_err(|error| extraction_error(None, "machine versions handle", error))?;
        let parent = versions
            .try_clone()
            .map_err(|error| extraction_error(None, "machine versions handle", error))?
            .into_std_file();
        match mode {
            DirectInstallMode::MachineSeamlessDirect => {
                keld_guard::validate_windows_machine_directory(&parent).map_err(|error| {
                    extraction_error(None, "machine versions protection", error)
                })?;
            }
            DirectInstallMode::MachineUacDirect | DirectInstallMode::PerUserDirect => {
                unreachable!("mode checked above")
            }
        }
        let installation = loaded.identity().clone();
        let floor =
            crate::provenance::validate_version_floor(&installation, loaded.version_floor())?;
        Ok(Self {
            installation,
            floor,
            // Retain the loaded identity/ancestry until activation consumes these pins.
            authority: match mode {
                DirectInstallMode::MachineSeamlessDirect => RootAuthority::Machine {
                    _loaded: Box::new(loaded),
                },
                DirectInstallMode::MachineUacDirect | DirectInstallMode::PerUserDirect => {
                    unreachable!("mode checked above")
                }
            },
            versions,
        })
    }

    /// Stages one context-matching, authenticated canonical package.
    ///
    /// The source is opened and locked internally. Full preflight precedes creation;
    /// retained copies are flushed and read back before returning exclusive ownership.
    /// No `.complete`, runnable pointer, version floor or activation state is written.
    ///
    /// # Errors
    /// Refuses mismatched admission/floor, invalid or writable source, unsupported
    /// archive, any collision or I/O failure. Post-creation errors name the incomplete
    /// stage; callers must preserve it for diagnosis rather than treat it as runnable.
    pub fn extract<'root>(
        &'root mut self,
        verified: &VerifiedFull,
        archive: &Path,
    ) -> Result<ExtractedWindowsStage<'root>, UpdateError> {
        self.extract_inner(verified, archive, |_, _| Ok(()))
    }

    fn extract_inner<'root>(
        &'root mut self,
        verified: &VerifiedFull,
        archive: &Path,
        mut observe: impl FnMut(ExtractionEvent, &str) -> io::Result<()>,
    ) -> Result<ExtractedWindowsStage<'root>, UpdateError> {
        match_identity(&self.installation, &verified.installation)?;
        let candidate = semver::Version::parse(&verified.identity().version)
            .map_err(|error| extraction_error(None, "candidate version", error))?;
        if !candidate.cmp_precedence(&self.floor).is_gt() {
            return Err(UpdateError::ProvenanceMismatch {
                field: ProvenanceField::VersionFloor,
                expected: format!("candidate precedence above {}", self.floor),
                found: verified.identity().version.clone(),
            });
        }
        let mut source = open_source(archive)
            .map_err(|error| extraction_error(None, "source admission", error))?;
        let validated = verified.validate_windows_archive(&mut source)?;
        let name = crate::windows_baseline::random_leaf_name("incomplete")
            .map_err(|error| extraction_error(None, "stage identity", error))?;
        let protection = match self.installation.install_mode {
            DirectInstallMode::PerUserDirect | DirectInstallMode::MachineSeamlessDirect => {
                StageProtection::OwnerPrivate
            }
            DirectInstallMode::MachineUacDirect
                if matches!(self.authority, RootAuthority::ActivationWriter { .. }) =>
            {
                StageProtection::MachineUac
            }
            DirectInstallMode::MachineUacDirect => {
                return Err(extraction_error(
                    None,
                    "machine-uac activation authority",
                    refusal("protected staging requires the authenticated exclusive writer lease"),
                ));
            }
        };
        observe(ExtractionEvent::PreCreate, &name)
            .map_err(|error| extraction_error(None, "before creation", error))?;
        // Re-observe policy at the mutation boundary; never repair a changed ACL.
        let parent = self
            .versions
            .try_clone()
            .map_err(|error| extraction_error(None, "versions handle", error))?
            .into_std_file();
        self.authority
            .validate_parent(&parent)
            .map_err(|error| extraction_error(None, "versions protection", error))?;
        let stage = protection
            .create_directory(&parent, &name)
            .map_err(|error| {
                extraction_error(Some(&name), "stage creation (outcome unconfirmed)", error)
            })?;
        let result = populate_stage(
            stage,
            &name,
            &validated,
            &mut source,
            protection,
            &mut observe,
        );
        let (directories, files) =
            result.map_err(|error| extraction_error(Some(&name), "stage contents", error))?;
        Ok(ExtractedWindowsStage {
            root: self,
            name,
            identity: verified.identity().clone(),
            content_size: verified.content_size(),
            directories,
            files,
        })
    }
}

pub(crate) fn open_source(path: &Path) -> io::Result<File> {
    let file = StdOpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let file = File::from_std(file);
    ensure_regular(&file.metadata()?)?;
    Ok(file)
}

pub(crate) fn populate_stage(
    stage: StdFile,
    name: &str,
    validated: &ValidatedArchive,
    source: &mut File,
    protection: StageProtection,
    observe: &mut impl FnMut(ExtractionEvent, &str) -> io::Result<()>,
) -> Result<(Vec<Dir>, Vec<File>), UpdateError> {
    let stage = Dir::from_std_file(stage);
    let mut directories = vec![stage];
    let mut files = Vec::new();
    let operation = |error| extraction_error(Some(name), "file or directory I/O", error);
    observe(ExtractionEvent::AfterStageCreate, name).map_err(operation)?;
    let (mut retained_content, copied_digest) = copy_read_back(
        &directories[0],
        "content.tar",
        "content.tar",
        source,
        CopyRange {
            offset: 0,
            size: validated.content_size(),
        },
        protection,
        observe,
    )
    .map_err(operation)?;
    if &copied_digest != validated.content_blake3() {
        return Err(UpdateError::ArtifactDigestMismatch {
            domain: ArtifactDomain::Content,
            expected: crate::error::hex_digest(validated.content_blake3()),
            actual: crate::error::hex_digest(&copied_digest),
        });
    }
    let tree_parent = directories[0]
        .try_clone()
        .map_err(operation)?
        .into_std_file();
    let tree = protection
        .create_directory(&tree_parent, "tree")
        .map_err(operation)?;
    directories.push(Dir::from_std_file(tree));
    let mut parents = BTreeMap::from([("", 1_usize)]);
    for entry in validated.entries() {
        let (parent_name, leaf) = entry.name().rsplit_once('/').unwrap_or(("", entry.name()));
        let parent_index = *parents.get(parent_name).ok_or_else(|| {
            extraction_error(Some(name), "directory order", "validated parent is absent")
        })?;
        let parent = &directories[parent_index];
        observe(ExtractionEvent::BeforeMember, entry.name()).map_err(operation)?;
        match entry.kind() {
            ArchiveEntryKind::Directory => {
                let parent_file = parent.try_clone().map_err(operation)?.into_std_file();
                let child = protection
                    .create_directory(&parent_file, leaf)
                    .map_err(operation)?;
                parents.insert(entry.name(), directories.len());
                directories.push(Dir::from_std_file(child));
            }
            ArchiveEntryKind::File => {
                let (file, _) = copy_read_back(
                    parent,
                    leaf,
                    entry.name(),
                    &mut retained_content,
                    CopyRange {
                        offset: entry.data_offset(),
                        size: entry.size(),
                    },
                    protection,
                    observe,
                )
                .map_err(operation)?;
                files.push(file);
            }
        }
    }
    files.push(retained_content);
    Ok((directories, files))
}

fn copy_read_back(
    parent: &Dir,
    leaf: &str,
    diagnostic: &str,
    source: &mut File,
    range: CopyRange,
    protection: StageProtection,
    observe: &mut impl FnMut(ExtractionEvent, &str) -> io::Result<()>,
) -> io::Result<(File, [u8; 32])> {
    let CopyRange { offset, size } = range;
    if protection == StageProtection::Machine {
        keld_guard::require_windows_system_token()?;
    }
    let mut output = match protection {
        StageProtection::MachineUac => File::from_std(create_file_relative_with_profile(
            &parent.try_clone()?.into_std_file(),
            leaf,
            protection.install_profile(),
        )?),
        StageProtection::OwnerPrivate | StageProtection::Machine => {
            let mut options = OpenOptions::new();
            options
                .write(true)
                .create_new(true)
                .share_mode(FILE_SHARE_READ)
                .follow(FollowSymlinks::No);
            if protection == StageProtection::Machine {
                options.access_mode(FILE_GENERIC_WRITE | WRITE_DAC);
            }
            parent.open_with(leaf, &options)?
        }
    };
    let original = output.metadata()?;
    ensure_regular(&original)?;
    if original.dev() != parent.dir_metadata()?.dev() {
        return Err(refusal("created file crosses its parent volume"));
    }
    match protection {
        StageProtection::OwnerPrivate | StageProtection::Machine => {
            keld_guard::validate_windows_owner_private_file(&output.try_clone()?.into_std())?;
        }
        StageProtection::MachineUac => {
            keld_guard::validate_windows_admin_machine_file(&output.try_clone()?.into_std())?;
        }
    }
    if protection == StageProtection::MachineUac {
        observe(ExtractionEvent::BeforePayloadWrite, diagnostic)?;
    }
    source.seek(SeekFrom::Start(offset))?;
    let mut left = size;
    let mut buffer = [0_u8; COPY_BYTES];
    let mut digest = blake3::Hasher::new();
    while left != 0 {
        let amount = usize::try_from(left.min(COPY_BYTES as u64)).map_err(io::Error::other)?;
        source.read_exact(&mut buffer[..amount])?;
        output.write_all(&buffer[..amount])?;
        digest.update(&buffer[..amount]);
        left -= amount as u64;
    }
    let mut output = output.into_std();
    if protection == StageProtection::Machine {
        // Seal on the original writer, before its final flush. Reopening a data
        // writer after readback would conflict with the retained read-only pins.
        keld_guard::seal_windows_machine_file(&mut output)?;
    }
    observe(ExtractionEvent::BeforeFileFlush, diagnostic)?;
    output.sync_all()?;
    drop(output);
    observe(ExtractionEvent::BeforeReadback, diagnostic)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .follow(FollowSymlinks::No);
    let mut retained = parent.open_with(leaf, &options)?;
    let observed = retained.metadata()?;
    ensure_regular(&observed)?;
    if observed.dev() != original.dev()
        || observed.ino() != original.ino()
        || observed.len() != size
    {
        return Err(refusal("readback object identity or length changed"));
    }
    let retained_object = retained.try_clone()?.into_std();
    match protection {
        StageProtection::OwnerPrivate | StageProtection::Machine | StageProtection::MachineUac => {
            keld_guard::validate_windows_install_file(
                &retained_object,
                protection.install_profile(),
            )?;
        }
    }
    let mut readback = blake3::Hasher::new();
    loop {
        let read = retained.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        readback.update(&buffer[..read]);
    }
    let expected = *digest.finalize().as_bytes();
    if readback.finalize().as_bytes() != &expected {
        return Err(refusal("flushed file readback digest changed"));
    }
    Ok((retained, expected))
}

pub(crate) fn ensure_directory(metadata: &Metadata) -> io::Result<()> {
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(refusal("expected a non-reparse directory"));
    }
    Ok(())
}

pub(crate) fn ensure_regular(metadata: &Metadata) -> io::Result<()> {
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.nlink() != 1
    {
        return Err(refusal("expected a non-reparse regular file with one link"));
    }
    Ok(())
}

fn refusal(detail: impl Into<String>) -> io::Error {
    io::Error::other(detail.into())
}

fn extraction_error(
    stage: Option<&str>,
    step: &'static str,
    error: impl std::fmt::Display,
) -> UpdateError {
    UpdateError::Extraction {
        incomplete_stage: stage.map(str::to_owned),
        step,
        detail: error.to_string(),
    }
}

#[cfg(test)]
pub(crate) mod lpac_probe;
#[cfg(test)]
mod tests;
