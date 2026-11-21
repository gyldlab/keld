//! Native D-64/D-68 journal publication beneath the existing KEL-53 repository writer.
//!
//! The owner of these retained directories must already hold the exclusive KEL-53
//! metadata admission lease and have validated the repository census, anchor, SID,
//! directory handles, volume, and file protection. This private I/O layer mints
//! no permit, acquires no lease, and does not create installation objects.
//! Its methods cannot be a substitute for D-90's repository-wide census.
//! Phase replacement and pointer retirement require additional T82-approved
//! relative-native operations. They remain unavailable here, so the writer
//! cannot arm installation effects or silently clear a recovery fence.

use super::record_publication::read_optional_record;
use super::repository_census::RepositoryLedgerCensus;
use super::{error, publish_new_record};
use crate::UpdateError;
use crate::error::hex_digest;
use crate::records::{
    AdmissionInspection, InitialAdmissionPhase, InitialAdmissionRecord, PendingOperationKind,
    PendingRepositoryOperation, RepositoryAnchor, decode_initial_admission_record,
    decode_pending_repository_operation, encode_initial_admission_record,
    encode_pending_repository_operation, inspect_initial_admission,
};
use cap_std::fs::Dir;

const PENDING_OPERATION: &str = "pending-operation.json";

/// Borrowed, prevalidated native repository handles held by the KEL-53 writer.
///
/// The caller owns both handles and the metadata lease for this entire lifetime.
/// A native D-90 eligibility witness is mandatory even to construct this I/O
/// wrapper. The witness has no constructor until its complete native producer
/// is qualified, so this code currently cannot publish a journal.
pub(super) struct InitialAdmissionJournalIo<'lease> {
    repository: &'lease Dir,
    admissions: &'lease Dir,
    anchor: &'lease RepositoryAnchor,
    _eligibility: &'lease RepositoryLedgerCensus,
    profile: keld_guard::WindowsInstallProtectionProfile,
}

impl<'lease> InitialAdmissionJournalIo<'lease> {
    /// Attaches to handles authenticated by the repository owner under its lease.
    /// The unforgeable D-90 witness must remain borrowed through every I/O call.
    pub(super) const fn new(
        repository: &'lease Dir,
        admissions: &'lease Dir,
        anchor: &'lease RepositoryAnchor,
        eligibility: &'lease RepositoryLedgerCensus,
        profile: keld_guard::WindowsInstallProtectionProfile,
    ) -> Self {
        Self {
            repository,
            admissions,
            anchor,
            _eligibility: eligibility,
            profile,
        }
    }

    /// Reads the exact protected pointer, refusing malformed or foreign bytes.
    /// Absence is only an observation; the caller must perform D-90 census.
    pub(super) fn read_pending(&self) -> Result<Option<PendingRepositoryOperation>, UpdateError> {
        let Some(bytes) = read_optional_record(self.repository, PENDING_OPERATION, self.profile)?
        else {
            return Ok(None);
        };
        let pointer = decode_pending_repository_operation(&bytes)?;
        if pointer.repository_id != self.anchor.repository_id
            || pointer.app_id != self.anchor.app_id
            || pointer.owner_sid != self.anchor.owner_sid
        {
            return Err(error(
                "repository pending pointer",
                "foreign repository, app or owner",
            ));
        }
        Ok(Some(pointer))
    }

    /// Loads a D-64 record through the retained parent. Caller must classify it
    /// against the matching pointer, if any: an effectful pointed journal would
    /// be misclassified as an orphan if inspected with no pointer.
    fn read_attempt(
        &self,
        attempt: &[u8; 32],
    ) -> Result<Option<InitialAdmissionRecord>, UpdateError> {
        let leaf = admission_leaf(attempt);
        let Some(bytes) = read_optional_record(self.admissions, &leaf, self.profile)? else {
            return Ok(None);
        };
        let record = decode_initial_admission_record(&bytes)?;
        if record.attempt_id != *attempt {
            return Err(error(
                "initial journal binding",
                "record does not match its derived leaf",
            ));
        }
        if record.repository_id != self.anchor.repository_id
            || record.owner_sid != self.anchor.owner_sid
            || record.destination_identity.app_id != self.anchor.app_id
        {
            return Err(error(
                "initial journal binding",
                "foreign repository, app or owner",
            ));
        }
        Ok(Some(record))
    }

