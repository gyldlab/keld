//! Read-only initial-baseline metadata, topology, and recovery handoff.
//!
//! Admit an exact protected baseline; a journal or unrecognized state routes to
//! existing fail-closed recovery, never writer/activation authority.

use super::{
    BaselineCompletion, Roots, ensure_directory, error, exact_entries, read_baseline_completion,
    read_record,
};
use crate::UpdateError;
use crate::activation::{ProcessFamilyObservation, RecoverySnapshot, recovery_decision};
use crate::records::{self, PointerKind};
use cap_std::fs::File;

pub(super) struct InitialMetadata {
    pub(super) floor: String,
    pub(super) records: Vec<File>,
    pub(super) completion: BaselineCompletion,
    pub(super) lock_present: bool,
}

pub(super) fn read_initial_metadata(roots: &Roots) -> Result<InitialMetadata, UpdateError> {
    let baseline = &roots.trust.installation.baseline;
    let update_name = roots
        .trust
        .installation
        .update_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| error("seed topology", "update component absent"))?;
    exact_entries(
        roots
            .install()
            .map_err(|cause| error("install root", cause))?,
        &[update_name, "install-provenance"],
    )
    .map_err(|cause| error("initial install contents", cause))?;
    let mut lock_present = false;
    let mut activation_journal_present = false;
    let mut previous_known_good_present = false;
    for entry in roots
        .update
        .entries()
        .map_err(|cause| error("initial update contents", cause))?
    {
        let name = entry
            .map_err(|cause| error("initial update contents", cause))?
            .file_name();
        match name.to_str() {
            Some(
                "versions"
                | "activation.lock"
                | "version-floor"
                | "current"
                | "last-known-good"
                | "previous-known-good",
            ) => {
                if name.to_str() == Some("previous-known-good") {
                    previous_known_good_present = true;
                }
            }
            Some("activation-journal") => activation_journal_present = true,
            Some("bootstrap.lock") => lock_present = true,
            _ => {
                return Err(error(
                    "initial update contents",
                    "unknown state requires activation/recovery admission",
                ));
            }
        }
    }
    if activation_journal_present {
        return Err(classify_activation_recovery(
            roots,
            previous_known_good_present,
        ));
    }
    if previous_known_good_present {
        return Err(error(
            "initial previous-known-good",
            "a previous-known-good record without an activation journal is not baseline state",
        ));
    }
    validate_version_census(roots)?;
    let mut retained = Vec::new();
    if lock_present {
        let (lock, bytes) = read_record(&roots.update, "bootstrap.lock", roots.profile())?;
        if !bytes.is_empty() {
            return Err(error("initial lock", "bootstrap marker is not empty"));
        }
        retained.push(lock);
    }
    let (floor_file, bytes) = read_record(&roots.update, "version-floor", roots.profile())?;
    let floor = records::decode_floor(&bytes)?;
    if floor != baseline.version {
        return Err(error("initial floor", "floor differs from exact baseline"));
    }
    retained.push(floor_file);
    for (name, kind) in [
        ("current", PointerKind::Current),
        ("last-known-good", PointerKind::LastKnownGood),
    ] {
        let (file, bytes) = read_record(&roots.update, name, roots.profile())?;
        if records::decode_pointer(kind, &bytes)? != *baseline {
            return Err(error(
                "initial pointer",
                "pointer differs from exact baseline",
            ));
        }
        retained.push(file);
    }
    Ok(InitialMetadata {
        floor,
        records: retained,
        completion: read_baseline_completion(roots)?,
        lock_present,
    })
}

fn classify_activation_recovery(roots: &Roots, previous_known_good_present: bool) -> UpdateError {
    let decision = read_activation_recovery_decision(roots, previous_known_good_present);
    let decision = match decision {
        Ok(decision) => decision,
        Err(error) => return error,
    };
    error(
        "activation recovery handoff",
        format!(
            "the read-only baseline loader cannot apply this journal phase; common recovery decision {decision:?} requires the T4b writer and process-family owner"
        ),
    )
}

fn read_activation_recovery_decision(
    roots: &Roots,
    previous_known_good_present: bool,
) -> Result<crate::activation::RecoveryDecision, UpdateError> {
    let (_, journal_bytes) = read_record(&roots.update, "activation-journal", roots.profile())?;
    let journal = records::decode_activation_journal(&journal_bytes)?;
    let (_, floor_bytes) = read_record(&roots.update, "version-floor", roots.profile())?;
    let version_floor = records::decode_floor(&floor_bytes)?;
    let (_, current_bytes) = read_record(&roots.update, "current", roots.profile())?;
    let current = records::decode_pointer(PointerKind::Current, &current_bytes)?;
    let (_, last_known_good_bytes) =
        read_record(&roots.update, "last-known-good", roots.profile())?;
    let last_known_good =
        records::decode_pointer(PointerKind::LastKnownGood, &last_known_good_bytes)?;
    let previous_known_good = if previous_known_good_present {
        let (_, bytes) = read_record(&roots.update, "previous-known-good", roots.profile())?;
        Some(records::decode_pointer(
            PointerKind::PreviousKnownGood,
            &bytes,
        )?)
    } else {
        None
    };
    let snapshot = RecoverySnapshot {
        current,
        version_floor,
        last_known_good,
        previous_known_good,
        // This baseline reader owns no coordinator Job/process-family lease.
        // Unknown therefore refuses before it returns any boot selection.
        process_family: ProcessFamilyObservation::NotProvenDead,
    };
    Ok(recovery_decision(&journal, &snapshot))
}

fn validate_version_census(roots: &Roots) -> Result<(), UpdateError> {
    for entry in roots
        .versions
        .entries()
        .map_err(|cause| error("initial versions", cause))?
    {
        let entry = entry.map_err(|cause| error("initial versions", cause))?;
        let name = entry.file_name();
        if name.to_str() == Some(roots.trust.installation.baseline.version.as_str()) {
            continue;
        }
        let diagnostic = name
            .to_str()
            .is_some_and(|name| super::super::is_generated_leaf(name, "incomplete"));
        if !diagnostic {
            return Err(error(
                "initial versions",
                "only the exact baseline and named incomplete diagnostics are admitted",
            ));
        }
        // Windows DirEntry metadata comes from parent enumeration; do not open a
        // SYSTEM-private stage or inspect its contents merely to admit its name.
        let metadata = entry
            .metadata()
            .map_err(|cause| error("diagnostic directory metadata", cause))?;
        ensure_directory(&metadata).map_err(|cause| error("diagnostic directory kind", cause))?;
    }
    Ok(())
}
