//! Common journaled activation transaction over the exclusive Windows writer lease.
//!
//! One [`Transaction`] owns the live attempt and crash recovery alike: every durable
//! action is chosen by [`next_activation_step`], so an interrupted attempt resumes only
//! the step an uninterrupted owner would have taken. Install mode changes how the
//! share-zero lease was acquired, never this state machine.

use std::collections::BTreeMap;

use cap_std::fs::File;

use super::{Roots, VersionPins, WindowsActivationWriteSnapshot, WindowsRecoveryInspection};
use crate::activation::{ActivationStep, ProtectedSlots, next_activation_step, retirement_due};
use crate::records::{
    self, ActivationFailureClass, ActivationJournal, ActivationPhase, PointerKind,
};
use crate::{ActivationEffect, ArtifactIdentity, UpdateError};

const JOURNAL: &str = "activation-journal";
const FLOOR: &str = "version-floor";
const CURRENT: &str = "current";
const LAST_KNOWN_GOOD: &str = "last-known-good";
const PREVIOUS_KNOWN_GOOD: &str = "previous-known-good";
/// The longest uninterrupted run is four steps; a larger count is a state-machine fault.
const MAX_STEPS_PER_RUN: usize = 8;

/// Attempt-bound health result delivered over the attempt's private health channel.
///
/// keld-update verifies only that the receipt names this exact attempt, health channel
/// and candidate. The health owner constructs it only after that candidate booted from
/// the attempt's version, reached application `Ready` and stayed alive for the contract
/// window without an unexpected generation exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationHealthReceipt {
    attempt_id: [u8; 32],
    health_channel_id: [u8; 32],
    candidate: ArtifactIdentity,
}

impl ActivationHealthReceipt {
    /// Binds a health result to one attempt, its private channel and its candidate.
    #[must_use]
    pub const fn new(
        attempt_id: [u8; 32],
        health_channel_id: [u8; 32],
        candidate: ArtifactIdentity,
    ) -> Self {
        Self {
            attempt_id,
            health_channel_id,
            candidate,
        }
    }
}

/// Exact attempt binding of an authenticated process-family retirement observation.
///
/// This value carries no evidence by itself. The composing host constructs it only from
/// an authenticated zero result for exactly these identifiers: a successor's QF1
/// lifecycle retirement witness that still reports zero active processes, or the live
/// coordinator's own retained attempt Job observed at zero. Job names, PIDs, closed
/// pipes and absence observations are never evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessFamilyRetirement {
    installation: [u8; 32],
    attempt: [u8; 32],
    lifecycle_channel: [u8; 32],
}

impl ProcessFamilyRetirement {
    /// Records an authenticated zero observation of one exact attempt's process family.
    #[must_use]
    pub const fn from_exact_zero_observation(
        installation_id: [u8; 32],
        attempt_id: [u8; 32],
        lifecycle_channel_id: [u8; 32],
    ) -> Self {
        Self {
            installation: installation_id,
            attempt: attempt_id,
            lifecycle_channel: lifecycle_channel_id,
        }
    }
}

/// How a resolved activation attempt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsActivationOutcome {
    /// Exact health was durably accepted; the candidate is current and last-known-good.
    Committed,
    /// The attempt failed; `current` is the journaled rollback target and the floor stays.
    RolledBack,
}

/// Resolved attempt: the journal is removed and the writer lease is released.
#[derive(Debug)]
pub struct WindowsActivationResolution {
    outcome: WindowsActivationOutcome,
    current: ArtifactIdentity,
    cleanup: Option<UpdateError>,
}

impl WindowsActivationResolution {
    /// Committed or rolled back.
    #[must_use]
    pub const fn outcome(&self) -> WindowsActivationOutcome {
        self.outcome
    }

    /// Artifact selected by `current` after resolution.
    #[must_use]
    pub const fn current(&self) -> &ArtifactIdentity {
        &self.current
    }

