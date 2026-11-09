//! Authenticated first-install stage publication and ordered pointer record seeding.
//!
//! Installer authority, bootstrap locks and final install-provenance commit
//! stay with initialize.rs. This module keeps exact signed byte and ACL checks.

use std::io;
use std::path::Path;

use super::super::{Roots, error, publish_new_record, random_leaf_name, seal_child};
use super::{BaselineAuthority, BaselineBoundary};
use crate::records::{self, PointerKind};
use crate::windows_extraction::{open_source, populate_stage};
use crate::{UpdateError, VerifiedBaseline};

pub(super) fn publish_baseline_version(
    verified: &VerifiedBaseline,
    archive: &Path,
    roots: &Roots,
    authority: BaselineAuthority,
    observe: &mut impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<(), UpdateError> {
    let mut source =
        open_source(archive).map_err(|cause| error("baseline source admission", cause))?;
    let validated = verified.validate_windows_archive(&mut source)?;
    let name =
        random_leaf_name("incomplete").map_err(|cause| error("temporary identity", cause))?;
    let parent = roots
        .versions
        .try_clone()
        .map_err(|cause| error("versions handle", cause))?
        .into_std_file();
    let stage = authority
        .stage_protection()
        .create_directory(&parent, &name)
        .map_err(|cause| error("stage creation", cause))?;
    observe(BaselineBoundary::StageCreated).map_err(|cause| error("stage boundary", cause))?;
    let (directories, files) = populate_stage(
        stage,
        &name,
        &validated,
        &mut source,
        authority.stage_protection(),
        &mut |_, _| Ok(()),
    )?;
    observe(BaselineBoundary::StagePopulated).map_err(|cause| error("content boundary", cause))?;
    let stage = &directories[0];
    let tree = &directories[1];
    match authority {
        BaselineAuthority::MachineSystem => {
            // Files already have final protected ACLs, applied before their original
            // writable handles were flushed. Seal only the remaining directories here.
            for entry in validated.entries().iter().rev() {
                if entry.kind() == crate::ArchiveEntryKind::Directory {
                    seal_child(tree, entry.name(), true)
                        .map_err(|cause| error("stage directory seal", cause))?;
                }
            }
            seal_child(stage, "tree", true).map_err(|cause| error("tree seal", cause))?;
            seal_child(&roots.versions, &name, true).map_err(|cause| error("stage seal", cause))?;
        }
        BaselineAuthority::PerUser => {
            for directory in &directories {
                let file = directory
                    .try_clone()
                    .map_err(|cause| error("stage directory handle", cause))?
                    .into_std_file();
                keld_guard::validate_windows_install_directory(&file, roots.profile())
                    .map_err(|cause| error("stage directory profile", cause))?;
            }
        }
        BaselineAuthority::MachineUac => {
            for directory in &directories {
                let file = directory
                    .try_clone()
                    .map_err(|cause| error("stage directory handle", cause))?
                    .into_std_file();
                keld_guard::validate_windows_install_directory(&file, roots.profile())
                    .map_err(|cause| error("stage directory profile", cause))?;
            }
            for file in &files {
                let file = file
                    .try_clone()
                    .map_err(|cause| error("stage file handle", cause))?
                    .into_std();
                keld_guard::validate_windows_install_file(&file, roots.profile())
                    .map_err(|cause| error("stage file profile", cause))?;
            }
        }
    }
    let complete = records::encode_complete(verified.identity(), verified.content_size())?;
    publish_new_record(stage, ".complete", &complete, roots.profile())?;
    observe(BaselineBoundary::CompletePublished)
        .map_err(|cause| error("complete boundary", cause))?;
    drop(files);
    drop(directories);
    crate::windows_fs::publish_new(&parent, &name, &verified.identity().version)
        .map_err(|cause| error("version publication", cause))?;
    observe(BaselineBoundary::VersionPublished)
        .map_err(|cause| error("version boundary", cause))?;
    Ok(())
}

pub(super) fn seed_baseline_pointers(
    verified: &VerifiedBaseline,
    roots: &Roots,
    observe: &mut impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<(), UpdateError> {
    publish_new_record(
        &roots.update,
        "version-floor",
        &records::encode_floor(&verified.identity().version)?,
        roots.profile(),
    )?;
    observe(BaselineBoundary::FloorPublished).map_err(|cause| error("floor boundary", cause))?;
    publish_new_record(
        &roots.update,
        "current",
        &records::encode_pointer(PointerKind::Current, verified.identity())?,
        roots.profile(),
    )?;
    observe(BaselineBoundary::CurrentPublished)
        .map_err(|cause| error("current boundary", cause))?;
    publish_new_record(
        &roots.update,
        "last-known-good",
        &records::encode_pointer(PointerKind::LastKnownGood, verified.identity())?,
        roots.profile(),
    )?;
    observe(BaselineBoundary::LastKnownGoodPublished)
        .map_err(|cause| error("LKG boundary", cause))?;
    Ok(())
}
