//! The accepted `keld-attempt` connection after `KELD-AR1`: the health records
//! and the health result (KEL-53 §4 "Candidate connect-back", *Health
//! records* and *Health sequence*; approved: KEL-270 owner decision
//! `eff8e2fb`, 2026-10-06).
//!
//! Each end owns its stream and never lends it. The owner reads `KELD-AB1`,
//! then `KELD-AY1` (or `KELD-AF1`), each magic first and admitted by position,
//! then waits out the health window and either writes `KELD-AK1` accepted and
//! waits for the candidate's end of file before it closes, or writes
//! `KELD-AK1` rolled back once and closes without waiting. The candidate
//! writes `KELD-AB1` and `KELD-AY1` (or `KELD-AF1`), then reads exactly one
//! `KELD-AK1` under a deadline and closes; only `KELD-AK1` accepted is
//! success.
//!
//! The order is a state machine on each end: a step called out of order is
//! refused before any I/O, and after any failure only the rollback remains.

use std::fmt;
use std::io::{self, Read as _};
#[cfg(test)]
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::ERROR_SEM_TIMEOUT;

use super::WindowsAttemptEndpointError;
use super::records::{
    AttemptBootAcknowledgement, AttemptFailureClass, AttemptHealthResult, AttemptReadPosition,
    AttemptRecord, AttemptRecordError, AttemptTranscript,
};
use super::window::{AttemptHealthWindowFailure, HealthWindow, WindowRead, WindowStep};
use crate::APP_LINK_IO_DEADLINE;
use crate::bootstrap::WindowsLifecyclePeerPin;
use crate::windows_named_pipe::WindowsNamedPipeStream;

/// The owner's end of an accepted connect-back connection: the claimant was
/// admitted, the one-shot is consumed and `KELD-AR1` was sent.
///
/// It retains the claimant's process pin from the caller's check: the window
/// asks that pin whether the launched process has exited. Dropping the channel
/// closes the connection.
#[derive(Debug)]
pub struct WindowsAttemptOwnerChannel<P> {
    stream: WindowsNamedPipeStream,
    pin: P,
    transcript: AttemptTranscript,
    phase: OwnerPhase,
    #[cfg(test)]
    close_wait_entered: Option<mpsc::Sender<()>>,
}

/// Where the owner is in the health sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnerPhase {
    /// `KELD-AR1` sent; `KELD-AB1` is next.
    Accepted,
    /// `KELD-AB1` matched; `KELD-AY1` is next.
    Booted,
    /// `KELD-AY1` read at `read_at`; the window is next.
    Ready { read_at: Instant },
    /// The window passed; only `KELD-AK1` remains.
    Healthy,
    /// The exchange failed; only the rollback remains.
    Ended { end_of_file: bool },
}

impl OwnerPhase {
    const fn describe(self) -> &'static str {
        match self {
            Self::Accepted => "after KELD-AR1",
            Self::Booted => "after KELD-AB1",
            Self::Ready { .. } => "after KELD-AY1",
            Self::Healthy => "after the health window",
            Self::Ended { .. } => "after a failure",
        }
    }
}

impl<P: WindowsLifecyclePeerPin> WindowsAttemptOwnerChannel<P> {
    pub(super) const fn accepted(
        stream: WindowsNamedPipeStream,
        pin: P,
        transcript: AttemptTranscript,
    ) -> Self {
        Self {
            stream,
            pin,
            transcript,
            phase: OwnerPhase::Accepted,
            #[cfg(test)]
            close_wait_entered: None,
        }
    }

    /// The accepted claim transcript: both IDs, nonces and process IDs.
    #[must_use]
    pub const fn transcript(&self) -> &AttemptTranscript {
        &self.transcript
    }

    /// The claimant's process pin that the caller's check returned.
    #[must_use]
    pub const fn process_pin(&self) -> &P {
        &self.pin
    }

