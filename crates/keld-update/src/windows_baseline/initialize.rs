//! Publication is a one-shot installer transaction, never ordinary host mutation.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt as _};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, WRITE_DAC,
};

use super::{
    LoadedWindowsBaseline, Roots, WindowsBaselineReceipt, WindowsBaselineTrust, error,
    exact_entries, open_roots, read_record, seal_child,
};
use crate::records::{self, PointerKind};
use crate::windows_extraction::{StageProtection, open_source, populate_stage};
use crate::windows_fs::{create_directory_relative, publish_new};
use crate::{InstallOwner, InstallProvenance, UpdateError, VerifiedBaseline};

/// Initializes one externally provisioned SYSTEM-private machine scaffold.
///
/// Trusted configuration must come from the deployment authority. Successful return
/// requires production read-only reload and exact baseline seed validation.
///
/// # Errors
/// Refuses non-SYSTEM callers, unsupported scaffolds, changed authenticated bytes,
/// existing/partial state, conflicts or any publication/readback failure. Failures
/// preserve incomplete state; there is no automatic cleanup, repair or retry.
pub fn initialize_windows_baseline(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_observer(verified, archive, trust, |_| Ok(()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BaselineBoundary {
    LockCreated,
    StageCreated,
    StagePopulated,
    CompletePublished,
    VersionPublished,
    FloorPublished,
    CurrentPublished,
    LastKnownGoodPublished,
    RootsSealed,
    ProvenancePublished,
}

pub(super) fn initialize_with_observer(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    mut observe: impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    keld_guard::require_windows_system_token()
        .map_err(|cause| error("installer authority", cause))?;
    crate::provenance::match_identity(&trust.installation, verified.installation())?;
    let roots =
        open_roots(trust, true).map_err(|cause| error("private scaffold admission", cause))?;
    let update_name = trust
        .installation
        .update_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| error("topology", "update name absent"))?;
    let mut lock = acquire_bootstrap_lock(&roots, update_name)?;
    observe(BaselineBoundary::LockCreated).map_err(|cause| error("lock boundary", cause))?;

    let mut source =
        open_source(archive).map_err(|cause| error("baseline source admission", cause))?;
    let validated = verified.validate_windows_archive(&mut source)?;
    let name = random_name("incomplete")?;
    let parent = roots
        .versions
        .try_clone()
        .map_err(|cause| error("versions handle", cause))?
        .into_std_file();
    let stage = create_directory_relative(&parent, &name)
        .map_err(|cause| error("stage creation", cause))?;
    observe(BaselineBoundary::StageCreated).map_err(|cause| error("stage boundary", cause))?;
    let (directories, files) = populate_stage(
        stage,
        &name,
        &validated,
        &mut source,
        StageProtection::Machine,
        &mut |_, _| Ok(()),
    )?;
    observe(BaselineBoundary::StagePopulated).map_err(|cause| error("content boundary", cause))?;
    let stage = &directories[0];
    let tree = &directories[1];
    // Files already have final protected ACLs, applied before their original
    // writable handles were flushed. Seal only the remaining directories here.
    for entry in validated.entries().iter().rev() {
        if entry.kind() == crate::ArchiveEntryKind::Directory {
            seal_child(tree, entry.name(), true)
                .map_err(|cause| error("stage directory seal", cause))?;
        }
    }
    seal_child(stage, "tree", true).map_err(|cause| error("tree seal", cause))?;
    let complete = records::encode_complete(verified.identity(), verified.content_size())?;
    publish_record(stage, ".complete", &complete)?;
    observe(BaselineBoundary::CompletePublished)
        .map_err(|cause| error("complete boundary", cause))?;
    seal_child(&roots.versions, &name, true).map_err(|cause| error("stage seal", cause))?;
    drop(files);
    drop(directories);
    publish_new(&parent, &name, &verified.identity().version)
        .map_err(|cause| error("version publication", cause))?;
    observe(BaselineBoundary::VersionPublished)
        .map_err(|cause| error("version boundary", cause))?;
    let version = super::load::validate_initial_version(&roots)?;

    publish_record(
        &roots.update,
        "version-floor",
        &records::encode_floor(&verified.identity().version)?,
    )?;
    observe(BaselineBoundary::FloorPublished).map_err(|cause| error("floor boundary", cause))?;
    publish_record(
        &roots.update,
        "current",
        &records::encode_pointer(PointerKind::Current, verified.identity())?,
    )?;
    observe(BaselineBoundary::CurrentPublished)
        .map_err(|cause| error("current boundary", cause))?;
    publish_record(
        &roots.update,
        "last-known-good",
        &records::encode_pointer(PointerKind::LastKnownGood, verified.identity())?,
    )?;
    observe(BaselineBoundary::LastKnownGoodPublished)
        .map_err(|cause| error("LKG boundary", cause))?;
    seal_roots_and_commit(&roots, update_name, &mut lock, &mut observe)?;
    drop(lock);
    drop(version);
    drop(roots);
    let loaded: LoadedWindowsBaseline = super::load_windows_baseline(trust)?;
    let version = super::load::validate_initial_seed(&loaded.roots)?;
    Ok(WindowsBaselineReceipt {
        loaded,
        _version: version,
    })
}

fn acquire_bootstrap_lock(roots: &Roots, update_name: &str) -> Result<File, UpdateError> {
    let install = roots
        .install()
        .map_err(|cause| error("install root", cause))?;
    exact_entries(install, &[update_name]).map_err(|cause| error("fresh install", cause))?;
    exact_entries(&roots.update, &["versions"]).map_err(|cause| error("fresh update", cause))?;
    exact_entries(&roots.versions, &[]).map_err(|cause| error("fresh versions", cause))?;
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC)
        .share_mode(0)
        .follow(FollowSymlinks::No);
    let lock = roots
        .update
        .open_with("bootstrap.lock", &options)
        .map_err(|cause| error("exclusive bootstrap lock", cause))?
        .into_std();
    keld_guard::validate_windows_owner_private_file(&lock)
        .map_err(|cause| error("bootstrap lock protection", cause))?;
    lock.sync_all()
        .map_err(|cause| error("bootstrap lock flush", cause))?;
    Ok(lock)
}

fn seal_roots_and_commit(
    roots: &Roots,
    update_name: &str,
    lock: &mut File,
    observe: &mut impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<(), UpdateError> {
    let trust = &roots.trust;
    let install = roots
        .install()
        .map_err(|cause| error("install root", cause))?;
    let provenance = records::encode_provenance(
        &InstallProvenance {
            identity: trust.installation.clone(),
            owner: InstallOwner::Direct,
        },
        &trust.publisher_scope,
        &trust.volume_guid,
    )?;
    // Prepare this private file before sealing the roots. The only later namespace
    // mutation is its absent-target publication, which is the final commit record.
    let provenance_temp = prepare_record(install, &provenance)?;
    keld_guard::seal_windows_machine_file(lock).map_err(|cause| error("lock seal", cause))?;
    lock.sync_all()
        .map_err(|cause| error("sealed lock flush", cause))?;
    seal_child(&roots.update, "versions", true).map_err(|cause| error("versions seal", cause))?;
    seal_child(install, update_name, true).map_err(|cause| error("update seal", cause))?;
    let parent_index = roots
        .ancestors
        .len()
        .checked_sub(2)
        .ok_or_else(|| error("install parent", "missing ancestor"))?;
    let install_name = trust
        .installation
        .install_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| error("install parent", "missing component"))?;
    seal_child(&roots.ancestors[parent_index], install_name, true)
        .map_err(|cause| error("install seal", cause))?;
    observe(BaselineBoundary::RootsSealed).map_err(|cause| error("root boundary", cause))?;
    publish_prepared(install, &provenance_temp, "install-provenance", &provenance)?;
    observe(BaselineBoundary::ProvenancePublished)
        .map_err(|cause| error("provenance boundary", cause))?;
    Ok(())
}

fn random_name(prefix: &str) -> Result<String, UpdateError> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(|cause| error("temporary identity", cause))?;
    Ok(format!("{prefix}-{}", crate::error::hex_digest(&random)))
}

