//! Read-only provenance loader and separate initializer-only seed verification.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::Read;

use cap_fs_ext::{DirExt as _, MetadataExt as _};
use cap_std::fs::{Dir, File};

use super::{
    LoadedWindowsBaseline, Roots, VersionPins, WindowsBaselineTrust, ensure_directory, error,
    exact_entries, open_machine_file, open_roots, read_record,
};
use super::{WindowsActivationWriteSnapshot, WindowsRecoveryInspection};
use crate::DirectInstallMode;
use crate::UpdateVerifier;
use crate::records::{self, PointerKind};
use crate::{ProvenanceObservation, UpdateError};

/// Loads coherent protected initial-baseline metadata without a writable handle.
///
/// Expected identity, publisher and volume must be trusted host configuration. The
/// returned owner retains namespace and record pins; it provides no repair, mutation,
/// active-package selection, current-image signer proof or strict-role authority.
///
/// # Errors
/// Refuses unknown/missing/unprotected records, unsupported ancestry, identity or
/// publisher/volume mismatches, non-baseline pointers/floor/completion, or state
/// requiring later activation/recovery. Private incomplete stages are diagnostic only.
pub fn load_windows_baseline(
    trust: &WindowsBaselineTrust,
) -> Result<LoadedWindowsBaseline, UpdateError> {
    trust.require_direct_owner()?;
    let roots =
        open_roots(trust, false).map_err(|cause| error("committed scaffold admission", cause))?;
    let activation_lease = super::open_activation_lease(&roots.update, roots.profile(), false)
        .map_err(|cause| error("activation snapshot lease", cause))?;
    let install = roots
        .install()
        .map_err(|cause| error("install root", cause))?;
    let (provenance_file, provenance_bytes) =
        read_record(install, "install-provenance", roots.profile())?;
    let record = records::decode_provenance(&provenance_bytes)?;
    crate::provenance::match_identity(&trust.installation, &record.provenance.identity)?;
    if record.publisher_scope != trust.publisher_scope || record.volume_guid != trust.volume_guid {
        return Err(error(
            "provenance trust binding",
            "publisher or expected volume differs",
        ));
    }
    if record.provenance.owner != trust.owner {
        return Err(error(
            "provenance owner binding",
            "protected owner differs from trusted deployment owner",
        ));
    }
    let metadata = read_initial_metadata(&roots)?;
    let floor = metadata.floor;
    let mut retained = metadata.records;
    retained.push(provenance_file);
    retained.push(metadata.completion.marker);
    retained.push(metadata.completion.content);
    Ok(LoadedWindowsBaseline {
        observation: ProvenanceObservation::Protected {
            record: record.provenance,
            version_floor: Some(floor.clone()),
        },
        floor,
        publisher_scope: record.publisher_scope,
        roots,
        _activation_lease: activation_lease,
        _records: retained,
        _baseline_version: metadata.completion.version,
        _baseline_tree: metadata.completion.tree,
    })
}

/// Selects the committed package tree for an ordinary startup (KEL-254 AC4).
///
/// Under the shared snapshot lease it binds protected provenance, reads the floor and
/// every pointer, and requires `current` to equal last-known-good or previous-known-good
/// with every selected version at or below the floor. The version census admits only
/// those versions plus generated diagnostics, and each selected version passes the same
/// metadata admission as the baseline loader (its exact entries, completion identity and
/// retained archive length) plus its signed no-migration package policy, whose exact
/// bytes `keld-pack` owns. Content is not rehashed at startup: `contentBlake3` was verified
/// before publication into the protected, immutable tree, and the boot-file and access
/// checks belong to KEL-254 and KEL-96. The lease and every record handle close before
/// return; the selected version, its tree and the protected ancestry stay open.
///
/// The only write is the startup repair of an invalid `current`: when the `current`
/// record is absent or does not decode, and last-known-good is valid, a `PerUserDirect`
/// installation takes the exclusive writer lease, re-validates everything, durably
/// republishes last-known-good as `current`, reads it back, and selects once more. A
/// `current` that decodes is never rewritten: one that is not a known-good artifact
/// refuses with its evidence intact.
///
/// # Errors
/// A pending activation journal refuses with
/// [`crate::ActivationEffect::JournalBoundRecoveryRequired`]: only journal-bound recovery
/// under the writer lease may continue it, so no tree is selected. A sharing conflict on
/// the lease, normally the updater's exclusive writer lease, refuses with
/// [`crate::ActivationEffect::WriterActive`] without retrying.
/// Managed ownership, a missing or damaged lease, unknown or malformed state, provenance
/// or pointer relationships that do not hold, an absent or changed package policy, and any
/// unreferenced, missing or substituted version each refuse. The selection never guesses
/// the newest directory or substitutes the installed baseline. An invalid `current` in a
/// machine installation refuses: only its elevated writer may republish last-known-good.
/// In `MachineUacDirect` a pending journal of any phase, and that invalid `current` once
/// the snapshot shows a valid last-known-good (the same read-only checks the per-user
/// repair runs, any failure of which keeps its own untyped refusal), refuse with the typed
/// [`crate::ActivationEffect::MachineRecoveryRequired`] instead, writing nothing: only
/// the elevated helper's recovery-only role resolves them.
pub fn select_windows_active_package(
    trust: &WindowsBaselineTrust,
) -> Result<super::ActivePackageSelection, UpdateError> {
    select_with_repair_gate(trust, None)
}

/// The image whose own path located its installation and the version tree that holds
/// it (KEL-254 T2b executable-located selection).
#[derive(Debug, Clone, Copy)]
pub(super) struct LocatedVersion<'a> {
    pub(super) image: crate::WindowsLocatedImage,
    pub(super) version: &'a str,
}

/// Shared selection body. `located`, when present, is the version tree that holds the
/// running executable (KEL-254 T2b): an invalid `current` is then repaired only when
/// last-known-good is exactly that version, so an executable started from any other
/// tree never causes a write.
pub(super) fn select_with_repair_gate(
    trust: &WindowsBaselineTrust,
    located: Option<LocatedVersion<'_>>,
) -> Result<super::ActivePackageSelection, UpdateError> {
    match select_committed_package(trust)? {
        CommittedSelection::Selected(selection) => Ok(*selection),
        CommittedSelection::CurrentInvalid(cause) => {
            repair_invalid_current(trust, &cause, located)?;
            // One reselection after a durable repair; a second failure is returned as is.
            match select_committed_package(trust)? {
                CommittedSelection::Selected(selection) => Ok(*selection),
                CommittedSelection::CurrentInvalid(cause) => Err(cause),
            }
        }
    }
}

