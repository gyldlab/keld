//! Retained Windows staging I/O: authenticated byte ranges, writer sealing and readback.
//!
//! Caller-supplied verified archive data and retained handles are the only
//! inputs; this owner creates exact-profile children without ambient path walks.

use std::collections::BTreeMap;
use std::fs::File as StdFile;
use std::io::{self, Read as _, Seek as _, SeekFrom, Write as _};

use cap_fs_ext::{FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt as _};
use cap_std::fs::{Dir, File, OpenOptions, OpenOptionsExt as _};
use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_WRITE, FILE_SHARE_READ, WRITE_DAC};

use super::{
    CopyRange, ExtractionEvent, StageProtection, ensure_regular, extraction_error, refusal,
};
use crate::windows_fs::create_file_relative_with_profile;
use crate::{ArchiveEntryKind, ArtifactDomain, UpdateError, ValidatedArchive};

const COPY_BYTES: usize = 16 * 1024;

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

pub(super) fn copy_read_back(
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
