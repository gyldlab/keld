//! The existing GH499 complete-tree capture adapted to Windows retained opens.
//! The stage transaction and all namespace/cleanup policy remain in boot.rs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;

use keld_update::{
    WindowsGraphChildKind, WindowsGraphMetadata, graph_locator_metadata, open_retained_graph_child,
    retained_graph_metadata, strict_graph_directory_names,
};
use sha2::{Digest as _, Sha256};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

use super::{BootCompileError, ProjectOwnershipError};

const BUFFER_BYTES: usize = 64 * 1024;
const MAX_FILES: usize = 32_768;
const MAX_CENSUS_ENTRIES: usize = 65_536;
const MAX_PATH_BYTES: usize = 4_096;

fn error(detail: impl std::fmt::Display) -> BootCompileError {
    BootCompileError::new("GH499 full graph capture", detail.to_string())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceEntry {
    relative: String,
    identity: WindowsGraphMetadata,
}

fn open_relative(
    root: &File,
    relative: &str,
    kind: WindowsGraphChildKind,
) -> Result<File, BootCompileError> {
    let mut parent = root.try_clone().map_err(error)?;
    if relative.is_empty() {
        return Ok(parent);
    }
    keld_core::app_session::validate_graph_relative_path(relative).map_err(error)?;
    let mut parts = relative.split('/').peekable();
    while let Some(part) = parts.next() {
        parent = open_retained_graph_child(
            &parent,
            part,
            if parts.peek().is_some() {
                WindowsGraphChildKind::Directory
            } else {
                kind
            },
        )
        .map_err(error)?;
    }
    Ok(parent)
}

fn same_source(
    file: &File,
    path: &Path,
    expected: &WindowsGraphMetadata,
) -> Result<(), BootCompileError> {
    if retained_graph_metadata(file).map_err(error)? != *expected
        || graph_locator_metadata(path).map_err(error)? != *expected
    {
        return Err(error(format!(
            "source identity changed during census/copy: {}",
            path.display()
        )));
    }
    Ok(())
}

fn scan_source<F>(
    project: &Path,
    root: &File,
    owner_check: &mut F,
) -> Result<Vec<SourceEntry>, BootCompileError>
where
    F: FnMut(&Path) -> Result<(), ProjectOwnershipError>,
{
    let root_identity = retained_graph_metadata(root).map_err(error)?;
    let mut pending = vec![(String::new(), root_identity)];
    let mut found = Vec::new();
    let mut files = 0;
    while let Some((relative, directory_identity)) = pending.pop() {
        let opened = open_relative(root, &relative, WindowsGraphChildKind::Directory)?;
        let path = project.join(&relative);
        same_source(&opened, &path, &directory_identity)?;
        let remaining = MAX_CENSUS_ENTRIES.saturating_sub(found.len());
        let mut names =
            strict_graph_directory_names(&opened, remaining, remaining * MAX_PATH_BYTES)
                .map_err(error)?;
        names.sort();
        for name in names {
            if relative.is_empty()
                && matches!(
                    name.as_str(),
                    ".git"
                        | ".keld"
                        | "keld.config.ts"
                        | "keld.build.ts"
                        | "keld.permissions.jsonc"
                )
            {
                continue;
            }
            let next = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            keld_core::app_session::validate_graph_relative_path(&next).map_err(error)?;
            if relative.is_empty()
                && matches!(
                    name.as_str(),
                    "keld-host" | "keld-host.exe" | "keld.boot.json" | "keld.stage-graph.json"
                )
            {
                return Err(error(format!(
                    "producer-owned leaf in project source: {next}"
                )));
            }
            if found.len() >= MAX_CENSUS_ENTRIES {
                return Err(error("more than 65536 source census entries"));
            }
            let selected = open_retained_graph_child(
                &opened,
                &name,
                WindowsGraphChildKind::DirectoryOrRegular,
            )
            .map_err(error)?;
            let identity = retained_graph_metadata(&selected).map_err(error)?;
            owner_check(&project.join(&next))?;
            same_source(&selected, &project.join(&next), &identity)?;
            if identity.is_dir() {
                pending.push((next.clone(), identity.clone()));
            } else if identity.is_file() {
                files += 1;
                if files > MAX_FILES {
                    return Err(error("more than 32768 source files"));
                }
            } else {
                return Err(error(
                    "source member is neither a real directory nor a regular file",
                ));
            }
            found.push(SourceEntry {
                relative: next,
                identity,
            });
        }
        same_source(&opened, &path, &directory_identity)?;
    }
    found.sort_by(|left, right| left.relative.cmp(&right.relative));
    let mut parents = BTreeSet::new();
    for row in found.iter().filter(|row| row.identity.is_file()) {
        let mut path = row.relative.as_str();
        while let Some((parent, _)) = path.rsplit_once('/') {
            parents.insert(parent.to_owned());
            path = parent;
        }
    }
    if let Some(row) = found
        .iter()
        .find(|row| row.identity.is_dir() && !parents.contains(&row.relative))
    {
        return Err(error(format!(
            "unlisted empty source directory: {}",
            row.relative
        )));
    }
    Ok(found)
}

/// Destination files and directories remain pinned through graph/boot publication.
/// These bounded handles are custody only; Core still owns graph admission.
#[derive(Debug)]
pub(super) struct CapturedTree {
    _files: Vec<File>,
    _directories: BTreeMap<String, File>,
}

fn stage_parent(
    stage: &Path,
    directories: &mut BTreeMap<String, File>,
    relative: &str,
) -> Result<(File, String), BootCompileError> {
    let (parent, leaf) = relative.rsplit_once('/').unwrap_or(("", relative));
    let mut current = String::new();
    for part in parent.split('/').filter(|part| !part.is_empty()) {
        let next = if current.is_empty() {
            part.to_owned()
        } else {
            format!("{current}/{part}")
        };
        if !directories.contains_key(&next) {
            let parent_handle = directories
                .get(&current)
                .ok_or_else(|| error("missing retained stage parent"))?;
            fs::create_dir(stage.join(&next)).map_err(error)?;
            let child =
                open_retained_graph_child(parent_handle, part, WindowsGraphChildKind::Directory)
                    .map_err(error)?;
            directories.insert(next.clone(), child);
        }
        current = next;
    }
    Ok((
        directories
            .get(parent)
            .ok_or_else(|| error("missing retained stage directory"))?
            .try_clone()
            .map_err(error)?,
        leaf.to_owned(),
    ))
}

fn streamed_capture(
    mut source: File,
    parent: &File,
    destination: &Path,
    leaf: &str,
    expected: &WindowsGraphMetadata,
) -> Result<File, BootCompileError> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(destination)
        .map_err(error)?;
    let mut buffer = vec![0_u8; BUFFER_BYTES].into_boxed_slice();
    let mut digest = Sha256::new();
    let mut copied = 0_u64;
    loop {
        let count = source.read(buffer.as_mut()).map_err(error)?;
        if count == 0 {
            break;
        }
        copied = copied
            .checked_add(count as u64)
            .ok_or_else(|| error("copy size overflow"))?;
        if copied > expected.len() {
            return Err(error("source grew during capture"));
        }
        output.write_all(&buffer[..count]).map_err(error)?;
        digest.update(&buffer[..count]);
    }
    output.sync_all().map_err(error)?;
    let first: [u8; 32] = digest.finalize().into();
    if copied != expected.len() || retained_graph_metadata(&source).map_err(error)? != *expected {
        return Err(error("source changed during selected capture"));
    }
    source.seek(SeekFrom::Start(0)).map_err(error)?;
    let mut second = Sha256::new();
    loop {
        let count = source.read(buffer.as_mut()).map_err(error)?;
        if count == 0 {
            break;
        }
        second.update(&buffer[..count]);
    }
    if <[u8; 32]>::from(second.finalize()) != first {
        return Err(error("source bytes changed during capture"));
    }
    drop(output);
    let mut destination_file =
        open_retained_graph_child(parent, leaf, WindowsGraphChildKind::RegularFile)
            .map_err(error)?;
    let mut observed = Sha256::new();
    let mut count = 0_u64;
    loop {
        let read = destination_file.read(buffer.as_mut()).map_err(error)?;
        if read == 0 {
            break;
        }
        count += read as u64;
        observed.update(&buffer[..read]);
    }
    if count != copied || <[u8; 32]>::from(observed.finalize()) != first {
        return Err(error("stage content differs from captured source"));
    }
    Ok(destination_file)
}