/// Outcome of one shared-lease selection snapshot.
enum CommittedSelection {
    Selected(Box<super::ActivePackageSelection>),
    /// The `current` record is absent or does not decode; this is why.
    CurrentInvalid(UpdateError),
}

/// Maps a failure to open the activation lease, typing a sharing conflict.
fn lease_error(step: &'static str, cause: &std::io::Error) -> UpdateError {
    if cause.raw_os_error()
        == Some(windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION.cast_signed())
    {
        UpdateError::activation(
            "active package selection",
            crate::ActivationEffect::WriterActive,
            "a conflicting handle, normally the exclusive writer lease, holds activation.lock",
        )
    } else {
        error(step, cause)
    }
}

/// What an ordinary startup returns for a Machine-UAC recovery-required state (KEL-53
/// "Machine-UAC recovery-required state and recovery-only role"). Every such state carries
/// `RecoveryDisabled` until the helper's recovery-only role is enabled: KEL-270 T4d slice
/// S10 for an unlaunched attempt or an invalid `current`, slice S12 for a launched attempt.
const MACHINE_RECOVERY_REQUIRED: crate::ActivationEffect =
    crate::ActivationEffect::MachineRecoveryRequired(
        crate::MachineRecoveryGuidance::RecoveryDisabled,
    );

/// The journal-free selection's refusal of a pending journal in any phase. In
/// `MachineUacDirect` only the elevated helper writes, so an ordinary process infers
/// nothing and returns the typed recovery-required state; the other modes keep
/// journal-bound recovery under the writer lease.
pub(super) fn pending_journal_refusal(
    mode: DirectInstallMode,
    phase: &records::ActivationPhase,
) -> UpdateError {
    let (effect, resolver) = match mode {
        DirectInstallMode::MachineUacDirect => (
            MACHINE_RECOVERY_REQUIRED,
            "the elevated helper's recovery-only role",
        ),
        DirectInstallMode::PerUserDirect | DirectInstallMode::MachineSeamlessDirect => (
            crate::ActivationEffect::JournalBoundRecoveryRequired,
            "journal-bound recovery",
        ),
    };
    UpdateError::activation(
        "active package selection",
        effect,
        format!(
            "a pending {phase:?} activation journal selects nothing until {resolver} resolves it"
        ),
    )
}

fn select_committed_package(
    trust: &WindowsBaselineTrust,
) -> Result<CommittedSelection, UpdateError> {
    trust.require_direct_owner()?;
    let roots =
        open_roots(trust, false).map_err(|cause| error("selection root admission", cause))?;
    let lease = super::open_activation_lease(&roots.update, roots.profile(), false)
        .map_err(|cause| lease_error("active selection lease", &cause))?;
    let provenance = read_writer_provenance(trust, &roots)?;
    let records = read_records(&roots, true)?;
    if let Some(journal) = &records.journal {
        return Err(pending_journal_refusal(
            trust.installation.install_mode,
            &journal.phase,
        ));
    }
    let current = match records.current {
        Ok(current) => current,
        Err(cause) => {
            // No Machine-UAC repair runs after this snapshot, so its source is validated
            // here, under the snapshot lease: only a valid last-known-good makes an
            // invalid `current` the typed recovery-required state.
            if trust.installation.install_mode == DirectInstallMode::MachineUacDirect {
                validate_current_repair_source(
                    &trust.installation.baseline,
                    &roots,
                    &records.version_floor,
                    &records.last_known_good,
                    records.previous_known_good.as_ref(),
                )?;
            }
            return Ok(CommittedSelection::CurrentInvalid(cause));
        }
    };
    let records = WriterRecords {
        version_floor: records.version_floor,
        current,
        last_known_good: records.last_known_good,
        previous_known_good: records.previous_known_good,
        journal: None,
    };
    let floor = semver::Version::parse(&records.version_floor)
        .map_err(|cause| error("version floor", cause))?;
    validate_writer_pointer_context(
        &trust.installation.baseline,
        &records.current,
        &records.last_known_good,
        records.previous_known_good.as_ref(),
        &floor,
    )?;
    let mut selected = vec![records.current.clone(), records.last_known_good.clone()];
    if let Some(previous) = &records.previous_known_good
        && !selected.contains(previous)
    {
        selected.push(previous.clone());
    }
    validate_activation_version_census(&roots, &selected, None)?;
    let mut current = None;
    for artifact in &selected {
        let completion = read_version_completion(&roots, artifact)?;
        validate_package_policy(&roots, &completion.tree)?;
        if *artifact == records.current {
            current = Some((completion.version, completion.tree));
        }
    }
    let (version, tree) =
        current.ok_or_else(|| error("active package selection", "current was not admitted"))?;
    drop(lease);
    Ok(CommittedSelection::Selected(Box::new(
        super::ActivePackageSelection {
            identity: provenance.provenance.identity,
            publisher_scope: provenance.publisher_scope,
            tree_root: trust
                .installation
                .update_root
                .join("versions")
                .join(&records.current.version)
                .join("tree"),
            artifact: records.current,
            _roots: roots,
            _version: version,
            tree,
        },
    )))
}