    /// Follows only a native-authenticated D-68 pointer to its exact D-64 journal.
    /// The returned D-64 disposition is diagnostic, never an effect permit.
    /// A missing pointer does not rule out an orphan admitted journal: the
    /// repository owner must also perform D-90's complete native ledger census.
    pub(super) fn inspect_pointed_attempt(
        &self,
    ) -> Result<Option<(InitialAdmissionRecord, AdmissionInspection)>, UpdateError> {
        let Some(pointer) = self.read_pending()? else {
            return Ok(None);
        };
        if pointer.kind != PendingOperationKind::InitialAdmission {
            return Err(error(
                "repository pending pointer",
                "another repository operation is active",
            ));
        }
        let record = self.read_attempt(&pointer.attempt_id)?.ok_or_else(|| {
            error(
                "repository pending pointer",
                "pointer names an absent initial journal",
            )
        })?;
        let disposition =
            inspect_initial_admission(self.anchor, Some(&record), Some(&pointer), &[])?;
        Ok(Some((record, disposition)))
    }

    /// Publishes and rereads `admitted` after the parent KEL-53 owner has proved
    /// full ledger vacancy, complete D-9 evidence, and an exact D-46 admission.
    /// The D-46 owner decides when its one-shot permit is consumed; this record
    /// writer does not consume, construct or return an installation permit.
    /// A failure can leave an unpointed journal: D-90 must census it on restart.
    pub(super) fn publish_admitted(
        &self,
        mut record: InitialAdmissionRecord,
    ) -> Result<InitialAdmissionRecord, UpdateError> {
        if record.phase != InitialAdmissionPhase::Admitted {
            return Err(error(
                "initial journal phase",
                "first phase must be admitted",
            ));
        }
        if self.read_pending()?.is_some() {
            return Err(error(
                "repository pending pointer",
                "another operation is already pending",
            ));
        }
        // KEL-53 owns entropy; never accept a caller-chosen attempt identifier.
        getrandom::fill(&mut record.attempt_id)
            .map_err(|cause| error("initial attempt entropy", cause))?;
        if inspect_initial_admission(self.anchor, Some(&record), None, &[])?
            != AdmissionInspection::OrphanAdmittedNeedsIndependentNoEffectProof
        {
            return Err(error(
                "initial journal validation",
                "admission is not an unarmed first phase",
            ));
        }
        let bytes = encode_initial_admission_record(&record)?;
        publish_new_record(
            self.admissions,
            &admission_leaf(&record.attempt_id),
            &bytes,
            self.profile,
        )?;
        if self.read_attempt(&record.attempt_id)?.as_ref() != Some(&record) {
            return Err(error(
                "initial journal readback",
                "published journal differs",
            ));
        }
        Ok(record)
    }

    /// Publishes the D-68 fence for an already durable `admitted` journal.
    /// Creation still requires a subsequent durable `creation-armed` transition.
    pub(super) fn publish_pending_for(
        &self,
        record: &InitialAdmissionRecord,
    ) -> Result<(), UpdateError> {
        if record.phase != InitialAdmissionPhase::Admitted {
            return Err(error(
                "repository pending pointer",
                "journal must still be admitted",
            ));
        }
        if self.read_attempt(&record.attempt_id)?.as_ref() != Some(record) {
            return Err(error(
                "repository pending pointer",
                "no matching durable admitted journal",
            ));
        }
        if inspect_initial_admission(self.anchor, Some(record), None, &[])?
            != AdmissionInspection::OrphanAdmittedNeedsIndependentNoEffectProof
        {
            return Err(error(
                "repository pending pointer",
                "record is not an admitted orphan",
            ));
        }
        if self.read_pending()?.is_some() {
            return Err(error(
                "repository pending pointer",
                "pointer is already present",
            ));
        }
        let pointer = PendingRepositoryOperation {
            repository_id: record.repository_id,
            app_id: record.destination_identity.app_id.clone(),
            owner_sid: record.owner_sid.clone(),
            kind: PendingOperationKind::InitialAdmission,
            attempt_id: record.attempt_id,
        };
        let bytes = encode_pending_repository_operation(&pointer)?;
        publish_new_record(self.repository, PENDING_OPERATION, &bytes, self.profile)?;
        if self.read_pending()?.as_ref() != Some(&pointer) {
            return Err(error(
                "repository pending readback",
                "published pointer differs",
            ));
        }
        Ok(())
    }
}

fn admission_leaf(attempt: &[u8; 32]) -> String {
    format!("{}.json", hex_digest(attempt))
}
