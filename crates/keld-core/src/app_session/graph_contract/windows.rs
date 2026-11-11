//! Windows handle adapter for the one shared graph validator.
//! Graph syntax, source selection, package receipts and census policy stay in its parent.

use super::{HostAppError, MAX_PATH_BYTES, graph_error};
use keld_update::{WindowsGraphChildKind, open_retained_graph_child};
use std::fs::File;
use std::io;
use std::path::{Component, Path};

pub(super) type RetainedRoot = File;
pub(super) type ObjectMetadata = keld_update::WindowsGraphMetadata;

pub(super) fn file_metadata(file: &File) -> io::Result<ObjectMetadata> {
    keld_update::retained_graph_metadata(file)
}
pub(super) fn path_metadata(path: impl AsRef<Path>) -> io::Result<ObjectMetadata> {
    keld_update::graph_locator_metadata(path.as_ref())
}
pub(super) fn same_identity(left: &ObjectMetadata, right: &ObjectMetadata) -> bool {
    left == right
}

pub(super) fn census_directory(root: &RetainedRoot, relative: &str) -> Result<File, HostAppError> {
    let mut directory = root
        .try_clone()
        .map_err(|error| graph_error("retained directory", error))?;
    if !relative.is_empty() {
        super::relative_path(relative)?;
        for component in relative.split('/') {
            directory =
                open_retained_graph_child(&directory, component, WindowsGraphChildKind::Directory)
                    .map_err(|error| graph_error("directory census", error))?;
        }
    }
    Ok(directory)
}

pub(super) fn open_relative_file(
    root: &RetainedRoot,
    relative: &Path,
    phase: &'static str,
) -> Result<File, HostAppError> {
    let text = relative
        .to_str()
        .ok_or_else(|| graph_error(phase, "non UTF-8 graph path"))?;
    // This private reader consumes native Path values, including a selected
    // root joined to its entry. Wire strings were validated before conversion.
    super::relative_path(&text.replace('\\', "/"))?;
    let mut parent = root
        .try_clone()
        .map_err(|error| graph_error(phase, error))?;
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(graph_error(phase, "noncanonical graph component"));
        };
        let name = name
            .to_str()
            .ok_or_else(|| graph_error(phase, "non UTF-8 component"))?;
        let kind = if components.peek().is_some() {
            WindowsGraphChildKind::Directory
        } else {
            WindowsGraphChildKind::RegularFile
        };
        parent = open_retained_graph_child(&parent, name, kind)
            .map_err(|error| graph_error(phase, error))?;
    }
    Ok(parent)
}

pub(super) fn sorted_census_children(
    _directory: &Path,
    held: &File,
    remaining: usize,
) -> Result<Vec<(String, ObjectMetadata)>, HostAppError> {
    let name_budget = remaining
        .checked_mul(MAX_PATH_BYTES)
        .ok_or_else(|| graph_error("directory census", "name budget overflow"))?;
    let mut names = keld_update::strict_graph_directory_names(held, remaining, name_budget)
        .map_err(|error| graph_error("directory census", error))?;
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let child =
                open_retained_graph_child(held, &name, WindowsGraphChildKind::DirectoryOrRegular)
                    .map_err(|error| graph_error("directory census", error))?;
            let metadata =
                file_metadata(&child).map_err(|error| graph_error("directory census", error))?;
            Ok((name, metadata))
        })
        .collect()
}

pub(super) fn root_identity(
    path: &Path,
    opened: &RetainedRoot,
) -> Result<(u64, u64), HostAppError> {
    let held = file_metadata(opened).map_err(|error| graph_error("retained stage", error))?;
    let named = path_metadata(path).map_err(|error| graph_error("retained stage", error))?;
    if !held.is_dir() || !named.is_dir() || held.dev() != named.dev() || held.ino() != named.ino() {
        return Err(graph_error("retained stage", "root was substituted"));
    }
    Ok((held.dev(), held.ino()))
}

pub(super) fn borrow_retained_root(
    path: &Path,
    retained: &File,
) -> Result<RetainedRoot, HostAppError> {
    if !path.is_absolute() {
        return Err(graph_error(
            "retained root",
            "expected an absolute source locator",
        ));
    }
    root_identity(path, retained)?;
    retained
        .try_clone()
        .map_err(|error| graph_error("retained root clone", error))
}

pub(super) fn missing_configuration_reference(
    root: &RetainedRoot,
    selected: &str,
) -> Result<(), HostAppError> {
    let (parent, leaf) = selected.rsplit_once('/').unwrap_or(("", selected));
    let directory = census_directory(root, parent)?;
    match open_retained_graph_child(&directory, leaf, WindowsGraphChildKind::DirectoryOrRegular) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(graph_error("configuration inheritance metadata", error)),
        Ok(_) => Err(graph_error(
            "configuration inheritance",
            format!("{selected}: physical input exists without its exact captured spelling"),
        )),
    }
}
