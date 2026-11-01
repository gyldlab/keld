//! The existing KEL-96 stage producer's explicit GH499 full-tree capture.
//! The normal `stage_dev_boot` copy path never calls this module.

use std::collections::BTreeSet;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Component, Path, PathBuf};

use rustix::fs::{Mode, OFlags, open, openat};
use sha2::{Digest as _, Sha256};

use super::{BootCompileError, ProjectOwnershipError};

const BUFFER_BYTES: usize = 64 * 1024;
const MAX_FILES: usize = 32_768;
const MAX_CENSUS_ENTRIES: usize = 65_536;

fn error(detail: impl std::fmt::Display) -> BootCompileError {
    BootCompileError::new("GH499 full graph capture", detail.to_string())
}

fn portable_path(path: &Path) -> Result<String, BootCompileError> {
    let relative = path
        .to_str()
        .ok_or_else(|| error("non-UTF-8 relative path"))?;
    keld_core::app_session::graph_contract::validate_graph_relative_path(relative)
        .map_err(|source| error(source.to_string()))?;
    Ok(relative.to_owned())
}

fn nofollow_directory(root: &File, relative: &Path) -> Result<File, BootCompileError> {
    let mut parent = root.try_clone().map_err(error)?;
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(error("source directory path is not portable"));
        };
        parent = File::from(
            openat(
                &parent,
                part,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(error)?,
        );
    }
    Ok(parent)
}