    /// Reads the candidate's first health record under `deadline`, magic
    /// first: `KELD-AB1`, which must equal the record the owner builds from the
    /// accepted attempt and health-channel IDs and `health_receipt_digest`,
    /// the §4 health-receipt digest that `keld-update` recomputes from the
    /// journal; or `KELD-AF1` with class `1` or `3`.
    ///
    /// # Errors
    ///
    /// - [`WindowsAttemptExchangeError::CandidateFailure`] for `KELD-AF1`;
    /// - [`WindowsAttemptExchangeError::Record`] for a refused, mismatched or
    ///   truncated record, end of file or the deadline;
    /// - [`WindowsAttemptExchangeError::OutOfSequence`] unless this is the
    ///   first step after the claim.
    ///
    /// After any error only [`Self::roll_back`] remains.
    pub fn read_boot(
        &mut self,
        health_receipt_digest: &[u8; 32],
        deadline: Instant,
    ) -> Result<(), WindowsAttemptExchangeError> {
        if self.phase != OwnerPhase::Accepted {
            return Err(self.out_of_sequence("read KELD-AB1"));
        }
        let expected = AttemptBootAcknowledgement::new(
            *self.transcript.attempt_id(),
            *self.transcript.health_channel_id(),
            *health_receipt_digest,
        );
        let position = AttemptReadPosition::OwnerBoot;
        let read = self
            .read_record(position, deadline)
            .and_then(|record| match record {
                AttemptRecord::BootAcknowledgement(received) => expected
                    .require_match(&received)
                    .map_err(WindowsAttemptExchangeError::Record),
                AttemptRecord::Failure(class) => {
                    Err(WindowsAttemptExchangeError::CandidateFailure { class })
                }
                other => Err(other.not_admitted_at(position).into()),
            });
        self.settle(read, OwnerPhase::Booted)
    }

    /// Reads the candidate's second health record under `deadline`, magic
    /// first: `KELD-AY1`, which starts the health window at the instant it is
    /// read; or `KELD-AF1` with class `2` or `3`.
    ///
    /// # Errors
    ///
    /// As [`Self::read_boot`]; out of sequence unless `KELD-AB1` was read.
    pub fn read_ready(&mut self, deadline: Instant) -> Result<(), WindowsAttemptExchangeError> {
        if self.phase != OwnerPhase::Booted {
            return Err(self.out_of_sequence("read KELD-AY1"));
        }
        let position = AttemptReadPosition::OwnerReady;
        let read = self
            .read_record(position, deadline)
            .and_then(|record| match record {
                AttemptRecord::Ready => Ok(Instant::now()),
                AttemptRecord::Failure(class) => {
                    Err(WindowsAttemptExchangeError::CandidateFailure { class })
                }
                other => Err(other.not_admitted_at(position).into()),
            });
        match read {
            Ok(read_at) => {
                self.phase = OwnerPhase::Ready { read_at };
                Ok(())
            }
            Err(error) => {
                self.end(&error);
                Err(error)
            }
        }
    }

    /// Waits out the health window,
    /// [`ATTEMPT_HEALTH_WINDOW`](super::ATTEMPT_HEALTH_WINDOW) plus `margin`
    /// after the owner read `KELD-AY1`. Health passes only if, at the window's
    /// end, the connection is open, no byte arrived after `KELD-AY1` and the
    /// claimant's pin reports the launched process still running.
    ///
    /// `margin` is G, the host's measured revocation-to-close latency (KEL-53
    /// §4 *Health sequence*): an exit whose close reaches the owner later than
    /// G after the window's 30 seconds can still commit. KEL-53 §6 S6c measures
    /// G; this module names no value for it.
    ///
    /// # Errors
    ///
    /// - [`WindowsAttemptExchangeError::HealthWindow`] for a byte, end of
    ///   file, a signaled or unreadable launch state, or a window whose end
    ///   does not fit the clock;
    /// - [`WindowsAttemptExchangeError::Record`] for any other read failure;
    /// - [`WindowsAttemptExchangeError::OutOfSequence`] unless `KELD-AY1` was
    ///   read.
    pub fn await_health_window(
        &mut self,
        margin: Duration,
    ) -> Result<(), WindowsAttemptExchangeError> {
        self.await_window(|read_at| HealthWindow::after_ready(read_at, margin))
    }

    /// [`Self::await_health_window`] with the whole window's length, so a test
    /// can observe the window without its 30 seconds.
    #[cfg(test)]
    pub(super) fn await_window_of(
        &mut self,
        length: Duration,
    ) -> Result<(), WindowsAttemptExchangeError> {
        self.await_window(|read_at| HealthWindow::of_length(read_at, length))
    }