fn prepare_record(parent: &Dir, bytes: &[u8]) -> Result<String, UpdateError> {
    let name = random_name("pending")?;
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC)
        .share_mode(FILE_SHARE_READ)
        .follow(FollowSymlinks::No);
    let mut output = parent
        .open_with(&name, &options)
        .map_err(|cause| error("record creation", cause))?;
    super::ensure_regular(
        &output
            .metadata()
            .map_err(|cause| error("record metadata", cause))?,
    )
    .map_err(|cause| error("record kind", cause))?;
    keld_guard::validate_windows_owner_private_file(
        &output
            .try_clone()
            .map_err(|cause| error("record handle", cause))?
            .into_std(),
    )
    .map_err(|cause| error("private record protection", cause))?;
    output
        .write_all(bytes)
        .map_err(|cause| error("record write", cause))?;
    let mut output = output.into_std();
    keld_guard::seal_windows_machine_file(&mut output)
        .map_err(|cause| error("record seal", cause))?;
    output
        .sync_all()
        .map_err(|cause| error("record flush", cause))?;
    drop(output);
    let (_, observed) = read_record(parent, &name)?;
    if observed != bytes {
        return Err(error("record readback", "flushed bytes differ"));
    }
    Ok(name)
}

fn publish_record(parent: &Dir, leaf: &str, bytes: &[u8]) -> Result<(), UpdateError> {
    let temporary = prepare_record(parent, bytes)?;
    publish_prepared(parent, &temporary, leaf, bytes)
}

fn publish_prepared(
    parent: &Dir,
    temporary: &str,
    leaf: &str,
    bytes: &[u8],
) -> Result<(), UpdateError> {
    let retained = parent
        .try_clone()
        .map_err(|cause| error("record parent", cause))?
        .into_std_file();
    publish_new(&retained, temporary, leaf).map_err(|cause| error("record publication", cause))?;
    let (_, observed) = read_record(parent, leaf)?;
    if observed != bytes {
        return Err(error("published record readback", "published bytes differ"));
    }
    Ok(())
}