/// Durably republishes last-known-good as `current` when `current` is invalid (KEL-53
/// "startup without a journal"; KEL-254 AC4).
///
/// Only `PerUserDirect` owns a startup writer. Under the exclusive writer lease every
/// fact is re-read: a journal refuses as journal-bound, a `current` that became valid is
/// left alone, and [`validate_current_repair_source`] must pass: last-known-good,
/// previous-known-good and the floor hold with `current` equal to last-known-good, and
/// both known-good versions pass the census, metadata admission and package policy.
/// Only then does one prepared record replace `current` through the shared write-through
/// publication and is read back. A crash leaves either the invalid record, repaired again
/// at the next start, or the valid one.
///
/// A machine installation refuses here before taking any lease or writing:
/// `MachineSeamlessDirect` with its untyped baseline refusal, and `MachineUacDirect` with
/// the typed [`crate::ActivationEffect::MachineRecoveryRequired`], whatever version holds
/// the running executable. In `MachineUacDirect` the same last-known-good validation has
/// already passed in the selection snapshot, under its shared lease; a failure there
/// refuses untyped and never reaches this function.
pub(super) fn repair_invalid_current(
    trust: &WindowsBaselineTrust,
    invalid: &UpdateError,
    located: Option<LocatedVersion<'_>>,
) -> Result<(), UpdateError> {
    match trust.installation.install_mode {
        DirectInstallMode::PerUserDirect => {}
        DirectInstallMode::MachineUacDirect => {
            return Err(UpdateError::activation(
                "current pointer repair",
                MACHINE_RECOVERY_REQUIRED,
                format!(
                    "current is invalid ({}) and only the elevated helper's recovery-only role may republish last-known-good",
                    super::activate::refusal_detail(invalid)
                ),
            ));
        }
        DirectInstallMode::MachineSeamlessDirect => {
            return Err(error(
                "current pointer repair",
                format!(
                    "current is invalid ({}) and only the elevated writer of a machine installation may republish last-known-good",
                    super::activate::refusal_detail(invalid)
                ),
            ));
        }
    }
    // Only a per-user host repairs `current`: the updater helper runs only in
    // `MachineUacDirect` (KEL-53 "Helper launch and self-anchor"), so a helper locator
    // never reaches the writer lease or a write in any mode.
    if let Some(located) = located
        && located.image == crate::WindowsLocatedImage::UpdaterHelper
    {
        return Err(UpdateError::ExecutableBinding {
            image: located.image,
            step: "current pointer repair",
            detail: "only a host repairs current; keld-updater-helper.exe never does".to_owned(),
        });
    }
    let roots = open_roots(trust, false).map_err(|cause| error("repair root admission", cause))?;
    let profile = roots.profile();
    let lease = super::open_activation_lease(&roots.update, profile, true)
        .map_err(|cause| lease_error("exclusive repair lease", &cause))?;
    drop(read_writer_provenance(trust, &roots)?);
    let records = read_records(&roots, true)?;
    if let Some(journal) = &records.journal {
        return Err(pending_journal_refusal(
            trust.installation.install_mode,
            &journal.phase,
        ));
    }
    if records.current.is_ok() {
        // Another writer repaired it after the snapshot; nothing to do.
        return Ok(());
    }
    if let Some(located) = located
        && records.last_known_good.version != located.version
    {
        return Err(UpdateError::ExecutableBinding {
            image: located.image,
            step: "current pointer repair",
            detail: format!(
                "the running executable is in version `{}`, not last-known-good `{}`; only a last-known-good {} repairs current",
                located.version,
                records.last_known_good.version,
                match located.image {
                    crate::WindowsLocatedImage::Host => "host",
                    crate::WindowsLocatedImage::UpdaterHelper => "keld-updater-helper.exe",
                }
            ),
        });
    }
    validate_current_repair_source(
        &trust.installation.baseline,
        &roots,
        &records.version_floor,
        &records.last_known_good,
        records.previous_known_good.as_ref(),
    )?;
    super::activate::remove_stale_record_preparations(&roots)?;
    let bytes = records::encode_pointer(PointerKind::Current, &records.last_known_good)?;
    let temporary = super::prepare_record(&roots.update, &bytes, profile)?;
    super::publish_prepared_record(
        &roots.update,
        &temporary,
        super::RecordTarget::Replace(crate::windows_fs::RecordSlot::Current),
        &bytes,
        profile,
    )?;
    drop(lease);
    Ok(())
}

/// Read-only validation of last-known-good as the source that replaces an invalid
/// `current` (KEL-254 AC4 "valid last-known-good"), over records the caller read under
/// its held lease: last-known-good, standing as `current`, with previous-known-good and
/// the floor; the version census; and each known-good version's completion record and
/// package policy. Every failure keeps its own untyped refusal, so the exact reason
/// survives (KEL-254 AC16).
pub(super) fn validate_current_repair_source(
    baseline: &crate::ArtifactIdentity,
    roots: &Roots,
    version_floor: &str,
    last_known_good: &crate::ArtifactIdentity,
    previous_known_good: Option<&crate::ArtifactIdentity>,
) -> Result<(), UpdateError> {
    let floor =
        semver::Version::parse(version_floor).map_err(|cause| error("version floor", cause))?;
    validate_writer_pointer_context(
        baseline,
        last_known_good,
        last_known_good,
        previous_known_good,
        &floor,
    )?;
    let mut selected = vec![last_known_good.clone()];
    if let Some(previous) = previous_known_good {
        selected.push(previous.clone());
    }
    validate_activation_version_census(roots, &selected, None)?;
    for artifact in &selected {
        let completion = read_version_completion(roots, artifact)?;
        validate_package_policy(roots, &completion.tree)?;
    }
    Ok(())
}

/// What the updater helper's self-anchor reads in one snapshot under the shared lease
/// (KEL-53 "Helper launch and self-anchor").
pub(super) struct AnchorRecords {
    /// The pending activation journal's `helper_image_blake3`, when a journal exists.
    pub(super) journaled_helper_image: Option<[u8; 32]>,
    /// The `current` pointer, or why it is absent or does not decode.
    pub(super) current: Result<crate::ArtifactIdentity, UpdateError>,
    /// The last-known-good pointer.
    pub(super) last_known_good: crate::ArtifactIdentity,
}

/// Reads, under the shared snapshot lease, the record bound to `trust` and the activation
/// records the self-anchor decides on, admitting a pending journal of any phase. It
/// writes nothing and closes the lease and every record before it returns.
pub(super) fn read_anchor_records(
    trust: &WindowsBaselineTrust,
) -> Result<AnchorRecords, UpdateError> {
    let roots =
        open_roots(trust, false).map_err(|cause| error("self-anchor root admission", cause))?;
    let lease = super::open_activation_lease(&roots.update, roots.profile(), false)
        .map_err(|cause| lease_error("self-anchor snapshot lease", &cause))?;
    drop(read_writer_provenance(trust, &roots)?);
    let records = read_records(&roots, true)?;
    drop(lease);
    Ok(AnchorRecords {
        journaled_helper_image: records.journal.map(|journal| journal.helper_image_blake3),
        current: records.current,
        last_known_good: records.last_known_good,
    })
}

/// What the accepted connect-back claimant matched in its claim and observed of its
/// owner (KEL-53 §4 "Candidate connect-back", *After acceptance*).
pub(super) struct AcceptedClaim<'a> {
    /// The attempt ID that the owner's `KELD-AC1` offered and the claim bound to the
    /// rendezvous name.
    pub(super) attempt_id: &'a [u8; 32],
    /// The health-channel ID that the same `KELD-AC1` offered.
    pub(super) health_channel_id: &'a [u8; 32],
    /// The process ID and creation time of the endpoint's server process.
    pub(super) owner: crate::AttemptOwner,
    /// That server process's executable image.
    pub(super) owner_image: &'a std::fs::File,
    /// The version whose tree holds the claimant's running executable.
    pub(super) located_version: &'a str,
}

