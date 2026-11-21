//! D-23 protected metadata records through the single KEL-53 repository owner.
//!
//! This retained-handle file adapter grants neither TUF authority nor a
//! `ReadyRevision`. The sole native metadata owner must hold the repository-wide
//! lease and the complete D-90 census/reader witness through each operation.
//! D-90 has no qualified witness producer on this base, so these operations
//! cannot authorize installation or package selection.
//!
//! The current Windows native owner can only publish new absent-target records
//! here. Mutating current, replacing the pending anchor and deleting a pending
//! pointer require T-82-reviewed native authority. No unsafe or pathname
//! fallback is added here.

use super::initial_admission_journal::InitialAdmissionJournalIo;
use super::record_publication::read_optional_record;
use super::repository_census::RepositoryLedgerCensus;
use super::repository_member_diagnostics::require_only_pointed_repository_member;
use super::{error, publish_new_record};
use crate::UpdateError;
use crate::error::hex_digest;
use crate::records::{
    AdmissionInspection, AnchorRecord, CurrentRecord, EnrollmentState, GenerationPhase,
    GenerationRecord, InitialAdmissionRecord, RepositoryAnchor, decode_metadata_anchor,
    decode_metadata_current, decode_metadata_generation, encode_metadata_anchor,
    encode_metadata_current, encode_metadata_generation,
};
use cap_std::fs::Dir;
use sha2::{Digest as _, Sha256};

const CURRENT: &str = "current.json";
const PENDING_OPERATION: &str = "pending-operation.json";

/// Classification of pinned, canonical metadata bytes, without a TUF grant.
#[derive(Debug)]
pub(super) enum NativeMetadataInspection {
    /// No anchor/current was observed; D-90 must prove no other ledger object.
    NoAnchorObserved,
    /// An interrupted enrollment requires the exact native recovery owner.
    EnrollmentPending {
        /// Durable pending enrollment anchor.
        anchor: AnchorRecord,
        /// Current may have been published before anchor completion.
        current: Option<CurrentRecord>,
    },
    /// Pointer and selected generation bytes agree, but TUF signature, floors,
    /// repository census and pending permits must still be validated.
    Established {
        /// Durable established enrollment anchor.
        anchor: AnchorRecord,
        /// Exact protected current record.
        current: CurrentRecord,
        /// Exact canonically decoded selected generation.
        generation: Box<GenerationRecord>,
    },
}

/// Read-only startup result under the same retained repository writer lease.
///
/// A pending D-64 operation never selects metadata, even if the established
/// current generation is complete. D-90 still owns any orphan ledger census.
#[derive(Debug)]
pub(super) enum NativeRepositoryRestart {
    /// No pending D-68 initial operation was found.
    Metadata(NativeMetadataInspection),
    /// Only the exact recorded initial admission owner may resume or abort.
    InitialAdmission {
        /// Journal read through its derived retained-parent leaf.
        record: Box<InitialAdmissionRecord>,
        /// Existing D-64 diagnostic; no effect or cleanup authority.
        disposition: AdmissionInspection,
    },
}

/// Native metadata and operation directories borrowed from the same KEL-53 writer.
///
/// The owner must have pinned and checked acquisition ancestry, no-reparse,
/// volume, actual captured SID, DACL and fixed NTFS before constructing this.
pub(super) struct RepositoryMetadataIo<'lease> {
    enrollments: &'lease Dir,
    repository: &'lease Dir,
    generations: &'lease Dir,
    admissions: &'lease Dir,
    recoveries: &'lease Dir,
    transitions: &'lease Dir,
    expected: &'lease RepositoryAnchor,
    eligibility: &'lease RepositoryLedgerCensus,
    profile: keld_guard::WindowsInstallProtectionProfile,
}

/// All operation namespaces from the same retained repository writer.
/// This bundles borrowed parents; it is not a lease or a D-90 witness.
pub(super) struct RetainedOperationParents<'lease> {
    admissions: &'lease Dir,
    recoveries: &'lease Dir,
    transitions: &'lease Dir,
}

impl<'lease> RetainedOperationParents<'lease> {
    /// Bundle already authenticated retained native parents without deriving
    /// a repository writer, census witness, or admission permission.
    pub(super) const fn new(
        admissions: &'lease Dir,
        recoveries: &'lease Dir,
        transitions: &'lease Dir,
    ) -> Self {
        Self {
            admissions,
            recoveries,
            transitions,
        }
    }
}

