//! Single primary-role dialog custody. Routing and physical UI remain separate.

use super::{CorrelationId, PrimaryLinkIdentity};
use keld_ipc::window::WindowRef;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct DialogOwner {
    pub(super) identity: PrimaryLinkIdentity,
    pub(super) attempt: u32,
    pub(super) correlation: CorrelationId,
    pub(super) nonce: u64,
    pub(super) parent: Option<WindowRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReserveError {
    Busy,
    Stale,
    InvalidTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CancelResult {
    Requested,
    AlreadyCompleted,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalKind {
    Cancelled,
    NativeResult,
    #[cfg(target_os = "macos")]
    NativeFailure,
    Expired,
}

#[derive(Clone, Copy)]
struct Active {
    owner: DialogOwner,
    outcome: Option<TerminalKind>,
    terminal_taken: bool,
    quiesced: bool,
    retired: bool,
}

#[derive(Clone, Copy)]
struct Completed {
    identity: PrimaryLinkIdentity,
    attempt: u32,
    nonce: u64,
}

/// Only the authenticated primary route uses this owner. Its one retained panel
/// is below the host-wide cap; this is not a registry for future role families.
#[derive(Default)]
pub(super) struct DialogCustody {
    active: Option<Active>,
    last_completed: Option<Completed>,
}

impl DialogCustody {
    /// The one currently reserved effect. A native callback may only report
    /// this owner; neither a callback nor an untrusted Cancel mints it.
    #[cfg(target_os = "macos")]
    pub(super) fn active_owner(&self) -> Option<DialogOwner> {
        self.active.map(|active| active.owner)
    }
    #[cfg(target_os = "macos")]
    pub(super) fn original_pending_for(
        &self,
        identity: PrimaryLinkIdentity,
        attempt: u32,
    ) -> Option<CorrelationId> {
        self.active
            .filter(|active| {
                !active.retired
                    && !active.terminal_taken
                    && active.owner.identity == identity
                    && active.owner.attempt == attempt
            })
            .map(|active| active.owner.correlation)
    }
    pub(super) fn reserve(&mut self, owner: DialogOwner) -> Result<(), ReserveError> {
        if self.active.is_some() {
            return Err(ReserveError::Busy);
        }
        self.active = Some(Active {
            owner,
            outcome: None,
            terminal_taken: false,
            quiesced: false,
            retired: false,
        });
        Ok(())
    }

    pub(super) fn cancel(
        &mut self,
        identity: PrimaryLinkIdentity,
        attempt: u32,
        nonce: u64,
    ) -> CancelResult {
        if let Some(active) = self.active.as_mut().filter(|active| {
            active.owner.identity == identity
                && active.owner.attempt == attempt
                && active.owner.nonce == nonce
                && !active.retired
        }) {
            return match active.outcome {
                None => {
                    active.outcome = Some(TerminalKind::Cancelled);
                    CancelResult::Requested
                }
                Some(TerminalKind::Cancelled) => CancelResult::Requested,
                Some(TerminalKind::NativeResult | TerminalKind::Expired) => {
                    CancelResult::AlreadyCompleted
                }
                #[cfg(target_os = "macos")]
                Some(TerminalKind::NativeFailure) => CancelResult::AlreadyCompleted,
            };
        }
        if self.last_completed.is_some_and(|owner| {
            owner.identity == identity && owner.attempt == attempt && owner.nonce == nonce
        }) {
            CancelResult::AlreadyCompleted
        } else {
            CancelResult::NotFound
        }
    }

    pub(super) fn may_present(&self, owner: DialogOwner) -> bool {
        self.active.is_some_and(|active| {
            active.owner == owner && active.outcome.is_none() && !active.retired
        })
    }

    pub(super) fn commit_native_result(&mut self, owner: DialogOwner) -> bool {
        let Some(active) = self.active.as_mut().filter(|active| active.owner == owner) else {
            return false;
        };
        if active.outcome.is_some() || active.retired {
            return false;
        }
        active.outcome = Some(TerminalKind::NativeResult);
        true
    }

    /// A real native failure (not user dismissal) is its own terminal cause.
    #[cfg(target_os = "macos")]
    pub(super) fn commit_native_failure(&mut self, owner: DialogOwner) -> bool {
        let Some(active) = self.active.as_mut().filter(|active| active.owner == owner) else {
            return false;
        };
        if active.outcome.is_some() || active.retired {
            return false;
        }
        active.outcome = Some(TerminalKind::NativeFailure);
        true
    }

    /// An expired original is not an application's Cancel. Keep the native
    /// panel reserved until actual callback/visibility quiescence is observed.
    pub(super) fn expire(&mut self, owner: DialogOwner) -> bool {
        let Some(active) = self.active.as_mut().filter(|active| active.owner == owner) else {
            return false;
        };
        if active.outcome.is_some() || active.retired {
            return false;
        }
        active.outcome = Some(TerminalKind::Expired);
        true
    }

    pub(super) fn take_original_terminal(
        &mut self,
        owner: DialogOwner,
    ) -> Option<(CorrelationId, TerminalKind)> {
        let active = self
            .active
            .as_mut()
            .filter(|active| active.owner == owner)?;
        if active.retired || active.terminal_taken {
            return None;
        }
        let outcome = active.outcome?;
        active.terminal_taken = true;
        let result = (active.owner.correlation, outcome);
        self.release_if_drained();
        Some(result)
    }

    pub(super) fn retire(&mut self, identity: PrimaryLinkIdentity, attempt: u32) -> bool {
        if self
            .last_completed
            .is_some_and(|owner| owner.identity == identity && owner.attempt == attempt)
        {
            self.last_completed = None;
        }
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.owner.identity == identity && active.owner.attempt == attempt)
        else {
            return false;
        };
        active.retired = true;
        self.release_if_drained();
        true
    }

    pub(super) fn quiesced(&mut self, owner: DialogOwner) -> Result<(), ReserveError> {
        let Some(active) = self.active.as_mut().filter(|active| active.owner == owner) else {
            return Err(ReserveError::Stale);
        };
        if active.outcome.is_none() && !active.retired {
            return Err(ReserveError::InvalidTransition);
        }
        active.quiesced = true;
        self.release_if_drained();
        Ok(())
    }

    fn release_if_drained(&mut self) {
        if self
            .active
            .is_some_and(|active| active.quiesced && (active.terminal_taken || active.retired))
        {
            let completed = self.active.take().filter(|active| !active.retired);
            if let Some(active) = completed {
                self.last_completed = Some(Completed {
                    identity: active.owner.identity,
                    attempt: active.owner.attempt,
                    nonce: active.owner.nonce,
                });
            }
        }
    }
}
