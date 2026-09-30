//! Publication is a one-shot installer transaction, never ordinary host mutation.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use cap_std::fs::{Dir, File as CapFile};

use super::{
    LoadedWindowsBaseline, Roots, WindowsBaselineReceipt, WindowsBaselineTrust, error,
    exact_entries, open_roots, read_record, seal_child,
};
use crate::records::{self, PointerKind};
use crate::windows_extraction::{StageProtection, open_source, populate_stage};
use crate::windows_fs::{
    create_directory_relative, create_file_relative_exclusive_with_profile,
    create_file_relative_with_profile, publish_new,
};
use crate::{InstallOwner, InstallProvenance, UpdateError, VerifiedBaseline};

/// Initializes one externally provisioned SYSTEM-protected machine scaffold.
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
    ActivationLockCreated,
    StageCreated,
    StagePopulated,
    CompletePublished,
    VersionPublished,
    FloorPublished,
    CurrentPublished,
    LastKnownGoodPublished,
    RootsVerified,
    ProvenancePublished,
}

pub(super) fn initialize_with_observer(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    mut observe: impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    trust.require_direct_owner()?;
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
    seed_activation_lock(&roots)?;
    observe(BaselineBoundary::ActivationLockCreated)
        .map_err(|cause| error("activation lock boundary", cause))?;

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
    commit_baseline(&roots, &mut lock, &mut observe)?;
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
    let update = roots
        .update
        .try_clone()
        .map_err(|cause| error("exclusive bootstrap parent", cause))?
        .into_std_file();
    let lock = create_file_relative_exclusive_with_profile(
        &update,
        "bootstrap.lock",
        keld_guard::WindowsInstallProtectionProfile::MachineSystem,
    )
    .map_err(|cause| error("exclusive bootstrap lock", cause))?;
    keld_guard::validate_windows_machine_file(&lock)
        .map_err(|cause| error("bootstrap lock protection", cause))?;
    lock.sync_all()
        .map_err(|cause| error("bootstrap lock flush", cause))?;
    Ok(lock)
}

fn seed_activation_lock(roots: &Roots) -> Result<(), UpdateError> {
    let update = roots
        .update
        .try_clone()
        .map_err(|cause| error("activation lease parent", cause))?
        .into_std_file();
    let file = create_file_relative_exclusive_with_profile(
        &update,
        "activation.lock",
        keld_guard::WindowsInstallProtectionProfile::MachineSystem,
    )
    .map_err(|cause| error("activation lease creation", cause))?;
    file.sync_all()
        .map_err(|cause| error("activation lease flush", cause))?;
    keld_guard::validate_windows_machine_file(&file)
        .map_err(|cause| error("activation lease readback protection", cause))?;
    Ok(())
}

fn commit_baseline(
    roots: &Roots,
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
    // Prepare the exact-profile provenance record before its absent-target publication,
    // which remains the final baseline commit record.
    let provenance_temp = prepare_record(install, &provenance)?;
    lock.sync_all()
        .map_err(|cause| error("bootstrap lock flush", cause))?;
    keld_guard::validate_windows_machine_file(lock)
        .map_err(|cause| error("bootstrap lock protection", cause))?;
    observe(BaselineBoundary::RootsVerified).map_err(|cause| error("root boundary", cause))?;
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
    let parent_file = parent
        .try_clone()
        .map_err(|cause| error("record parent", cause))?
        .into_std_file();
    let mut output = CapFile::from_std(
        create_file_relative_with_profile(
            &parent_file,
            &name,
            keld_guard::WindowsInstallProtectionProfile::MachineSystem,
        )
        .map_err(|cause| error("record creation", cause))?,
    );
    super::ensure_regular(
        &output
            .metadata()
            .map_err(|cause| error("record metadata", cause))?,
    )
    .map_err(|cause| error("record kind", cause))?;
    keld_guard::validate_windows_machine_file(
        &output
            .try_clone()
            .map_err(|cause| error("record handle", cause))?
            .into_std(),
    )
    .map_err(|cause| error("private record protection", cause))?;
    output
        .write_all(bytes)
        .map_err(|cause| error("record write", cause))?;
    let output = output.into_std();
    output
        .sync_all()
        .map_err(|cause| error("record flush", cause))?;
    drop(output);
    let (_, observed) = read_record(
        parent,
        &name,
        keld_guard::WindowsInstallProtectionProfile::MachineSystem,
    )?;
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
    let (_, observed) = read_record(
        parent,
        leaf,
        keld_guard::WindowsInstallProtectionProfile::MachineSystem,
    )?;
    if observed != bytes {
        return Err(error("published record readback", "published bytes differ"));
    }
    Ok(())
}
