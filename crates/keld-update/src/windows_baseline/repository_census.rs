//! KEL-270 D-89/D-90 admission boundary for the existing KEL-53 repository owner.
//!
//! The KEL-53 installation activation lease covers one installed update tree. It is
//! not the repository-wide metadata lease, and a directory discovery list is not a
//! complete repository membership census. The native T-82 repository owner has not
//! supplied a qualified, versioned producer on this implementation base. Therefore
//! this boundary must not mint eligibility, including for an apparently empty root
//! or for a source whose executable digest is known.

use crate::{Channel, UpdateError};

/// The operation for which repository membership and source-reader eligibility is needed.
///
/// Every variant must be checked under the SAME KEL-53 repository lease and pending
/// fence before an initial effect, after a wait, before commit, and again on restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowsRepositoryOperation {
    /// First enrollment, which requires proof of zero existing channel members.
    FreshEnrollment,
    /// Initial installation after metadata admission, with no installation effect yet.
    InitialInstallation,
    /// An established same-channel metadata or package update.
    OrdinaryUpdate,
    /// Recovery or selection of an exact installed last-known-good package.
    Rollback,
    /// A policy A-to-B change on the one commissioned release channel.
    PlannedTransition,
    /// A narrowly authorized D-65 manual recovery.
    ManualRecovery,
}

/// Claimed identity and channels for the D-89 preliminary refusal.
///
/// This is a request, NOT a native census, ownership attestation or permission.
/// The native KEL-53 producer must independently bind all these values to the
/// authenticated signed Host/repository policy and the actual admitted worker:
///
/// * The OS-captured owner SID, volume and retained repository/installation parent
///   handles, including every supported historical v1 and v2 root namespace and
///   explicit authenticated exclusions for managed/other roots.
/// * The exclusive repository metadata lease and pending-operation fence, held
///   across enrollment, relocation, retirement, permit issuance and rechecks.
/// * Protected current/LKG/previous-LKG provenance and all retained, journaled,
///   pending and already admitted permits; a filesystem scan alone omits owners.
/// * Complete source Host, Guard and launcher reader versions, independently
///   qualified to enforce pending-pointer, prepared-journal and membership fences.
/// * A seal on new ordinary launch admission and witnessed retirement/drain of
///   admitted old-generation launches through existing process/browser barriers,
///   re-established after any pre-switch restart.
///
/// None of these observations may be supplied by an untrusted caller-provided
/// `true`, an empty list, an image digest alone or a separate census mutex.
#[derive(Debug)]
pub(crate) struct WindowsRepositoryEligibilityRequest<'a> {
    /// Repository ID obtained from the authenticated Host and repository policy.
    pub repository_id: &'a [u8; 32],
    /// Exact authenticated application ID.
    pub app_id: &'a str,
    /// Canonical SID captured from the admitted Windows worker.
    pub owner_sid: &'a str,
    /// Release-wide channel only after the independent T-81 commissioning.
    pub commissioned_channel: Option<Channel>,
    /// Channel selected by the authenticated acquisition, never a request override.
    pub requested_channel: Channel,
    /// Source installed channel, if a source exists.
    pub source_channel: Option<Channel>,
    /// Destination installed channel, if a destination has been selected.
    pub destination_channel: Option<Channel>,
    /// The operation that needs authority.
    pub operation: WindowsRepositoryOperation,
}

/// KEL-53-native repository census and reader eligibility, retained for one operation.
///
/// Private and non-cloneable by design. Only a future KEL-53 native T-82 owner may
/// construct one after its versioned complete producer and real Windows tests are
/// independently qualified. Nothing on this implementation base constructs it.
#[derive(Debug)]
#[must_use = "the native writer must retain eligibility until the protected operation is finished"]
pub(crate) struct RepositoryLedgerCensus {
    _native_owner_only: (),
}

fn refuse(step: &'static str, detail: impl Into<String>) -> UpdateError {
    UpdateError::Baseline {
        step,
        detail: detail.into(),
    }
}

