//! Protected writer-state provenance, pointer and activation-record admission.
//!
//! This read-only owner validates exact recorded identity, floor, candidate
//! pointers and journal state; writer leases and publication stay with load.rs.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use super::{
    Roots, WindowsBaselineTrust, error, exact_entries, read_record, validate_selected_artifact,
};
use crate::records::{self, PointerKind};
use crate::{ProvenanceObservation, UpdateError, UpdateVerifier};

pub(super) struct WriterState {
    pub(super) admitted: crate::AdmittedInstallation,
    pub(super) version_floor: String,
    pub(super) current: crate::ArtifactIdentity,
    pub(super) last_known_good: crate::ArtifactIdentity,
    pub(super) previous_known_good: Option<crate::ArtifactIdentity>,
    pub(super) selected: Vec<crate::ArtifactIdentity>,
}

pub(super) struct WriterRecords {
    pub(super) version_floor: String,
    pub(super) current: crate::ArtifactIdentity,
    pub(super) last_known_good: crate::ArtifactIdentity,
    pub(super) previous_known_good: Option<crate::ArtifactIdentity>,
    pub(super) journal: Option<records::ActivationJournal>,
}

pub(super) fn load_writer_state(
    trust: &WindowsBaselineTrust,
    verifier: &UpdateVerifier,
    roots: &Roots,
) -> Result<WriterState, UpdateError> {
    let provenance = read_writer_provenance(trust, roots)?;
    let records = read_writer_records(roots, false)?;
    let version_floor = records.version_floor;
    let current = records.current;
    let last_known_good = records.last_known_good;
    let previous_known_good = records.previous_known_good;
    let observation = ProvenanceObservation::Protected {
        record: provenance.provenance,
        version_floor: Some(version_floor.clone()),
    };
    let admitted = verifier.admit(&observation)?;
    let floor =
        semver::Version::parse(&version_floor).map_err(|cause| error("version floor", cause))?;
    validate_writer_pointer_context(
        &trust.installation.baseline,
        &current,
        &last_known_good,
        previous_known_good.as_ref(),
        &floor,
    )?;
    let mut selected = vec![current.clone(), last_known_good.clone()];
    if let Some(previous) = &previous_known_good
        && !selected.contains(previous)
    {
        selected.push(previous.clone());
    }
    Ok(WriterState {
        admitted,
        version_floor,
        current,
        last_known_good,
        previous_known_good,
        selected,
    })
}

pub(super) fn read_writer_provenance(
    trust: &WindowsBaselineTrust,
    roots: &Roots,
) -> Result<records::ProvenanceRecord, UpdateError> {
    let install = roots
        .install()
        .map_err(|cause| error("install root", cause))?;
    let update_name = trust
        .installation
        .update_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| error("install topology", "update component absent"))?;
    exact_entries(install, &[update_name, "install-provenance"])
        .map_err(|cause| error("install contents", cause))?;
    let (provenance_file, provenance_bytes) =
        read_record(install, "install-provenance", roots.profile())?;
    drop(provenance_file);
    let provenance = records::decode_provenance(&provenance_bytes)?;
    crate::provenance::match_identity(&trust.installation, &provenance.provenance.identity)?;
    if provenance.publisher_scope != trust.publisher_scope
        || provenance.volume_guid != trust.volume_guid
    {
        return Err(error(
            "writer trust binding",
            "publisher or expected volume differs",
        ));
    }
    if provenance.provenance.owner != trust.owner {
        return Err(error(
            "writer owner binding",
            "protected owner differs from trusted deployment owner",
        ));
    }
    Ok(provenance)
}

/// Every protected activation record, with `current` kept as a result: an absent record
/// or one that does not decode as a canonical pointer is the one recoverable invalid
/// state. Every other fault, including an unreadable or unprotected record, is an error.
pub(super) struct RecordSet {
    pub(super) version_floor: String,
    pub(super) current: Result<crate::ArtifactIdentity, UpdateError>,
    pub(super) last_known_good: crate::ArtifactIdentity,
    pub(super) previous_known_good: Option<crate::ArtifactIdentity>,
    pub(super) journal: Option<records::ActivationJournal>,
}

/// Reads the records for the writer and recovery loaders, which require a valid `current`.
pub(super) fn read_writer_records(
    roots: &Roots,
    allow_pending_journal: bool,
) -> Result<WriterRecords, UpdateError> {
    let records = read_records(roots, allow_pending_journal)?;
    Ok(WriterRecords {
        version_floor: records.version_floor,
        current: records.current?,
        last_known_good: records.last_known_good,
        previous_known_good: records.previous_known_good,
        journal: records.journal,
    })
}