    /// First failure while deleting never-read leftovers, if any.
    ///
    /// Leftovers are the renamed journal (`pending-*`) and retired version trees
    /// (`retired-*`). Resolution does not depend on deleting them: later census admits
    /// both as never-selectable diagnostics, the next transaction removes stale
    /// `pending-*` siblings before its first step, and a later resolution retries the
    /// retired-tree deletion.
    #[must_use]
    pub const fn cleanup_error(&self) -> Option<&UpdateError> {
        self.cleanup.as_ref()
    }
}

/// Result of journal-bound recovery.
#[derive(Debug)]
pub enum WindowsRecoveryOutcome {
    /// The attempt finished commit or rollback.
    Resolved(WindowsActivationResolution),
    /// A publish-pending attempt resumed to `AwaitingHealth`; the recovering owner now
    /// holds the writer lease and must launch the candidate and resolve its health.
    AwaitingHealth(Box<WindowsActivationAttempt>),
}

/// Live attempt in `AwaitingHealth` that retains the share-zero writer lease.
///
/// Dropping it without resolution leaves the protected journal in `AwaitingHealth`;
/// later recovery rolls that attempt back once the process family is proven retired.
#[derive(Debug)]
pub struct WindowsActivationAttempt {
    transaction: Transaction,
    installation_id: [u8; 32],
}

impl WindowsActivationAttempt {
    /// Fresh single-use attempt identity recorded in the protected journal.
    #[must_use]
    pub const fn attempt_id(&self) -> &[u8; 32] {
        &self.transaction.journal.attempt_id
    }

    /// Identity of this attempt's private health channel.
    #[must_use]
    pub const fn health_channel_id(&self) -> &[u8; 32] {
        &self.transaction.journal.health_channel_id
    }

    /// One-shot lifecycle-keeper channel identity, distinct from attempt and health IDs.
    #[must_use]
    pub const fn lifecycle_channel_id(&self) -> &[u8; 32] {
        &self.transaction.journal.lifecycle_channel_id
    }

    /// Lifecycle installation ID derived from this installation's protected provenance.
    #[must_use]
    pub const fn lifecycle_installation_id(&self) -> &[u8; 32] {
        &self.installation_id
    }

    /// Exact candidate selected by `current` while health is pending.
    #[must_use]
    pub const fn candidate(&self) -> &ArtifactIdentity {
        &self.transaction.journal.candidate
    }

    /// Creates a non-writable, non-inheritable reference to this attempt's exact
    /// share-zero activation lease for the bounded lifecycle keeper.
    ///
    /// # Errors
    /// Returns an error if Windows cannot create and read back the reduced duplicate.
    pub fn duplicate_lifecycle_lease_retention(
        &self,
    ) -> Result<std::os::windows::io::OwnedHandle, UpdateError> {
        super::duplicate_lifecycle_lease_retention(&self.transaction.lease)
    }

    /// Durably accepts exact health, then commits the candidate as last-known-good.
    ///
    /// The prior last-known-good moves to `previous-known-good`, the superseded older
    /// version is retired and the journal is removed, in that order.
    ///
    /// # Errors
    /// A receipt for another attempt, channel or candidate refuses with no protected
    /// write; the journal stays `AwaitingHealth` for rollback by later recovery. A write
    /// failure after acceptance preserves the journal for journal-bound recovery.
    pub fn accept_health(
        mut self,
        receipt: &ActivationHealthReceipt,
    ) -> Result<WindowsActivationResolution, UpdateError> {
        let journal = &self.transaction.journal;
        if receipt.attempt_id != journal.attempt_id
            || receipt.health_channel_id != journal.health_channel_id
            || receipt.candidate != journal.candidate
        {
            return Err(unchanged(
                "health receipt binding",
                "receipt does not name this exact attempt, health channel and candidate",
            ));
        }
        let health_receipt_digest = records::activation_health_receipt_digest(
            &receipt.attempt_id,
            &receipt.health_channel_id,
            &receipt.candidate,
        )?;
        self.transaction.write_phase(
            ActivationPhase::HealthAccepted {
                health_receipt_digest,
            },
            "health-accepted",
        )?;
        self.transaction.run_to_resolution()
    }

