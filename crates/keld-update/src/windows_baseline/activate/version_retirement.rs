//! Version retirement and generated leftovers under the retained Windows writer lease.

use std::collections::BTreeSet;

use cap_fs_ext::DirExt as _;

use super::super::Roots;
use super::leftover_error;
use crate::UpdateError;

/// Admits every complete version that no record references for retirement, renaming
/// nothing, and returns their names.
///
/// Referenced versions (compared case-insensitively, as NTFS resolves them) are skipped.
/// Generated `incomplete-*`/`retired-*` entries must be non-reparse directories and are
/// otherwise left alone. Every other entry must be a protected directory whose completion
/// record names exactly that version within this installation's scope. Any unknown or
/// damaged entry refuses with its own diagnostic, which needs manual recovery. The caller
/// holds the exclusive writer lease, keeps the referenced trees pinned, and has
/// established that no activation journal exists.
pub(in super::super) fn admit_unreferenced_versions(
    roots: &Roots,
    referenced: &BTreeSet<String>,
) -> Result<Vec<String>, UpdateError> {
    let mut retiring = Vec::new();
    for entry in roots
        .versions
        .entries()
        .map_err(|error| super::super::error("unjournaled version census", error))?
    {
        let entry =
            entry.map_err(|error| super::super::error("unjournaled version census", error))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| super::super::error("unjournaled version census", "non-UTF-8 entry"))?;
        if referenced
            .iter()
            .any(|version| version.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        if super::super::is_generated_leaf(&name, "incomplete")
            || super::super::is_generated_leaf(&name, "retired")
        {
            version_present(roots, &name)?;
            continue;
        }
        super::super::load::validate_unjournaled_version(roots, &name)?;
        retiring.push(name);
    }
    Ok(retiring)
}

/// Renames each admitted version to a generated `retired-*` sibling; returns the count.
pub(in super::super) fn retire_admitted_versions(
    roots: &Roots,
    admitted: &[String],
) -> Result<usize, UpdateError> {
    for name in admitted {
        retire_version_directory(roots, name)?;
    }
    Ok(admitted.len())
}

/// Renames one version directory to a generated `retired-*` sibling and proves its
/// version name is gone.
pub(super) fn retire_version_directory(roots: &Roots, version: &str) -> Result<(), UpdateError> {
    if !version_present(roots, version)? {
        return Err(super::super::error(
            "version retirement",
            "retiring version is absent",
        ));
    }
    let leaf = super::super::random_leaf_name("retired")
        .map_err(|cause| super::super::error("retired identity", cause))?;
    let versions = versions_for_rename(roots)?;
    crate::windows_fs::publish_new(&versions, version, &leaf)
        .map_err(|cause| super::super::error("version retirement", cause))?;
    if version_present(roots, version)? {
        return Err(super::super::error(
            "version retirement",
            "retired version name is still present after rename",
        ));
    }
    Ok(())
}

/// The retained `versions` handle for a rename beneath it, after re-checking its exact
/// installation descriptor: a write never trusts an earlier observation of the parent.
pub(super) fn versions_for_rename(roots: &Roots) -> Result<std::fs::File, UpdateError> {
    let versions = roots
        .versions
        .try_clone()
        .map_err(|cause| super::super::error("versions parent", cause))?
        .into_std_file();
    keld_guard::validate_windows_install_directory(&versions, roots.profile())
        .map_err(|cause| super::super::error("versions profile", cause))?;
    Ok(versions)
}

pub(super) fn version_present(roots: &Roots, version: &str) -> Result<bool, UpdateError> {
    match roots.versions.symlink_metadata(version) {
        Ok(metadata) => {
            crate::windows_extraction::ensure_directory(&metadata)
                .map_err(|cause| super::super::error("version presence", cause))?;
            Ok(true)
        }
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(cause) => Err(super::super::error("version presence", cause)),
    }
}

/// Deletes generated `retired-*` trees and stale completed `incomplete-*` stages.
///
/// Only the writer-lease holder retires a version or completes a stage, and the caller
/// holds that lease, so neither can belong to a live operation. (The initializer also
/// completes a stage, but only in an empty `versions` before provenance exists, when no
/// activation writer can run.) A completed stage left
/// by a refused start, a crash before its journal or an abandoned attempt is never
/// referenced again. A stage without a completion record may be a live extraction by an
/// owner-private root that holds no lease, or a failed one that its caller must preserve
/// for diagnosis, so it is left untouched; such an extraction always creates a fresh
/// name and never reuses an existing one.
/// Listing and deletion both go through the retained `versions` handle, so no pathname
/// is resolved again and a link inside a tree is never followed. A `retired-*` entry
/// that is not a directory refuses, as before.
pub(super) fn remove_generated_leftovers(roots: &Roots) -> Result<(), UpdateError> {
    let versions = roots
        .versions
        .try_clone()
        .map_err(|cause| leftover_error(cause.to_string()))?
        .into_std_file();
    let names = crate::windows_fs::child_names(&versions)
        .map_err(|cause| leftover_error(cause.to_string()))?;
    for name in names {
        let stale = super::super::is_generated_leaf(&name, "retired")
            || (super::super::is_generated_leaf(&name, "incomplete")
                && completed_stage(roots, &name)
                    .map_err(|cause| leftover_error(format!("{name}: {cause}")))?);
        if stale {
            crate::windows_fs::remove_directory_tree(&versions, &name)
                .map_err(|cause| leftover_error(format!("{name}: {cause}")))?;
        }
    }
    Ok(())
}

/// Whether the generated stage `name` is a non-reparse directory holding a completion
/// record. The check opens the stage read-only and never follows a link, so it neither
/// conflicts with the handles of a live extraction, which deny delete sharing, nor
/// reports such a stage as a leftover. Any other failure is returned.
fn completed_stage(roots: &Roots, name: &str) -> std::io::Result<bool> {
    let metadata = roots.versions.symlink_metadata(name)?;
    if crate::windows_extraction::ensure_directory(&metadata).is_err() {
        return Ok(false);
    }
    super::super::load::completion_present(&roots.versions.open_dir_nofollow(name)?)
}