impl<'lease> RepositoryMetadataIo<'lease> {
    /// Attaches only to the caller-retained lease and D-90 proof; this type
    /// cannot be constructed by a production caller before T-82 qualification.
    pub(super) const fn new(
        enrollments: &'lease Dir,
        repository: &'lease Dir,
        generations: &'lease Dir,
        operations: &RetainedOperationParents<'lease>,
        expected: &'lease RepositoryAnchor,
        eligibility: &'lease RepositoryLedgerCensus,
        profile: keld_guard::WindowsInstallProtectionProfile,
    ) -> Self {
        Self {
            enrollments,
            repository,
            generations,
            admissions: operations.admissions,
            recoveries: operations.recoveries,
            transitions: operations.transitions,
            expected,
            eligibility,
            profile,
        }
    }

    /// Binds D-23 inspection and D-64/D-68 journal diagnosis to the same
    /// D-90 witness and repository parent. No successful result grants launch.
    pub(super) fn inspect_restart(&self) -> Result<NativeRepositoryRestart, UpdateError> {
        let journal = InitialAdmissionJournalIo::new(
            self.repository,
            self.admissions,
            self.recoveries,
            self.transitions,
            self.expected,
            self.eligibility,
            self.profile,
        );
        if let Some((record, disposition)) = journal.inspect_pointed_attempt()? {
            return Ok(NativeRepositoryRestart::InitialAdmission {
                record: Box::new(record),
                disposition,
            });
        }
        // The ordinary path must refuse every unpointed operation namespace,
        // including an orphan admitted at the D-64/D-68 publication cut.
        // inspect() performs that fence again before inspecting metadata.
        self.inspect().map(NativeRepositoryRestart::Metadata)
    }

    /// Diagnoses anchor, current and selected generation by their retained
    /// handles. An existing D-68 pointer stops ordinary metadata selection.
    pub(super) fn inspect(&self) -> Result<NativeMetadataInspection, UpdateError> {
        self.require_no_pending()?;
        let anchor = self.read_anchor()?;
        let current = self.read_current()?;
        match anchor {
            None if current.is_none() => {
                // A previous enrollment can crash after publishing a non-genesis
                // generation or a pending publication sibling. Absence of
                // generations/1.json is never proof of a fresh repository.
                // This retained-parent check is negative evidence only; D-90
                // still owns the complete repository census and live lease.
                require_empty_generation_namespace(self.generations)?;
                Ok(NativeMetadataInspection::NoAnchorObserved)
            }
            None => Err(error(
                "metadata enrollment readback",
                "current or genesis generation exists without an authenticated enrollment anchor",
            )),
            Some(anchor) if anchor.enrollment_state == EnrollmentState::Pending => {
                // Pending enrollment cannot ignore unreferenced generation
                // members. This negative check does not mint D-90 authority.
                require_pending_generation_namespace(self.generations, current.is_some())?;
                if let Some(current) = &current {
                    let generation = self.require_selected_generation(current)?;
                    if current.sequence != 1
                        || current.generation_sha256 != anchor.initial_generation_sha256
                        || generation.phase != GenerationPhase::Ready
                    {
                        return Err(error(
                            "metadata pending enrollment",
                            "pending current differs from the exact accepted Ready genesis",
                        ));
                    }
                } else if let Some((_generation, bytes)) = self.read_generation(1)?
                    && sha256(&bytes) != anchor.initial_generation_sha256
                {
                    return Err(error(
                        "metadata pending enrollment",
                        "unpointed genesis differs from the pending anchor digest",
                    ));
                }
                Ok(NativeMetadataInspection::EnrollmentPending { anchor, current })
            }
            Some(anchor) => {
                let current = current.ok_or_else(|| {
                    error(
                        "metadata current readback",
                        "established enrollment has no current pointer",
                    )
                })?;
                // D-23 retires older generations, so a selected N>1 cannot
                // be tied to the enrollment anchor by a naive local walk.
                // Only D-90's qualified historical owner can prove its
                // versioned retained chain and independent application floors.
                if current.sequence != 1 {
                    return Err(error(
                        "metadata history qualification",
                        "established non-genesis current needs authenticated D-90 history",
                    ));
                }
                let generation = self.require_selected_generation(&current)?;
                require_established_ready_generation(&generation)?;
                if current.generation_sha256 != anchor.initial_generation_sha256 {
                    return Err(error(
                        "metadata enrollment binding",
                        "genesis current differs from the anchor initial-generation digest",
                    ));
                }
                // Even a correctly hashed Ready genesis is not a clean
                // restart while an unreferenced successor or interrupted
                // publication sibling remains. Do not silently select past
                // D-24's ordinary-retirement recovery boundary.
                require_only_genesis_generation(self.generations)?;
                Ok(NativeMetadataInspection::Established {
                    anchor,
                    current,
                    generation: Box::new(generation),
                })
            }
        }
    }