/// Applies D-89's cheap refusal and D-90's mandatory native completeness gate.
///
/// This is the only permitted v2 entry until the existing KEL-53 repository owner
/// implements the versioned complete producer. Equal channel strings, a valid
/// D-68 pointer, a lease on one installation, or a fully decoded record cannot
/// prove the entire repository and its eligible readers. The owner MUST implement
/// the complete proof inside its own lease and fence before this function can ever
/// return an eligibility value. No caller can mint or opt out of the result.
///
/// # Errors
/// Refuses missing commissioning, a channel mismatch, missing trusted identity,
/// or the unqualified native owner before any protected installation effect.
pub(crate) fn require_windows_repository_eligibility(
    request: &WindowsRepositoryEligibilityRequest<'_>,
) -> Result<RepositoryLedgerCensus, UpdateError> {
    if request.app_id.is_empty() || request.owner_sid.is_empty() {
        return Err(refuse(
            "repository identity",
            "D-90 requires the authenticated app ID and OS-captured owner SID",
        ));
    }
    let Some(commissioned) = request.commissioned_channel else {
        return Err(refuse(
            "repository release commissioning",
            "D-89 requires an independently authenticated release-wide T-81 channel",
        ));
    };
    if request.requested_channel != commissioned
        || request
            .source_channel
            .is_some_and(|source| source != commissioned)
        || request
            .destination_channel
            .is_some_and(|destination| destination != commissioned)
    {
        return Err(refuse(
            "repository channel mismatch",
            "D-89 refuses any request, source or destination outside the commissioned release channel",
        ));
    }
    if matches!(
        request.operation,
        WindowsRepositoryOperation::OrdinaryUpdate
            | WindowsRepositoryOperation::Rollback
            | WindowsRepositoryOperation::PlannedTransition
            | WindowsRepositoryOperation::ManualRecovery
    ) && request.source_channel.is_none()
    {
        return Err(refuse(
            "repository source membership",
            "D-90 requires authenticated source membership for an established operation",
        ));
    }
    if matches!(
        request.operation,
        WindowsRepositoryOperation::FreshEnrollment
    ) && request.source_channel.is_some()
    {
        return Err(refuse(
            "repository fresh membership",
            "D-89 fresh enrollment refuses an incumbent installed channel",
        ));
    }
    // The repository ID and operation are intentionally present in the request
    // but never authorize an empty census. T-82 must check them against retained
    // OS handles and every protected owner under its exclusive repository lease.
    Err(refuse(
        "repository census qualification",
        "KEL-270 D-90 requires a qualified versioned KEL-53 native repository member and source-reader producer; v1/v2 topology, pending permits, old readers and drain are unproven",
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        WindowsRepositoryEligibilityRequest, WindowsRepositoryOperation,
        require_windows_repository_eligibility,
    };
    use crate::{Channel, UpdateError};

    const REPOSITORY: [u8; 32] = [0x42; 32];

    fn request() -> WindowsRepositoryEligibilityRequest<'static> {
        WindowsRepositoryEligibilityRequest {
            repository_id: &REPOSITORY,
            app_id: "com.example.keld",
            owner_sid: "S-1-5-21-123-456-789-1001",
            commissioned_channel: Some(Channel::Stable),
            requested_channel: Channel::Stable,
            source_channel: Some(Channel::Stable),
            destination_channel: Some(Channel::Stable),
            operation: WindowsRepositoryOperation::PlannedTransition,
        }
    }

    fn refusal_step(request: &WindowsRepositoryEligibilityRequest<'_>) -> &'static str {
        match require_windows_repository_eligibility(request)
            .expect_err("no unqualified request can mint repository eligibility")
        {
            UpdateError::Baseline { step, .. } => step,
            other => panic!("unexpected repository error: {other}"),
        }
    }

    #[test]
    fn uncommissioned_or_different_channel_never_obtains_eligibility() {
        let mut attempt = request();
        attempt.commissioned_channel = None;
        assert_eq!(refusal_step(&attempt), "repository release commissioning");
        attempt.commissioned_channel = Some(Channel::Stable);
        attempt.requested_channel = Channel::Beta;
        assert_eq!(refusal_step(&attempt), "repository channel mismatch");
        attempt.requested_channel = Channel::Stable;
        attempt.source_channel = Some(Channel::Canary);
        assert_eq!(refusal_step(&attempt), "repository channel mismatch");
        attempt.source_channel = Some(Channel::Stable);
        attempt.destination_channel = Some(Channel::Beta);
        assert_eq!(refusal_step(&attempt), "repository channel mismatch");
    }

    #[test]
    fn missing_source_or_incumbent_fresh_install_refuses_before_census() {
        let mut attempt = request();
        attempt.source_channel = None;
        assert_eq!(refusal_step(&attempt), "repository source membership");
        attempt.operation = WindowsRepositoryOperation::FreshEnrollment;
        attempt.source_channel = Some(Channel::Stable);
        assert_eq!(refusal_step(&attempt), "repository fresh membership");
    }

    #[test]
    fn apparently_complete_channel_match_cannot_replace_native_census_and_reader_proof() {
        let mut attempt = request();
        for operation in [
            WindowsRepositoryOperation::InitialInstallation,
            WindowsRepositoryOperation::OrdinaryUpdate,
            WindowsRepositoryOperation::Rollback,
            WindowsRepositoryOperation::PlannedTransition,
            WindowsRepositoryOperation::ManualRecovery,
        ] {
            attempt.operation = operation;
            assert_eq!(refusal_step(&attempt), "repository census qualification");
        }
        attempt.operation = WindowsRepositoryOperation::FreshEnrollment;
        attempt.source_channel = None;
        assert_eq!(refusal_step(&attempt), "repository census qualification");
    }
}