    fn await_window(
        &mut self,
        window: impl FnOnce(Instant) -> Result<HealthWindow, AttemptHealthWindowFailure>,
    ) -> Result<(), WindowsAttemptExchangeError> {
        let OwnerPhase::Ready { read_at } = self.phase else {
            return Err(self.out_of_sequence("await the health window"));
        };
        let decided = window(read_at)
            .map_err(window_refusal)
            .and_then(|window| self.decide_window(window));
        self.settle(decided, OwnerPhase::Healthy)
    }

    /// Drives [`HealthWindow`] with the deadline-bounded read and the pin.
    fn decide_window(&mut self, window: HealthWindow) -> Result<(), WindowsAttemptExchangeError> {
        let mut step = window.begin(Instant::now());
        loop {
            step = match step {
                WindowStep::Read { until } => {
                    self.stream.set_absolute_deadline(Some(until));
                    let mut byte = [0_u8; 1];
                    let read = match self.stream.read(&mut byte) {
                        Ok(0) => WindowRead::EndOfFile,
                        Ok(_) => WindowRead::Byte,
                        Err(source) if is_deadline(&source) => WindowRead::DeadlineElapsed,
                        Err(source) => {
                            return Err(WindowsAttemptExchangeError::Record(
                                AttemptRecordError::Io { source },
                            ));
                        }
                    };
                    window.after_read(read, Instant::now())
                }
                WindowStep::CheckLaunch => {
                    return match self.pin.has_exited() {
                        Ok(exited) => {
                            HealthWindow::after_launch_check(Some(exited)).map_err(window_refusal)
                        }
                        Err(source) => Err(WindowsAttemptExchangeError::HealthWindow {
                            failure: AttemptHealthWindowFailure::LaunchUnverifiable,
                            source: Some(source),
                        }),
                    };
                }
                WindowStep::Refuse(failure) => return Err(window_refusal(failure)),
            };
        }
    }

    /// Writes `KELD-AK1` accepted, then waits until `close_deadline` for the
    /// candidate's end of file before it closes its own end, so closing never
    /// discards an unread `KELD-AK1` (KEL-53 §4 *Health sequence*).
    ///
    /// Call it only after `HealthAccepted` is durable: an owner lost before
    /// that write has sent no `KELD-AK1`, and recovery rolls back. Health is
    /// then decided, so nothing observed here changes the outcome; the
    /// returned value says what was observed.
    ///
    /// # Errors
    ///
    /// [`WindowsAttemptExchangeError::OutOfSequence`] unless the health window
    /// passed; nothing is written and the dropped channel closes the
    /// connection, which leaves the candidate unarmed.
    pub fn accept(
        mut self,
        close_deadline: Instant,
    ) -> Result<WindowsAttemptCloseWait, WindowsAttemptExchangeError> {
        if self.phase != OwnerPhase::Healthy {
            return Err(self.out_of_sequence("write KELD-AK1 accepted"));
        }
        self.stream.set_absolute_deadline(Some(close_deadline));
        if let Err(error) =
            AttemptRecord::HealthResult(AttemptHealthResult::Accepted).write_to(&mut self.stream)
        {
            return Ok(WindowsAttemptCloseWait::WriteFailed { error });
        }
        #[cfg(test)]
        if let Some(entered) = self.close_wait_entered.take() {
            let _ = entered.send(());
        }
        let mut byte = [0_u8; 1];
        Ok(match self.stream.read(&mut byte) {
            Ok(0) => WindowsAttemptCloseWait::CandidateClosed,
            Ok(_) => WindowsAttemptCloseWait::BytesReceived,
            Err(source) if is_deadline(&source) => WindowsAttemptCloseWait::DeadlineElapsed,
            Err(source) => WindowsAttemptCloseWait::ReadFailed { source },
        })
    }