fn nofollow_source(root: &File, relative: &Path) -> Result<File, BootCompileError> {
    let leaf = relative
        .file_name()
        .ok_or_else(|| error("empty captured relative path"))?;
    let parent = nofollow_directory(
        root,
        relative
            .parent()
            .ok_or_else(|| error("source file has no parent"))?,
    )?;
    let selected = File::from(
        openat(
            &parent,
            leaf,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(error)?,
    );
    if !selected.metadata().map_err(error)?.is_file() {
        return Err(error("captured file is not regular"));
    }
    Ok(selected)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceIdentity {
    dev: u64,
    ino: u64,
    mode: u32,
    len: u64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
}

impl SourceIdentity {
    fn capture(metadata: &Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            mode: metadata.mode(),
            len: metadata.len(),
            mtime: metadata.mtime(),
            mtime_nsec: metadata.mtime_nsec(),
            ctime: metadata.ctime(),
            ctime_nsec: metadata.ctime_nsec(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceEntry {
    relative: PathBuf,
    identity: SourceIdentity,
    directory: bool,
}

fn confirm_source_identity(
    selected: &File,
    path: &Path,
    expected: &SourceIdentity,
) -> Result<(), BootCompileError> {
    let from_fd = SourceIdentity::capture(&selected.metadata().map_err(error)?);
    let from_path = SourceIdentity::capture(&fs::symlink_metadata(path).map_err(error)?);
    if from_fd != *expected || from_path != *expected {
        return Err(error(format!(
            "source identity changed during physical census/copy: {}",
            path.display(),
        )));
    }
    Ok(())
}

/// The only source inventory; a second invocation must reproduce both file
/// and directory inode identities. Every path is reopened relative to the
/// same retained project-root FD before and after its directory enumeration.
fn scan_source<F>(
    project: &Path,
    root: &File,
    owner_check: &mut F,
) -> Result<Vec<SourceEntry>, BootCompileError>
where
    F: FnMut(&Path) -> Result<(), ProjectOwnershipError>,
{
    let root_identity = SourceIdentity::capture(&root.metadata().map_err(error)?);
    let mut pending = vec![(PathBuf::new(), root_identity)];
    let mut found = Vec::new();
    let mut files = 0_usize;
    while let Some((relative, directory_identity)) = pending.pop() {
        let path = project.join(&relative);
        let opened = nofollow_directory(root, &relative)?;
        confirm_source_identity(&opened, &path, &directory_identity)?;
        let mut entries = fs::read_dir(&path)
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let name = entry.file_name();
            if relative.as_os_str().is_empty()
                && (name == ".git"
                    || name == ".keld"
                    || name == "keld.config.ts"
                    || name == "keld.permissions.jsonc")
            {
                continue;
            }
            let next = relative.join(name);
            let rel = portable_path(&next)?;
            if relative.as_os_str().is_empty()
                && matches!(
                    rel.as_str(),
                    "keld-host" | "keld.boot.json" | "keld.stage-graph.json"
                )
            {
                return Err(error(format!(
                    "producer-owned leaf in project source: {rel}"
                )));
            }
            if found.len() >= MAX_CENSUS_ENTRIES {
                return Err(error("more than 65536 source census entries"));
            }
            let child_path = project.join(&next);
            let metadata = fs::symlink_metadata(&child_path).map_err(error)?;
            if metadata.file_type().is_symlink() {
                return Err(error(format!("source symlink is not admitted: {rel}")));
            }
            let is_dir = metadata.is_dir();
            if !is_dir && !metadata.is_file() {
                return Err(error(format!("source special file is not admitted: {rel}")));
            }
            let identity = SourceIdentity::capture(&metadata);
            owner_check(&child_path)?;
            let selected = if is_dir {
                nofollow_directory(root, &next)?
            } else {
                nofollow_source(root, &next)?
            };
            confirm_source_identity(&selected, &child_path, &identity)?;
            found.push(SourceEntry {
                relative: next.clone(),
                identity: identity.clone(),
                directory: is_dir,
            });
            if is_dir {
                pending.push((next, identity));
            } else {
                files += 1;
                if files > MAX_FILES {
                    return Err(error("more than 32768 regular source files"));
                }
            }
        }
        confirm_source_identity(&opened, &path, &directory_identity)?;
    }
    found.sort_by(|left, right| left.relative.cmp(&right.relative));
    let mut expected_dirs = BTreeSet::new();
    for file in found.iter().filter(|row| !row.directory) {
        let mut parent = file.relative.parent();
        while let Some(path) = parent {
            if path.as_os_str().is_empty() {
                break;
            }
            expected_dirs.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    if let Some(extra) = found
        .iter()
        .find(|row| row.directory && !expected_dirs.contains(&row.relative))
    {
        return Err(error(format!(
            "unlisted empty source directory: {}",
            extra.relative.display(),
        )));
    }
    Ok(found)
}

fn streamed_capture(mut source: File, target: &Path) -> Result<(), BootCompileError> {
    let before = source.metadata().map_err(error)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o400)
        .open(target)
        .map_err(error)?;
    let mut digest = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = vec![0_u8; BUFFER_BYTES].into_boxed_slice();
    loop {
        let n = source.read(buffer.as_mut()).map_err(error)?;
        if n == 0 {
            break;
        }
        output.write_all(&buffer[..n]).map_err(error)?;
        digest.update(&buffer[..n]);
        count = count
            .checked_add(n as u64)
            .ok_or_else(|| error("captured size overflow"))?;
    }
    output.sync_all().map_err(error)?;
    let copied_hash: [u8; 32] = digest.finalize().into();
    let after = source.metadata().map_err(error)?;
    if count != before.len()
        || after.len() != before.len()
        || before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err(error("source changed during selected in-place capture"));
    }
    // Two independent 64 KiB passes catch an in-place byte replacement and
    // ensure the bytes committed at the destination match selected input.
    source.seek(SeekFrom::Start(0)).map_err(error)?;
    let mut second = Sha256::new();
    let mut checked = 0_u64;
    loop {
        let n = source.read(buffer.as_mut()).map_err(error)?;
        if n == 0 {
            break;
        }
        second.update(&buffer[..n]);
        checked = checked
            .checked_add(n as u64)
            .ok_or_else(|| error("recheck size overflow"))?;
    }
    let second_sha: [u8; 32] = second.finalize().into();
    if checked != count || second_sha != copied_hash {
        return Err(error("source content changed during copy"));
    }
    drop(output);
    let mut verify_file = File::open(target).map_err(error)?;
    let mut dest_hash = Sha256::new();
    let mut dest_count = 0_u64;
    loop {
        let n = verify_file.read(buffer.as_mut()).map_err(error)?;
        if n == 0 {
            break;
        }
        dest_hash.update(&buffer[..n]);
        dest_count = dest_count
            .checked_add(n as u64)
            .ok_or_else(|| error("destination recheck overflow"))?;
    }
    let destination_sha: [u8; 32] = dest_hash.finalize().into();
    if destination_sha != copied_hash || dest_count != count {
        return Err(error("stage content differed from captured source"));
    }
    Ok(())
}

/// This caller already owns the same private nonce and `StageGuard` transaction.
/// Source capture is not the GH499 installed-package or authorization decision.
pub(super) fn capture_complete_tree<F>(
    project: &Path,
    stage: &Path,
    owner_check: &mut F,
) -> Result<(), BootCompileError>
where
    F: FnMut(&Path) -> Result<(), ProjectOwnershipError>,
{
    let root = File::from(
        open(
            project,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(error)?,
    );
    if !root.metadata().map_err(error)?.is_dir() {
        return Err(error("project capture root is not a directory"));
    }
    let inventory = scan_source(project, &root, owner_check)?;
    for selected in inventory.iter().filter(|row| !row.directory) {
        let source = nofollow_source(&root, &selected.relative)?;
        let path = project.join(&selected.relative);
        confirm_source_identity(&source, &path, &selected.identity)?;
        let destination = stage.join(&selected.relative);
        fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(|| error("no destination parent"))?,
        )
        .map_err(error)?;
        streamed_capture(source, &destination)?;
        let source_again = nofollow_source(&root, &selected.relative)?;
        confirm_source_identity(&source_again, &path, &selected.identity)?;
    }
    // This full second census uses the same retained directory object, checks
    // all source file and parent identities, and refuses a late inserted
    // sibling even when all copied stage hashes are internally consistent.
    let final_inventory = scan_source(project, &root, owner_check)?;
    if final_inventory != inventory {
        return Err(error(
            "source census identity or membership changed during capture",
        ));
    }
    Ok(())
}