    /// Durably records the failure and restores the journaled rollback target.
    ///
    /// The failed candidate is retired before the journal is removed; the trust floor
    /// stays at the candidate, so automatic reselection needs a newer signed release.
    ///
    /// # Errors
    /// A retirement binding for another installation, attempt or lifecycle channel
    /// refuses with no protected write. Later failures preserve the journal.
    pub fn roll_back(
        mut self,
        failure: ActivationFailureClass,
        retirement: &ProcessFamilyRetirement,
    ) -> Result<WindowsActivationResolution, UpdateError> {
        require_retirement_binding(retirement, &self.installation_id, &self.transaction.journal)?;
        self.transaction.write_phase(
            ActivationPhase::RollbackPending { failure },
            "rollback-pending",
        )?;
        self.transaction.run_to_resolution()
    }
}

impl WindowsActivationWriteSnapshot {
    /// Journals and selects one complete version published under this exact lease.
    pub(crate) fn begin_activation(
        self,
        candidate: &ArtifactIdentity,
        coordinator_image_blake3: [u8; 32],
    ) -> Result<WindowsActivationAttempt, UpdateError> {
        if coordinator_image_blake3 == [0; 32] {
            return Err(unchanged(
                "coordinator identity",
                "coordinator image digest is required",
            ));
        }
        let installation_id = self.roots.trust.lifecycle_installation_id()?;
        super::load::validate_artifact_scope_and_baseline(
            &self.roots.trust.installation.baseline,
            candidate,
        )?;
        let Self {
            roots,
            lease,
            admitted: _,
            version_floor,
            current,
            last_known_good,
            previous_known_good,
            version_pins: mut pins,
        } = self;
        remove_stale_record_preparations(&roots)?;
        // The snapshot already verified and pinned every referenced version under this
        // lease. Verify and pin the candidate; the census refuses any other complete
        // version before a journal can reference it.
        if pins.contains_key(&candidate.version) {
            return Err(unchanged(
                "candidate version",
                "candidate collides with a referenced known-good version",
            ));
        }
        let referenced: Vec<ArtifactIdentity> = [&current, &last_known_good]
            .into_iter()
            .chain(previous_known_good.iter())
            .cloned()
            .collect();
        let candidate_pins = super::load::pin_published_version(&roots, &referenced, candidate)?;
        pins.insert(candidate.version.clone(), candidate_pins);
        let [attempt_id, health_channel_id, lifecycle_channel_id] = mint_attempt_identities()?;
        let journal = ActivationJournal {
            attempt_id,
            candidate: candidate.clone(),
            rollback_target: current.clone(),
            prior_floor: version_floor.clone(),
            prior_last_known_good: last_known_good.clone(),
            prior_previous_known_good: previous_known_good.clone(),
            helper_image_blake3: coordinator_image_blake3,
            health_channel_id,
            lifecycle_channel_id,
            phase: ActivationPhase::PublishPending,
        };
        let bytes = records::encode_activation_journal(&journal)?;
        let mut transaction = Transaction {
            roots,
            lease,
            journal,
            version_floor,
            current,
            last_known_good,
            previous_known_good,
            pins,
            mutated: false,
        };
        transaction.write_record(JOURNAL, &bytes, Slot::Absent, "publish-pending")?;
        match transaction.advance()? {
            Progress::AwaitingHealth => Ok(WindowsActivationAttempt {
                transaction,
                installation_id,
            }),
            Progress::Resolved(_) => {
                Err(transaction.fault("activation start", "a fresh attempt resolved before health"))
            }
        }
    }
}