/// Criterion 20's authenticated candidate-boot read, below the claimant's own binding:
/// the one reader exception to the writer lease (KEL-53 §4 "Candidate connect-back",
/// *After acceptance*).
///
/// It opens no lease. It reads the mutable records once and closes each before
/// returning, and it writes nothing. It requires, in order: a pending journal in
/// `AwaitingHealth`; its attempt and health-channel IDs equal to `claim`'s; a v2 owner
/// whose process ID and creation time equal `claim.owner`; the BLAKE3 of the owner's
/// image equal to the journaled `helper_image_blake3`; the journal's candidate in the
/// claimant's own version tree; `current`, the floor and the known-good slots exactly as
/// an `AwaitingHealth` journal requires; the version census; and the candidate's
/// completion record and package policy. It returns the candidate's selection, which
/// pins only its immutable version and tree with the protected ancestry, and the
/// health-receipt digest over the attempt, the health channel and the candidate that the
/// journal names.
pub(super) fn read_candidate_boot(
    roots: Roots,
    claim: &AcceptedClaim<'_>,
) -> Result<(super::ActivePackageSelection, [u8; 32]), UpdateError> {
    let records = read_records(&roots, true)?;
    let journal = records
        .journal
        .ok_or_else(|| error("candidate attempt", "no activation journal is pending"))?;
    bind_accepted_claim(&journal, claim)?;
    let current = records.current?;
    crate::activation::validate_protected_recovery_state(
        &journal,
        &current,
        &records.version_floor,
        &records.last_known_good,
        records.previous_known_good.as_ref(),
    )
    .map_err(|refusal| error("candidate attempt state", format!("{refusal:?}")))?;
    let trust = &roots.trust;
    validate_artifact_scope_and_baseline(&trust.installation.baseline, &journal.candidate)?;
    let slots = [Some(&current), Some(&records.last_known_good)]
        .into_iter()
        .chain([records.previous_known_good.as_ref()])
        .flatten();
    validate_activation_version_census(&roots, &recovery_selection(&journal, slots, &[]), None)?;
    let completion = read_version_completion(&roots, &journal.candidate)?;
    validate_package_policy(&roots, &completion.tree)?;
    let health_receipt_digest = records::activation_health_receipt_digest(
        &journal.attempt_id,
        &journal.health_channel_id,
        &journal.candidate,
    )?;
    let identity = trust.installation.clone();
    let publisher_scope = trust.publisher_scope;
    let tree_root = identity
        .update_root
        .join("versions")
        .join(&journal.candidate.version)
        .join("tree");
    let selection = super::ActivePackageSelection {
        identity,
        publisher_scope,
        tree_root,
        artifact: journal.candidate,
        _roots: roots,
        _version: completion.version,
        tree: completion.tree,
    };
    Ok((selection, health_receipt_digest))
}

/// The claim-binding part of [`read_candidate_boot`]: the pending journal names the
/// attempt that accepted this claimant, its owner and the claimant's own tree.
fn bind_accepted_claim(
    journal: &records::ActivationJournal,
    claim: &AcceptedClaim<'_>,
) -> Result<(), UpdateError> {
    if journal.phase != records::ActivationPhase::AwaitingHealth {
        return Err(error(
            "candidate attempt phase",
            format!(
                "the pending journal is {:?}, not AwaitingHealth",
                journal.phase
            ),
        ));
    }
    if &journal.attempt_id != claim.attempt_id {
        return Err(error(
            "candidate attempt identity",
            "the journaled attempt is not the one the owner offered in KELD-AC1",
        ));
    }
    if &journal.health_channel_id != claim.health_channel_id {
        return Err(error(
            "candidate health channel",
            "the journaled health channel is not the one the owner offered in KELD-AC1",
        ));
    }
    let owner = journal
        .ownership
        .ok_or_else(|| {
            error(
                "candidate attempt owner",
                "a keld.activation-journal/v1 record names no attempt owner and admits no claim",
            )
        })?
        .attempt_owner;
    if owner.process_id() != claim.owner.process_id() {
        return Err(error(
            "candidate owner process",
            format!(
                "the endpoint's server process {} is not the journaled owner {}",
                claim.owner.process_id(),
                owner.process_id()
            ),
        ));
    }
    if owner.creation_time() != claim.owner.creation_time() {
        return Err(error(
            "candidate owner creation time",
            "the endpoint's server process was not created when the journaled owner was",
        ));
    }
    let image = super::image_blake3(claim.owner_image)
        .map_err(|cause| error("candidate owner image", cause))?;
    if image != journal.helper_image_blake3 {
        return Err(error(
            "candidate owner image",
            format!(
                "the server image's BLAKE3 `{}` is not the journaled `{}`",
                crate::error::hex_digest(&image),
                crate::error::hex_digest(&journal.helper_image_blake3)
            ),
        ));
    }
    if journal.candidate.version != claim.located_version {
        return Err(error(
            "candidate tree",
            format!(
                "the running executable is in version `{}`, not the attempt's candidate `{}`",
                claim.located_version, journal.candidate.version
            ),
        ));
    }
    Ok(())
}

/// Checks one immutable tree's signed no-migration policy (KEL-53 criterion 13): the
/// exact `.keld/update-policy.v1` bytes owned by `keld-pack`. An absent, unprotected or
/// changed policy refuses before launch.
fn validate_package_policy(roots: &Roots, tree: &Dir) -> Result<(), UpdateError> {
    let (directory, leaf) = keld_pack::UPDATE_POLICY_PATH
        .split_once('/')
        .ok_or_else(|| error("package policy", "policy path has no directory"))?;
    let policy_directory = open_directory(tree, directory, roots.profile())?;
    let file = open_machine_file(&policy_directory, leaf, roots.profile())
        .map_err(|cause| error("package policy", cause))?;
    let mut bytes = Vec::new();
    file.take(keld_pack::NO_MIGRATION_POLICY.len() as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|cause| error("package policy", cause))?;
    if bytes != keld_pack::NO_MIGRATION_POLICY {
        return Err(error(
            "package policy",
            "the tree's update policy differs from the signed no-migration policy",
        ));
    }
    Ok(())
}