    /// Publishes and rereads the first pending anchor. The existing metadata
    /// owner must already have authenticated the new repository and genesis
    /// digest and proved that no hidden metadata owner exists.
    pub(super) fn publish_pending_anchor(&self, anchor: &AnchorRecord) -> Result<(), UpdateError> {
        self.require_no_pending()?;
        self.require_anchor_owner(anchor)?;
        if anchor.enrollment_state != EnrollmentState::Pending {
            return Err(error(
                "metadata anchor publication",
                "new anchor must be pending",
            ));
        }
        if self.read_anchor()?.is_some() || self.read_current()?.is_some() {
            return Err(error(
                "metadata anchor publication",
                "existing anchor or current needs recovery",
            ));
        }
        // Read the full retained generations namespace before first enrollment.
        // One unknown, non-genesis or interrupted publication entry blocks
        // creation; a matching name alone never authenticates an owner.
        require_empty_generation_namespace(self.generations)?;
        let bytes = encode_metadata_anchor(anchor)?;
        publish_new_record(self.enrollments, &self.anchor_leaf(), &bytes, self.profile)?;
        if self.read_anchor()?.as_ref() != Some(anchor) {
            return Err(error(
                "metadata anchor readback",
                "published pending anchor differs",
            ));
        }
        Ok(())
    }

    /// Publishes an immutable generation at an absent derived sequence leaf.
    /// It checks the protected current head, prior-generation SHA-256, epoch,
    /// UTC floor, policy and sequence before any I/O. The owner must still
    /// independently authenticate TUF and clean orphan state under the lease.
    pub(super) fn publish_immutable_generation(
        &self,
        generation: &GenerationRecord,
    ) -> Result<[u8; 32], UpdateError> {
        self.require_no_pending()?;
        let anchor = self.read_anchor()?.ok_or_else(|| {
            error(
                "metadata generation publication",
                "enrollment anchor is missing",
            )
        })?;
        let bytes = encode_metadata_generation(generation)?;
        if generation.repository_id != self.expected.repository_id {
            return Err(error(
                "metadata generation publication",
                "candidate generation names another repository",
            ));
        }
        let digest = sha256(&bytes);
        match anchor.enrollment_state {
            EnrollmentState::Pending => {
                if self.read_current()?.is_some()
                    || generation.sequence != 1
                    || digest != anchor.initial_generation_sha256
                {
                    return Err(error(
                        "metadata generation publication",
                        "initial generation disagrees with pending anchor or current exists",
                    ));
                }
                // The anchor was published before this generation. A crash
                // or foreign writer may have left another generation or a
                // prepared sibling in the meantime: refuse before creation.
                require_empty_generation_namespace(self.generations)?;
            }
            EnrollmentState::Established => {
                let current = self.read_current()?.ok_or_else(|| {
                    error(
                        "metadata generation publication",
                        "established current is missing",
                    )
                })?;
                if current.sequence != 1
                    || current.generation_sha256 != anchor.initial_generation_sha256
                {
                    return Err(error(
                        "metadata history qualification",
                        "ordinary successor requires D-90 qualified retained history beyond genesis",
                    ));
                }
                let previous = self.require_selected_generation(&current)?;
                require_established_ready_generation(&previous)?;
                // A previous uncommitted N+1, foreign generation or pending
                // record sibling requires D-24's exact retirement owner. It
                // cannot be bypassed by publishing another generation.
                require_only_genesis_generation(self.generations)?;
                if current.sequence.checked_add(1) != Some(generation.sequence)
                    || generation.previous_generation_sha256 != Some(current.generation_sha256)
                    || generation.recovery_epoch != previous.recovery_epoch
                    || generation.utc_floor_millis < previous.utc_floor_millis
                    || generation.policy_sha256 != previous.policy_sha256
                {
                    return Err(error(
                        "metadata generation publication",
                        "ordinary successor, epoch, policy, previous hash or UTC floor differs",
                    ));
                }
            }
        }
        if self.read_generation(generation.sequence)?.is_some() {
            return Err(error(
                "metadata generation publication",
                "immutable generation name already exists",
            ));
        }
        publish_new_record(
            self.generations,
            &generation_leaf(generation.sequence),
            &bytes,
            self.profile,
        )?;
        let (observed, readback) = self.read_generation(generation.sequence)?.ok_or_else(|| {
            error(
                "metadata generation readback",
                "published generation is missing",
            )
        })?;
        if &observed != generation || readback != bytes {
            return Err(error(
                "metadata generation readback",
                "protected immutable generation differs",
            ));
        }
        // Check the complete retained namespace again after publication, not
        // only the candidate leaf. This is a negative check under the caller's
        // existing writer lease, never a repository-wide D-90 witness.
        match anchor.enrollment_state {
            EnrollmentState::Pending => require_only_genesis_generation(self.generations)?,
            EnrollmentState::Established => {
                require_only_genesis_and_successor(self.generations)?;
            }
        }
        Ok(digest)
    }