pub(super) fn read_records(
    roots: &Roots,
    allow_pending_journal: bool,
) -> Result<RecordSet, UpdateError> {
    let profile = roots.profile();
    let (_, floor_bytes) = read_record(&roots.update, "version-floor", profile)?;
    let version_floor = records::decode_floor(&floor_bytes)?;
    let (_, lkg_bytes) = read_record(&roots.update, "last-known-good", profile)?;
    let last_known_good = records::decode_pointer(PointerKind::LastKnownGood, &lkg_bytes)?;
    let entries = roots
        .update
        .entries()
        .map_err(|cause| error("writer update contents", cause))?;
    let names = entries
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|cause| error("writer update contents", cause))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    for name in &names {
        match name.to_str() {
            Some(
                "versions"
                | "activation.lock"
                | "version-floor"
                | "current"
                | "last-known-good"
                | "previous-known-good"
                | "bootstrap.lock",
            ) => {}
            Some("activation-journal") if allow_pending_journal => {}
            Some(name) if super::super::is_generated_leaf(name, "pending") => {}
            Some("activation-journal") => {
                return Err(error(
                    "activation recovery",
                    "pending journal requires the process-family recovery owner",
                ));
            }
            _ => return Err(error("writer update contents", "unknown state entry")),
        }
    }
    if names.contains(std::ffi::OsStr::new("bootstrap.lock")) {
        let (_, bytes) = read_record(&roots.update, "bootstrap.lock", profile)?;
        if !bytes.is_empty() {
            return Err(error("bootstrap marker", "bootstrap lock is not empty"));
        }
    }
    let previous_known_good = if names.contains(std::ffi::OsStr::new("previous-known-good")) {
        let (_, bytes) = read_record(&roots.update, "previous-known-good", profile)?;
        Some(records::decode_pointer(
            PointerKind::PreviousKnownGood,
            &bytes,
        )?)
    } else {
        None
    };
    let current = if names.contains(std::ffi::OsStr::new("current")) {
        let (_, bytes) = read_record(&roots.update, "current", profile)?;
        records::decode_pointer(PointerKind::Current, &bytes)
    } else {
        Err(error("current pointer", "the current record is absent"))
    };
    let journal = if names.contains(std::ffi::OsStr::new("activation-journal")) {
        let (_, bytes) = read_record(&roots.update, "activation-journal", profile)?;
        Some(records::decode_activation_journal(&bytes)?)
    } else {
        None
    };
    Ok(RecordSet {
        version_floor,
        current,
        last_known_good,
        previous_known_good,
        journal,
    })
}

pub(super) fn validate_writer_pointer_context(
    baseline: &crate::ArtifactIdentity,
    current: &crate::ArtifactIdentity,
    last_known_good: &crate::ArtifactIdentity,
    previous_known_good: Option<&crate::ArtifactIdentity>,
    floor: &semver::Version,
) -> Result<(), UpdateError> {
    if last_known_good != baseline && previous_known_good.is_none() {
        return Err(error(
            "previous-known-good pointer",
            "a successful update must retain its prior known-good artifact",
        ));
    }
    validate_selected_artifact(baseline, current, floor)?;
    validate_selected_artifact(baseline, last_known_good, floor)?;
    if current != last_known_good && previous_known_good.is_none_or(|previous| current != previous)
    {
        return Err(error(
            "current pointer",
            "current is not a known-good artifact",
        ));
    }
    if let Some(previous) = previous_known_good {
        validate_selected_artifact(baseline, previous, floor)?;
        if previous == last_known_good
            || semver::Version::parse(&previous.version)
                .map_err(|cause| error("previous-known-good version", cause))?
                .cmp_precedence(
                    &semver::Version::parse(&last_known_good.version)
                        .map_err(|cause| error("last-known-good version", cause))?,
                )
                != Ordering::Less
        {
            return Err(error(
                "previous-known-good pointer",
                "previous version is not strictly older than last-known-good",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_recovery_known_good_history(
    baseline: &crate::ArtifactIdentity,
    prior_last_known_good: &crate::ArtifactIdentity,
    prior_previous_known_good: Option<&crate::ArtifactIdentity>,
) -> Result<(), UpdateError> {
    if prior_last_known_good != baseline && prior_previous_known_good.is_none() {
        return Err(error(
            "activation journal previous-known-good history",
            "prior last-known-good differs from the installed baseline but has no prior previous-known-good",
        ));
    }
    Ok(())
}