/// Loads mutable activation state under the per-user installation's exclusive writer lease.
///
/// It validates protected provenance, floor, current/LKG/previous and every referenced
/// complete version, then drops all mutable-record read handles while retaining only
/// immutable version pins and the exclusive lease. Pending journals refuse because this
/// entry point has no process-family recovery observation. Machine-seamless is gated.
///
/// # Errors
/// Refuses mode/configuration mismatch, busy/missing lease, unknown local state, pending
/// recovery, malformed state, invalid version relationships or a selected tree mismatch.
pub fn load_windows_activation_write_snapshot(
    trust: &WindowsBaselineTrust,
    verifier: &UpdateVerifier,
) -> Result<WindowsActivationWriteSnapshot, UpdateError> {
    load_activation_write_snapshot_inner(trust, verifier, false)
}

/// Loads a protected pending activation's journal and pointer observations under the
/// exclusive per-user writer lease.
///
/// The returned owner retains the lease, protected ancestry and referenced version pins.
/// Loading writes nothing; the owner's only mutation paths are
/// [`WindowsRecoveryInspection::recover`], which first requires an exact process-family
/// retirement binding for the inspected journal, and
/// [`WindowsRecoveryInspection::resume_unlaunched`] for a never-launched attempt. The
/// ordinary writer loader continues to refuse every pending journal.
///
/// # Errors
///
/// Refuses managed or privileged modes, a busy/missing lease, provenance/profile mismatch,
/// absent or malformed journal, unsupported state, inconsistent phase/pointer/floor facts,
/// or any referenced version tree that is missing, substituted or unreferenced. Only the
/// one version whose retirement is the journal's next step may already be absent.
pub fn load_windows_recovery_inspection(
    trust: &WindowsBaselineTrust,
    verifier: &UpdateVerifier,
) -> Result<WindowsRecoveryInspection, UpdateError> {
    trust.require_direct_owner()?;
    crate::provenance::match_identity(&trust.installation, &verifier.expected)?;
    if trust.installation.install_mode != DirectInstallMode::PerUserDirect {
        return Err(error(
            "activation recovery inspection mode",
            "only PerUserDirect has production recovery-inspection admission",
        ));
    }
    let roots =
        open_roots(trust, false).map_err(|cause| error("recovery root admission", cause))?;
    let profile = roots.profile();
    let lease = super::open_activation_lease(&roots.update, profile, true)
        .map_err(|cause| error("exclusive recovery inspection lease", cause))?;
    let provenance = read_writer_provenance(trust, &roots)?;
    let lifecycle_installation_id = records::lifecycle_installation_id(
        &provenance.provenance,
        &provenance.publisher_scope,
        &provenance.volume_guid,
    )?;
    let records = read_writer_records(&roots, true)?;
    let journal = records
        .journal
        .ok_or_else(|| error("activation recovery inspection", "pending journal absent"))?;
    let observation = ProvenanceObservation::Protected {
        record: provenance.provenance,
        version_floor: Some(records.version_floor.clone()),
    };
    let admitted = verifier.admit(&observation)?;
    let floor = semver::Version::parse(&records.version_floor)
        .map_err(|cause| error("recovery version floor", cause))?;
    crate::activation::validate_protected_recovery_state(
        &journal,
        &records.current,
        &records.version_floor,
        &records.last_known_good,
        records.previous_known_good.as_ref(),
    )
    .map_err(|refusal| error("activation recovery inspection", format!("{refusal:?}")))?;
    validate_recovery_known_good_history(
        &trust.installation.baseline,
        &journal.prior_last_known_good,
        journal.prior_previous_known_good.as_ref(),
    )?;

    // A version whose retirement is the next journal step may already be renamed by
    // a lost owner; it is admitted by name but never pinned, validated or selected.
    // A publish-pending attempt whose floor is still prior may not have renamed its
    // stage yet: its candidate is located here and verified by the transaction itself.
    let unpublished_candidate = journal.phase == crate::records::ActivationPhase::PublishPending
        && records.version_floor == journal.prior_floor;
    let retiree = crate::activation::retirement_due(
        &journal,
        &crate::activation::ProtectedSlots {
            version_floor: &records.version_floor,
            current: &records.current,
            last_known_good: &records.last_known_good,
            previous_known_good: records.previous_known_good.as_ref(),
        },
    )
    .cloned();
    let excluded = [
        retiree.as_ref(),
        unpublished_candidate.then_some(&journal.candidate),
    ];
    let slots = [Some(&records.current), Some(&records.last_known_good)]
        .into_iter()
        .chain([records.previous_known_good.as_ref()])
        .flatten();
    let selected = recovery_selection(&journal, slots, &excluded);
    for artifact in [&records.current, &records.last_known_good]
        .into_iter()
        .chain(records.previous_known_good.iter())
    {
        validate_selected_artifact(&trust.installation.baseline, artifact, &floor)?;
    }
    let prior_floor = semver::Version::parse(&journal.prior_floor)
        .map_err(|cause| error("recovery prior floor", cause))?;
    for artifact in [&journal.rollback_target, &journal.prior_last_known_good]
        .into_iter()
        .chain(journal.prior_previous_known_good.iter())
    {
        validate_selected_artifact(&trust.installation.baseline, artifact, &prior_floor)?;
    }
    validate_artifact_scope_and_baseline(&trust.installation.baseline, &journal.candidate)?;
    let (admitted_by_name, candidate_stage) = if unpublished_candidate {
        locate_unpublished_candidate(&roots, &journal.candidate)?
    } else {
        (retiree.clone(), None)
    };
    let version_pins = pin_selected_versions(&roots, &selected, admitted_by_name.as_ref())?;
    Ok(WindowsRecoveryInspection {
        roots,
        lease,
        admitted,
        lifecycle_installation_id,
        journal,
        version_floor: records.version_floor,
        current: records.current,
        last_known_good: records.last_known_good,
        previous_known_good: records.previous_known_good,
        version_pins,
        candidate_stage,
    })
}

