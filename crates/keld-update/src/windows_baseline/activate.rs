//! Common journaled activation transaction over the exclusive Windows writer lease.
//!
//! One [`Transaction`] owns the live attempt and crash recovery alike. Every floor,
//! pointer, retirement and journal-removal step is chosen by [`next_activation_step`],
//! so an interrupted attempt resumes only the step an uninterrupted owner would have
//! taken. The only writes made outside it record an external fact: the attempt start,
//! accepted health, a failure, or fresh channel identities for a resumed owner. Install
//! mode changes how the share-zero lease was acquired, never this state machine.
//!
//! Minting and the first record that reveals the minted identities are separate calls
//! (the mint-then-journal seam): [`WindowsMintedAttempt`] carries identities that no
//! record names yet, so the attempt owner can create and hold its connect-back endpoint
//! before [`WindowsMintedAttempt::journal`] durably records them with its owner facts.

use std::collections::{BTreeMap, BTreeSet};

use cap_fs_ext::DirExt as _;
use cap_std::fs::File;

use super::{
    RecordTarget, Roots, VersionPins, WindowsActivationWriteSnapshot, WindowsRecoveryInspection,
};
use crate::activation::{
    ActivationStep, CandidateLocation, ProtectedSlots, next_activation_step, retirement_due,
};
use crate::records::{
    self, ActivationFailureClass, ActivationJournal, ActivationPhase, AttemptOwner,
    AttemptOwnership, InitiatingLogon, PointerKind,
};
use crate::windows_fs::RecordSlot;
use crate::{ActivationEffect, ArtifactIdentity, UpdateError};

const JOURNAL: &str = "activation-journal";
/// The longest uninterrupted run is five steps; a larger count is a state-machine fault.
const MAX_STEPS_PER_RUN: usize = 8;

/// Attempt-bound health result delivered over the attempt's private health channel.
///
/// keld-update verifies only that the receipt names this exact attempt, health channel
/// and candidate; it cannot observe health itself. The host health owner constructs it
/// only after that candidate booted from the attempt's version over the attempt's
/// private channel, reached application `Ready` and stayed alive for the contract
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
/// This value carries no evidence by itself and keld-update cannot authenticate who
/// built it; it only prevents one attempt's observation from being applied to another.
/// The composing host constructs it only from an authenticated zero result for exactly
/// these identifiers: a successor's QF1 lifecycle retirement witness that still reports
/// zero active processes, or the live coordinator's own retained attempt Job observed
/// at zero. Job names, PIDs, closed pipes and absence observations are never evidence.
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
    /// The attempt never selected its candidate because no stage recording the exact
    /// candidate and no published candidate version was present. Floor, pointers and
    /// known-good slots are unchanged. An attempt whose every copy fails verification is
    /// also abandoned, but is reported as an [`UpdateError::Activation`] refusal at step
    /// `candidate verification`.
    Abandoned,
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
    /// retired-tree deletion. The error's effect is
    /// [`ActivationEffect::ResolvedWithLeftovers`].
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
    /// A publish-pending attempt received freshly minted health and lifecycle channel
    /// identities and nothing was written; the recovering owner holds the writer lease and
    /// journals them with [`WindowsMintedAttempt::journal`] once its endpoint is held.
    Reminted(Box<WindowsMintedAttempt>),
}

/// Result of journaling minted identities and continuing the common transaction.
#[derive(Debug)]
pub enum WindowsJournaledAttempt {
    /// The candidate is published, verified, selected and journaled `AwaitingHealth`; the
    /// owner now launches it and resolves its health.
    AwaitingHealth(Box<WindowsActivationAttempt>),
    /// A resumed attempt resolved without a launch: its candidate was neither staged nor
    /// published, so it was abandoned. A fresh attempt never resolves here.
    Resolved(WindowsActivationResolution),
}

/// Attempt identities minted under the exclusive writer lease that no record names yet.
///
/// This is the mint-then-journal seam of KEL-53 §4 "Candidate connect-back". The attempt
/// owner derives its connect-back endpoint from [`Self::lifecycle_installation_id`],
/// [`Self::attempt_id`] and [`Self::health_channel_id`], creates and holds it and reads
/// its descriptor back, and only then calls [`Self::journal`], which writes the first
/// record that reveals them. Minting wrote nothing. Dropping this value, or
/// [`Self::refuse`], releases the lease: a fresh attempt leaves only its completed stage
/// and a resumed attempt leaves its journal unchanged.
#[derive(Debug)]
pub struct WindowsMintedAttempt {
    transaction: Transaction,
    installation_id: [u8; 32],
    /// Fresh channels of a resumed attempt. A fresh attempt has none: its unwritten
    /// journal already holds every identity it minted.
    reminted: Option<RemintedChannels>,
}

