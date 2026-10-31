//! Authenticated retained version-tree membership and byte equality validation.
//!
//! The parent loader owns selection, pointer/floor decisions, and protected
//! directory opening; this owner compares canonical archive members to pinned
//! filesystem objects under their existing root authority.

use std::collections::BTreeMap;
use std::io::{Read as _, Seek as _, SeekFrom};

use super::{
    BaselineCompletion, Roots, VersionPins, error, exact_entries, open_directory, open_machine_file,
};
use crate::{ArchiveEntryKind, UpdateError};

pub(super) fn validate_version_contents(
    roots: &Roots,
    completion: BaselineCompletion,
) -> Result<VersionPins, UpdateError> {
    let BaselineCompletion {
        version,
        tree,
        mut content,
        marker: complete,
        record: complete_record,
    } = completion;
    let expected = crate::full::ContentIdentity {
        identity: complete_record.artifact.clone(),
        content_size: complete_record.content_size,
        content_blake3: complete_record.artifact.content_blake3,
    };
    let validated = crate::archive::parse_content_archive(
        &expected,
        &mut content,
        keld_guard::validate_windows_package_paths,
    )?;
    let mut directories = vec![version, tree];
    let mut files = vec![complete];
    let mut parents = BTreeMap::from([(String::new(), 1_usize)]);
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::from([(String::new(), Vec::new())]);
    for entry in validated.entries() {
        let (parent_name, leaf) = entry.name().rsplit_once('/').unwrap_or(("", entry.name()));
        let index = *parents
            .get(parent_name)
            .ok_or_else(|| error("tree order", "validated parent absent"))?;
        children
            .entry(parent_name.to_owned())
            .or_default()
            .push(leaf.to_owned());
        let parent = &directories[index];
        match entry.kind() {
            ArchiveEntryKind::Directory => {
                let directory = open_directory(parent, leaf, roots.profile())?;
                parents.insert(entry.name().to_owned(), directories.len());
                children.entry(entry.name().to_owned()).or_default();
                directories.push(directory);
            }
            ArchiveEntryKind::File => {
                let mut file = open_machine_file(parent, leaf, roots.profile())
                    .map_err(|cause| error("tree file", cause))?;
                if file
                    .metadata()
                    .map_err(|cause| error("tree metadata", cause))?
                    .len()
                    != entry.size()
                {
                    return Err(error(
                        "tree bytes",
                        "member length differs from authenticated archive",
                    ));
                }
                content
                    .seek(SeekFrom::Start(entry.data_offset()))
                    .map_err(|cause| error("archive member seek", cause))?;
                let mut remaining = entry.size();
                let mut actual = [0_u8; 16 * 1024];
                let mut wanted = [0_u8; 16 * 1024];
                while remaining != 0 {
                    let size = usize::try_from(remaining.min(actual.len() as u64))
                        .map_err(|cause| error("member size", cause))?;
                    file.read_exact(&mut actual[..size])
                        .map_err(|cause| error("tree readback", cause))?;
                    content
                        .read_exact(&mut wanted[..size])
                        .map_err(|cause| error("archive readback", cause))?;
                    if actual[..size] != wanted[..size] {
                        return Err(error(
                            "tree bytes",
                            "member differs from authenticated archive",
                        ));
                    }
                    remaining -= size as u64;
                }
                files.push(file);
            }
        }
    }
    for (name, index) in parents {
        let wanted = children
            .get(&name)
            .ok_or_else(|| error("tree census", "directory census absent"))?;
        let wanted: Vec<&str> = wanted.iter().map(String::as_str).collect();
        exact_entries(&directories[index], &wanted).map_err(|cause| error("tree census", cause))?;
    }
    files.push(content);
    Ok(VersionPins {
        _directories: directories,
        files,
    })
}