/// Retires complete versions that neither a protected record nor a journal references,
/// under the exclusive per-user writer lease.
///
/// Such a version is left only by an activation start that crashed, or whose own
/// retirement failed, after publication but before its `PublishPending` journal. The
/// ordinary writer loader keeps halting on it, because selection never follows directory
/// presence. This explicit repair is admitted only when no activation journal exists and
/// every protected record validates. It first re-verifies and pins every referenced
/// version, then removes stale `pending-*` record siblings, then validates every other
/// non-generated entry as a strict-SemVer version with a completion record in this
/// installation's scope before renaming any of them to a generated, never-selectable
/// `retired-*` name, and finally re-runs the version census. It returns how many versions
/// it retired.
///
/// # Errors
/// Refuses managed or privileged modes, a busy or missing lease, a pending journal (use
/// journal-bound recovery), any provenance or record inconsistency, and any unknown or
/// damaged version entry (including a non-directory under a generated name), each before
/// any rename and with its own diagnostic: such an entry needs manual recovery, because
/// repeating the repair cannot fix it. Only a failed rename reports the retryable
/// [`crate::ActivationEffect::UnjournaledVersionRetained`]. No path changes a record.
pub fn repair_windows_unjournaled_versions(
    trust: &WindowsBaselineTrust,
    verifier: &UpdateVerifier,
) -> Result<usize, UpdateError> {
    trust.require_direct_owner()?;
    crate::provenance::match_identity(&trust.installation, &verifier.expected)?;
    if trust.installation.install_mode != DirectInstallMode::PerUserDirect {
        return Err(error(
            "unjournaled version repair mode",
            "only PerUserDirect has production writer admission",
        ));
    }
    let roots = open_roots(trust, false).map_err(|cause| error("repair root admission", cause))?;
    let lease = super::open_activation_lease(&roots.update, roots.profile(), true)
        .map_err(|cause| error("exclusive repair lease", cause))?;
    let state = load_writer_state(trust, verifier, &roots)?;
    let referenced = state
        .selected
        .iter()
        .map(|artifact| artifact.version.clone())
        .collect::<BTreeSet<_>>();
    // Hold every referenced tree open first: a referenced version must be verified before
    // anything moves, and its open handles keep any later rename away from it.
    let pins = pin_versions(&roots, &state.selected)?;
    super::activate::remove_stale_record_preparations(&roots)?;
    // An unknown or damaged entry keeps its own diagnostic: repeating the repair cannot
    // fix it, so it needs manual recovery. Only a failed rename is retryable.
    let admitted = super::activate::admit_unreferenced_versions(&roots, &referenced)?;
    let retired =
        super::activate::retire_admitted_versions(&roots, &admitted).map_err(|cause| {
            crate::UpdateError::activation(
                "unjournaled version repair",
                crate::ActivationEffect::UnjournaledVersionRetained,
                super::activate::refusal_detail(&cause),
            )
        })?;
    validate_activation_version_census(&roots, &state.selected, None)?;
    drop(pins);
    drop(lease);
    Ok(retired)
}

#[cfg(test)]
pub(crate) fn load_windows_activation_write_snapshot_for_test(
    trust: &WindowsBaselineTrust,
    verifier: &UpdateVerifier,
) -> Result<WindowsActivationWriteSnapshot, UpdateError> {
    load_activation_write_snapshot_inner(trust, verifier, true)
}

fn load_activation_write_snapshot_inner(
    trust: &WindowsBaselineTrust,
    verifier: &UpdateVerifier,
    allow_uac_fixture: bool,
) -> Result<WindowsActivationWriteSnapshot, UpdateError> {
    trust.require_direct_owner()?;
    crate::provenance::match_identity(&trust.installation, &verifier.expected)?;
    match trust.installation.install_mode {
        DirectInstallMode::PerUserDirect => {}
        DirectInstallMode::MachineUacDirect if allow_uac_fixture => {}
        DirectInstallMode::MachineUacDirect => {
            return Err(error(
                "activation writer mechanism",
                "MachineUacDirect remains gated on authenticated elevated-helper admission",
            ));
        }
        DirectInstallMode::MachineSeamlessDirect => {
            return Err(error(
                "activation writer mechanism",
                "MachineSeamlessDirect remains gated on its authenticated privileged mechanism",
            ));
        }
    }
    let roots = open_roots(trust, false).map_err(|cause| error("writer root admission", cause))?;
    let profile = roots.profile();
    let lease = super::open_activation_lease(&roots.update, profile, true)
        .map_err(|cause| error("exclusive activation writer lease", cause))?;
    let state = load_writer_state(trust, verifier, &roots)?;
    let version_pins = pin_selected_versions(&roots, &state.selected, None)?;
    Ok(WindowsActivationWriteSnapshot {
        roots,
        lease,
        admitted: state.admitted,
        version_floor: state.version_floor,
        current: state.current,
        last_known_good: state.last_known_good,
        previous_known_good: state.previous_known_good,
        version_pins,
    })
}

mod writer_records;
use writer_records::{
    WriterRecords, load_writer_state, read_records, read_writer_provenance, read_writer_records,
    validate_recovery_known_good_history, validate_writer_pointer_context,
};

/// Re-verifies and pins every selected version, keyed by its version directory name.
///
/// `retiree` is the one journal-authorized unreferenced version that may still be
/// present under its version name; it is admitted by the census but never opened.
pub(super) fn pin_selected_versions(
    roots: &Roots,
    selected: &[crate::ArtifactIdentity],
    retiree: Option<&crate::ArtifactIdentity>,
) -> Result<BTreeMap<String, VersionPins>, UpdateError> {
    validate_activation_version_census(roots, selected, retiree)?;
    pin_versions(roots, selected)
}

/// Re-verifies and pins each selected version without a directory census.
fn pin_versions(
    roots: &Roots,
    selected: &[crate::ArtifactIdentity],
) -> Result<BTreeMap<String, VersionPins>, UpdateError> {
    selected
        .iter()
        .map(|artifact| {
            let completion = read_version_completion(roots, artifact)?;
            Ok((
                artifact.version.clone(),
                validate_version_contents(roots, completion)?,
            ))
        })
        .collect()
}

/// Admits one unreferenced entry for retirement: a protected directory whose completion
/// record decodes and names exactly this version within the installation's scope.
pub(super) fn validate_unjournaled_version(roots: &Roots, name: &str) -> Result<(), UpdateError> {
    let version = open_directory(&roots.versions, name, roots.profile())?;
    let (_, bytes) = read_record(&version, ".complete", roots.profile())?;
    let complete = records::decode_complete(&bytes)?;
    if complete.artifact.version != name {
        return Err(error(
            "unjournaled version admission",
            "completion record names a different version",
        ));
    }
    validate_artifact_scope_and_baseline(&roots.trust.installation.baseline, &complete.artifact)?;
    Ok(())
}

/// Every version a recovery must keep pinned: the protected slots and the journal's
/// artifacts, except the `excluded` ones that the transaction handles by name.
fn recovery_selection<'a>(
    journal: &'a records::ActivationJournal,
    slots: impl Iterator<Item = &'a crate::ArtifactIdentity>,
    excluded: &[Option<&crate::ArtifactIdentity>],
) -> Vec<crate::ArtifactIdentity> {
    let mut selected: Vec<crate::ArtifactIdentity> = Vec::new();
    for artifact in slots
        .chain([
            &journal.candidate,
            &journal.rollback_target,
            &journal.prior_last_known_good,
        ])
        .chain(journal.prior_previous_known_good.iter())
    {
        if !selected.contains(artifact) && !excluded.contains(&Some(artifact)) {
            selected.push(artifact.clone());
        }
    }
    selected
}