#[derive(Debug, Clone, Copy)]
struct RemintedChannels {
    health_channel_id: [u8; 32],
    lifecycle_channel_id: [u8; 32],
}

impl WindowsMintedAttempt {
    /// Attempt identity: freshly minted, or the journaled one that a resumed owner keeps.
    #[must_use]
    pub const fn attempt_id(&self) -> &[u8; 32] {
        &self.transaction.journal.attempt_id
    }

    /// Freshly minted identity of this owner's private health channel.
    #[must_use]
    pub const fn health_channel_id(&self) -> &[u8; 32] {
        match &self.reminted {
            Some(channels) => &channels.health_channel_id,
            None => &self.transaction.journal.health_channel_id,
        }
    }

    /// Lifecycle installation ID that this transaction validated for its binding checks.
    #[must_use]
    pub const fn lifecycle_installation_id(&self) -> &[u8; 32] {
        &self.installation_id
    }

    /// Durably writes the first record that reveals these identities, with the owner
    /// facts, then continues the common transaction to the health decision.
    ///
    /// A fresh attempt writes its `PublishPending` journal. A resumed attempt replaces
    /// its journal with the re-mint record, which keeps the attempt identity and phase and
    /// records the fresh channels with this owner's facts. Both are
    /// `keld.activation-journal/v2`. The transaction then renames the stage, verifies and
    /// pins the candidate, advances the floor, selects `current` and journals
    /// `AwaitingHealth`, as an uninterrupted owner would.
    ///
    /// # Errors
    /// Owner facts that name no process or logon session (a zero process ID or creation
    /// time, a zero authentication ID, or a zero or negative logon time) refuse before
    /// any write: a fresh attempt at step `start` with
    /// [`ActivationEffect::ProtectedStateUnchanged`], a resumed attempt with its journal
    /// unchanged and [`ActivationEffect::JournalBoundRecoveryRequired`]. A failed durable
    /// step refuses with the effect its journal decides; a fresh attempt refused before
    /// its journal exists leaves only its stage. A copy that fails verification after its
    /// rename is retired under the journal and the next stage recording the exact
    /// candidate is tried. With none left the journal is removed, and the refusal names
    /// step `candidate verification` with [`ActivationEffect::ProtectedStateUnchanged`]
    /// (or [`ActivationEffect::ResolvedWithLeftovers`] if cleanup is incomplete).
    pub fn journal(
        self,
        attempt_owner: AttemptOwner,
        initiating_logon: InitiatingLogon,
    ) -> Result<WindowsJournaledAttempt, UpdateError> {
        let Self {
            mut transaction,
            installation_id,
            reminted,
        } = self;
        let mut journal = transaction.journal.clone();
        journal.ownership = Some(AttemptOwnership {
            initiating_logon,
            attempt_owner,
        });
        if let Some(channels) = reminted {
            journal.health_channel_id = channels.health_channel_id;
            journal.lifecycle_channel_id = channels.lifecycle_channel_id;
        }
        // Encoding validates every journal fact, including the owner facts, before the
        // first protected write of this call.
        let bytes = records::encode_activation_journal(&journal)
            .map_err(|cause| transaction.before_record(&cause))?;
        remove_stale_record_preparations(&transaction.roots)
            .map_err(|cause| transaction.before_record(&cause))?;
        if reminted.is_some() {
            transaction.write_record(
                RecordTarget::Replace(RecordSlot::Journal),
                &bytes,
                "channels-reminted",
            )?;
            transaction.journal = journal;
        } else {
            transaction.journal = journal;
            if let Err(cause) =
                transaction.write_record(RecordTarget::Absent(JOURNAL), &bytes, "publish-pending")
            {
                if transaction.journal_present() {
                    transaction.journaled = true;
                    return Err(transaction.refault(&cause));
                }
                return Err(transaction.before_record(&cause));
            }
            transaction.journaled = true;
        }
        match transaction.advance()? {
            Progress::AwaitingHealth => Ok(WindowsJournaledAttempt::AwaitingHealth(Box::new(
                WindowsActivationAttempt {
                    transaction,
                    installation_id,
                },
            ))),
            Progress::Resolved(resolution) if reminted.is_some() => {
                Ok(WindowsJournaledAttempt::Resolved(resolution))
            }
            Progress::Resolved(_) => {
                Err(transaction.fault("start", "a fresh attempt resolved before health"))
            }
        }
    }