impl WindowsRecoveryInspection {
    /// Continues the inspected journal through the common transaction.
    ///
    /// A publish-pending attempt resumes to `AwaitingHealth` and is returned live; an
    /// `AwaitingHealth` attempt is rolled back because its owner was lost before health;
    /// `HealthAccepted` and `RollbackPending` finish their recorded resolution.
    ///
    /// # Errors
    /// A retirement binding or coordinator digest that differs from the protected
    /// journal refuses with no protected write. Later failures preserve the journal.
    pub fn recover(
        self,
        retirement: &ProcessFamilyRetirement,
        coordinator_image_blake3: [u8; 32],
    ) -> Result<WindowsRecoveryOutcome, UpdateError> {
        require_retirement_binding(retirement, &self.lifecycle_installation_id, &self.journal)?;
        if coordinator_image_blake3 != self.journal.helper_image_blake3 {
            return Err(unchanged(
                "coordinator identity",
                "recovering coordinator image differs from the journaled coordinator",
            ));
        }
        let Self {
            roots,
            lease,
            admitted: _,
            lifecycle_installation_id,
            journal,
            version_floor,
            current,
            last_known_good,
            previous_known_good,
            version_pins,
        } = self;
        remove_stale_record_preparations(&roots)?;
        let mut transaction = Transaction {
            roots,
            lease,
            journal,
            version_floor,
            current,
            last_known_good,
            previous_known_good,
            pins: version_pins,
            mutated: false,
        };
        if transaction.journal.phase == ActivationPhase::AwaitingHealth {
            transaction.write_phase(
                ActivationPhase::RollbackPending {
                    failure: ActivationFailureClass::ProcessCrash,
                },
                "rollback-pending",
            )?;
        }
        match transaction.advance()? {
            Progress::AwaitingHealth => Ok(WindowsRecoveryOutcome::AwaitingHealth(Box::new(
                WindowsActivationAttempt {
                    transaction,
                    installation_id: lifecycle_installation_id,
                },
            ))),
            Progress::Resolved(resolution) => Ok(WindowsRecoveryOutcome::Resolved(resolution)),
        }
    }
}