    /// Ends the exchange on the rollback path. While the connection is still
    /// open, it writes `KELD-AK1` rolled back once and returns without waiting
    /// for the candidate to read it; after end of file, or when the pin
    /// reports the launched process exited, it writes nothing. Dropping the
    /// channel then closes the connection. A failed write changes nothing:
    /// the candidate never arms without `KELD-AK1` accepted.
    #[must_use]
    pub fn roll_back(mut self) -> WindowsAttemptRollBack {
        if let OwnerPhase::Ended { end_of_file: true } = self.phase {
            return WindowsAttemptRollBack::NotSentAfterEndOfFile;
        }
        if matches!(self.pin.has_exited(), Ok(true)) {
            return WindowsAttemptRollBack::NotSentAfterLaunchExit;
        }
        // The record fits the pipe buffer, so the write does not wait for the
        // candidate to read it; the deadline only bounds a wedged write.
        self.stream.set_absolute_deadline(Some(io_deadline()));
        match AttemptRecord::HealthResult(AttemptHealthResult::RolledBack)
            .write_to(&mut self.stream)
        {
            Ok(()) => WindowsAttemptRollBack::Sent,
            Err(error) => WindowsAttemptRollBack::WriteFailed { error },
        }
    }

    /// Signals `entered` once `KELD-AK1` accepted is written and the owner
    /// starts waiting for the candidate's end of file.
    #[cfg(test)]
    pub(super) fn install_close_wait_witness(&mut self, entered: mpsc::Sender<()>) {
        self.close_wait_entered = Some(entered);
    }

    /// The owner's own stream, for tests that play a faulty owner.
    #[cfg(test)]
    pub(super) const fn stream_mut(&mut self) -> &mut WindowsNamedPipeStream {
        &mut self.stream
    }

    fn read_record(
        &mut self,
        position: AttemptReadPosition,
        deadline: Instant,
    ) -> Result<AttemptRecord, WindowsAttemptExchangeError> {
        self.stream.set_absolute_deadline(Some(deadline));
        AttemptRecord::read_from(&mut self.stream, position).map_err(Into::into)
    }

    fn settle(
        &mut self,
        result: Result<(), WindowsAttemptExchangeError>,
        next: OwnerPhase,
    ) -> Result<(), WindowsAttemptExchangeError> {
        match &result {
            Ok(()) => self.phase = next,
            Err(error) => self.end(error),
        }
        result
    }

    fn end(&mut self, error: &WindowsAttemptExchangeError) {
        self.phase = OwnerPhase::Ended {
            end_of_file: error.is_end_of_file(),
        };
    }

    fn out_of_sequence(&self, step: &'static str) -> WindowsAttemptExchangeError {
        WindowsAttemptExchangeError::OutOfSequence {
            step,
            phase: self.phase.describe(),
        }
    }
}

/// What the owner observed after it wrote `KELD-AK1` accepted. Health was
/// already decided, so none of these changes the outcome.
#[derive(Debug)]
pub enum WindowsAttemptCloseWait {
    /// The candidate closed its end: it read `KELD-AK1` and can arm.
    CandidateClosed,
    /// The close deadline elapsed first; the owner closed its end anyway.
    DeadlineElapsed,
    /// The candidate sent bytes, which it never does after `KELD-AY1`; the
    /// owner closed its end.
    BytesReceived,
    /// Waiting for end of file failed; the owner closed its end.
    ReadFailed {
        /// The read failure.
        source: io::Error,
    },
    /// Writing `KELD-AK1` failed, so the candidate never arms.
    WriteFailed {
        /// The write failure (`KELD-IPC-017`).
        error: AttemptRecordError,
    },
}

/// What the owner did on the rollback path. None of these changes the
/// rollback.
#[derive(Debug)]
pub enum WindowsAttemptRollBack {
    /// `KELD-AK1` rolled back was written into the open connection.
    Sent,
    /// The candidate had closed its end, so nothing was written.
    NotSentAfterEndOfFile,
    /// The launched process had exited, so nothing was written.
    NotSentAfterLaunchExit,
    /// The write failed.
    WriteFailed {
        /// The write failure (`KELD-IPC-017`).
        error: AttemptRecordError,
    },
}

/// The candidate's end of an accepted connect-back connection: the owner
/// accepted its claim and `KELD-AR1` matched its transcript. Dropping the
/// channel closes the connection.
#[derive(Debug)]
pub struct WindowsAttemptClaimantChannel {
    stream: WindowsNamedPipeStream,
    transcript: AttemptTranscript,
    phase: ClaimantPhase,
}

/// Where the candidate is in the health sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaimantPhase {
    /// `KELD-AR1` matched; `KELD-AB1` or `KELD-AF1` is next.
    Accepted,
    /// `KELD-AB1` sent; `KELD-AY1` or `KELD-AF1` is next.
    Booted,
    /// `KELD-AY1` or `KELD-AF1` sent; only the one `KELD-AK1` read remains.
    Reported,
    /// A write failed; nothing remains.
    Ended,
}