    /// Consumes the minted identities when the owner cannot create, hold or read back
    /// its endpoint, and returns the typed refusal for `step`.
    ///
    /// Nothing was written. A fresh attempt refuses with
    /// [`ActivationEffect::ProtectedStateUnchanged`] and leaves only its completed stage;
    /// a resumed attempt keeps its journal unchanged and refuses with
    /// [`ActivationEffect::JournalBoundRecoveryRequired`]. The writer lease is released.
    #[must_use]
    pub fn refuse(self, step: &'static str, detail: impl std::fmt::Display) -> UpdateError {
        self.transaction.fault(step, detail)
    }
}

/// Live attempt in `AwaitingHealth` that retains the share-zero writer lease.
///
/// Dropping it without resolution, or any refusal from its consuming methods, releases
/// the lease and leaves the protected journal authoritative; later recovery rolls an
/// `AwaitingHealth` attempt back once its process family is proven retired.
#[derive(Debug)]
pub struct WindowsActivationAttempt {
    transaction: Transaction,
    installation_id: [u8; 32],
}

impl WindowsActivationAttempt {
    /// Attempt identity recorded in the protected journal; a resumed owner keeps it and
    /// receives fresh channel identities instead.
    #[must_use]
    pub const fn attempt_id(&self) -> &[u8; 32] {
        &self.transaction.journal.attempt_id
    }

    /// Identity of this owner's private health channel.
    #[must_use]
    pub const fn health_channel_id(&self) -> &[u8; 32] {
        &self.transaction.journal.health_channel_id
    }

    /// One-shot lifecycle-keeper channel identity, distinct from attempt and health IDs.
    #[must_use]
    pub const fn lifecycle_channel_id(&self) -> &[u8; 32] {
        &self.transaction.journal.lifecycle_channel_id
    }

    /// Lifecycle installation ID that this transaction validated for its binding checks.
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

    /// Durably records the exact health receipt, then commits the candidate as
    /// last-known-good.
    ///
    /// The prior last-known-good moves to `previous-known-good`, a superseded older
    /// version (if any) is retired and the journal is removed, in that order.
    ///
    /// # Errors
    /// A receipt for another attempt, channel or candidate refuses before any write.
    /// Every refusal consumes the attempt and releases the lease; the journal stays
    /// authoritative ([`ActivationEffect::JournalBoundRecoveryRequired`]) and recovery
    /// rolls an unaccepted attempt back.
    pub fn accept_health(
        mut self,
        receipt: &ActivationHealthReceipt,
    ) -> Result<WindowsActivationResolution, UpdateError> {
        let journal = &self.transaction.journal;
        if receipt.attempt_id != journal.attempt_id
            || receipt.health_channel_id != journal.health_channel_id
            || receipt.candidate != journal.candidate
        {
            return Err(self.transaction.fault(
                "health receipt binding",
                "receipt does not name this exact attempt, health channel and candidate",
            ));
        }
        let health_receipt_digest = records::activation_health_receipt_digest(
            &receipt.attempt_id,
            &receipt.health_channel_id,
            &receipt.candidate,
        )
        .map_err(|cause| self.transaction.fault("health receipt digest", cause))?;
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
    /// refuses before any write. Every refusal consumes the attempt and releases the
    /// lease; the journal stays authoritative for journal-bound recovery.
    pub fn roll_back(
        mut self,
        failure: ActivationFailureClass,
        retirement: &ProcessFamilyRetirement,
    ) -> Result<WindowsActivationResolution, UpdateError> {
        self.transaction
            .require_retirement_binding(retirement, &self.installation_id)?;
        self.transaction.write_phase(
            ActivationPhase::RollbackPending { failure },
            "rollback-pending",
        )?;
        self.transaction.run_to_resolution()
    }
}

impl WindowsActivationWriteSnapshot {
    /// Mints the identities of an attempt for one stage completed under this exact lease,
    /// writing nothing; [`WindowsMintedAttempt::journal`] journals and continues it.
    ///
    /// Nothing is renamed before the `PublishPending` journal is durable, so a refusal or
    /// crash before it leaves only the stage, which every census tolerates.
    pub(crate) fn begin_activation(
        self,
        stage: String,
        candidate: &ArtifactIdentity,
        coordinator_image_blake3: [u8; 32],
    ) -> Result<WindowsMintedAttempt, UpdateError> {
        let Self {
            roots,
            lease,
            admitted: _,
            version_floor,
            current,
            last_known_good,
            previous_known_good,
            version_pins: pins,
        } = self;
        let preflight = (|| {
            if coordinator_image_blake3 == [0; 32] {
                return Err(UpdateError::activation(
                    "coordinator identity",
                    ActivationEffect::ProtectedStateUnchanged,
                    "coordinator image digest is required",
                ));
            }
            let installation_id = roots.trust.lifecycle_installation_id()?;
            super::load::validate_artifact_scope_and_baseline(
                &roots.trust.installation.baseline,
                candidate,
            )?;
            if pins.contains_key(&candidate.version) {
                return Err(UpdateError::activation(
                    "candidate version",
                    ActivationEffect::ProtectedStateUnchanged,
                    "candidate collides with a referenced known-good version",
                ));
            }
            // The stage's completion record must name exactly this candidate.
            if super::load::completed_stage_identity(&roots, &stage)?.as_ref() != Some(candidate) {
                return Err(UpdateError::activation(
                    "candidate stage",
                    ActivationEffect::ProtectedStateUnchanged,
                    "the completed stage records a different candidate",
                ));
            }
            let [attempt_id, health_channel_id, lifecycle_channel_id] = mint_identities()?;
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
                // The owner supplies its facts when it journals these identities.
                ownership: None,
                phase: ActivationPhase::PublishPending,
            };
            // The candidate must be strictly above the prior floor and the prior context
            // complete before any minted identity leaves this call.
            records::validate_activation_journal(&journal)?;
            Ok((installation_id, journal))
        })();
        let (installation_id, journal) = preflight.map_err(|cause| {
            UpdateError::activation(
                "start",
                ActivationEffect::ProtectedStateUnchanged,
                refusal_detail(&cause),
            )
        })?;
        Ok(WindowsMintedAttempt {
            transaction: Transaction {
                roots,
                lease,
                journal,
                version_floor,
                current,
                last_known_good,
                previous_known_good,
                pins,
                stage: Some(stage),
                journaled: false,
            },
            installation_id,
            reminted: None,
        })
    }
}

