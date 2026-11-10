//! Typed fixture failures of the recovery control handshake.
//!
//! Redaction holds by construction: a variant carries at most a fixed record kind or an
//! `io::ErrorKind`, never a control-record field, path, token or Host output, so its
//! `Debug` text is safe to print in a failure panic. The failure site, not a message, is
//! the diagnostic: nothing in the fixture classifies text by keyword.
use std::io::ErrorKind;

/// Which of the two initial control records a failure concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ControlRecord {
    Hello,
    Descendant,
}

impl ControlRecord {
    /// Maps the zero-based initial control line to its record.
    pub(super) const fn of_line(line: u8) -> Self {
        if line == 0 {
            Self::Hello
        } else {
            Self::Descendant
        }
    }
}

/// One variant per failure site of `accept_recovery_until`, `read_initial_line_until`,
/// `collect_initial_control_until` and `RecoveryGeneration::try_accept`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecoveryControlFailure {
    /// The clock was already past the original deadline when the accept loop polled.
    AcceptDeadline,
    /// `accept` returned a peer, but only after the original deadline.
    AcceptCompletedLate,
    /// The nonblocking accept kept reporting no peer until the deadline passed.
    AcceptNoPeer,
    /// `accept` itself failed.
    AcceptFailed(ErrorKind),
    /// The first switch of the accepted stream to blocking mode failed.
    NormalizeBeforeRecords(ErrorKind),
    /// Installing the steady-state read timeout failed. On Darwin this is `EINVAL` when
    /// the peer already closed; it is a socket-option error, not a timeout.
    SetReadTimeout(ErrorKind),
    /// Switching the accepted stream to nonblocking mode for the byte reads failed.
    PollSetup(ErrorKind),
    /// The clock passed the original deadline before a byte of this record was read.
    LineReadDeadline(ControlRecord),
    /// This record's LF arrived, but only after the original deadline.
    LineLfLate(ControlRecord),
    /// The clock passed the original deadline right after a byte of this record.
    LineByteDeadline(ControlRecord),
    /// A control record was not valid UTF-8.
    LineInvalidUtf8,
    /// A control record exceeded the line cap.
    LineOversized,
    /// The peer closed before the LF of a control record.
    LineEof,
    /// A byte read failed with something other than `WouldBlock` or `Interrupted`.
    LineReadFailed(ErrorKind),
    /// Both records were read, but only after the original deadline.
    RecordsCompletedLate,
    /// The final switch of the accepted stream back to blocking mode failed.
    NormalizeAfterRecords(ErrorKind),
    /// The final switch to blocking mode finished after the original deadline.
    NormalizedAfterDeadline,
    /// The record did not start with its expected tag.
    RecordMalformed(ControlRecord),
    /// The record had no numeric process-ID field.
    RecordPidMissing(ControlRecord),
    /// The record's process-ID field was not a number.
    RecordPidInvalid(ControlRecord),
    /// The HELLO record had no app-link field.
    HelloAppLinkMissing,
    /// The HELLO app-link field had no `#` token separator.
    HelloAppLinkMalformed,
    /// Both records parsed, but only after the original deadline.
    HandshakeAfterDeadline,
    /// Cloning the accepted stream for the steady-state reader failed.
    ReaderClone(ErrorKind),
}
