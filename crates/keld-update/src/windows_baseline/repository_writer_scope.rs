//! D-90 rejection of an installation-scoped KEL-53 writer as a repository writer.
//!
//! A `WindowsActivationWriteSnapshot` does hold the real protected KEL-53
//! `updates/activation.lock` and pinned installed roots. Its exclusive lock
//! serializes one installation's activation records. It DOES NOT serialize
//! independent v1/v2 installs, metadata enrollment, admission permits or
//! reader launches for the same repository. It must never mint the repository
//! eligibility witness from a single installation snapshot.

use super::repository_census::{RepositoryLedgerCensus, WindowsRepositoryEligibilityRequest};
use super::{WindowsActivationWriteSnapshot, error};
use crate::UpdateError;

/// Check the facts available from one live KEL-53 installation writer and refuse
/// to promote its activation lease into the missing repository-wide metadata lease.
///
/// This function uses the writer's already authenticated installed identity,
/// retained handles and exclusive install lease. It opens no path and creates no
/// new mutex. It can diagnose an actual protected app/channel mismatch before
/// the general D-90 completeness refusal.
///
/// The original writer has no repository ID, worker SID, authenticated closed
/// v1/v2 topology, cross-install journal/admitted-permit census, or approved
/// Host/Guard/launcher reader inventory. Fields in `request` are comparison
/// inputs only, never substitutes for those missing OS observations.
///
/// # Errors
/// Refuses missing commissioning, a mismatched protected source app/channel,
/// and every otherwise matching installation lease. Never yields repository
/// eligibility or grants the first installation effect.
pub(super) fn refuse_installation_lease_as_repository_owner(
    writer: &WindowsActivationWriteSnapshot,
    request: &WindowsRepositoryEligibilityRequest<'_>,
) -> Result<RepositoryLedgerCensus, UpdateError> {
    let installed = writer.identity();
    if installed.app_id != request.app_id {
        return Err(error(
            "repository writer app binding",
            "KEL-270 D-90 refuses a protected installation writer for a different application",
        ));
    }
    let Some(commissioned) = request.commissioned_channel else {
        return Err(error(
            "repository writer commissioning",
            "KEL-270 D-89 requires independent T-81 commissioning before repository admission",
        ));
    };
    if installed.channel != commissioned || request.requested_channel != commissioned {
        return Err(error(
            "repository writer channel",
            format!(
                "KEL-270 D-89 refuses protected installation channel {} outside commissioned {}",
                installed.channel.as_str(),
                commissioned.as_str(),
            ),
        ));
    }
    Err(error(
        "repository writer scope",
        "KEL-270 D-90 cannot treat an exclusive installation activation.lock as an existing repository metadata lease or a versioned complete member and source-reader census",
    ))
}
