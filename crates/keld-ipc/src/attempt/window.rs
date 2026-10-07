//! The owner's health window (KEL-53 §4 "Candidate connect-back", *Health
//! sequence*; approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06).
//!
//! The owner accepts health only when, 30 monotonic seconds plus a margin G
//! after it read `KELD-AY1`, the launch handle is unsignaled, the connection
//! is open and no further byte has arrived. This module is the decision as a
//! pure state machine over an injected clock: the caller passes every
//! `Instant`, so the exact window edges are tested without a real wait, and
//! the pipe driver (`channel.rs`) only feeds it what the deadline-bounded read
//! and the launch-handle query observed.
//!
//! G is the host's measured revocation-to-close latency, which KEL-53 §6 S6c
//! measures and fixes. Until then no constant names it: the owner passes its
//! margin to [`super::WindowsAttemptOwnerChannel::await_health_window`].

use std::time::{Duration, Instant};

/// The fixed part of the health window: 30 monotonic seconds after the owner
/// reads `KELD-AY1`. The whole window is this plus the margin G.
pub const ATTEMPT_HEALTH_WINDOW: Duration = Duration::from_secs(30);

/// Why the owner's health window refused health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptHealthWindowFailure {
    /// A byte arrived after `KELD-AY1`; the candidate sends nothing after it.
    ByteReceived,
    /// The candidate's end of the connection closed during the window.
    EndOfFile,
    /// At the end of the window the launch handle was signaled: the launched
    /// process had exited.
    LaunchExited,
    /// At the end of the window the launch handle's state could not be read,
    /// so nothing proves the launched process was still running.
    LaunchUnverifiable,
    /// The window's end does not fit the monotonic clock (an unbounded margin).
    Unrepresentable,
}

/// What one deadline-bounded read of the connection observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowRead {
    /// At least one byte arrived.
    Byte,
    /// The candidate's end closed.
    EndOfFile,
    /// The read's deadline elapsed with nothing received.
    DeadlineElapsed,
}

/// The driver's next action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowStep {
    /// Read the connection with this absolute deadline.
    Read {
        /// The window's end.
        until: Instant,
    },
    /// The window has ended with the connection open and silent: query the
    /// launch handle once and pass the result to
    /// [`HealthWindow::after_launch_check`].
    CheckLaunch,
    /// Health is refused.
    Refuse(AttemptHealthWindowFailure),
}

/// One health window, fixed when the owner reads `KELD-AY1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HealthWindow {
    ends: Instant,
}

impl HealthWindow {
    /// The health window that starts when the owner read `KELD-AY1`:
    /// [`ATTEMPT_HEALTH_WINDOW`] plus `margin`, the caller's G.
    ///
    /// # Errors
    ///
    /// [`AttemptHealthWindowFailure::Unrepresentable`] when its length or end
    /// does not fit the monotonic clock.
    pub(super) fn after_ready(
        ready_read_at: Instant,
        margin: Duration,
    ) -> Result<Self, AttemptHealthWindowFailure> {
        ATTEMPT_HEALTH_WINDOW
            .checked_add(margin)
            .ok_or(AttemptHealthWindowFailure::Unrepresentable)
            .and_then(|length| Self::of_length(ready_read_at, length))
    }

    /// A window of the whole `length` from `ready_read_at`; tests shorten the
    /// window with it.
    ///
    /// # Errors
    ///
    /// As [`Self::after_ready`].
    pub(super) fn of_length(
        ready_read_at: Instant,
        length: Duration,
    ) -> Result<Self, AttemptHealthWindowFailure> {
        ready_read_at
            .checked_add(length)
            .map(|ends| Self { ends })
            .ok_or(AttemptHealthWindowFailure::Unrepresentable)
    }

    /// The first step at `now`: read until the end, or, if the window has
    /// already ended, check the launch handle.
    pub(super) fn begin(self, now: Instant) -> WindowStep {
        if now < self.ends {
            WindowStep::Read { until: self.ends }
        } else {
            WindowStep::CheckLaunch
        }
    }

    /// The step after a read that returned at `now`. Any byte or end of file
    /// refuses, whenever it is observed: the window admits neither. A read
    /// whose deadline elapsed before the end (a wait may wake early) reads
    /// again; one that elapsed at or after the end checks the launch handle.
    pub(super) fn after_read(self, read: WindowRead, now: Instant) -> WindowStep {
        match read {
            WindowRead::Byte => WindowStep::Refuse(AttemptHealthWindowFailure::ByteReceived),
            WindowRead::EndOfFile => WindowStep::Refuse(AttemptHealthWindowFailure::EndOfFile),
            WindowRead::DeadlineElapsed => self.begin(now),
        }
    }

    /// The decision after the launch-handle query at the window's end:
    /// `Some(true)` signaled, `Some(false)` unsignaled, `None` unreadable.
    ///
    /// # Errors
    ///
    /// [`AttemptHealthWindowFailure::LaunchExited`] or
    /// [`AttemptHealthWindowFailure::LaunchUnverifiable`].
    pub(super) const fn after_launch_check(
        launch_exited: Option<bool>,
    ) -> Result<(), AttemptHealthWindowFailure> {
        match launch_exited {
            Some(false) => Ok(()),
            Some(true) => Err(AttemptHealthWindowFailure::LaunchExited),
            None => Err(AttemptHealthWindowFailure::LaunchUnverifiable),
        }
    }
}

#[cfg(test)]
mod tests;
