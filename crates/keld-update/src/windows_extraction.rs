//! Unpublished Windows staging bound to verified bytes and retained OS objects.

use std::fs::{File as StdFile, OpenOptions as StdOpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;

use cap_fs_ext::{MetadataExt as _, OsMetadataExt as _};
use cap_std::fs::{Dir, File, Metadata};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

use crate::provenance::match_identity;
use crate::windows_fs::{create_directory_relative, create_directory_relative_with_profile};
use crate::{
    AdmittedInstallation, ArtifactIdentity, DirectInstallMode, DirectInstallationIdentity,
    LoadedWindowsBaseline, ProvenanceField, UpdateError, VerifiedFull, VersionPublicationOutcome,
};

mod root_admission;
pub(crate) use root_admission::open_ancestors;
mod stage_io;
#[cfg(test)]
use stage_io::copy_read_back;
pub(crate) use stage_io::populate_stage;

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
}

/// A stage whose completion record is published, still under its `incomplete-*` name.
///
/// It selects nothing and is not a version: only
/// [`WindowsExtractionRoot::begin_activation`] journals an attempt and then renames
/// it to its version name. Until then the stage is a tolerated diagnostic leftover.
#[derive(Debug)]
pub struct CompletedWindowsStage {
    name: String,
    identity: ArtifactIdentity,
}

impl CompletedWindowsStage {
    /// Signed identity recorded in the stage's completion record.
    #[must_use]
    pub const fn identity(&self) -> &ArtifactIdentity {
        &self.identity
    }

    /// Generated `incomplete-*` directory name beneath the admitted root's `versions`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
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

    /// Publishes this verified stage's completion record without renaming it.
    ///
    /// The stage keeps its generated `incomplete-*` name, so a crash at any point
    /// leaves only a tolerated leftover and never an unreferenced version.
    /// [`WindowsExtractionRoot::begin_activation`] journals the attempt before it
    /// renames the stage to its version name. This changes no version floor, pointer,
    /// known-good slot or journal.
    ///
    /// # Errors
    /// Refuses a stage without the writer lease, any stage or destination substitution,
    /// a case-insensitive version-name collision, and any failure at the
    /// completion-marker boundary. The stage is retained in every case.
    pub fn complete(self) -> Result<CompletedWindowsStage, UpdateError> {
        self.complete_inner(|_, _| Ok(()))
    }

    #[cfg(test)]
    pub(crate) fn complete_with_observer(
        self,
        observe: impl FnMut(VersionPublicationBoundary, &str) -> io::Result<()>,
    ) -> Result<CompletedWindowsStage, UpdateError> {
        self.complete_inner(observe)
    }

    fn complete_inner(
        self,
        mut observe: impl FnMut(VersionPublicationBoundary, &str) -> io::Result<()>,
    ) -> Result<CompletedWindowsStage, UpdateError> {
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
        // The retained tree/file handles have no delete sharing; release them so the
        // journaled transaction can rename the stage.
        drop(self.files);
        drop(self.directories);
        Ok(CompletedWindowsStage {
            name: stage_name,
            identity: candidate,
        })
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
        root_admission::open_root(self)
            .map_err(|error| extraction_error(None, "root admission", error))
    }
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

    /// Mints the attempt identities of the common journaled activation for a completed
    /// stage, writing nothing.
    ///
    /// The root must retain the exclusive activation-writer lease. The stage value is
    /// not bound to the lease session that completed it: under this root's lease the
    /// transaction re-reads the stage's completion record, re-verifies and pins every
    /// referenced version and mints fresh attempt, health and lifecycle identities that
    /// no record names yet. The owner creates its connect-back endpoint from them, then
    /// [`crate::WindowsMintedAttempt::journal`] durably journals `PublishPending` with its
    /// owner facts. Only then does the transaction rename the stage to its version name,
    /// fully re-verify and pin the candidate, advance the floor, select `current` and
    /// journal `AwaitingHealth`. The attempt keeps the lease until health commits or
    /// rolls it back.
    ///
    /// `coordinator` is the attempt owner's own image from its one `keld-guard`
    /// Authenticode verification. The journaled `helper_image_blake3` is derived inside
    /// `keld-update` from every byte of that image's pinned handle; no caller computes
    /// or supplies the digest (KEL-53 §4 "Candidate connect-back", *Order*). The
    /// verified image is the only accepted argument:
    ///
    /// ```no_run
    /// # use keld_update::{CompletedWindowsStage, WindowsExtractionRoot};
    /// fn begin(
    ///     root: WindowsExtractionRoot,
    ///     stage: CompletedWindowsStage,
    ///     coordinator: &keld_guard::VerifiedWindowsImage,
    /// ) {
    ///     let _ = root.begin_activation(stage, coordinator);
    /// }
    /// ```
    ///
    /// The same call with a bare file does not compile. Only the parameter type differs
    /// from the example above, which pins every other part of the snippet; rustdoc
    /// checks the error code on nightly builds only:
    ///
    /// ```compile_fail,E0308
    /// # use keld_update::{CompletedWindowsStage, WindowsExtractionRoot};
    /// fn begin(
    ///     root: WindowsExtractionRoot,
    ///     stage: CompletedWindowsStage,
    ///     coordinator: &std::fs::File,
    /// ) {
    ///     let _ = root.begin_activation(stage, coordinator);
    /// }
    /// ```
    ///
    /// # Errors
    /// Refuses a root without the writer lease, a coordinator image that cannot be
    /// read, a stage whose completion record does not name that exact candidate, a
    /// candidate outside the installation scope or not above the floor, or any
    /// unverified or unreferenced version. Every refusal is [`UpdateError::Activation`]
    /// with [`crate::ActivationEffect::ProtectedStateUnchanged`]: nothing was written
    /// and only the stage remains, a tolerated leftover, never an unreferenced version.
    /// [`crate::WindowsMintedAttempt::journal`] documents the refusals of the durable
    /// steps.
    pub fn begin_activation(
        self,
        stage: CompletedWindowsStage,
        coordinator: &keld_guard::VerifiedWindowsImage,
    ) -> Result<crate::WindowsMintedAttempt, UpdateError> {
        self.begin_activation_with_image_file(stage, coordinator.file())
    }

    /// [`Self::begin_activation`] for the coordinator image's open file: the one
    /// crate-private path behind that entry point, which crate-internal tests drive
    /// with plain files. The digest is derived by the writer snapshot's transaction.
    pub(crate) fn begin_activation_with_image_file(
        self,
        stage: CompletedWindowsStage,
        coordinator_image: &std::fs::File,
    ) -> Result<crate::WindowsMintedAttempt, UpdateError> {
        let Self {
            authority,
            versions,
            ..
        } = self;
        drop(versions);
        match authority {
            RootAuthority::ActivationWriter { snapshot } => {
                snapshot.begin_activation(stage.name, &stage.identity, coordinator_image)
            }
            RootAuthority::OwnerPrivate { .. } | RootAuthority::Machine { .. } => {
                Err(UpdateError::activation(
                    "writer authority",
                    crate::ActivationEffect::ProtectedStateUnchanged,
                    "activation requires the retained exclusive activation-writer lease",
                ))
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
