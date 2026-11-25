//! Read-only, bounded canonical D-64/D-68 initial-admission repository records.
//!
//! These are authenticated-input *formats*, not OS authentication or permits.
//! KEL-53 T82 owns the repository lease, physical ledger census, native retained
//! handle proofs, durable writes and readbacks. None of those effects are here.

use super::{
    DirectInstallationIdentity, UpdateError, WireIdentity, decode, digest, encode, hex_digest,
    identity_from_wire, invalid, wire_identity,
};
use serde::{Deserialize, Serialize};

const ADMISSION_SCHEMA: &str = "keld.windows.initial-admission/v1";
const POINTER_SCHEMA: &str = "keld.repository-pending-operation/v1";
const MAX_ADMISSION_BYTES: usize = 32_768;
const MAX_POINTER_BYTES: usize = 2_048;

mod observations;
use observations::{
    DestinationRootBinding, NativeObjectObservation, ObjectKind, RootBindingKind, WireObject,
    WireRoot, object_from_wire, root_from_wire, validate_object, validate_root, wire_object,
    wire_root,
};

/// D-64's only accepted persisted initial-admission phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InitialAdmissionPhase {
    Admitted,
    CreationArmed,
    BootstrapOwned,
    ProvenancePublished,
    Complete,
    Aborted,
}

/// Exact D-64 initial admission context, independent of KEL-53 activation journals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InitialAdmissionRecord {
    pub(crate) attempt_id: [u8; 32],
    pub(crate) repository_id: [u8; 32],
    pub(crate) owner_sid: String,
    pub(crate) metadata_sequence: u64,
    pub(crate) metadata_generation_sha256: [u8; 32],
    pub(crate) recovery_epoch: u64,
    pub(crate) host_sha256: [u8; 32],
    pub(crate) catalogue_sha256: [u8; 32],
    pub(crate) repository_policy_sha256: [u8; 32],
    pub(crate) discovery_policy_sha256: [u8; 32],
    pub(crate) manifest_sha256: [u8; 32],
    pub(crate) package_content_blake3: [u8; 32],
    pub(crate) package_transport_blake3: [u8; 32],
    pub(crate) destination_identity: DirectInstallationIdentity,
    pub(crate) destination_root_identity: DestinationRootBinding,
    pub(crate) source_identity: NativeObjectObservation,
    pub(crate) phase: InitialAdmissionPhase,
}

/// The three mutually exclusive D-68 repository operation kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PendingOperationKind {
    InitialAdmission,
    ManualRecovery,
    PlannedTransition,
}

/// One exact D-68 pending pointer, never containing a caller-supplied path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingRepositoryOperation {
    pub(crate) repository_id: [u8; 32],
    pub(crate) app_id: String,
    pub(crate) owner_sid: String,
    pub(crate) kind: PendingOperationKind,
    pub(crate) attempt_id: [u8; 32],
}

impl PendingRepositoryOperation {
    /// Derives the sole journal leaf from closed kind and attempt bytes.
    #[must_use]
    pub(crate) fn expected_journal_leaf(&self) -> String {
        let directory = match self.kind {
            PendingOperationKind::InitialAdmission => "admissions",
            PendingOperationKind::ManualRecovery => "recoveries",
            PendingOperationKind::PlannedTransition => "transitions",
        };
        format!("{directory}/{}.json", hex_digest(&self.attempt_id))
    }
}

/// Trusted expected anchor values supplied separately by the native owner.
///
/// Constructing this object cannot attest to a native SID, descriptor or lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositoryAnchor {
    pub(crate) repository_id: [u8; 32],
    pub(crate) app_id: String,
    pub(crate) owner_sid: String,
}

/// Read-only classification; no variant permits installation creation or cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionInspection {
    /// An inspected empty input set does not prove that the native ledger is empty.
    NoRecordObserved,
    /// The admitted journal is unreferenced and needs independent D-64 no-effect proof.
    OrphanAdmittedNeedsIndependentNoEffectProof,
    /// A matching admitted journal and pointer still cannot mint a consumed permit.
    AdmittedUnarmed,
    /// Creation-armed may have effects: the original owner must resolve it.
    CreationArmedOperatorDispositionRequired,
    /// Effectful states require the native journal-bound KEL-53 recovery owner.
    BoundRecoveryRequired,
    /// Terminal journal readback and coherent application state must be proved.
    TerminalCoherenceProofRequired,
    /// This single terminal record is diagnostic, not renewed authority.
    RetainedTerminalDiagnostic,
}