    /// Publishes the one absent first current record after the exact Ready
    /// genesis generation is fully bound to the pending anchor. Enrollment
    /// remains pending until T-82 atomically establishes the anchor.
    pub(super) fn publish_initial_current(
        &self,
        current: &CurrentRecord,
    ) -> Result<(), UpdateError> {
        self.require_no_pending()?;
        let anchor = self
            .read_anchor()?
            .ok_or_else(|| error("metadata initial current", "enrollment anchor is missing"))?;
        if anchor.enrollment_state != EnrollmentState::Pending
            || current.sequence != 1
            || current.repository_id != self.expected.repository_id
        {
            return Err(error(
                "metadata initial current",
                "only a matching pending repository and genesis current are admitted",
            ));
        }
        if self.read_current()?.is_some() {
            return Err(error("metadata initial current", "current already exists"));
        }
        // A valid genesis does not authorize initial Current while another
        // prepared, foreign or unreferenced generation is still present.
        require_only_genesis_generation(self.generations)?;
        let generation = self.require_selected_generation(current)?;
        if generation.phase != GenerationPhase::Ready
            || current.generation_sha256 != anchor.initial_generation_sha256
        {
            return Err(error(
                "metadata initial current",
                "genesis is incomplete or differs from pending anchor",
            ));
        }
        let bytes = encode_metadata_current(current)?;
        publish_new_record(self.repository, CURRENT, &bytes, self.profile)?;
        if self.read_current()?.as_ref() != Some(current) {
            return Err(error(
                "metadata current readback",
                "published current pointer differs",
            ));
        }
        Ok(())
    }

    fn require_no_pending(&self) -> Result<(), UpdateError> {
        if read_optional_record(self.repository, PENDING_OPERATION, self.profile)?.is_some() {
            return Err(error(
                "metadata pending-operation fence",
                "pending operation owns the repository; ordinary mutation or selection refused",
            ));
        }
        // A missing D-68 pointer can follow a durable D-64/D-65/D-85 journal
        // or a crash that left a pending publication sibling. A local empty
        // scan is only a negative check; D-90 still owns full census proof.
        require_only_pointed_repository_member(
            self.admissions,
            self.recoveries,
            self.transitions,
            self.expected,
            None,
        )?;
        Ok(())
    }

    fn anchor_leaf(&self) -> String {
        format!("{}.anchor", hex_digest(&self.expected.repository_id))
    }

    fn require_anchor_owner(&self, anchor: &AnchorRecord) -> Result<(), UpdateError> {
        if anchor.repository_id != self.expected.repository_id
            || anchor.app_id != self.expected.app_id
            || anchor.owner_sid != self.expected.owner_sid
        {
            return Err(error(
                "metadata enrollment binding",
                "anchor differs from repository ID, app or native owner SID",
            ));
        }
        Ok(())
    }

    fn read_anchor(&self) -> Result<Option<AnchorRecord>, UpdateError> {
        let Some(bytes) =
            read_optional_record(self.enrollments, &self.anchor_leaf(), self.profile)?
        else {
            return Ok(None);
        };
        let anchor = decode_metadata_anchor(&bytes)?;
        self.require_anchor_owner(&anchor)?;
        Ok(Some(anchor))
    }

    fn read_current(&self) -> Result<Option<CurrentRecord>, UpdateError> {
        let Some(bytes) = read_optional_record(self.repository, CURRENT, self.profile)? else {
            return Ok(None);
        };
        let current = decode_metadata_current(&bytes)?;
        if current.repository_id != self.expected.repository_id {
            return Err(error(
                "metadata current binding",
                "current names another repository",
            ));
        }
        Ok(Some(current))
    }