/// Process-wide crash-cut hook for isolated subprocess tests: `(durable, label)`.
///
/// `durable == false` reports a flushed, read-back sibling that is not yet published.
#[cfg(test)]
pub(crate) static CRASH_CUT_HOOK: std::sync::OnceLock<fn(bool, &'static str)> =
    std::sync::OnceLock::new();

#[cfg(test)]
fn observe(durable: bool, label: &'static str) {
    if let Some(hook) = CRASH_CUT_HOOK.get() {
        hook(durable, label);
    }
}

#[cfg(not(test))]
const fn observe(_: bool, _: &'static str) {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Absent,
    Present,
}

enum Progress {
    AwaitingHealth,
    Resolved(WindowsActivationResolution),
}

/// The one writer of journal, floor, pointers and version retirement.
#[derive(Debug)]
struct Transaction {
    roots: Roots,
    lease: File,
    journal: ActivationJournal,
    version_floor: String,
    current: ArtifactIdentity,
    last_known_good: ArtifactIdentity,
    previous_known_good: Option<ArtifactIdentity>,
    pins: BTreeMap<String, VersionPins>,
    /// Whether this call may have changed protected state; selects the error effect.
    mutated: bool,
}

impl Transaction {
    fn slots(&self) -> ProtectedSlots<'_> {
        ProtectedSlots {
            version_floor: &self.version_floor,
            current: &self.current,
            last_known_good: &self.last_known_good,
            previous_known_good: self.previous_known_good.as_ref(),
        }
    }

    fn run_to_resolution(mut self) -> Result<WindowsActivationResolution, UpdateError> {
        match self.advance()? {
            Progress::Resolved(resolution) => Ok(resolution),
            Progress::AwaitingHealth => Err(self.fault(
                "activation resolution",
                "a resolving journal phase returned to the health decision",
            )),
        }
    }

    /// Applies durable steps until the health decision or journal removal.
    fn advance(&mut self) -> Result<Progress, UpdateError> {
        for _ in 0..MAX_STEPS_PER_RUN {
            let retiree = retirement_due(&self.journal, &self.slots()).cloned();
            let retiree_present = match &retiree {
                Some(retiree) => self.version_present(&retiree.version)?,
                None => false,
            };
            let step = next_activation_step(&self.journal, &self.slots(), retiree_present)
                .map_err(|refusal| {
                    self.fault(
                        "activation state",
                        format!("journal and protected slots disagree: {refusal:?}"),
                    )
                })?;
            match step {
                ActivationStep::AwaitHealth => return Ok(Progress::AwaitingHealth),
                ActivationStep::RemoveJournal => {
                    return self.remove_journal().map(Progress::Resolved);
                }
                ActivationStep::AdvanceFloor => {
                    let floor = self.journal.candidate.version.clone();
                    let bytes = records::encode_floor(&floor)?;
                    self.write_record(FLOOR, &bytes, Slot::Present, "floor-advanced")?;
                    self.version_floor = floor;
                }
                ActivationStep::SelectCandidate => {
                    let candidate = self.journal.candidate.clone();
                    self.write_pointer(PointerKind::Current, &candidate, "candidate-selected")?;
                    self.current = candidate;
                }
                ActivationStep::EnterAwaitingHealth => {
                    self.write_phase(ActivationPhase::AwaitingHealth, "awaiting-health")?;
                }
                ActivationStep::PreservePriorKnownGood => {
                    let prior = self.journal.prior_last_known_good.clone();
                    self.write_pointer(
                        PointerKind::PreviousKnownGood,
                        &prior,
                        "prior-known-good-preserved",
                    )?;
                    self.previous_known_good = Some(prior);
                }
                ActivationStep::CommitCandidate => {
                    let candidate = self.journal.candidate.clone();
                    self.write_pointer(
                        PointerKind::LastKnownGood,
                        &candidate,
                        "candidate-committed",
                    )?;
                    self.last_known_good = candidate;
                }
                ActivationStep::RestoreRollbackTarget => {
                    let target = self.journal.rollback_target.clone();
                    self.write_pointer(PointerKind::Current, &target, "rollback-target-restored")?;
                    self.current = target;
                }
                ActivationStep::RetireVersion => {
                    let retiree = retiree.ok_or_else(|| {
                        self.fault("version retirement", "no journal-authorized retiree")
                    })?;
                    self.retire(&retiree)?;
                }
            }
        }
        Err(self.fault(
            "activation state",
            "transaction exceeded its bounded step count",
        ))
    }

    fn write_phase(
        &mut self,
        phase: ActivationPhase,
        label: &'static str,
    ) -> Result<(), UpdateError> {
        let mut journal = self.journal.clone();
        journal.phase = phase;
        let bytes = records::encode_activation_journal(&journal)?;
        self.write_record(JOURNAL, &bytes, Slot::Present, label)?;
        self.journal = journal;
        Ok(())
    }

    fn write_pointer(
        &mut self,
        kind: PointerKind,
        artifact: &ArtifactIdentity,
        label: &'static str,
    ) -> Result<(), UpdateError> {
        let (leaf, slot) = match kind {
            PointerKind::Current => (CURRENT, Slot::Present),
            PointerKind::LastKnownGood => (LAST_KNOWN_GOOD, Slot::Present),
            PointerKind::PreviousKnownGood if self.previous_known_good.is_some() => {
                (PREVIOUS_KNOWN_GOOD, Slot::Present)
            }
            PointerKind::PreviousKnownGood => (PREVIOUS_KNOWN_GOOD, Slot::Absent),
        };
        let bytes = records::encode_pointer(kind, artifact)?;
        self.write_record(leaf, &bytes, slot, label)
    }

    /// Writes one protected sibling, flushes and reads it back, then publishes it at an
    /// absent slot or replaces the fixed slot, and finally rereads exact bytes and profile.
    fn write_record(
        &mut self,
        leaf: &'static str,
        bytes: &[u8],
        slot: Slot,
        label: &'static str,
    ) -> Result<(), UpdateError> {
        let profile = self.roots.profile();
        let temporary = super::prepare_record(&self.roots.update, bytes, profile)
            .map_err(|cause| self.fault("record preparation", cause))?;
        observe(false, label);
        let parent = self
            .roots
            .update
            .try_clone()
            .map_err(|cause| self.fault("record parent", cause))?
            .into_std_file();
        self.mutated = true;
        let published = match slot {
            Slot::Absent => crate::windows_fs::publish_new(&parent, &temporary, leaf),
            Slot::Present => crate::windows_fs::replace_record_slot(&parent, &temporary, leaf),
        };
        published.map_err(|cause| self.fault("record publication", cause))?;
        let (_, observed) = super::read_record(&self.roots.update, leaf, profile)
            .map_err(|cause| self.fault("record readback", cause))?;
        if observed != bytes {
            return Err(self.fault("record readback", "published bytes differ"));
        }
        observe(true, label);
        Ok(())
    }

    /// Atomically renames one journal-authorized unreferenced version to a generated,
    /// never-selectable `retired-*` name under the same parent.
    fn retire(&mut self, retiree: &ArtifactIdentity) -> Result<(), UpdateError> {
        if [&self.current, &self.last_known_good]
            .into_iter()
            .chain(self.previous_known_good.iter())
            .any(|artifact| artifact.version == retiree.version)
        {
            return Err(self.fault(
                "version retirement",
                "a pointer still references the retiring version",
            ));
        }
        // Release this owner's own handles first; any other open handle in the tree
        // makes the rename fail and leaves the journal for later recovery.
        drop(self.pins.remove(&retiree.version));
        let leaf = super::random_leaf_name("retired")
            .map_err(|cause| self.fault("retired identity", cause))?;
        let versions = self
            .roots
            .versions
            .try_clone()
            .map_err(|cause| self.fault("versions parent", cause))?
            .into_std_file();
        self.mutated = true;
        crate::windows_fs::publish_new(&versions, &retiree.version, &leaf)
            .map_err(|cause| self.fault("version retirement", cause))?;
        if self.version_present(&retiree.version)? {
            return Err(self.fault(
                "version retirement",
                "retired version name is still present after rename",
            ));
        }
        observe(true, "version-retired");
        Ok(())
    }

    fn remove_journal(&mut self) -> Result<WindowsActivationResolution, UpdateError> {
        let outcome = match self.journal.phase {
            ActivationPhase::HealthAccepted { .. } => WindowsActivationOutcome::Committed,
            ActivationPhase::RollbackPending { .. } => WindowsActivationOutcome::RolledBack,
            ActivationPhase::PublishPending | ActivationPhase::AwaitingHealth => {
                return Err(self.fault(
                    "journal removal",
                    "an unresolved phase cannot remove its journal",
                ));
            }
        };
        // Removal is a write-through rename to a generated never-read `pending-*` leaf,
        // so the fixed journal name is durably absent before resolution is reported.
        let removed = super::random_leaf_name("pending")
            .map_err(|cause| self.fault("journal removal", cause))?;
        let parent = self
            .roots
            .update
            .try_clone()
            .map_err(|cause| self.fault("journal removal", cause))?
            .into_std_file();
        self.mutated = true;
        crate::windows_fs::publish_new(&parent, JOURNAL, &removed)
            .map_err(|cause| self.fault("journal removal", cause))?;
        observe(true, "journal-removed");
        let cleanup = self
            .roots
            .update
            .remove_file(&removed)
            .map_err(|cause| cleanup_error(format!("{removed}: {cause}")))
            .and_then(|()| self.remove_retired_versions());
        Ok(WindowsActivationResolution {
            outcome,
            current: self.current.clone(),
            cleanup: cleanup.err(),
        })
    }

    /// Deletes generated `retired-*` trees left by this or an earlier resolution.
    fn remove_retired_versions(&self) -> Result<(), UpdateError> {
        let entries = self
            .roots
            .versions
            .entries()
            .map_err(|cause| cleanup_error(cause.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|cause| cleanup_error(cause.to_string()))?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if super::is_generated_leaf(&name, "retired") {
                self.roots
                    .versions
                    .remove_dir_all(&name)
                    .map_err(|cause| cleanup_error(format!("{name}: {cause}")))?;
            }
        }
        Ok(())
    }

    fn version_present(&self, version: &str) -> Result<bool, UpdateError> {
        match self.roots.versions.symlink_metadata(version) {
            Ok(metadata) => {
                crate::windows_extraction::ensure_directory(&metadata)
                    .map_err(|cause| self.fault("version presence", cause))?;
                Ok(true)
            }
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(cause) => Err(self.fault("version presence", cause)),
        }
    }

    fn fault(&self, step: &'static str, detail: impl std::fmt::Display) -> UpdateError {
        UpdateError::Activation {
            step,
            effect: if self.mutated {
                ActivationEffect::JournalBoundRecoveryRequired
            } else {
                ActivationEffect::ProtectedStateUnchanged
            },
            detail: detail.to_string(),
        }
    }
}