impl ClaimantPhase {
    const fn describe(self) -> &'static str {
        match self {
            Self::Accepted => "after KELD-AR1",
            Self::Booted => "after KELD-AB1",
            Self::Reported => "after KELD-AY1 or KELD-AF1",
            Self::Ended => "after a failure",
        }
    }
}

impl WindowsAttemptClaimantChannel {
    pub(super) const fn accepted(
        stream: WindowsNamedPipeStream,
        transcript: AttemptTranscript,
    ) -> Self {
        Self {
            stream,
            transcript,
            phase: ClaimantPhase::Accepted,
        }
    }

    /// The accepted claim transcript: the attempt and health-channel IDs from
    /// `KELD-AC1` that the locator check matched to the rendezvous name, and
    /// the owner's process ID, which criterion 20's candidate-boot read
    /// requires of the journal.
    #[must_use]
    pub const fn transcript(&self) -> &AttemptTranscript {
        &self.transcript
    }

    /// Sends `KELD-AB1` after criterion 20's candidate-boot read: the accepted
    /// attempt and health-channel IDs and `health_receipt_digest`, the §4
    /// health-receipt digest over them and the candidate artifact identity
    /// that the read matched to this executable's version tree.
    ///
    /// # Errors
    ///
    /// [`WindowsAttemptExchangeError::Record`] when the write fails, and
    /// [`WindowsAttemptExchangeError::OutOfSequence`] unless this is the first
    /// step after the claim.
    pub fn send_boot(
        &mut self,
        health_receipt_digest: &[u8; 32],
    ) -> Result<(), WindowsAttemptExchangeError> {
        if self.phase != ClaimantPhase::Accepted {
            return Err(self.out_of_sequence("send KELD-AB1"));
        }
        let boot = AttemptBootAcknowledgement::new(
            *self.transcript.attempt_id(),
            *self.transcript.health_channel_id(),
            *health_receipt_digest,
        );
        self.send(
            AttemptRecord::BootAcknowledgement(boot),
            ClaimantPhase::Booted,
        )
    }

    /// Sends `KELD-AY1` at the application's Ready.
    ///
    /// # Errors
    ///
    /// As [`Self::send_boot`]; out of sequence unless `KELD-AB1` was sent.
    pub fn send_ready(&mut self) -> Result<(), WindowsAttemptExchangeError> {
        if self.phase != ClaimantPhase::Booted {
            return Err(self.out_of_sequence("send KELD-AY1"));
        }
        self.send(AttemptRecord::Ready, ClaimantPhase::Reported)
    }

    /// Sends `KELD-AF1` with `class`, which must be admitted where the owner
    /// reads it: class `1` or `3` before `KELD-AB1`, class `2` or `3` after.
    ///
    /// # Errors
    ///
    /// - [`WindowsAttemptExchangeError::Record`] with
    ///   [`AttemptRecordError::FailureClassNotAdmitted`] for a class the owner
    ///   would refuse here, before anything is sent, or when the write fails;
    /// - [`WindowsAttemptExchangeError::OutOfSequence`] after `KELD-AY1` or
    ///   `KELD-AF1`.
    pub fn send_failure(
        &mut self,
        class: AttemptFailureClass,
    ) -> Result<(), WindowsAttemptExchangeError> {
        let position = match self.phase {
            ClaimantPhase::Accepted => AttemptReadPosition::OwnerBoot,
            ClaimantPhase::Booted => AttemptReadPosition::OwnerReady,
            ClaimantPhase::Reported | ClaimantPhase::Ended => {
                return Err(self.out_of_sequence("send KELD-AF1"));
            }
        };
        if !position.admits_failure(class) {
            return Err(AttemptRecordError::FailureClassNotAdmitted { position, class }.into());
        }
        self.send(AttemptRecord::Failure(class), ClaimantPhase::Reported)
    }