    fn read_generation(
        &self,
        sequence: u64,
    ) -> Result<Option<(GenerationRecord, Vec<u8>)>, UpdateError> {
        if sequence == 0 {
            return Err(error(
                "metadata generation lookup",
                "zero sequence is invalid",
            ));
        }
        let Some(bytes) =
            read_optional_record(self.generations, &generation_leaf(sequence), self.profile)?
        else {
            return Ok(None);
        };
        let generation = decode_metadata_generation(&bytes)?;
        if generation.repository_id != self.expected.repository_id
            || generation.sequence != sequence
        {
            return Err(error(
                "metadata generation binding",
                "generation differs from repository or derived sequence leaf",
            ));
        }
        Ok(Some((generation, bytes)))
    }

    fn require_selected_generation(
        &self,
        current: &CurrentRecord,
    ) -> Result<GenerationRecord, UpdateError> {
        let (generation, bytes) = self.read_generation(current.sequence)?.ok_or_else(|| {
            error(
                "metadata selected generation",
                "current names an absent generation",
            )
        })?;
        if current.generation_sha256 != sha256(&bytes) {
            return Err(error(
                "metadata selected generation",
                "generation bytes disagree with protected current SHA-256",
            ));
        }
        Ok(generation)
    }
}

fn generation_leaf(sequence: u64) -> String {
    format!("{sequence}.json")
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// A negative check for any generation residue when no enrollment anchor exists.
///
/// This observes the entire already-retained parent, including non-genesis
/// generations and interrupted pending siblings. An empty directory is only a
/// local observation; D-90's independent repository-wide member and reader
/// proof is still mandatory before anyone may create the first anchor.
fn require_empty_generation_namespace(generations: &Dir) -> Result<(), UpdateError> {
    let mut entries = generations
        .entries()
        .map_err(|cause| error("metadata generation listing", cause))?;
    match entries.next() {
        None => Ok(()),
        Some(Err(cause)) => Err(error("metadata generation listing", cause)),
        Some(Ok(_)) => Err(error(
            "metadata generation residue",
            "a retained generation or pending publication sibling blocks fresh enrollment",
        )),
    }
}

/// Pending enrollment permits only an empty pre-genesis namespace or one
/// exact genesis leaf. A published Current additionally requires that leaf.
///
/// This one-pass retained-parent check refuses unknown or interrupted siblings.
/// Only D-90's future authenticated repository owner can grant eligibility.
fn require_pending_generation_namespace(
    generations: &Dir,
    has_current: bool,
) -> Result<(), UpdateError> {
    let mut entries = generations
        .entries()
        .map_err(|cause| error("metadata generation listing", cause))?;
    let first = match entries.next() {
        None if !has_current => return Ok(()),
        None => {
            return Err(error(
                "metadata generation residue",
                "pending current has no genesis generation",
            ));
        }
        Some(Err(cause)) => return Err(error("metadata generation listing", cause)),
        Some(Ok(entry)) => entry,
    };
    if first.file_name().as_os_str() != std::ffi::OsStr::new("1.json") {
        return Err(error(
            "metadata generation residue",
            "pending enrollment has a foreign generation or publication sibling",
        ));
    }
    match entries.next() {
        None => Ok(()),
        Some(Err(cause)) => Err(error("metadata generation listing", cause)),
        Some(Ok(_)) => Err(error(
            "metadata generation residue",
            "pending enrollment contains another generation or publication sibling",
        )),
    }
}

/// A negative exact-name census of the one supported committed genesis.
///
/// The selected leaf is still read separately by the native protected-record
/// reader for canonical bytes, DACL, type and SHA-256. This directory shape
/// cannot mint D-90 repository eligibility or authenticate an unknown writer.
fn require_only_genesis_generation(generations: &Dir) -> Result<(), UpdateError> {
    super::exact_entries(generations, &["1.json"])
        .map_err(|cause| error("metadata generation residue", cause))
}

/// Negative readback census after the only currently supported N=1 to N=2
/// preparation. No third, foreign, or pending sibling can be silently ignored.
/// Publishing Current=N+2 is still unavailable without T-82 native replacement.
fn require_only_genesis_and_successor(generations: &Dir) -> Result<(), UpdateError> {
    super::exact_entries(generations, &["1.json", "2.json"])
        .map_err(|cause| error("metadata generation residue", cause))
}

/// An established D-23 current pointer must select a full ready generation.
/// The canonical codec intentionally accepts non-ready phases for in-progress
/// records, so pointer interpretation must enforce the separate readiness rule.
fn require_established_ready_generation(generation: &GenerationRecord) -> Result<(), UpdateError> {
    if generation.phase != GenerationPhase::Ready {
        return Err(error(
            "metadata established readiness",
            "established current selects incomplete root-refresh or roles generation",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
