//! Exact recovery decision owner for guardian generations.
use super::{Instant, Mutex, RuntimeError, io, lifecycle_error};

#[derive(Debug)]
pub(super) struct RecoveryState {
    pub(super) initial_ready: bool,
    pub(super) denied: bool,
    pub(super) retired_attempt: Option<u32>,
    pub(super) cleared_attempt: Option<u32>,
    pub(super) release_attempt: Option<u32>,
    pub(super) expected_attempt: u32,
    pub(super) shutdown_acknowledged: bool,
    /// Only the retained Child owner can mark this after observing terminal
    /// Guardian status. The worker may then fail closed if the native peer
    /// stays open past the unchanged shutdown budget.
    pub(super) guardian_reaped_shutdown_at: Option<Instant>,
}

impl RecoveryState {
    pub(super) const fn new() -> Self {
        Self {
            initial_ready: false,
            denied: false,
            retired_attempt: None,
            cleared_attempt: None,
            release_attempt: None,
            expected_attempt: 1,
            shutdown_acknowledged: false,
            guardian_reaped_shutdown_at: None,
        }
    }

    pub(super) fn arm(&mut self) {
        if self.denied {
            return;
        }
        if !self.initial_ready {
            self.initial_ready = true;
            return;
        }
        self.release_attempt = self
            .retired_attempt
            .and_then(|attempt| attempt.checked_add(1));
    }

    pub(super) fn deny(&mut self) {
        self.denied = true;
        self.release_attempt = None;
    }

    pub(super) fn retire(&mut self, attempt: u32) {
        self.retired_attempt = Some(attempt);
        self.cleared_attempt = None;
        self.release_attempt = None;
    }

    pub(super) fn cleared(&mut self, attempt: u32) -> Result<(), RuntimeError> {
        self.expected_attempt = attempt.checked_add(1).ok_or_else(|| {
            lifecycle_error(
                "macOS guardian generation host transition",
                io::Error::other("supervisor attempt counter exhausted"),
            )
        })?;
        self.cleared_attempt = Some(attempt);
        Ok(())
    }

    pub(super) fn permits(&self, attempt: u32) -> bool {
        !self.denied
            && self.expected_attempt == attempt
            && (attempt == 1
                || (self.initial_ready
                    && self.release_attempt == Some(attempt)
                    && self.cleared_attempt == self.retired_attempt
                    && self
                        .retired_attempt
                        .and_then(|retired| retired.checked_add(1))
                        == Some(attempt)))
    }
}

pub(super) fn lock_recovery(
    state: &Mutex<RecoveryState>,
) -> std::sync::MutexGuard<'_, RecoveryState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