    /// Reads exactly one `KELD-AK1` under `deadline`, then closes the
    /// connection. Only `KELD-AK1` accepted returns `Ok`: the candidate arms
    /// its recovery gate on that alone, never on end of file, a read failure,
    /// the deadline or `KELD-AK1` rolled back.
    ///
    /// # Errors
    ///
    /// - [`WindowsAttemptExchangeError::HealthRolledBack`] for `KELD-AK1`
    ///   rolled back;
    /// - [`WindowsAttemptExchangeError::Record`] for a refused or truncated
    ///   record, end of file, a read failure or the deadline;
    /// - [`WindowsAttemptExchangeError::OutOfSequence`] unless `KELD-AY1` or
    ///   `KELD-AF1` was sent.
    pub fn await_health_accepted(
        mut self,
        deadline: Instant,
    ) -> Result<(), WindowsAttemptExchangeError> {
        if self.phase != ClaimantPhase::Reported {
            return Err(self.out_of_sequence("read KELD-AK1"));
        }
        self.stream.set_absolute_deadline(Some(deadline));
        let position = AttemptReadPosition::CandidateHealthResult;
        match AttemptRecord::read_from(&mut self.stream, position)? {
            AttemptRecord::HealthResult(AttemptHealthResult::Accepted) => Ok(()),
            AttemptRecord::HealthResult(AttemptHealthResult::RolledBack) => {
                Err(WindowsAttemptExchangeError::HealthRolledBack)
            }
            other => Err(other.not_admitted_at(position).into()),
        }
    }

    /// The candidate's own stream, for tests that play a faulty candidate.
    #[cfg(test)]
    pub(super) const fn stream_mut(&mut self) -> &mut WindowsNamedPipeStream {
        &mut self.stream
    }

    fn send(
        &mut self,
        record: AttemptRecord,
        next: ClaimantPhase,
    ) -> Result<(), WindowsAttemptExchangeError> {
        self.stream.set_absolute_deadline(Some(io_deadline()));
        match record.write_to(&mut self.stream) {
            Ok(()) => {
                self.phase = next;
                Ok(())
            }
            Err(error) => {
                self.phase = ClaimantPhase::Ended;
                Err(error.into())
            }
        }
    }

    fn out_of_sequence(&self, step: &'static str) -> WindowsAttemptExchangeError {
        WindowsAttemptExchangeError::OutOfSequence {
            step,
            phase: self.phase.describe(),
        }
    }
}

/// The landed writer deadline, [`APP_LINK_IO_DEADLINE`], from now; an
/// unrepresentable one is now, so the write fails closed rather than waits.
fn io_deadline() -> Instant {
    let now = Instant::now();
    now.checked_add(APP_LINK_IO_DEADLINE).unwrap_or(now)
}

/// The deadline-bounded read's expiry: `ERROR_SEM_TIMEOUT`, which
/// `ERROR_OPERATION_ABORTED` (a cancellation) shares its `io::ErrorKind`
/// with, so only the raw code separates them.
fn is_deadline(error: &io::Error) -> bool {
    error.raw_os_error() == Some(ERROR_SEM_TIMEOUT.cast_signed())
}

const fn window_refusal(failure: AttemptHealthWindowFailure) -> WindowsAttemptExchangeError {
    WindowsAttemptExchangeError::HealthWindow {
        failure,
        source: None,
    }
}

/// Typed failure of the `keld-attempt` exchange. Before `KELD-AR1` every
/// failure refuses the claim; after it the owner cannot commit health and
/// rolls back, and the candidate never arms its recovery gate (KEL-53 §4
/// "Candidate connect-back", *Refusal* and *Health sequence*).
#[derive(Debug)]
pub enum WindowsAttemptExchangeError {
    /// An endpoint operation failed; its `KELD-IPC-008` to `KELD-IPC-014`
    /// code applies.
    Endpoint(WindowsAttemptEndpointError),
    /// A record was refused or mismatched, or its I/O failed or met its
    /// deadline; its `KELD-IPC-015` to `KELD-IPC-017` code applies.
    Record(AttemptRecordError),
    /// `KELD-IPC-018`: the claim deadline passed before the owner accepted a
    /// claimant; the endpoint is closed.
    ClaimDeadline,
    /// `KELD-IPC-019`: an exchange step was called out of the KEL-53 order;
    /// nothing was sent or read.
    OutOfSequence {
        /// The refused step.
        step: &'static str,
        /// Where the exchange was.
        phase: &'static str,
    },
    /// `KELD-IPC-020`: the candidate reported `KELD-AF1` with this class.
    CandidateFailure {
        /// The reported failure class.
        class: AttemptFailureClass,
    },
    /// `KELD-IPC-020`: the health window refused health.
    HealthWindow {
        /// Why.
        failure: AttemptHealthWindowFailure,
        /// The launch-state query failure, for
        /// [`AttemptHealthWindowFailure::LaunchUnverifiable`].
        source: Option<io::Error>,
    },
    /// `KELD-IPC-021`: the owner's `KELD-AK1` is rolled back.
    HealthRolledBack,
}