impl WindowsRecoveryInspection {
    /// Continues the inspected journal through the common transaction.
    ///
    /// A publish-pending attempt receives freshly minted health and lifecycle channel
    /// identities and nothing is written ([`WindowsRecoveryOutcome::Reminted`]); its
    /// owner journals them with [`WindowsMintedAttempt::journal`]. An `AwaitingHealth`
    /// attempt is rolled back because its owner was lost before health; `HealthAccepted`
    /// and `RollbackPending` finish their recorded resolution. A publish-pending journal
    /// also accepts [`Self::resume_unlaunched`], which needs no binding because nothing
    /// was launched; this method checks the binding for every phase.
    ///
    /// # Errors
    /// A retirement binding or coordinator digest that differs from the protected
    /// journal refuses before any write. The journal stays authoritative after every
    /// refusal ([`ActivationEffect::JournalBoundRecoveryRequired`]).
    pub fn recover(
        self,
        retirement: &ProcessFamilyRetirement,
        coordinator_image_blake3: [u8; 32],
    ) -> Result<WindowsRecoveryOutcome, UpdateError> {
        let (mut transaction, installation_id) = self.into_transaction(coordinator_image_blake3)?;
        transaction.require_retirement_binding(retirement, &installation_id)?;
        if transaction.journal.phase == ActivationPhase::PublishPending {
            return transaction
                .remint(installation_id)
                .map(|minted| WindowsRecoveryOutcome::Reminted(Box::new(minted)));
        }
        transaction.remove_stale_record_preparations()?;
        if transaction.journal.phase == ActivationPhase::AwaitingHealth {
            transaction.write_phase(
                ActivationPhase::RollbackPending {
                    failure: ActivationFailureClass::ProcessCrash,
                },
                "rollback-pending",
            )?;
        }
        transaction
            .run_to_resolution()
            .map(WindowsRecoveryOutcome::Resolved)
    }

    /// Resumes a `PublishPending` attempt under the exclusive writer lease alone.
    ///
    /// No candidate is launched before `AwaitingHealth` is durable, and every live
    /// transaction owner keeps the share-zero lease (or its keeper retains a duplicate),
    /// so acquiring this lease proves no prior owner can still write and no candidate
    /// family exists. The resumed owner receives freshly minted health and lifecycle
    /// channel identities, so no witness from an earlier owner can bind to it. This call
    /// writes nothing; [`WindowsMintedAttempt::journal`] durably re-mints them with the
    /// resumed owner's facts and continues. An attempt whose candidate is neither staged
    /// nor published then resolves as [`WindowsActivationOutcome::Abandoned`].
    ///
    /// # Errors
    /// Refuses every other phase: a launched attempt needs
    /// [`Self::recover`] with an exact process-family retirement binding. A coordinator
    /// digest that differs from the journal also refuses. Every refusal writes nothing
    /// and keeps the journal authoritative.
    pub fn resume_unlaunched(
        self,
        coordinator_image_blake3: [u8; 32],
    ) -> Result<WindowsMintedAttempt, UpdateError> {
        let (transaction, installation_id) = self.into_transaction(coordinator_image_blake3)?;
        if transaction.journal.phase != ActivationPhase::PublishPending {
            return Err(transaction.fault(
                "unlaunched resume",
                "a launched attempt requires an exact process-family retirement binding",
            ));
        }
        transaction.remint(installation_id)
    }

