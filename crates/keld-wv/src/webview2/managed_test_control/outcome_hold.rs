//! Finite X10 scheduling metadata for the existing app-wake worker.
//! Payloads remain in its original receiver/current owner; this grants no credit.
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::native_hold::control_file_exists;
use super::{TEST_WINDOWS_NATIVE_HOLD_ENV, invalid_hook};
use crate::WvError;

// Test-only scheduling bound; product deadlines are unchanged.
const OUTCOME_HOLD_BUDGET: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::webview2) enum HoldMode {
    BeforeIntake,
    AfterNextEvent,
}
impl HoldMode {
    const fn name(self) -> &'static str {
        match self {
            Self::BeforeIntake => "before-intake",
            Self::AfterNextEvent => "after-next-event",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Waiting,
    Arming,
    Armed,
    Holding,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::webview2) struct EventKey {
    pub(in crate::webview2) webview: u32,
    pub(in crate::webview2) navigation: u64,
    pub(in crate::webview2) delivery: u32,
}

pub(in crate::webview2) struct OutcomeHold {
    directory: PathBuf,
    mode: HoldMode,
    phase: Phase,
    deadline: Option<Instant>,
    event: Option<EventKey>,
    just_armed: bool,
    just_held: bool,
    just_released: bool,
}

impl OutcomeHold {
    pub(in crate::webview2) fn new(directory: PathBuf, mode: HoldMode) -> Self {
        Self {
            directory,
            mode,
            phase: Phase::Waiting,
            deadline: None,
            event: None,
            just_armed: false,
            just_held: false,
            just_released: false,
        }
    }

    /// Called only by the existing worker, outside every input/Core/native guard.
    pub(in crate::webview2) fn poll(&mut self) -> Result<(), WvError> {
        match self.phase {
            Phase::Released => return Ok(()),
            Phase::Waiting => {
                if control_file_exists(&self.directory.join("release"))? {
                    return Err(failure("release appeared before outcome hold was armed"));
                }
                if control_file_exists(&self.directory.join("arm"))? {
                    // One budget starts before the fixture submits its event.
                    // Product deadlines are unchanged and this never resets.
                    self.deadline = Some(Instant::now() + OUTCOME_HOLD_BUDGET);
                    self.phase = Phase::Arming;
                }
                return Ok(());
            }
            Phase::Arming | Phase::Armed | Phase::Holding => {}
        }
        if self
            .deadline
            .is_none_or(|deadline| Instant::now() >= deadline)
        {
            return Err(failure("original outcome hold budget expired"));
        }
        if control_file_exists(&self.directory.join("release"))? {
            if self.phase != Phase::Holding {
                return Err(failure("release appeared before the selected outcome cut"));
            }
            self.phase = Phase::Released;
            self.just_released = true;
        }
        Ok(())
    }

    /// Pure metadata under the input guard; an occupied current is not bypassed.
    pub(in crate::webview2) fn before_turn(&mut self, current_empty: bool) {
        if self.phase != Phase::Arming || !current_empty {
            return;
        }
        self.just_armed = true;
        self.phase = match self.mode {
            HoldMode::BeforeIntake => {
                self.just_held = true;
                Phase::Holding
            }
            HoldMode::AfterNextEvent => Phase::Armed,
        };
    }

    /// Scalars from the actual Event already attached to the original current.
    /// The next receive is held before that Event's unit wake can be posted.
    pub(in crate::webview2) fn selected_event(&mut self, event: Option<EventKey>) {
        if self.phase != Phase::Armed {
            return;
        }
        if let Some(event) = event {
            self.event = Some(event);
            self.phase = Phase::Holding;
            self.just_held = true;
        }
    }

    pub(in crate::webview2) fn is_holding(&self) -> bool {
        self.phase == Phase::Holding
    }

    /// Trace only after all input/Core/native guards end. These rows prove a
    /// receiver cut, never Core admission, native entry, consumption or Close.
    pub(in crate::webview2) fn observe(&mut self) {
        if self.just_armed {
            self.just_armed = false;
            eprintln!(
                "KELD_X10_OUTCOME_HOLD phase=armed mode={}",
                self.mode.name()
            );
        }
        if self.just_held {
            self.just_held = false;
            if let Some(event) = self.event {
                eprintln!(
                    "KELD_X10_OUTCOME_HOLD phase=held mode={} webview={} navigation={} delivery={}",
                    self.mode.name(),
                    event.webview,
                    event.navigation,
                    event.delivery
                );
            } else {
                eprintln!("KELD_X10_OUTCOME_HOLD phase=held mode={}", self.mode.name());
            }
        }
        if self.just_released {
            self.just_released = false;
            eprintln!(
                "KELD_X10_OUTCOME_HOLD phase=released mode={}",
                self.mode.name()
            );
        }
    }
}

fn failure(detail: &'static str) -> WvError {
    invalid_hook(TEST_WINDOWS_NATIVE_HOLD_ENV, detail)
}

#[cfg(test)]
#[path = "tests/outcome_hold.rs"]
mod tests;