impl WindowsAttemptExchangeError {
    /// Whether the peer's end of the connection was observed closed.
    fn is_end_of_file(&self) -> bool {
        match self {
            Self::Record(AttemptRecordError::Io { source }) => {
                source.kind() == io::ErrorKind::UnexpectedEof
            }
            Self::HealthWindow { failure, .. } => *failure == AttemptHealthWindowFailure::EndOfFile,
            _ => false,
        }
    }
}

impl From<AttemptRecordError> for WindowsAttemptExchangeError {
    fn from(error: AttemptRecordError) -> Self {
        Self::Record(error)
    }
}

impl From<WindowsAttemptEndpointError> for WindowsAttemptExchangeError {
    fn from(error: WindowsAttemptEndpointError) -> Self {
        Self::Endpoint(error)
    }
}

const ROLL_BACK_FIX: &str = "The owner cannot commit health: roll the attempt back, writing \
     KELD-AK1 rolled back once while the connection is still open, then end the candidate \
     family.";

impl fmt::Display for WindowsAttemptExchangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Endpoint(source) => write!(f, "{source}"),
            Self::Record(source) => write!(f, "{source}"),
            Self::ClaimDeadline => f.write_str(
                "KELD-IPC-018: no keld-attempt claimant was accepted before the claim deadline; \
                 the endpoint is closed. Roll the attempt back: health cannot commit, and \
                 refused claimants never extend this deadline.",
            ),
            Self::OutOfSequence { step, phase } => write!(
                f,
                "KELD-IPC-019: keld-attempt step `{step}` refused {phase}; nothing was sent or \
                 read. Follow the KEL-53 health sequence: the claim, KELD-AB1, KELD-AY1 (or \
                 KELD-AF1), the owner's health window, then one KELD-AK1; after a failure the \
                 owner only rolls back and the candidate never arms."
            ),
            Self::CandidateFailure { class } => write!(
                f,
                "KELD-IPC-020: keld-attempt health not proven (the candidate reported KELD-AF1 \
                 class {}). {ROLL_BACK_FIX}",
                *class as u8
            ),
            Self::HealthWindow { failure, source } => {
                let what = match failure {
                    AttemptHealthWindowFailure::ByteReceived => "a byte arrived after KELD-AY1",
                    AttemptHealthWindowFailure::EndOfFile => {
                        "the candidate closed the connection during the window"
                    }
                    AttemptHealthWindowFailure::LaunchExited => {
                        "the launched process exited before the window ended"
                    }
                    AttemptHealthWindowFailure::LaunchUnverifiable => {
                        "the launched process's state could not be read at the window's end"
                    }
                    AttemptHealthWindowFailure::Unrepresentable => {
                        "the window's end does not fit the monotonic clock"
                    }
                };
                write!(f, "KELD-IPC-020: keld-attempt health not proven ({what}")?;
                if let Some(source) = source {
                    write!(f, ": {source}")?;
                }
                write!(f, "). {ROLL_BACK_FIX}")
            }
            Self::HealthRolledBack => f.write_str(
                "KELD-IPC-021: keld-attempt health rolled back by the owner (KELD-AK1 result 2). \
                 Never arm the recovery gate: end the host on any later application-generation \
                 exit.",
            ),
        }
    }
}

impl std::error::Error for WindowsAttemptExchangeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Endpoint(source) => Some(source),
            Self::Record(source) => Some(source),
            Self::HealthWindow {
                source: Some(source),
                ..
            } => Some(source),
            Self::ClaimDeadline
            | Self::OutOfSequence { .. }
            | Self::CandidateFailure { .. }
            | Self::HealthWindow { source: None, .. }
            | Self::HealthRolledBack => None,
        }
    }
}