    fn into_transaction(
        self,
        coordinator_image_blake3: [u8; 32],
    ) -> Result<(Transaction, [u8; 32]), UpdateError> {
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
            candidate_stage,
        } = self;
        let transaction = Transaction {
            roots,
            lease,
            journal,
            version_floor,
            current,
            last_known_good,
            previous_known_good,
            pins: version_pins,
            stage: candidate_stage,
            journaled: true,
        };
        if coordinator_image_blake3 != transaction.journal.helper_image_blake3 {
            return Err(transaction.fault(
                "coordinator identity",
                "recovering coordinator image differs from the journaled coordinator",
            ));
        }
        Ok((transaction, lifecycle_installation_id))
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
    /// Completed `incomplete-*` stage of a candidate not yet renamed to its version.
    stage: Option<String>,
    /// Whether the protected journal exists; it decides every refusal's effect.
    journaled: bool,
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
                "resolution",
                "a resolving journal phase returned to the health decision",
            )),
        }
    }

    /// Mints fresh channel identities for a never-launched attempt, writing nothing.
    fn remint(self, installation_id: [u8; 32]) -> Result<WindowsMintedAttempt, UpdateError> {
        let [_, health_channel_id, lifecycle_channel_id] =
            mint_identities().map_err(|cause| self.refault(&cause))?;
        if health_channel_id == self.journal.attempt_id
            || lifecycle_channel_id == self.journal.attempt_id
        {
            return Err(self.fault(
                "attempt identity",
                "re-minted channels collide with the attempt identity",
            ));
        }
        Ok(WindowsMintedAttempt {
            transaction: self,
            installation_id,
            reminted: Some(RemintedChannels {
                health_channel_id,
                lifecycle_channel_id,
            }),
        })
    }

    /// Applies durable steps until the health decision or journal removal.
    fn advance(&mut self) -> Result<Progress, UpdateError> {
        for _ in 0..MAX_STEPS_PER_RUN {
            let retiree = retirement_due(&self.journal, &self.slots()).cloned();
            let retiree_present = match &retiree {
                Some(retiree) => self.version_present(&retiree.version)?,
                None => false,
            };
            let candidate = self.candidate_location()?;
            let step =
                next_activation_step(&self.journal, &self.slots(), candidate, retiree_present)
                    .map_err(|refusal| {
                        self.fault(
                            "state",
                            format!("journal and protected slots disagree: {refusal:?}"),
                        )
                    })?;
            match step {
                ActivationStep::AwaitHealth => return Ok(Progress::AwaitingHealth),
                ActivationStep::RemoveJournal => {
                    return self.remove_journal().map(Progress::Resolved);
                }
                ActivationStep::AbandonAttempt => {
                    return self
                        .remove_journal_as(WindowsActivationOutcome::Abandoned)
                        .map(Progress::Resolved);
                }
                ActivationStep::PublishCandidate => self.publish_candidate()?,
                ActivationStep::AdvanceFloor => {
                    // The floor never names a candidate that has not been fully verified.
                    self.ensure_candidate_pinned()?;
                    let floor = self.journal.candidate.version.clone();
                    let bytes =
                        records::encode_floor(&floor).map_err(|cause| self.refault(&cause))?;
                    self.write_record(
                        RecordTarget::Replace(RecordSlot::Floor),
                        &bytes,
                        "floor-advanced",
                    )?;
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
        Err(self.fault("state", "transaction exceeded its bounded step count"))
    }

    fn write_phase(
        &mut self,
        phase: ActivationPhase,
        label: &'static str,
    ) -> Result<(), UpdateError> {
        let mut journal = self.journal.clone();
        journal.phase = phase;
        self.write_journal(journal, label)
    }

    fn write_journal(
        &mut self,
        journal: ActivationJournal,
        label: &'static str,
    ) -> Result<(), UpdateError> {
        let bytes =
            records::encode_activation_journal(&journal).map_err(|cause| self.refault(&cause))?;
        self.write_record(RecordTarget::Replace(RecordSlot::Journal), &bytes, label)?;
        self.journal = journal;
        Ok(())
    }

    fn write_pointer(
        &mut self,
        kind: PointerKind,
        artifact: &ArtifactIdentity,
        label: &'static str,
    ) -> Result<(), UpdateError> {
        let target = match kind {
            PointerKind::Current => RecordTarget::Replace(RecordSlot::Current),
            PointerKind::LastKnownGood => RecordTarget::Replace(RecordSlot::LastKnownGood),
            PointerKind::PreviousKnownGood if self.previous_known_good.is_some() => {
                RecordTarget::Replace(RecordSlot::PreviousKnownGood)
            }
            PointerKind::PreviousKnownGood => {
                RecordTarget::Absent(RecordSlot::PreviousKnownGood.leaf())
            }
        };
        let bytes =
            records::encode_pointer(kind, artifact).map_err(|cause| self.refault(&cause))?;
        self.write_record(target, &bytes, label)
    }

    /// Prepares a flushed, read-back protected sibling, then publishes it through the
    /// shared record owner, which rereads exact bytes and profile.
    fn write_record(
        &mut self,
        target: RecordTarget<'_>,
        bytes: &[u8],
        label: &'static str,
    ) -> Result<(), UpdateError> {
        let profile = self.roots.profile();
        let temporary = super::prepare_record(&self.roots.update, bytes, profile)
            .map_err(|cause| self.refault(&cause))?;
        observe(false, label);
        super::publish_prepared_record(&self.roots.update, &temporary, target, bytes, profile)
            .map_err(|cause| self.refault(&cause))?;
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
        retire_version_directory(&self.roots, &retiree.version)
            .map_err(|cause| self.refault(&cause))?;
        observe(true, "version-retired");
        Ok(())
    }

    /// Where the journal's candidate is: published under its version name, still in its
    /// completed stage, or neither.
    fn candidate_location(&self) -> Result<CandidateLocation, UpdateError> {
        if self.version_present(&self.journal.candidate.version)? {
            Ok(CandidateLocation::Published)
        } else if self.stage.is_some() {
            Ok(CandidateLocation::Staged)
        } else {
            Ok(CandidateLocation::Absent)
        }
    }

    /// Renames the completed stage to the candidate's version name under the journal.
    fn publish_candidate(&mut self) -> Result<(), UpdateError> {
        let stage = self
            .stage
            .clone()
            .ok_or_else(|| self.fault("candidate publication", "no completed stage is known"))?;
        let versions = versions_for_rename(&self.roots).map_err(|cause| self.refault(&cause))?;
        observe(false, "candidate-published");
        crate::windows_fs::publish_new(&versions, &stage, &self.journal.candidate.version)
            .map_err(|cause| self.fault("candidate publication", cause))?;
        self.stage = None;
        observe(true, "candidate-published");
        Ok(())
    }

    /// Fully re-verifies and pins the published candidate.
    ///
    /// A census fault concerns other entries, so it keeps the journal and retires
    /// nothing. A copy whose own content fails verification is retired under the journal
    /// and the next stage recording the exact candidate is published and verified in its
    /// place; each pass renames one stage away, so the loop ends. With no copy left, the
    /// attempt is abandoned with nothing selected.
    fn ensure_candidate_pinned(&mut self) -> Result<(), UpdateError> {
        let candidate = self.journal.candidate.clone();
        if self.pins.contains_key(&candidate.version) {
            return Ok(());
        }
        let referenced: Vec<ArtifactIdentity> = [&self.current, &self.last_known_good]
            .into_iter()
            .chain(self.previous_known_good.iter())
            .cloned()
            .collect();
        loop {
            super::load::validate_publication_census(&self.roots, &referenced, &candidate)
                .map_err(|cause| self.refault(&cause))?;
            let cause = match super::load::verify_published_version(&self.roots, &candidate) {
                Ok(pins) => {
                    self.pins.insert(candidate.version.clone(), pins);
                    return Ok(());
                }
                Err(cause) => cause,
            };
            self.retire(&candidate).map_err(|retirement| {
                let (step, detail) = retirement.step_and_detail();
                self.fault(
                    step,
                    format!(
                        "{detail}; the candidate had failed verification: {}",
                        refusal_detail(&cause)
                    ),
                )
            })?;
            self.stage = super::load::find_candidate_stage(&self.roots, &candidate)
                .map_err(|stage_fault| self.refault(&stage_fault))?;
            if self.stage.is_some() {
                self.publish_candidate()?;
                continue;
            }
            let resolution = self.remove_journal_as(WindowsActivationOutcome::Abandoned)?;
            let effect = if resolution.cleanup.is_some() {
                ActivationEffect::ResolvedWithLeftovers
            } else {
                ActivationEffect::ProtectedStateUnchanged
            };
            return Err(UpdateError::activation(
                "candidate verification",
                effect,
                format!(
                    "the published candidate failed verification and was retired: {}",
                    refusal_detail(&cause)
                ),
            ));
        }
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
        self.remove_journal_as(outcome)
    }

    fn remove_journal_as(
        &mut self,
        outcome: WindowsActivationOutcome,
    ) -> Result<WindowsActivationResolution, UpdateError> {
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
        crate::windows_fs::publish_new(&parent, JOURNAL, &removed)
            .map_err(|cause| self.fault("journal removal", cause))?;
        self.journaled = false;
        observe(true, "journal-removed");
        let cleanup = crate::windows_fs::remove_file_relative(&parent, &removed)
            .map_err(|cause| leftover_error(format!("{removed}: {cause}")))
            .and_then(|()| remove_generated_leftovers(&self.roots));
        Ok(WindowsActivationResolution {
            outcome,
            current: self.current.clone(),
            cleanup: cleanup.err(),
        })
    }

    /// Removes stale record siblings only after every recovery identity check passed.
    fn remove_stale_record_preparations(&self) -> Result<(), UpdateError> {
        remove_stale_record_preparations(&self.roots).map_err(|cause| self.refault(&cause))
    }

    fn version_present(&self, version: &str) -> Result<bool, UpdateError> {
        version_present(&self.roots, version).map_err(|cause| self.refault(&cause))
    }

    fn journal_present(&self) -> bool {
        // Any answer other than a definite absence keeps the journal authoritative.
        !matches!(
            self.roots.update.symlink_metadata(JOURNAL),
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound
        )
    }

    fn require_retirement_binding(
        &self,
        retirement: &ProcessFamilyRetirement,
        installation_id: &[u8; 32],
    ) -> Result<(), UpdateError> {
        if &retirement.installation != installation_id
            || retirement.attempt != self.journal.attempt_id
            || retirement.lifecycle_channel != self.journal.lifecycle_channel_id
        {
            return Err(self.fault(
                "process-family retirement binding",
                "the retirement binding does not name this installation, attempt and lifecycle channel",
            ));
        }
        Ok(())
    }

    /// Refusal whose effect follows from whether the protected journal exists.
    fn fault(&self, step: &'static str, detail: impl std::fmt::Display) -> UpdateError {
        UpdateError::activation(
            step,
            if self.journaled {
                ActivationEffect::JournalBoundRecoveryRequired
            } else {
                ActivationEffect::ProtectedStateUnchanged
            },
            detail,
        )
    }

    /// Re-labels a lower-level refusal with this transaction's effect.
    fn refault(&self, cause: &UpdateError) -> UpdateError {
        let (step, detail) = cause.step_and_detail();
        self.fault(step, detail)
    }

    /// Refusal before this call's first journal record: a fresh attempt's is a refused
    /// `start`, and a resumed attempt's keeps its journal authoritative.
    fn before_record(&self, cause: &UpdateError) -> UpdateError {
        if self.journaled {
            self.refault(cause)
        } else {
            self.fault("start", refusal_detail(cause))
        }
    }
}

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
pub(super) fn admit_unreferenced_versions(
    roots: &Roots,
    referenced: &BTreeSet<String>,
) -> Result<Vec<String>, UpdateError> {
    let mut retiring = Vec::new();
    for entry in roots
        .versions
        .entries()
        .map_err(|error| super::error("unjournaled version census", error))?
    {
        let entry = entry.map_err(|error| super::error("unjournaled version census", error))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| super::error("unjournaled version census", "non-UTF-8 entry"))?;
        if referenced
            .iter()
            .any(|version| version.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        if super::is_generated_leaf(&name, "incomplete")
            || super::is_generated_leaf(&name, "retired")
        {
            version_present(roots, &name)?;
            continue;
        }
        super::load::validate_unjournaled_version(roots, &name)?;
        retiring.push(name);
    }
    Ok(retiring)
}

/// Renames each admitted version to a generated `retired-*` sibling; returns the count.
pub(super) fn retire_admitted_versions(
    roots: &Roots,
    admitted: &[String],
) -> Result<usize, UpdateError> {
    for name in admitted {
        retire_version_directory(roots, name)?;
    }
    Ok(admitted.len())
}

/// The step and detail of a lower-level refusal, without its own code or guidance.
pub(super) fn refusal_detail(cause: &UpdateError) -> String {
    let (step, detail) = cause.step_and_detail();
    format!("{step}: {detail}")
}

/// Renames one version directory to a generated `retired-*` sibling and proves its
/// version name is gone.
fn retire_version_directory(roots: &Roots, version: &str) -> Result<(), UpdateError> {
    if !version_present(roots, version)? {
        return Err(super::error(
            "version retirement",
            "retiring version is absent",
        ));
    }
    let leaf = super::random_leaf_name("retired")
        .map_err(|cause| super::error("retired identity", cause))?;
    let versions = versions_for_rename(roots)?;
    crate::windows_fs::publish_new(&versions, version, &leaf)
        .map_err(|cause| super::error("version retirement", cause))?;
    if version_present(roots, version)? {
        return Err(super::error(
            "version retirement",
            "retired version name is still present after rename",
        ));
    }
    Ok(())
}

/// The retained `versions` handle for a rename beneath it, after re-checking its exact
/// installation descriptor: a write never trusts an earlier observation of the parent.
fn versions_for_rename(roots: &Roots) -> Result<std::fs::File, UpdateError> {
    let versions = roots
        .versions
        .try_clone()
        .map_err(|cause| super::error("versions parent", cause))?
        .into_std_file();
    keld_guard::validate_windows_install_directory(&versions, roots.profile())
        .map_err(|cause| super::error("versions profile", cause))?;
    Ok(versions)
}

fn version_present(roots: &Roots, version: &str) -> Result<bool, UpdateError> {
    match roots.versions.symlink_metadata(version) {
        Ok(metadata) => {
            crate::windows_extraction::ensure_directory(&metadata)
                .map_err(|cause| super::error("version presence", cause))?;
            Ok(true)
        }
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(cause) => Err(super::error("version presence", cause)),
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
fn remove_generated_leftovers(roots: &Roots) -> Result<(), UpdateError> {
    let versions = roots
        .versions
        .try_clone()
        .map_err(|cause| leftover_error(cause.to_string()))?
        .into_std_file();
    let names = crate::windows_fs::child_names(&versions)
        .map_err(|cause| leftover_error(cause.to_string()))?;
    for name in names {
        let stale = super::is_generated_leaf(&name, "retired")
            || (super::is_generated_leaf(&name, "incomplete")
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
    super::load::completion_present(&roots.versions.open_dir_nofollow(name)?)
}

/// Removes flushed record siblings left by a crash before their publication rename.
///
/// Such `pending-*` files are created only by this writer under the held lease and are
/// never read as records. The census admits them by name; this removal additionally
/// requires a regular single-link file with the installation's exact profile and refuses
/// anything else. The checks and the deletion bind to one handle, so the object that
/// passed admission is the object deleted.
pub(super) fn remove_stale_record_preparations(roots: &Roots) -> Result<(), UpdateError> {
    let update = roots
        .update
        .try_clone()
        .map_err(|cause| super::error("stale record census", cause))?
        .into_std_file();
    let names = crate::windows_fs::child_names(&update)
        .map_err(|cause| super::error("stale record census", cause))?;
    for name in names {
        if super::is_generated_leaf(&name, "pending") {
            let stale = crate::windows_fs::open_for_delete(
                &update,
                &name,
                crate::windows_fs::DeletePurpose::Admit,
            )
            .map_err(|cause| super::error("stale record admission", cause))?;
            super::admit_machine_file(&roots.update, stale.file(), roots.profile())
                .map_err(|cause| super::error("stale record admission", cause))?;
            stale
                .delete()
                .map_err(|cause| super::error("stale record removal", cause))?;
        }
    }
    Ok(())
}

/// Mints three distinct, nonzero identities: attempt, health channel, lifecycle channel.
fn mint_identities() -> Result<[[u8; 32]; 3], UpdateError> {
    let mut identities = [[0_u8; 32]; 3];
    for identity in &mut identities {
        getrandom::fill(identity).map_err(|cause| super::error("attempt identity", cause))?;
    }
    let [attempt, health, lifecycle] = identities;
    if attempt == health
        || attempt == lifecycle
        || health == lifecycle
        || identities.contains(&[0; 32])
    {
        return Err(super::error(
            "attempt identity",
            "minted attempt, health and lifecycle identities are not distinct",
        ));
    }
    Ok(identities)
}

fn leftover_error(detail: String) -> UpdateError {
    UpdateError::activation(
        "resolved leftover cleanup",
        ActivationEffect::ResolvedWithLeftovers,
        detail,
    )
}
