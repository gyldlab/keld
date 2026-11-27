//! D-90 negative source-reader admission against the actual KEL-53 selected package.
//!
//! This is a read-only integration with KEL-53's existing retained installation
//! provenance and selected version handles. It does not enumerate a repository,
//! borrow a repository-wide metadata lease, or authorize a transition. The
//! present KEL-53 selector proves only ONE installed source. It cannot prove the
//! supported v1/v2 namespace universe, other installed members, admitted permits
//! or independently qualified Host/Guard/launcher fence behavior.
//!
//! Existing offline application boot does not call this check. It is for new
//! KEL-270 repository admission, including planned transition and manual recovery.

use super::repository_census::WindowsRepositoryOperation;
use super::{ActivePackageSelection, error};
use crate::{Channel, UpdateError};

/// Refuse new v2 authority when the inspected source was selected by the existing
/// KEL-53 installation owner but no complete repository reader proof exists.
///
/// The caller MUST obtain `source` through KEL-53's authenticated, pinned
/// selection, using `select_active_package_for_executable` for a running signed
/// image. Unlike a candidate's self-description, the channel and version below
/// come from admitted protected installation state. A caller-provided value
/// cannot change the observed source's channel.
///
/// This guard checks a *known source* only. Even exact channel equality does not
/// establish D-90 eligibility: a source's binary hash, its KEL-53 version pins,
/// and an absent activation journal do not show that every Host, Guard and
/// launcher understands the D-68 pointer, prepared journal and membership fences.
/// Nor can this one selection prove that all old admitted launches and browsers
/// are drained. Only the KEL-53 metadata owner, while retaining its one
/// repository lease and the versioned complete D-90 census, may eventually
/// construct an eligibility witness.
///
/// `commissioned_channel` is a comparison value from T-81; this function cannot
/// authenticate its origin and therefore NEVER returns success, including when
/// that value matches the actual selected channel.
///
/// # Errors
/// Refuses missing T-81 commissioning, a selected foreign channel, any installed
/// incumbent at fresh enrollment, or every unqualified old reader. The call
/// opens no new path, mutates no install or repository record and takes no lease.
pub(super) fn refuse_unqualified_selected_source(
    source: &ActivePackageSelection,
    commissioned_channel: Option<Channel>,
    operation: WindowsRepositoryOperation,
) -> Result<(), UpdateError> {
    let Some(commissioned) = commissioned_channel else {
        return Err(error(
            "repository source commissioning",
            "KEL-270 D-89 refuses an uncommissioned release channel before source-reader eligibility",
        ));
    };

    // This is the actual channel from KEL-53's pinned, protected source, not the
    // request's sourceChannel assertion. An equal policy cannot excuse a sibling.
    if source.install_identity().channel != commissioned {
        return Err(error(
            "repository source channel",
            format!(
                "KEL-270 D-89 refuses a protected installed source on channel {} outside the commissioned {} channel",
                source.install_identity().channel.as_str(),
                commissioned.as_str(),
            ),
        ));
    }

    if matches!(operation, WindowsRepositoryOperation::FreshEnrollment) {
        return Err(error(
            "repository fresh membership",
            "KEL-270 D-89 fresh enrollment refuses an existing protected installed source",
        ));
    }

    // The source identity and selected artifact are real KEL-53 observations,
    // but the old-reader capability set is not encoded in either. No version
    // hash or caller-supplied Boolean may turn this refusal into authorization.
    Err(error(
        "repository reader qualification",
        format!(
            "KEL-270 D-90 cannot qualify {} source version {} for {:?}: complete source Host/Guard/launcher fence versions, admission seal, process/browser drain and repository-wide writer custody are absent",
            source.install_identity().app_id,
            source.artifact().version,
            operation,
        ),
    ))
}
