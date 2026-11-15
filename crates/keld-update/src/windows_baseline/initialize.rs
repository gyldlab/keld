//! Publication is a one-shot installer transaction, never ordinary host mutation.

use std::fs::File;
use std::io;
use std::path::Path;

use super::{
    LoadedWindowsBaseline, RecordTarget, Roots, WindowsBaselineReceipt, WindowsBaselineTrust,
    error, exact_entries, open_roots, prepare_record, publish_prepared_record,
};
use crate::records;
use crate::windows_extraction::StageProtection;
use crate::windows_fs::create_file_relative_exclusive_with_profile;
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
    initialize_with_authority(
        verified,
        archive,
        trust,
        BaselineAuthority::MachineSystem,
        |_| Ok(()),
    )
}

/// Initializes a `PerUserDirect` baseline using the installing user's authority.
///
/// The caller supplies trusted installer configuration and the exact verified baseline.
/// This function does not infer install mode or choose an install path; it requires
/// preprovisioned owner-private roots that pass the guard-owned profile validator. The
/// same initial seed order is used as for the machine baseline, with provenance published
/// last as the commit record.
///
/// # Errors
/// Refuses any non-PerUserDirect identity, managed owner, SYSTEM token or token-query
/// failure, mismatched baseline, unsupported scaffold, existing/partial state or
/// publication/readback failure. A refusal never repairs or automatically reseeds
/// existing state.
pub fn initialize_windows_per_user_baseline(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_authority(verified, archive, trust, BaselineAuthority::PerUser, |_| {
        Ok(())
    })
}

