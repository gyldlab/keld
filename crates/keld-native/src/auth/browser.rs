//! Private cold macOS system-browser launch owner for KEL-89/T3.
//! The auth worker retains this owner until Complete, including fatal cleanup.
//! This owns the launch helper, not the OS-selected external browser process.

#![forbid(unsafe_code)]

use super::primitives::{MAX_OAUTH_TEXT_BYTES, OutgoingText};

use std::fmt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Absolute bounds chosen once by the existing auth worker.
#[derive(Clone, Copy, Debug)]
pub(super) struct BrowserDeadlines {
    pub(super) launch: Instant,
    pub(super) cleanup: Instant,
}
#[derive(Clone, Copy, Debug)]
enum Failure {
    Launch,
    Deadline,
    Cancelled,
    Lifecycle,
}
/// Typed redacted launch failure with separate cleanup facts.
pub(super) struct BrowserError {
    failure: Failure,
    pub(super) exit_status: Option<ExitStatus>,
    pub(super) kill_failed: bool,
    pub(super) observe_failed: bool,
    pub(super) cleanup_unproven: bool,
}
impl BrowserError {
    fn new(failure: Failure) -> Self {
        Self {
            failure,
            exit_status: None,
            kill_failed: false,
            observe_failed: false,
            cleanup_unproven: false,
        }
    }
    /// Error code first on the owning broker's reply envelope.
    pub(super) fn code(&self) -> &'static str {
        match self.failure {
            Failure::Launch => "KELD-AUTH-004",
            Failure::Deadline => "KELD-AUTH-008",
            Failure::Cancelled => "KELD-AUTH-007",
            Failure::Lifecycle => "KELD-AUTH-016",
        }
    }
}
impl fmt::Debug for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrowserError")
            .field("code", &self.code())
            .field("exit_status", &self.exit_status)
            .field("kill_failed", &self.kill_failed)
            .field("observe_failed", &self.observe_failed)
            .field("cleanup_unproven", &self.cleanup_unproven)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.failure {
            Failure::Launch => f.write_str("KELD-AUTH-004: the system-browser launch helper failed. Configure a default browser and restore the macOS URL-opening facility, then start a fresh user-initiated auth.begin.")?,
            Failure::Deadline => f.write_str("KELD-AUTH-008: the browser launch did not complete within the absolute host bound. Start a fresh user-initiated auth.begin; no automatic retry occurs.")?,
            Failure::Cancelled => f.write_str("KELD-AUTH-007: the owning browser launch was cancelled. Start a fresh user-initiated auth.begin; no automatic retry occurs.")?,
            Failure::Lifecycle => f.write_str("KELD-AUTH-016: the host browser-launch lifecycle could not be proved. Retain the launch owner through fatal cleanup and relaunch the host before a fresh sign-in.")?,
        }
        if self.observe_failed {
            f.write_str(" The owned helper exit observation failed.")?;
        }
        if self.kill_failed {
            f.write_str(" The owned helper stop request failed.")?;
        }
        if self.cleanup_unproven {
            f.write_str(" Helper reaping was not proved at the cleanup bound.")?;
        }
        Ok(())
    }
}
impl std::error::Error for BrowserError {}
/// `CleanupUnproven` is nonterminal: keep the `Child` and poll it through fatal cleanup.
pub(super) enum BrowserProgress {
    Pending,
    Complete(Result<ExitStatus, BrowserError>),
    CleanupUnproven(BrowserError),
}
// The state owns the handle: a reaped launch cannot retain a live Child.
enum ChildState {
    Running(Child),
    Reaped,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum StopState {
    NotRequested,
    Requested,
    Failed,
}
/// One launch helper, no worker, capture pipes, restart or PID-based authority.
pub(super) struct BrowserLaunch {
    child: ChildState,
    deadlines: BrowserDeadlines,
    cancelled: Arc<AtomicBool>,
    failure: Option<Failure>,
    stop: StopState,
    observe_failed: bool,
    cleanup_reported: bool,
}
impl fmt::Debug for BrowserLaunch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrowserLaunch")
            .field("child_owned", &matches!(self.child, ChildState::Running(_)))
            .field("cleanup_reported", &self.cleanup_reported)
            .field("consumed", &matches!(self.child, ChildState::Reaped))
            .finish_non_exhaustive()
    }
}
impl BrowserLaunch {
    /// Cold worker-only spawn. URL is the existing generated authorization text.
    /// No shell or browser wait flag; the raw URL is never reformatted or logged.
    pub(super) fn start(
        url: &OutgoingText,
        deadlines: BrowserDeadlines,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, BrowserError> {
        if url.as_str().len() > MAX_OAUTH_TEXT_BYTES || !url.as_str().starts_with("https://") {
            return Err(BrowserError::new(Failure::Lifecycle));
        }
        let mut command = Command::new("/usr/bin/open");
        command
            .arg("-u")
            .arg(url.as_str())
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Self::start_command(command, deadlines, cancelled)
    }
    fn start_command(
        mut command: Command,
        deadlines: BrowserDeadlines,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, BrowserError> {
        if deadlines.cleanup < deadlines.launch {
            return Err(BrowserError::new(Failure::Lifecycle));
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(BrowserError::new(Failure::Cancelled));
        }
        if Instant::now() >= deadlines.launch {
            return Err(BrowserError::new(Failure::Deadline));
        }
        // std::Command::spawn itself is a cold OS call, not advertised as an
        // interruptible actor operation. The auth worker owns this call.
        let child = command
            .spawn()
            .map_err(|_| BrowserError::new(Failure::Launch))?;
        Ok(Self {
            child: ChildState::Running(child),
            deadlines,
            cancelled,
            failure: None,
            stop: StopState::NotRequested,
            observe_failed: false,
            cleanup_reported: false,
        })
    }
    /// Stops only this retained launch helper; the external browser is OS-owned.
    pub(super) fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.failure.get_or_insert(Failure::Cancelled);
        self.request_stop();
    }
    fn request_stop(&mut self) {
        if self.stop == StopState::NotRequested {
            self.stop = StopState::Requested;
            if let ChildState::Running(child) = &mut self.child
                && child.kill().is_err()
            {
                self.stop = StopState::Failed;
            }
        }
    }
    fn error(&self, failure: Failure, exit_status: Option<ExitStatus>) -> BrowserError {
        BrowserError {
            failure,
            exit_status,
            kill_failed: self.stop == StopState::Failed,
            observe_failed: self.observe_failed,
            cleanup_unproven: self.cleanup_reported,
        }
    }
    /// One nonblocking kernel exit observation per poll; never calls `wait()`.
    /// Only `try_wait(Some)` proves this owned helper has been reaped.
    pub(super) fn poll(&mut self) -> BrowserProgress {
        if matches!(self.child, ChildState::Reaped) {
            return BrowserProgress::Complete(Err(BrowserError::new(Failure::Lifecycle)));
        }
        let now = Instant::now();
        if self.cancelled.load(Ordering::Acquire) {
            self.failure.get_or_insert(Failure::Cancelled);
            self.request_stop();
        } else if now >= self.deadlines.launch {
            self.failure.get_or_insert(Failure::Deadline);
            self.request_stop();
        }
        let ChildState::Running(child) = &mut self.child else {
            return BrowserProgress::Complete(Err(BrowserError::new(Failure::Lifecycle)));
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.child = ChildState::Reaped;
                if let Some(failure) = self.failure {
                    BrowserProgress::Complete(Err(self.error(failure, Some(status))))
                } else if status.success() {
                    BrowserProgress::Complete(Ok(status))
                } else {
                    BrowserProgress::Complete(Err(self.error(Failure::Launch, Some(status))))
                }
            }
            Ok(None) => self.pending(now),
            Err(_) => {
                self.observe_failed = true;
                self.failure.get_or_insert(Failure::Lifecycle);
                self.request_stop();
                self.pending(now)
            }
        }
    }
    fn pending(&mut self, now: Instant) -> BrowserProgress {
        if now >= self.deadlines.cleanup && !self.cleanup_reported {
            self.cleanup_reported = true;
            let failure = *self.failure.get_or_insert(Failure::Lifecycle);
            self.request_stop();
            BrowserProgress::CleanupUnproven(self.error(failure, None))
        } else {
            BrowserProgress::Pending
        }
    }
}
impl Drop for BrowserLaunch {
    fn drop(&mut self) {
        if matches!(self.child, ChildState::Running(_)) {
            self.request_stop();
        }
        // Emergency stop only, NOT a reap receipt. Broker teardown MUST retain
        // this owner until Complete, including CleanupUnproven/fatal cleanup;
        // ordinary Drop must not be used as the terminal cleanup path.
    }
}
#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