/// Same census/copy/census algorithm as the reviewed predecessor. Source members
/// are opened only from the retained project root; staged files use one private
/// `StageGuard` transaction and bounded read-only retention through publication.
pub(super) fn capture_complete_tree<F>(
    project: &Path,
    root: &File,
    stage: &Path,
    stage_root: &File,
    owner_check: &mut F,
) -> Result<CapturedTree, BootCompileError>
where
    F: FnMut(&Path) -> Result<(), ProjectOwnershipError>,
{
    let inventory = scan_source(project, root, owner_check)?;
    let mut directories = BTreeMap::from([(String::new(), stage_root.try_clone().map_err(error)?)]);
    let mut files = Vec::new();
    for selected in inventory.iter().filter(|row| row.identity.is_file()) {
        owner_check(&project.join(&selected.relative))?;
        let source = open_relative(root, &selected.relative, WindowsGraphChildKind::RegularFile)?;
        same_source(
            &source,
            &project.join(&selected.relative),
            &selected.identity,
        )?;
        let (parent, leaf) = stage_parent(stage, &mut directories, &selected.relative)?;
        let held = streamed_capture(
            source,
            &parent,
            &stage.join(&selected.relative),
            &leaf,
            &selected.identity,
        )?;
        let source_again =
            open_relative(root, &selected.relative, WindowsGraphChildKind::RegularFile)?;
        same_source(
            &source_again,
            &project.join(&selected.relative),
            &selected.identity,
        )?;
        files.push(held);
    }
    if scan_source(project, root, owner_check)? != inventory {
        return Err(error(
            "source census identity or membership changed during capture",
        ));
    }
    Ok(CapturedTree {
        _files: files,
        _directories: directories,
    })
}