/// Locates a publish-pending candidate whose floor is still prior: a published version
/// is admitted by name for the transaction to verify; otherwise its completed stage, if
/// any, is returned for publication.
fn locate_unpublished_candidate(
    roots: &Roots,
    candidate: &crate::ArtifactIdentity,
) -> Result<(Option<crate::ArtifactIdentity>, Option<String>), UpdateError> {
    match roots.versions.symlink_metadata(&candidate.version) {
        Ok(_) => Ok((Some(candidate.clone()), None)),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {
            Ok((None, find_candidate_stage(roots, candidate)?))
        }
        Err(cause) => Err(error("candidate presence", cause)),
    }
}

/// Whether `stage` holds a completion record. Only a definite absence is `false`; every
/// other failure is returned, never read as absence.
pub(super) fn completion_present(stage: &Dir) -> std::io::Result<bool> {
    match stage.symlink_metadata(".complete") {
        Ok(_) => Ok(true),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(cause) => Err(cause),
    }
}

/// Reads the completion identity of one generated `incomplete-*` stage after admitting
/// the stage directory and its completion record against the installation profile.
///
/// `None` means the stage has no completion record. Every other failure, including a
/// stage or record that does not admit, is a fault.
pub(super) fn completed_stage_identity(
    roots: &Roots,
    stage: &str,
) -> Result<Option<crate::ArtifactIdentity>, UpdateError> {
    if !super::is_generated_leaf(stage, "incomplete") {
        return Err(error("candidate stage", "not a generated stage name"));
    }
    let directory = open_directory(&roots.versions, stage, roots.profile())?;
    if !completion_present(&directory).map_err(|cause| error("stage completion", cause))? {
        return Ok(None);
    }
    let (_, bytes) = read_record(&directory, ".complete", roots.profile())?;
    Ok(Some(records::decode_complete(&bytes)?.artifact))
}

/// Finds the lowest-named completed stage whose completion record names exactly
/// `candidate`.
///
/// Only two answers skip a stage: it has no completion record, or its record names
/// another artifact. Any fault reading a stage is returned, so recovery halts with the
/// journal intact instead of mistaking an unreadable stage for an absent one. Every copy
/// is fully re-verified after its rename, and a copy that fails is retired before the
/// next exact copy is tried, so the order among exact matches never decides the outcome.
pub(super) fn find_candidate_stage(
    roots: &Roots,
    candidate: &crate::ArtifactIdentity,
) -> Result<Option<String>, UpdateError> {
    let mut stages = Vec::new();
    for entry in roots
        .versions
        .entries()
        .map_err(|cause| error("candidate stage census", cause))?
    {
        let entry = entry.map_err(|cause| error("candidate stage census", cause))?;
        if let Ok(name) = entry.file_name().into_string()
            && super::is_generated_leaf(&name, "incomplete")
        {
            stages.push(name);
        }
    }
    stages.sort();
    for stage in stages {
        if completed_stage_identity(roots, &stage)?.as_ref() == Some(candidate) {
            return Ok(Some(stage));
        }
    }
    Ok(None)
}

/// Validates the version census around a published candidate: only the already-pinned
/// selections, this candidate and generated diagnostics may exist. A failure here is
/// about other entries, never about the candidate's own content.
pub(super) fn validate_publication_census(
    roots: &Roots,
    pinned: &[crate::ArtifactIdentity],
    candidate: &crate::ArtifactIdentity,
) -> Result<(), UpdateError> {
    let mut selected = pinned.to_vec();
    selected.push(candidate.clone());
    validate_activation_version_census(roots, &selected, None)
}

/// Fully re-verifies and pins one version published under the held writer lease: its
/// completion record, archive and tree.
pub(super) fn verify_published_version(
    roots: &Roots,
    candidate: &crate::ArtifactIdentity,
) -> Result<VersionPins, UpdateError> {
    validate_version_contents(roots, read_version_completion(roots, candidate)?)
}

pub(super) fn validate_selected_artifact(
    baseline: &crate::ArtifactIdentity,
    selected: &crate::ArtifactIdentity,
    floor: &semver::Version,
) -> Result<(), UpdateError> {
    let version = validate_artifact_scope_and_baseline(baseline, selected)?;
    if version.cmp_precedence(floor) == Ordering::Greater {
        return Err(error(
            "activation pointer version",
            "selected version is above the protected floor",
        ));
    }
    Ok(())
}

pub(super) fn validate_artifact_scope_and_baseline(
    baseline: &crate::ArtifactIdentity,
    selected: &crate::ArtifactIdentity,
) -> Result<semver::Version, UpdateError> {
    if selected.app_id != baseline.app_id
        || selected.channel != baseline.channel
        || selected.target != baseline.target
    {
        return Err(error(
            "activation pointer scope",
            "selected artifact differs from the trusted application/channel/target",
        ));
    }
    let version = semver::Version::parse(&selected.version)
        .map_err(|cause| error("activation pointer version", cause))?;
    let baseline_version = semver::Version::parse(&baseline.version)
        .map_err(|cause| error("baseline artifact version", cause))?;
    match version.cmp_precedence(&baseline_version) {
        Ordering::Less => {
            return Err(error(
                "activation pointer version",
                "selected version is below the installed baseline",
            ));
        }
        Ordering::Equal if selected != baseline => {
            return Err(error(
                "activation pointer identity",
                "baseline precedence must retain the exact installed artifact identity",
            ));
        }
        Ordering::Equal | Ordering::Greater => {}
    }
    Ok(version)
}

fn validate_activation_version_census(
    roots: &Roots,
    selected: &[crate::ArtifactIdentity],
    retiree: Option<&crate::ArtifactIdentity>,
) -> Result<(), UpdateError> {
    let selected_names = selected
        .iter()
        .chain(retiree)
        .map(|artifact| artifact.version.as_str())
        .collect::<BTreeSet<_>>();
    for entry in roots
        .versions
        .entries()
        .map_err(|cause| error("activation versions", cause))?
    {
        let entry = entry.map_err(|cause| error("activation versions", cause))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| error("activation versions", "non-UTF-8 entry"))?;
        if selected_names.contains(name.as_str()) {
            continue;
        }
        // Incomplete stages and retired versions carry generated names that can never
        // equal a SemVer version directory, so they are never selectable.
        let diagnostic = super::is_generated_leaf(&name, "incomplete")
            || super::is_generated_leaf(&name, "retired");
        if !diagnostic {
            return Err(error(
                "activation versions",
                format!("unreferenced version entry {name:?} requires recovery"),
            ));
        }
        let metadata = entry
            .metadata()
            .map_err(|cause| error("activation diagnostic metadata", cause))?;
        ensure_directory(&metadata).map_err(|cause| error("activation diagnostic kind", cause))?;
    }
    Ok(())
}