/// Removes flushed record siblings left by a crash before their publication rename.
///
/// Such `pending-*` files are created only by this writer under the held lease and are
/// never read as records, so deleting them cannot change protected state.
fn remove_stale_record_preparations(roots: &Roots) -> Result<(), UpdateError> {
    let entries = roots
        .update
        .entries()
        .map_err(|cause| unchanged("stale record census", cause))?;
    for entry in entries {
        let entry = entry.map_err(|cause| unchanged("stale record census", cause))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if super::is_generated_leaf(&name, "pending") {
            // Admit only this writer's own artifact: a regular, single-link file with
            // the installation's exact protection profile. Close it before deletion.
            drop(
                super::open_machine_file(&roots.update, &name, roots.profile())
                    .map_err(|cause| unchanged("stale record admission", cause))?,
            );
            roots
                .update
                .remove_file(&name)
                .map_err(|cause| unchanged("stale record removal", cause))?;
        }
    }
    Ok(())
}

fn require_retirement_binding(
    retirement: &ProcessFamilyRetirement,
    installation_id: &[u8; 32],
    journal: &ActivationJournal,
) -> Result<(), UpdateError> {
    if &retirement.installation != installation_id
        || retirement.attempt != journal.attempt_id
        || retirement.lifecycle_channel != journal.lifecycle_channel_id
    {
        return Err(unchanged(
            "process-family retirement binding",
            "retirement evidence does not name this installation, attempt and lifecycle channel",
        ));
    }
    Ok(())
}

/// Mints the attempt, health-channel and lifecycle-channel identities, in that order.
fn mint_attempt_identities() -> Result<[[u8; 32]; 3], UpdateError> {
    let mut identities = [[0_u8; 32]; 3];
    for identity in &mut identities {
        getrandom::fill(identity).map_err(|cause| unchanged("attempt identity", cause))?;
    }
    let [attempt, health, lifecycle] = identities;
    if attempt == health || attempt == lifecycle || health == lifecycle || attempt == [0; 32] {
        return Err(unchanged(
            "attempt identity",
            "minted attempt, health and lifecycle identities are not distinct",
        ));
    }
    Ok(identities)
}

fn unchanged(step: &'static str, detail: impl std::fmt::Display) -> UpdateError {
    UpdateError::Activation {
        step,
        effect: ActivationEffect::ProtectedStateUnchanged,
        detail: detail.to_string(),
    }
}

fn cleanup_error(detail: String) -> UpdateError {
    UpdateError::Activation {
        step: "resolved leftover cleanup",
        effect: ActivationEffect::ProtectedStateUnchanged,
        detail,
    }
}
