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
    FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use crate::provenance::match_identity;
use crate::windows_fs::{create_directory_relative, qualify_volume};
use crate::{
    AdmittedInstallation, ArchiveEntryKind, ArtifactDomain, ArtifactIdentity,
    DirectInstallationIdentity, ProvenanceField, UpdateError, ValidatedArchive, VerifiedFull,
};

const COPY_BYTES: usize = 16 * 1024;

/// Actual owner-private, fixed-NTFS staging directories retained by the host.
///
/// This proves observed filesystem protection, not installer provenance or live
/// strict-profile admission. It creates no installation scaffolding.
#[derive(Debug)]
pub struct WindowsExtractionRoot {
    installation: DirectInstallationIdentity,
    floor: semver::Version,
    _ancestors: Vec<Dir>,
    versions: Dir,
}

/// Flushed and read-back bytes in an unpublished, incomplete Windows stage.
///
/// Retains all directory and read handles, and exclusively borrows its root.
/// It is neither a runnable version nor a durable publication receipt. Dropping
/// it releases handles and leaves the named incomplete stage for diagnosis.
#[derive(Debug)]
pub struct ExtractedWindowsStage<'root> {
    _root: &'root mut WindowsExtractionRoot,
    name: String,
    identity: ArtifactIdentity,
    _directories: Vec<Dir>,
    _files: Vec<File>,
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
}

impl AdmittedInstallation {
    /// Opens and validates the existing protected Windows staging scaffold.
    ///
    /// # Errors
    /// Refuses unsupported paths/volumes, reparses, missing `versions`, or any
    /// root descriptor outside the exact current-user protected ACL policy.
    pub fn open_windows_extraction_root(&self) -> Result<WindowsExtractionRoot, UpdateError> {
        open_root(self).map_err(|error| extraction_error(None, "root admission", error))
    }
}

fn open_root(admitted: &AdmittedInstallation) -> io::Result<WindowsExtractionRoot> {
    if admitted.identity.target != "windows-x64" {
        return Err(refusal("only the Windows x64 v0 package cell is supported"));
    }
    let path = &admitted.identity.update_root;
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
    let root = ancestors
        .last()
        .ok_or_else(|| refusal("missing staging root"))?;
    let root_file = root.try_clone()?.into_std_file();
    keld_guard::validate_windows_owner_private_directory(&root_file)?;
    qualify_volume(&root_file)?;
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
        _ancestors: ancestors,
        versions,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtractionEvent {
    PreCreate,
    AfterStageCreate,
    BeforeMember,
    BeforeReadback,
}

impl WindowsExtractionRoot {
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
        let mut random = [0_u8; 32];
        getrandom::fill(&mut random)
            .map_err(|error| extraction_error(None, "stage identity", error))?;
        let name = format!("incomplete-{}", crate::error::hex_digest(&random));
        observe(ExtractionEvent::PreCreate, &name)
            .map_err(|error| extraction_error(None, "before creation", error))?;
        // Re-observe policy at the mutation boundary; never repair a changed ACL.
        let parent = self
            .versions
            .try_clone()
            .map_err(|error| extraction_error(None, "versions handle", error))?
            .into_std_file();
        keld_guard::validate_windows_owner_private_directory(&parent)
            .map_err(|error| extraction_error(None, "versions protection", error))?;
        let stage = create_directory_relative(&parent, &name).map_err(|error| {
            extraction_error(Some(&name), "stage creation (outcome unconfirmed)", error)
        })?;
        let result = populate_stage(stage, &name, &validated, &mut source, &mut observe);
        let (directories, files) =
            result.map_err(|error| extraction_error(Some(&name), "stage contents", error))?;
        Ok(ExtractedWindowsStage {
            _root: self,
            name,
            identity: verified.identity().clone(),
            _directories: directories,
            _files: files,
        })
    }
}

fn open_source(path: &Path) -> io::Result<File> {
    let file = StdOpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let file = File::from_std(file);
    ensure_regular(&file.metadata()?)?;
    Ok(file)
}

fn populate_stage(
    stage: StdFile,
    name: &str,
    validated: &ValidatedArchive,
    source: &mut File,
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
        0,
        validated.content_size(),
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
    let tree = create_directory_relative(&tree_parent, "tree").map_err(operation)?;
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
                let child = create_directory_relative(&parent_file, leaf).map_err(operation)?;
                parents.insert(entry.name(), directories.len());
                directories.push(Dir::from_std_file(child));
            }
            ArchiveEntryKind::File => {
                let (file, _) = copy_read_back(
                    parent,
                    leaf,
                    entry.name(),
                    &mut retained_content,
                    entry.data_offset(),
                    entry.size(),
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
    offset: u64,
    size: u64,
    observe: &mut impl FnMut(ExtractionEvent, &str) -> io::Result<()>,
) -> io::Result<(File, [u8; 32])> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .follow(FollowSymlinks::No);
    let mut output = parent.open_with(leaf, &options)?;
    let original = output.metadata()?;
    ensure_regular(&original)?;
    if original.dev() != parent.dir_metadata()?.dev() {
        return Err(refusal("created file crosses its parent volume"));
    }
    keld_guard::validate_windows_owner_private_file(&output.try_clone()?.into_std())?;
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
    keld_guard::validate_windows_owner_private_file(&retained.try_clone()?.into_std())?;
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

fn ensure_directory(metadata: &Metadata) -> io::Result<()> {
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(refusal("expected a non-reparse directory"));
    }
    Ok(())
}

fn ensure_regular(metadata: &Metadata) -> io::Result<()> {
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
mod tests;