struct BaselineCompletion {
    version: Dir,
    tree: Dir,
    content: File,
    marker: File,
    record: records::CompleteRecord,
}

mod initial_metadata;
use initial_metadata::read_initial_metadata;

pub(super) fn validate_initial_seed(roots: &Roots) -> Result<VersionPins, UpdateError> {
    let metadata = read_initial_metadata(roots)?;
    if !metadata.lock_present {
        return Err(error(
            "initial lock",
            "initializer bootstrap marker is missing",
        ));
    }
    // The initializer's postcommit proof is stricter than later read-only metadata
    // admission: its fresh transaction has not produced diagnostic sibling stages.
    exact_entries(
        &roots.versions,
        &[&roots.trust.installation.baseline.version],
    )
    .map_err(|cause| error("initial versions", cause))?;
    let mut version = validate_version_contents(roots, metadata.completion)?;
    version.files.extend(metadata.records);
    Ok(version)
}

pub(super) fn validate_initial_version(roots: &Roots) -> Result<VersionPins, UpdateError> {
    validate_version_contents(roots, read_baseline_completion(roots)?)
}

mod version_contents;
use version_contents::validate_version_contents;

fn read_baseline_completion(roots: &Roots) -> Result<BaselineCompletion, UpdateError> {
    read_version_completion(roots, &roots.trust.installation.baseline)
}

fn read_version_completion(
    roots: &Roots,
    expected: &crate::ArtifactIdentity,
) -> Result<BaselineCompletion, UpdateError> {
    let version = open_directory(&roots.versions, &expected.version, roots.profile())?;
    exact_entries(&version, &["content.tar", "tree", ".complete"])
        .map_err(|cause| error("version contents", cause))?;
    let (complete, bytes) = read_record(&version, ".complete", roots.profile())?;
    let complete_record = records::decode_complete(&bytes)?;
    if complete_record.artifact != *expected {
        return Err(error(
            "completion marker",
            "artifact differs from the expected selected record",
        ));
    }
    let content = open_machine_file(&version, "content.tar", roots.profile())
        .map_err(|cause| error("retained archive", cause))?;
    if content
        .metadata()
        .map_err(|cause| error("archive metadata", cause))?
        .len()
        != complete_record.content_size
    {
        return Err(error(
            "completion size",
            "marker size differs from retained archive length",
        ));
    }
    let tree = open_directory(&version, "tree", roots.profile())?;
    Ok(BaselineCompletion {
        version,
        tree,
        content,
        marker: complete,
        record: complete_record,
    })
}

fn open_directory(
    parent: &Dir,
    leaf: &str,
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> Result<Dir, UpdateError> {
    let directory = parent
        .open_dir_nofollow(leaf)
        .map_err(|cause| error("protected directory open", cause))?;
    ensure_directory(
        &directory
            .dir_metadata()
            .map_err(|cause| error("directory metadata", cause))?,
    )
    .map_err(|cause| error("directory kind", cause))?;
    if directory
        .dir_metadata()
        .map_err(|cause| error("directory volume", cause))?
        .dev()
        != parent
            .dir_metadata()
            .map_err(|cause| error("parent volume", cause))?
            .dev()
    {
        return Err(error("directory volume", "directory crosses parent volume"));
    }
    keld_guard::validate_windows_install_directory(
        &directory
            .try_clone()
            .map_err(|cause| error("directory handle", cause))?
            .into_std_file(),
        profile,
    )
    .map_err(|cause| error("directory protection", cause))?;
    Ok(directory)
}

#[cfg(test)]
mod writer_state_tests {
    use super::{
        validate_recovery_known_good_history, validate_selected_artifact,
        validate_writer_pointer_context,
    };

    #[test]
    fn previous_known_good_is_required_after_first_successful_update() {
        let baseline = crate::tests::expected_identity().baseline;
        let baseline_floor = semver::Version::parse(&baseline.version).expect("baseline version");
        validate_selected_artifact(&baseline, &baseline, &baseline_floor)
            .expect("exact baseline identity is the initial positive control");
        validate_writer_pointer_context(&baseline, &baseline, &baseline, None, &baseline_floor)
            .expect("initial baseline has no earlier known-good release");

        let mut updated = baseline.clone();
        updated.version = "2.0.0".to_owned();
        updated.content_blake3 = [0x82; 32];
        let updated_floor = semver::Version::parse(&updated.version).expect("updated version");
        assert!(
            validate_writer_pointer_context(&baseline, &updated, &updated, None, &updated_floor,)
                .is_err()
        );
        validate_writer_pointer_context(
            &baseline,
            &updated,
            &updated,
            Some(&baseline),
            &updated_floor,
        )
        .expect("successful update retains the exact prior known-good artifact");

        let mut below_baseline = baseline.clone();
        below_baseline.version = "0.5.0".to_owned();
        assert!(
            validate_writer_pointer_context(
                &baseline,
                &baseline,
                &baseline,
                Some(&below_baseline),
                &baseline_floor,
            )
            .is_err()
        );

        let mut same_precedence_substitution = baseline.clone();
        same_precedence_substitution.version = "1.0.0+substituted".to_owned();
        assert!(
            validate_selected_artifact(&baseline, &same_precedence_substitution, &baseline_floor,)
                .is_err(),
            "equal precedence cannot substitute the exact installed baseline identity"
        );
        assert!(
            validate_writer_pointer_context(
                &baseline,
                &same_precedence_substitution,
                &same_precedence_substitution,
                None,
                &baseline_floor,
            )
            .is_err()
        );
    }

    #[test]
    fn recovery_requires_historical_previous_slot_after_baseline_is_superseded() {
        let baseline = crate::tests::expected_identity().baseline;
        validate_recovery_known_good_history(&baseline, &baseline, None)
            .expect("initial baseline has no earlier known-good release");

        let mut updated = baseline.clone();
        updated.version = "2.0.0".to_owned();
        updated.content_blake3 = [0x82; 32];
        assert!(validate_recovery_known_good_history(&baseline, &updated, None).is_err());
        validate_recovery_known_good_history(&baseline, &updated, Some(&baseline))
            .expect("superseded last-known-good retains its previous slot");
    }
}