/// Initializes a `MachineUacDirect` baseline under a live, explicitly elevated
/// Administrators token.
///
/// The installer supplies trusted identity/configuration and the exact verified
/// baseline. This function does not create or launch a helper, does not mutate an
/// existing installation, and does not activate updates. It seeds the same common
/// baseline transaction used by the other direct modes, with the Program Files
/// Administrators/SYSTEM protection profile.
///
/// # Errors
/// Refuses managed or mismatched installations, thread impersonation, non-elevated or
/// filtered administrator tokens, malformed token data, unsupported scaffolds,
/// existing/partial state and every publication/readback failure.
pub fn initialize_windows_machine_uac_baseline(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_authority(
        verified,
        archive,
        trust,
        BaselineAuthority::MachineUac,
        |_| Ok(()),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BaselineAuthority {
    MachineSystem,
    PerUser,
    MachineUac,
}

impl BaselineAuthority {
    const fn mode(self) -> crate::DirectInstallMode {
        match self {
            Self::MachineSystem => crate::DirectInstallMode::MachineSeamlessDirect,
            Self::PerUser => crate::DirectInstallMode::PerUserDirect,
            Self::MachineUac => crate::DirectInstallMode::MachineUacDirect,
        }
    }

    const fn stage_protection(self) -> StageProtection {
        match self {
            Self::MachineSystem => StageProtection::Machine,
            Self::PerUser => StageProtection::OwnerPrivate,
            Self::MachineUac => StageProtection::MachineUac,
        }
    }
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

#[cfg(test)]
pub(super) fn initialize_with_observer(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    mut observe: impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_authority(
        verified,
        archive,
        trust,
        BaselineAuthority::MachineSystem,
        &mut observe,
    )
}

#[cfg(test)]
pub(super) fn initialize_per_user_with_observer(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    mut observe: impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_authority(
        verified,
        archive,
        trust,
        BaselineAuthority::PerUser,
        &mut observe,
    )
}

#[cfg(test)]
pub(super) fn initialize_machine_uac_with_observer(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    mut observe: impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_authority(
        verified,
        archive,
        trust,
        BaselineAuthority::MachineUac,
        &mut observe,
    )
}

#[cfg(test)]
pub(super) fn initialize_per_user_with_token_check(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    token_check: impl FnOnce() -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_authority_and_token_check(
        verified,
        archive,
        trust,
        BaselineAuthority::PerUser,
        |_| Ok(()),
        |_| token_check(),
    )
}

#[cfg(test)]
pub(super) fn initialize_machine_uac_with_token_check(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    token_check: impl FnOnce() -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_authority_and_token_check(
        verified,
        archive,
        trust,
        BaselineAuthority::MachineUac,
        |_| Ok(()),
        |_| token_check(),
    )
}

fn initialize_with_authority(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    authority: BaselineAuthority,
    observe: impl FnMut(BaselineBoundary) -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    initialize_with_authority_and_token_check(
        verified,
        archive,
        trust,
        authority,
        observe,
        |authority| match authority {
            BaselineAuthority::MachineSystem => keld_guard::require_windows_system_token(),
            BaselineAuthority::PerUser => keld_guard::require_windows_non_system_token(),
            BaselineAuthority::MachineUac => {
                keld_guard::require_windows_non_system_token()?;
                keld_guard::require_windows_machine_uac_owner_token()
            }
        },
    )
}

fn initialize_with_authority_and_token_check(
    verified: &VerifiedBaseline,
    archive: &Path,
    trust: &WindowsBaselineTrust,
    authority: BaselineAuthority,
    mut observe: impl FnMut(BaselineBoundary) -> io::Result<()>,
    token_check: impl FnOnce(BaselineAuthority) -> io::Result<()>,
) -> Result<WindowsBaselineReceipt, UpdateError> {
    trust.require_direct_owner()?;
    if trust.installation.install_mode != authority.mode() {
        return Err(error(
            "installer mode",
            format!(
                "initializer requires explicit {} mode, found {}",
                authority.mode().as_str(),
                trust.installation.install_mode.as_str()
            ),
        ));
    }
    token_check(authority).map_err(|cause| error("installer authority", cause))?;
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

    publish_baseline_version(verified, archive, &roots, authority, &mut observe)?;
    let version = super::load::validate_initial_version(&roots)?;
    seed_baseline_pointers(verified, &roots, &mut observe)?;
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

mod publication;
use publication::{publish_baseline_version, seed_baseline_pointers};

fn acquire_bootstrap_lock(roots: &Roots, update_name: &str) -> Result<File, UpdateError> {
    let profile = roots.profile();
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
    let lock = create_file_relative_exclusive_with_profile(&update, "bootstrap.lock", profile)
        .map_err(|cause| error("exclusive bootstrap lock", cause))?;
    keld_guard::validate_windows_install_file(&lock, profile)
        .map_err(|cause| error("bootstrap lock profile", cause))?;
    lock.sync_all()
        .map_err(|cause| error("bootstrap lock flush", cause))?;
    Ok(lock)
}

fn seed_activation_lock(roots: &Roots) -> Result<(), UpdateError> {
    let profile = roots.profile();
    let update = roots
        .update
        .try_clone()
        .map_err(|cause| error("activation lease parent", cause))?
        .into_std_file();
    let file = create_file_relative_exclusive_with_profile(&update, "activation.lock", profile)
        .map_err(|cause| error("activation lease creation", cause))?;
    file.sync_all()
        .map_err(|cause| error("activation lease flush", cause))?;
    keld_guard::validate_windows_install_file(&file, profile)
        .map_err(|cause| error("activation lease readback profile", cause))?;
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
    let provenance_temp = prepare_record(
        install,
        &provenance,
        trust.installation.install_mode.protection_profile(),
    )?;
    lock.sync_all()
        .map_err(|cause| error("bootstrap lock flush", cause))?;
    keld_guard::validate_windows_install_file(lock, roots.profile())
        .map_err(|cause| error("bootstrap lock profile", cause))?;
    observe(BaselineBoundary::RootsVerified).map_err(|cause| error("root boundary", cause))?;
    publish_prepared_record(
        install,
        &provenance_temp,
        RecordTarget::Absent("install-provenance"),
        &provenance,
        trust.installation.install_mode.protection_profile(),
    )?;
    observe(BaselineBoundary::ProvenancePublished)
        .map_err(|cause| error("provenance boundary", cause))?;
    Ok(())
}