impl InitialAdmissionPhase {
    /// Classifies an exact pointed D-64 phase for read-only recovery.
    ///
    /// The caller must first authenticate the D-68 pointer and journal. Every
    /// result is diagnostic: even a terminal record still requires independent
    /// native coherence and no-effect proof before retirement.
    pub(crate) const fn pointed_inspection(self) -> AdmissionInspection {
        match self {
            Self::Admitted => AdmissionInspection::AdmittedUnarmed,
            Self::CreationArmed => AdmissionInspection::CreationArmedOperatorDispositionRequired,
            Self::BootstrapOwned | Self::ProvenancePublished => {
                AdmissionInspection::BoundRecoveryRequired
            }
            Self::Complete | Self::Aborted => AdmissionInspection::TerminalCoherenceProofRequired,
        }
    }

    /// Classifies an unpointed journal after the bounded local member check.
    /// A phase with possible effects cannot lose its D-68 fence and become a
    /// fresh enrollment. A second retained terminal is never silently rotated.
    fn unpointed_inspection(
        self,
        no_retained_terminal: bool,
    ) -> Result<AdmissionInspection, UpdateError> {
        match self {
            Self::Admitted => Ok(AdmissionInspection::OrphanAdmittedNeedsIndependentNoEffectProof),
            Self::Complete | Self::Aborted if no_retained_terminal => {
                Ok(AdmissionInspection::RetainedTerminalDiagnostic)
            }
            _ => Err(invalid("unpointed effectful or excess terminal journal")),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WireAdmission {
    schema: String,
    attempt_id: String,
    repository_id: String,
    owner_sid: String,
    metadata_sequence: u64,
    metadata_generation_sha256: String,
    recovery_epoch: u64,
    host_sha256: String,
    catalogue_sha256: String,
    repository_policy_sha256: String,
    discovery_policy_sha256: String,
    manifest_sha256: String,
    package_content_blake3: String,
    package_transport_blake3: String,
    destination_identity: WireIdentity,
    destination_root_identity: WireRoot,
    source_identity: WireObject,
    phase: InitialAdmissionPhase,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WirePending {
    schema: String,
    repository_id: String,
    app_id: String,
    owner_sid: String,
    kind: PendingOperationKind,
    attempt_id: String,
}

/// Encodes one canonical 32,768-byte-bounded D-64 record. No disk I/O occurs.
pub(crate) fn encode_initial_admission_record(
    record: &InitialAdmissionRecord,
) -> Result<Vec<u8>, UpdateError> {
    validate_record(record)?;
    let wire = WireAdmission {
        schema: ADMISSION_SCHEMA.to_owned(),
        attempt_id: hex_digest(&record.attempt_id),
        repository_id: hex_digest(&record.repository_id),
        owner_sid: record.owner_sid.clone(),
        metadata_sequence: record.metadata_sequence,
        metadata_generation_sha256: hex_digest(&record.metadata_generation_sha256),
        recovery_epoch: record.recovery_epoch,
        host_sha256: hex_digest(&record.host_sha256),
        catalogue_sha256: hex_digest(&record.catalogue_sha256),
        repository_policy_sha256: hex_digest(&record.repository_policy_sha256),
        discovery_policy_sha256: hex_digest(&record.discovery_policy_sha256),
        manifest_sha256: hex_digest(&record.manifest_sha256),
        package_content_blake3: hex_digest(&record.package_content_blake3),
        package_transport_blake3: hex_digest(&record.package_transport_blake3),
        destination_identity: wire_identity(&record.destination_identity)?,
        destination_root_identity: wire_root(&record.destination_root_identity)?,
        source_identity: wire_object(&record.source_identity)?,
        phase: record.phase,
    };
    encode_bounded(&wire, MAX_ADMISSION_BYTES)
}

/// Decodes an exact D-64 record, rejecting unknown, reordered and malformed bytes.
pub(crate) fn decode_initial_admission_record(
    bytes: &[u8],
) -> Result<InitialAdmissionRecord, UpdateError> {
    let wire: WireAdmission = decode_bounded(bytes, MAX_ADMISSION_BYTES)?;
    super::schema(&wire.schema, ADMISSION_SCHEMA)?;
    let record = InitialAdmissionRecord {
        attempt_id: digest(&wire.attempt_id)?,
        repository_id: digest(&wire.repository_id)?,
        owner_sid: wire.owner_sid,
        metadata_sequence: wire.metadata_sequence,
        metadata_generation_sha256: digest(&wire.metadata_generation_sha256)?,
        recovery_epoch: wire.recovery_epoch,
        host_sha256: digest(&wire.host_sha256)?,
        catalogue_sha256: digest(&wire.catalogue_sha256)?,
        repository_policy_sha256: digest(&wire.repository_policy_sha256)?,
        discovery_policy_sha256: digest(&wire.discovery_policy_sha256)?,
        manifest_sha256: digest(&wire.manifest_sha256)?,
        package_content_blake3: digest(&wire.package_content_blake3)?,
        package_transport_blake3: digest(&wire.package_transport_blake3)?,
        destination_identity: identity_from_wire(wire.destination_identity)?,
        destination_root_identity: root_from_wire(wire.destination_root_identity)?,
        source_identity: object_from_wire(wire.source_identity)?,
        phase: wire.phase,
    };
    validate_record(&record)?;
    Ok(record)
}

/// Encodes the six-field D-68 pending pointer with its separate 2,048-byte cap.
pub(crate) fn encode_pending_repository_operation(
    pointer: &PendingRepositoryOperation,
) -> Result<Vec<u8>, UpdateError> {
    validate_sid(&pointer.owner_sid)?;
    validate_app(&pointer.app_id)?;
    encode_bounded(
        &WirePending {
            schema: POINTER_SCHEMA.to_owned(),
            repository_id: hex_digest(&pointer.repository_id),
            app_id: pointer.app_id.clone(),
            owner_sid: pointer.owner_sid.clone(),
            kind: pointer.kind,
            attempt_id: hex_digest(&pointer.attempt_id),
        },
        MAX_POINTER_BYTES,
    )
}

/// Strictly decodes D-68 without resolving any path or opening a protected file.
pub(crate) fn decode_pending_repository_operation(
    bytes: &[u8],
) -> Result<PendingRepositoryOperation, UpdateError> {
    let wire: WirePending = decode_bounded(bytes, MAX_POINTER_BYTES)?;
    super::schema(&wire.schema, POINTER_SCHEMA)?;
    let pointer = PendingRepositoryOperation {
        repository_id: digest(&wire.repository_id)?,
        app_id: wire.app_id,
        owner_sid: wire.owner_sid,
        kind: wire.kind,
        attempt_id: digest(&wire.attempt_id)?,
    };
    validate_sid(&pointer.owner_sid)?;
    validate_app(&pointer.app_id)?;
    Ok(pointer)
}

/// Fail-closed comparison of already loaded, native-authenticated inputs.
///
/// This does not enumerate the ledger or authorize writer operations. The T82
/// native owner must separately prove the absence of further records, exact
/// retained handles, all required D-9 objects and coherent terminal state.
pub(crate) fn inspect_initial_admission(
    expected: &RepositoryAnchor,
    record: Option<&InitialAdmissionRecord>,
    pointer: Option<&PendingRepositoryOperation>,
    retained_terminals: &[InitialAdmissionRecord],
) -> Result<AdmissionInspection, UpdateError> {
    validate_sid(&expected.owner_sid)?;
    validate_app(&expected.app_id)?;
    if retained_terminals.len() > 1 {
        return Err(invalid(
            "repository has more than one retained terminal diagnostic",
        ));
    }
    if let Some(journal) = record {
        validate_for_anchor(journal, expected)?;
    }
    if let Some(terminal) = retained_terminals.first() {
        validate_for_anchor(terminal, expected)?;
        if !matches!(
            terminal.phase,
            InitialAdmissionPhase::Complete | InitialAdmissionPhase::Aborted
        ) {
            return Err(invalid("retained diagnostic is not terminal"));
        }
        if record.is_some_and(|journal| journal.attempt_id == terminal.attempt_id) {
            return Err(invalid("retained terminal duplicates current attempt"));
        }
    }
    if let Some(pending) = pointer {
        validate_sid(&pending.owner_sid)?;
        validate_app(&pending.app_id)?;
        if pending.repository_id != expected.repository_id
            || pending.app_id != expected.app_id
            || pending.owner_sid != expected.owner_sid
            || pending.kind != PendingOperationKind::InitialAdmission
        {
            return Err(invalid("foreign pending repository operation"));
        }
        let Some(journal) = record else {
            return Err(invalid("pending pointer has no authenticated journal"));
        };
        if pending.attempt_id != journal.attempt_id {
            return Err(invalid("pending pointer and journal attempt differ"));
        }
        return Ok(journal.phase.pointed_inspection());
    }
    let Some(journal) = record else {
        return Ok(if retained_terminals.is_empty() {
            AdmissionInspection::NoRecordObserved
        } else {
            AdmissionInspection::RetainedTerminalDiagnostic
        });
    };
    journal
        .phase
        .unpointed_inspection(retained_terminals.is_empty())
}

fn validate_for_anchor(
    record: &InitialAdmissionRecord,
    expected: &RepositoryAnchor,
) -> Result<(), UpdateError> {
    validate_record(record)?;
    if record.repository_id != expected.repository_id
        || record.owner_sid != expected.owner_sid
        || record.destination_identity.app_id != expected.app_id
    {
        return Err(invalid(
            "foreign repository, application or owner in journal",
        ));
    }
    // The D-43 independently witnessed absent destination is the only
    // commissioned root mode. An existing scaffold needs D-43 operator proof.
    if record.destination_root_identity.kind != RootBindingKind::Absent {
        return Err(invalid(
            "existing destination root needs independent uncommissioned proof",
        ));
    }
    Ok(())
}

fn validate_record(record: &InitialAdmissionRecord) -> Result<(), UpdateError> {
    if record.metadata_sequence == 0 {
        return Err(invalid("admission metadata sequence must be positive"));
    }
    validate_sid(&record.owner_sid)?;
    validate_app(&record.destination_identity.app_id)?;
    super::wire_identity(&record.destination_identity)?;
    validate_root(&record.destination_root_identity)?;
    validate_object(&record.source_identity)?;
    if record.source_identity.kind != ObjectKind::RegularFile {
        return Err(invalid("admission source must be an observed regular file"));
    }
    Ok(())
}

pub(super) fn validate_app(value: &str) -> Result<(), UpdateError> {
    // Exact KEL-135 CanonicalAppId grammar, as owned by
    // keld-wv/src/profile/identity.rs::validate_canonical_app_id.
    // That owner-local validator is private and updater cannot import keld-wv
    // without reversing the production crate dependency. No new grammar or
    // authority is defined here: the closed record refuses any byte sequence
    // KEL-135 would refuse, without minting a validated profile identity.
    const NONCANONICAL: &str =
        "application identity must use KEL-135 canonical lowercase ASCII dot segments";
    if value.is_empty() || value.len() > 255 || !value.is_ascii() {
        return Err(invalid(NONCANONICAL));
    }
    let lower_alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    for segment in value.as_bytes().split(|byte| *byte == b'.') {
        let Some(first) = segment.first() else {
            return Err(invalid(NONCANONICAL));
        };
        let Some(last) = segment.last() else {
            return Err(invalid(NONCANONICAL));
        };
        if !lower_alphanumeric(*first)
            || !lower_alphanumeric(*last)
            || !segment
                .iter()
                .all(|byte| lower_alphanumeric(*byte) || *byte == b'-')
        {
            return Err(invalid(NONCANONICAL));
        }
    }
    Ok(())
}

pub(super) fn validate_sid(sid: &str) -> Result<(), UpdateError> {
    if sid.len() > 184 || !sid.is_ascii() || !sid.starts_with("S-1-") {
        return Err(invalid(
            "owner SID must be canonical ASCII within 184 bytes",
        ));
    }
    let fields: Vec<_> = sid.split('-').skip(2).collect();
    if !(2..=16).contains(&fields.len()) {
        return Err(invalid("owner SID has an invalid subauthority count"));
    }
    for (index, field) in fields.iter().enumerate() {
        if field.is_empty()
            || (field.len() > 1 && field.starts_with('0'))
            || !field.bytes().all(|digit| digit.is_ascii_digit())
        {
            return Err(invalid(
                "owner SID contains noncanonical decimal components",
            ));
        }
        let parsed = field
            .parse::<u64>()
            .map_err(|_| invalid("owner SID overflows"))?;
        if (index == 0 && parsed > 0xFFFF_FFFF_FFFF) || (index > 0 && parsed > u64::from(u32::MAX))
        {
            return Err(invalid("owner SID component is outside Windows bounds"));
        }
    }
    Ok(())
}

fn encode_bounded<T: Serialize>(wire: &T, limit: usize) -> Result<Vec<u8>, UpdateError> {
    let encoded = encode(wire)?;
    if encoded.len() > limit {
        return Err(invalid("repository record exceeds its versioned byte cap"));
    }
    Ok(encoded)
}

fn decode_bounded<T: serde::de::DeserializeOwned + Serialize>(
    bytes: &[u8],
    limit: usize,
) -> Result<T, UpdateError> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err(invalid(
            "repository record is empty or exceeds its byte cap",
        ));
    }
    decode(bytes)
}

#[cfg(test)]
mod fresh_v2_leaf;
#[cfg(test)]
mod tests;
