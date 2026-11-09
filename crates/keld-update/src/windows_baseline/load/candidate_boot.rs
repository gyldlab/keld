//! Candidate connect-back proof against one authenticated activation journal.
//!
//! The claimant owns process identity and image pins; this reader preserves
//! the original writer-lease exception and protected-record admission order.

use super::{
    Roots, error, read_records, read_version_completion, recovery_selection,
    validate_activation_version_census, validate_artifact_scope_and_baseline,
    validate_package_policy,
};
use crate::{UpdateError, records};

/// What the accepted connect-back claimant matched in its claim and observed of its
/// owner (KEL-53 §4 "Candidate connect-back", *After acceptance*).
pub(in super::super) struct AcceptedClaim<'a> {
    /// The attempt ID that the owner's `KELD-AC1` offered and the claim bound to the
    /// rendezvous name.
    pub(in super::super) attempt_id: &'a [u8; 32],
    /// The health-channel ID that the same `KELD-AC1` offered.
    pub(in super::super) health_channel_id: &'a [u8; 32],
    /// The process ID and creation time of the endpoint's server process.
    pub(in super::super) owner: crate::AttemptOwner,
    /// That server process's executable image.
    pub(in super::super) owner_image: &'a std::fs::File,
    /// The version whose tree holds the claimant's running executable.
    pub(in super::super) located_version: &'a str,
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
pub(in super::super) fn read_candidate_boot(
    roots: Roots,
    claim: &AcceptedClaim<'_>,
) -> Result<(super::super::ActivePackageSelection, [u8; 32]), UpdateError> {
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
    let selection = super::super::ActivePackageSelection {
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
    let image = super::super::image_blake3(claim.owner_image)
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
