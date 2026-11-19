//! Native publication phase and independent physical custody.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PostPhase {
    Prepared,
    Cancelled,
    Posting,
    Returned,
    OutcomeUnknown,
}

#[derive(Debug)]
pub(super) enum PostResult {
    /// No API entry; keep the pending original operation or cancel it explicitly.
    NotAdmitted,
    /// The API returned; this never establishes the separate consumer receipt.
    Returned(windows::core::Result<()>),
    /// An earlier invocation returned; no API retry and no new result is implied.
    AlreadyReturned,
    /// Reentry/panic/owner failure: retain all custody and fail qualification.
    OutcomeUnknown,
}

impl PostPhase {
    pub(super) fn prior_attempt_result(self) -> Option<PostResult> {
        match self {
            Self::Prepared => None,
            Self::Cancelled => Some(PostResult::NotAdmitted),
            Self::Posting | Self::OutcomeUnknown => Some(PostResult::OutcomeUnknown),
            Self::Returned => Some(PostResult::AlreadyReturned),
        }
    }

    /// Irreversible no-entry cancellation on this same operation. A later
    /// source/role change cannot turn it back into a prepared publication.
    pub(super) fn cancel_unentered(&mut self) -> bool {
        if matches!(self, Self::Prepared | Self::Cancelled) {
            *self = Self::Cancelled;
            true
        } else {
            false
        }
    }

    /// An actual same Close may settle unknown native custody only outside the
    /// in-call phase. API return and consumer/logical terminal remain separate.
    pub(super) fn physically_quiescent(self, actual_closed: bool) -> bool {
        match self {
            Self::Prepared | Self::Cancelled | Self::Returned => true,
            Self::Posting => false,
            Self::OutcomeUnknown => actual_closed,
        }
    }
}

#[cfg(test)]
mod tests;
