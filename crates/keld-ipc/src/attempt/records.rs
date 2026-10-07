//! `keld-attempt` claim and health records (KEL-53 §4 "Candidate
//! connect-back": *Messages*, *Transcript*, *Health records* and *Health
//! sequence*; approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06).
//!
//! Each record is fixed-size: its 8 ASCII magic bytes, then its fields in the
//! `LC1` order, with raw 32-byte IDs, nonces and digests and little-endian
//! `u32` process IDs. A record's length follows from its magic, so no record
//! carries a length field. Every read names its [`AttemptReadPosition`]: the
//! reader reads the magic first, refuses a magic that the position does not
//! admit before it reads any other byte, and only then reads the rest of the
//! record under the stream's own deadline. Class and result bytes are closed
//! sets numbered from `1`, so a zeroed byte never decodes.
//!
//! This is the codec only, and it is crate-private (KEL-270 comment
//! `136c2682`): the claim and health exchange that sends and reads these
//! records runs inside the endpoint and the client (`claim.rs`, `channel.rs`),
//! so no other crate can build, send or read a record. Only the types that its
//! refusals and transcripts name are public. The non-product `fuzzing` feature
//! adds the raw-byte fuzz hook [`fuzz_attempt_records`]. The bootstrap records
//! `BH1` to `BO1` land with S11.
//!
//! Off Windows the module builds only for its tests and the fuzz hook, so the
//! parts that only the Windows exchange uses are unused there.
#![cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "off Windows only the codec tests and the fuzzing hook build this module; \
                  the exchange that uses the rest is Windows-only"
    )
)]

use std::fmt;
use std::io::{self, Read, Write};

use crate::token::SessionToken;

const MAGIC_LEN: usize = 8;
const ID_LEN: usize = 32;
const PID_LEN: usize = 4;
/// `KELD-AA1` and `KELD-AR1`, the longest records.
const MAX_RECORD_LEN: usize = MAGIC_LEN + 5 * ID_LEN + 2 * PID_LEN;

/// One record of the closed `keld-attempt` claim and health message set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptRecordKind {
    /// `KELD-AH1`, the candidate's claim.
    Claim,
    /// `KELD-AC1`, the owner's challenge.
    Challenge,
    /// `KELD-AA1`, the candidate's acknowledgement of the whole transcript.
    Acknowledgement,
    /// `KELD-AR1`, the owner's acceptance receipt.
    Receipt,
    /// `KELD-AB1`, the candidate's boot acknowledgement.
    BootAcknowledgement,
    /// `KELD-AY1`, the candidate's Ready.
    Ready,
    /// `KELD-AF1`, the candidate's failure.
    Failure,
    /// `KELD-AK1`, the owner's health result.
    HealthResult,
}

impl AttemptRecordKind {
    const ALL: [Self; 8] = [
        Self::Claim,
        Self::Challenge,
        Self::Acknowledgement,
        Self::Receipt,
        Self::BootAcknowledgement,
        Self::Ready,
        Self::Failure,
        Self::HealthResult,
    ];

    /// The record's 8 ASCII magic bytes.
    #[must_use]
    pub(crate) const fn magic(self) -> [u8; MAGIC_LEN] {
        match self {
            Self::Claim => *b"KELD-AH1",
            Self::Challenge => *b"KELD-AC1",
            Self::Acknowledgement => *b"KELD-AA1",
            Self::Receipt => *b"KELD-AR1",
            Self::BootAcknowledgement => *b"KELD-AB1",
            Self::Ready => *b"KELD-AY1",
            Self::Failure => *b"KELD-AF1",
            Self::HealthResult => *b"KELD-AK1",
        }
    }

    /// The record's whole length in bytes, magic included.
    #[must_use]
    pub(crate) const fn record_len(self) -> usize {
        match self {
            Self::Claim => MAGIC_LEN + 2 * ID_LEN + PID_LEN,
            Self::Challenge => MAGIC_LEN + 3 * ID_LEN + PID_LEN,
            Self::Acknowledgement | Self::Receipt => MAX_RECORD_LEN,
            Self::BootAcknowledgement => MAGIC_LEN + 3 * ID_LEN,
            Self::Ready => MAGIC_LEN,
            Self::Failure | Self::HealthResult => MAGIC_LEN + 1,
        }
    }

    fn from_magic(magic: [u8; MAGIC_LEN]) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.magic() == magic)
    }
}

impl fmt::Display for AttemptRecordKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.magic().escape_ascii())
    }
}

/// Where a record is read, which fixes the records it admits (KEL-53 §4
/// *Health sequence*).
///
/// After `KELD-AY1` the owner admits no record and after `KELD-AF1` the
/// candidate sends nothing, so neither has a position here: the owner's
/// health window treats any byte as a failure (S6). After `KELD-AK1` the
/// candidate reads nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptReadPosition {
    /// The owner's first read on a connection: `KELD-AH1` only.
    OwnerClaim,
    /// The owner, after its `KELD-AC1`: `KELD-AA1` only.
    OwnerAcknowledgement,
    /// The owner, after its `KELD-AR1`: `KELD-AB1`, or `KELD-AF1` with class
    /// `1` (bootstrap read refused) or `3` (boot error).
    OwnerBoot,
    /// The owner, after `KELD-AB1`: `KELD-AY1`, or `KELD-AF1` with class `2`
    /// (application exit before Ready) or `3` (boot error).
    OwnerReady,
    /// The candidate, after its `KELD-AH1`: `KELD-AC1` only.
    CandidateChallenge,
    /// The candidate, after its `KELD-AA1`: `KELD-AR1` only.
    CandidateReceipt,
    /// The candidate, after its `KELD-AY1` or `KELD-AF1`: exactly one
    /// `KELD-AK1`.
    CandidateHealthResult,
}

impl AttemptReadPosition {
    const fn admits(self, kind: AttemptRecordKind) -> bool {
        use AttemptRecordKind as Kind;
        matches!(
            (self, kind),
            (Self::OwnerClaim, Kind::Claim)
                | (Self::OwnerAcknowledgement, Kind::Acknowledgement)
                | (Self::OwnerBoot, Kind::BootAcknowledgement | Kind::Failure)
                | (Self::OwnerReady, Kind::Ready | Kind::Failure)
                | (Self::CandidateChallenge, Kind::Challenge)
                | (Self::CandidateReceipt, Kind::Receipt)
                | (Self::CandidateHealthResult, Kind::HealthResult)
        )
    }

    /// The application starts only after `KELD-AB1`: a refused bootstrap read
    /// can only precede it, an application exit before Ready can only follow
    /// it, and a boot error can come on either side.
    pub(crate) const fn admits_failure(self, class: AttemptFailureClass) -> bool {
        use AttemptFailureClass as Class;
        matches!(
            (self, class),
            (
                Self::OwnerBoot,
                Class::BootstrapReadRefused | Class::BootError
            ) | (
                Self::OwnerReady,
                Class::ApplicationExitBeforeReady | Class::BootError
            )
        )
    }

    const fn describe(self) -> &'static str {
        match self {
            Self::OwnerClaim => "as the owner's first record (only KELD-AH1)",
            Self::OwnerAcknowledgement => "after the owner's KELD-AC1 (only KELD-AA1)",
            Self::OwnerBoot => "after the owner's KELD-AR1 (KELD-AB1, or KELD-AF1 class 1 or 3)",
            Self::OwnerReady => "after KELD-AB1 (KELD-AY1, or KELD-AF1 class 2 or 3)",
            Self::CandidateChallenge => "after the candidate's KELD-AH1 (only KELD-AC1)",
            Self::CandidateReceipt => "after the candidate's KELD-AA1 (only KELD-AR1)",
            Self::CandidateHealthResult => {
                "after the candidate's KELD-AY1 or KELD-AF1 (only KELD-AK1)"
            }
        }
    }
}

/// The closed `KELD-AF1` failure class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AttemptFailureClass {
    /// `1`: criterion 20's bootstrap read refused, before `KELD-AB1`.
    BootstrapReadRefused = 1,
    /// `2`: the application exited before Ready, after `KELD-AB1`.
    ApplicationExitBeforeReady = 2,
    /// `3`: a boot error, on either side of `KELD-AB1`.
    BootError = 3,
}

impl AttemptFailureClass {
    const fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::BootstrapReadRefused),
            2 => Some(Self::ApplicationExitBeforeReady),
            3 => Some(Self::BootError),
            _ => None,
        }
    }
}

/// The closed `KELD-AK1` health result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum AttemptHealthResult {
    /// `1`: `HealthAccepted` is durable.
    Accepted = 1,
    /// `2`: the owner rolls the attempt back.
    RolledBack = 2,
}

impl AttemptHealthResult {
    const fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Accepted),
            2 => Some(Self::RolledBack),
            _ => None,
        }
    }
}

/// `KELD-AH1`: what the candidate knows without the journal (76 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttemptClaim {
    installation_id: [u8; ID_LEN],
    client_nonce: SessionToken,
    client_pid: u32,
}

impl AttemptClaim {
    /// The candidate's claim: the installation ID from its immutable
    /// provenance record, a fresh nonce from [`SessionToken::random`] and its
    /// own process ID.
    #[must_use]
    pub(crate) const fn new(
        installation_id: [u8; 32],
        client_nonce: SessionToken,
        client_pid: u32,
    ) -> Self {
        Self {
            installation_id,
            client_nonce,
            client_pid,
        }
    }
}

/// `KELD-AC1`: the owner's challenge (108 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttemptChallenge {
    attempt_id: [u8; ID_LEN],
    health_channel_id: [u8; ID_LEN],
    server_nonce: SessionToken,
    server_pid: u32,
}

impl AttemptChallenge {
    /// The owner's challenge: the attempt and health-channel IDs that
    /// `keld-update` minted, a fresh nonce from [`SessionToken::random`] and
    /// the owner's own process ID.
    #[must_use]
    pub(crate) const fn new(
        attempt_id: [u8; 32],
        health_channel_id: [u8; 32],
        server_nonce: SessionToken,
        server_pid: u32,
    ) -> Self {
        Self {
            attempt_id,
            health_channel_id,
            server_nonce,
            server_pid,
        }
    }
}

/// The whole claim transcript, the fields of `KELD-AH1` and `KELD-AC1`
/// together: the body of `KELD-AA1` and of its same-context receipt
/// `KELD-AR1` (176 bytes each), which differ only in magic.
///
/// A transcript is the body that each side's acceptance check returns, and
/// only the crate-private exchange builds one: it sends only the transcript
/// that the claimant's or the owner's check returned, and the claimant runs
/// its locator check before it writes `KELD-AA1`. Outside `keld-ipc` a
/// transcript is read-only: the accepted claim that a channel reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptTranscript {
    installation_id: [u8; ID_LEN],
    attempt_id: [u8; ID_LEN],
    health_channel_id: [u8; ID_LEN],
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
}

impl AttemptTranscript {
    /// The owner's acceptance of `claim`: its installation ID is the owner's
    /// own and its client process ID is the connected client's
    /// (`GetNamedPipeClientProcessId`). On success the result, joined with the
    /// owner's own `challenge`, is the `KELD-AA1` the owner requires and the
    /// `KELD-AR1` it sends.
    ///
    /// # Errors
    ///
    /// [`AttemptRecordError::ForeignInstallation`] or
    /// [`AttemptRecordError::ClientProcessMismatch`].
    pub(crate) fn for_owner(
        claim: &AttemptClaim,
        installation_id: &[u8; 32],
        connected_client_pid: u32,
        challenge: &AttemptChallenge,
    ) -> Result<Self, AttemptRecordError> {
        if claim.installation_id != *installation_id {
            return Err(AttemptRecordError::ForeignInstallation);
        }
        if claim.client_pid != connected_client_pid {
            return Err(AttemptRecordError::ClientProcessMismatch {
                claimed: claim.client_pid,
                connected: connected_client_pid,
            });
        }
        Ok(Self::join(claim, challenge))
    }

    /// The claimant's acceptance of `challenge` for its own `claim`, before it
    /// sends `KELD-AA1`: the stated server process ID is the connected
    /// server's (`GetNamedPipeServerProcessId`), and the locator over the
    /// claim's installation ID and the offered attempt and health-channel IDs
    /// yields exactly `rendezvous`, the name the candidate was launched with.
    /// On success the result is the `KELD-AA1` it sends and the `KELD-AR1` it
    /// requires.
    ///
    /// # Errors
    ///
    /// [`AttemptRecordError::ServerProcessMismatch`] or
    /// [`AttemptRecordError::LocatorMismatch`] (which includes IDs the locator
    /// refuses).
    #[cfg(windows)]
    pub(crate) fn for_claimant(
        claim: &AttemptClaim,
        challenge: &AttemptChallenge,
        rendezvous: &str,
        connected_server_pid: u32,
    ) -> Result<Self, AttemptRecordError> {
        if challenge.server_pid != connected_server_pid {
            return Err(AttemptRecordError::ServerProcessMismatch {
                challenged: challenge.server_pid,
                connected: connected_server_pid,
            });
        }
        let derived = super::locator::windows_attempt_connect_back_endpoint(
            &claim.installation_id,
            &challenge.attempt_id,
            &challenge.health_channel_id,
        )
        .map_err(|_| AttemptRecordError::LocatorMismatch)?;
        if derived != rendezvous {
            return Err(AttemptRecordError::LocatorMismatch);
        }
        Ok(Self::join(claim, challenge))
    }

    const fn join(claim: &AttemptClaim, challenge: &AttemptChallenge) -> Self {
        Self {
            installation_id: claim.installation_id,
            attempt_id: challenge.attempt_id,
            health_channel_id: challenge.health_channel_id,
            client_nonce: claim.client_nonce,
            server_nonce: challenge.server_nonce,
            client_pid: claim.client_pid,
            server_pid: challenge.server_pid,
        }
    }

    /// Requires a received `KELD-AA1` (owner) or `KELD-AR1` (claimant) body to
    /// be exactly this transcript; with its magic already admitted, that is
    /// the whole record.
    ///
    /// # Errors
    ///
    /// [`AttemptRecordError::TranscriptMismatch`] when any field differs.
    pub(crate) fn require_match(&self, received: &Self) -> Result<(), AttemptRecordError> {
        if self == received {
            Ok(())
        } else {
            Err(AttemptRecordError::TranscriptMismatch)
        }
    }

    /// The bound installation ID.
    #[must_use]
    pub const fn installation_id(&self) -> &[u8; 32] {
        &self.installation_id
    }

    /// The bound attempt ID.
    #[must_use]
    pub const fn attempt_id(&self) -> &[u8; 32] {
        &self.attempt_id
    }

    /// The bound health-channel ID.
    #[must_use]
    pub const fn health_channel_id(&self) -> &[u8; 32] {
        &self.health_channel_id
    }

    /// The candidate's nonce.
    #[must_use]
    pub const fn client_nonce(&self) -> &SessionToken {
        &self.client_nonce
    }

    /// The owner's nonce.
    #[must_use]
    pub const fn server_nonce(&self) -> &SessionToken {
        &self.server_nonce
    }

    /// The candidate's process ID.
    #[must_use]
    pub const fn client_pid(&self) -> u32 {
        self.client_pid
    }

    /// The owner's process ID.
    #[must_use]
    pub const fn server_pid(&self) -> u32 {
        self.server_pid
    }
}

/// `KELD-AB1`: the candidate's boot acknowledgement after criterion 20's read
/// (104 bytes): the attempt, the health channel and the §4 health-receipt
/// digest over them and the candidate artifact identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttemptBootAcknowledgement {
    attempt_id: [u8; ID_LEN],
    health_channel_id: [u8; ID_LEN],
    health_receipt_digest: [u8; ID_LEN],
}

impl AttemptBootAcknowledgement {
    /// A boot acknowledgement. The candidate sends the digest it computed
    /// for its own version tree; the owner builds the one it expects from
    /// the journal.
    #[must_use]
    pub(crate) const fn new(
        attempt_id: [u8; 32],
        health_channel_id: [u8; 32],
        health_receipt_digest: [u8; 32],
    ) -> Self {
        Self {
            attempt_id,
            health_channel_id,
            health_receipt_digest,
        }
    }

    /// Requires a received `KELD-AB1` to be exactly this one.
    ///
    /// # Errors
    ///
    /// [`AttemptRecordError::BootMismatch`] when any field differs.
    pub(crate) fn require_match(&self, received: &Self) -> Result<(), AttemptRecordError> {
        if self == received {
            Ok(())
        } else {
            Err(AttemptRecordError::BootMismatch)
        }
    }
}

/// One `keld-attempt` claim or health record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptRecord {
    /// `KELD-AH1`.
    Claim(AttemptClaim),
    /// `KELD-AC1`.
    Challenge(AttemptChallenge),
    /// `KELD-AA1`.
    Acknowledgement(AttemptTranscript),
    /// `KELD-AR1`.
    Receipt(AttemptTranscript),
    /// `KELD-AB1`.
    BootAcknowledgement(AttemptBootAcknowledgement),
    /// `KELD-AY1`.
    Ready,
    /// `KELD-AF1`.
    Failure(AttemptFailureClass),
    /// `KELD-AK1`.
    HealthResult(AttemptHealthResult),
}

impl AttemptRecord {
    /// This record's kind.
    #[must_use]
    pub(crate) const fn kind(&self) -> AttemptRecordKind {
        match self {
            Self::Claim(_) => AttemptRecordKind::Claim,
            Self::Challenge(_) => AttemptRecordKind::Challenge,
            Self::Acknowledgement(_) => AttemptRecordKind::Acknowledgement,
            Self::Receipt(_) => AttemptRecordKind::Receipt,
            Self::BootAcknowledgement(_) => AttemptRecordKind::BootAcknowledgement,
            Self::Ready => AttemptRecordKind::Ready,
            Self::Failure(_) => AttemptRecordKind::Failure,
            Self::HealthResult(_) => AttemptRecordKind::HealthResult,
        }
    }

    /// The refusal for this record read at `position`, which does not admit
    /// it. A reader that names its position never returns such a record; the
    /// exchange uses this where it matches the record that position admits.
    #[cfg(windows)]
    pub(crate) const fn not_admitted_at(
        &self,
        position: AttemptReadPosition,
    ) -> AttemptRecordError {
        AttemptRecordError::MagicNotAdmitted {
            position,
            magic: self.kind().magic(),
        }
    }

    /// Reads exactly one record admitted at `position`. It reads the 8-byte
    /// magic first and refuses a magic that `position` does not admit before
    /// it reads any other byte; only then does it read the rest of that
    /// record. The reader's own deadline bounds both reads.
    ///
    /// # Errors
    ///
    /// - [`AttemptRecordError::MagicNotAdmitted`], with nothing read after the
    ///   magic;
    /// - [`AttemptRecordError::ValueOutOfSet`] or
    ///   [`AttemptRecordError::FailureClassNotAdmitted`] for a class or result
    ///   byte;
    /// - [`AttemptRecordError::Io`] for end of file before a whole record, a
    ///   read failure or an expired deadline.
    pub(crate) fn read_from<R: Read + ?Sized>(
        reader: &mut R,
        position: AttemptReadPosition,
    ) -> Result<Self, AttemptRecordError> {
        let mut magic = [0_u8; MAGIC_LEN];
        reader
            .read_exact(&mut magic)
            .map_err(AttemptRecordError::io)?;
        let kind = admitted_kind(position, magic)?;
        let len = kind.record_len();
        let mut record = [0_u8; MAX_RECORD_LEN];
        record[..MAGIC_LEN].copy_from_slice(&magic);
        reader
            .read_exact(&mut record[MAGIC_LEN..len])
            .map_err(AttemptRecordError::io)?;
        parse(position, kind, &record)
    }

    /// Decodes `bytes` as exactly one record admitted at `position`, by the
    /// same magic-first rule as [`AttemptRecord::read_from`]. The exchange
    /// reads streams; only the tests and the fuzz hook decode slices.
    ///
    /// # Errors
    ///
    /// - [`AttemptRecordError::Truncated`] when fewer than 8 bytes, or fewer
    ///   than the admitted record's length, are present;
    /// - [`AttemptRecordError::MagicNotAdmitted`];
    /// - [`AttemptRecordError::TrailingBytes`] when any byte follows the
    ///   record;
    /// - [`AttemptRecordError::ValueOutOfSet`] or
    ///   [`AttemptRecordError::FailureClassNotAdmitted`].
    #[cfg(any(test, feature = "fuzzing"))]
    pub(crate) fn decode(
        position: AttemptReadPosition,
        bytes: &[u8],
    ) -> Result<Self, AttemptRecordError> {
        let Some(magic) = bytes.first_chunk::<MAGIC_LEN>() else {
            return Err(AttemptRecordError::Truncated {
                expected: MAGIC_LEN,
                actual: bytes.len(),
            });
        };
        let kind = admitted_kind(position, *magic)?;
        let expected = kind.record_len();
        if bytes.len() < expected {
            return Err(AttemptRecordError::Truncated {
                expected,
                actual: bytes.len(),
            });
        }
        if bytes.len() > expected {
            return Err(AttemptRecordError::TrailingBytes {
                expected,
                actual: bytes.len(),
            });
        }
        let mut record = [0_u8; MAX_RECORD_LEN];
        record[..expected].copy_from_slice(bytes);
        parse(position, kind, &record)
    }

    /// Writes this record's exact bytes with one `write_all`.
    ///
    /// # Errors
    ///
    /// [`AttemptRecordError::Io`] when the write fails.
    pub(crate) fn write_to<W: Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<(), AttemptRecordError> {
        let (bytes, len) = self.encode();
        writer
            .write_all(&bytes[..len])
            .map_err(AttemptRecordError::io)
    }

    fn encode(&self) -> ([u8; MAX_RECORD_LEN], usize) {
        let mut out = RecordWriter::default();
        out.put(&self.kind().magic());
        match self {
            Self::Claim(claim) => {
                out.put(&claim.installation_id);
                out.put(claim.client_nonce.as_bytes());
                out.put(&claim.client_pid.to_le_bytes());
            }
            Self::Challenge(challenge) => {
                out.put(&challenge.attempt_id);
                out.put(&challenge.health_channel_id);
                out.put(challenge.server_nonce.as_bytes());
                out.put(&challenge.server_pid.to_le_bytes());
            }
            Self::Acknowledgement(transcript) | Self::Receipt(transcript) => {
                out.put(&transcript.installation_id);
                out.put(&transcript.attempt_id);
                out.put(&transcript.health_channel_id);
                out.put(transcript.client_nonce.as_bytes());
                out.put(transcript.server_nonce.as_bytes());
                out.put(&transcript.client_pid.to_le_bytes());
                out.put(&transcript.server_pid.to_le_bytes());
            }
            Self::BootAcknowledgement(boot) => {
                out.put(&boot.attempt_id);
                out.put(&boot.health_channel_id);
                out.put(&boot.health_receipt_digest);
            }
            Self::Ready => {}
            Self::Failure(class) => out.put(&[*class as u8]),
            Self::HealthResult(result) => out.put(&[*result as u8]),
        }
        debug_assert_eq!(out.len, self.kind().record_len());
        (out.bytes, out.len)
    }
}

/// The magic-first admission rule: a magic outside the closed set and a known
/// magic that `position` does not admit are refused alike.
fn admitted_kind(
    position: AttemptReadPosition,
    magic: [u8; MAGIC_LEN],
) -> Result<AttemptRecordKind, AttemptRecordError> {
    AttemptRecordKind::from_magic(magic)
        .filter(|kind| position.admits(*kind))
        .ok_or(AttemptRecordError::MagicNotAdmitted { position, magic })
}

/// Decodes the fields of one whole record whose magic `position` admitted.
/// Both readers pass the record in a buffer as long as the longest record,
/// after their own length rule, so no field read can fall short.
fn parse(
    position: AttemptReadPosition,
    kind: AttemptRecordKind,
    record: &[u8; MAX_RECORD_LEN],
) -> Result<AttemptRecord, AttemptRecordError> {
    let mut fields = FieldReader {
        record,
        at: MAGIC_LEN,
    };
    let parsed = match kind {
        AttemptRecordKind::Claim => AttemptRecord::Claim(AttemptClaim {
            installation_id: fields.take(),
            client_nonce: SessionToken::from_bytes(fields.take()),
            client_pid: u32::from_le_bytes(fields.take()),
        }),
        AttemptRecordKind::Challenge => AttemptRecord::Challenge(AttemptChallenge {
            attempt_id: fields.take(),
            health_channel_id: fields.take(),
            server_nonce: SessionToken::from_bytes(fields.take()),
            server_pid: u32::from_le_bytes(fields.take()),
        }),
        AttemptRecordKind::Acknowledgement | AttemptRecordKind::Receipt => {
            let transcript = AttemptTranscript {
                installation_id: fields.take(),
                attempt_id: fields.take(),
                health_channel_id: fields.take(),
                client_nonce: SessionToken::from_bytes(fields.take()),
                server_nonce: SessionToken::from_bytes(fields.take()),
                client_pid: u32::from_le_bytes(fields.take()),
                server_pid: u32::from_le_bytes(fields.take()),
            };
            if kind == AttemptRecordKind::Acknowledgement {
                AttemptRecord::Acknowledgement(transcript)
            } else {
                AttemptRecord::Receipt(transcript)
            }
        }
        AttemptRecordKind::BootAcknowledgement => {
            AttemptRecord::BootAcknowledgement(AttemptBootAcknowledgement {
                attempt_id: fields.take(),
                health_channel_id: fields.take(),
                health_receipt_digest: fields.take(),
            })
        }
        AttemptRecordKind::Ready => AttemptRecord::Ready,
        AttemptRecordKind::Failure => {
            let [value] = fields.take();
            let class = AttemptFailureClass::from_byte(value)
                .ok_or(AttemptRecordError::ValueOutOfSet { kind, value })?;
            if !position.admits_failure(class) {
                return Err(AttemptRecordError::FailureClassNotAdmitted { position, class });
            }
            AttemptRecord::Failure(class)
        }
        AttemptRecordKind::HealthResult => {
            let [value] = fields.take();
            AttemptRecord::HealthResult(
                AttemptHealthResult::from_byte(value)
                    .ok_or(AttemptRecordError::ValueOutOfSet { kind, value })?,
            )
        }
    };
    debug_assert_eq!(fields.at, kind.record_len());
    Ok(parsed)
}

/// Sequential fixed-width field reads from a buffer that holds the longest
/// record, the mirror of [`RecordWriter`].
struct FieldReader<'a> {
    record: &'a [u8; MAX_RECORD_LEN],
    at: usize,
}

impl FieldReader<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let mut field = [0; N];
        field.copy_from_slice(&self.record[self.at..self.at + N]);
        self.at += N;
        field
    }
}

/// Sequential field writes into a buffer that holds the longest record.
struct RecordWriter {
    bytes: [u8; MAX_RECORD_LEN],
    len: usize,
}

impl Default for RecordWriter {
    fn default() -> Self {
        Self {
            bytes: [0; MAX_RECORD_LEN],
            len: 0,
        }
    }
}

impl RecordWriter {
    fn put(&mut self, field: &[u8]) {
        let end = self.len + field.len();
        self.bytes[self.len..end].copy_from_slice(field);
        self.len = end;
    }
}

/// Typed refusal of a `keld-attempt` record. Every refusal ends the exchange:
/// before `KELD-AR1` it refuses the claimant, and after it the owner cannot
/// commit health and rolls back (KEL-53 §4 *Health sequence*).
#[derive(Debug)]
pub enum AttemptRecordError {
    /// `KELD-IPC-015`: the magic is outside the closed set or not admitted at
    /// this position; nothing after it was read.
    MagicNotAdmitted {
        /// Where the record was read.
        position: AttemptReadPosition,
        /// The 8 bytes received.
        magic: [u8; 8],
    },
    /// `KELD-IPC-015`: fewer bytes than the magic or the admitted record.
    Truncated {
        /// The bytes the magic or record needs.
        expected: usize,
        /// The bytes present.
        actual: usize,
    },
    /// `KELD-IPC-015`: bytes follow the admitted record.
    TrailingBytes {
        /// The admitted record's length.
        expected: usize,
        /// The bytes present.
        actual: usize,
    },
    /// `KELD-IPC-015`: a `KELD-AF1` class or `KELD-AK1` result byte outside
    /// its closed set, `0` included.
    ValueOutOfSet {
        /// The record whose byte it is.
        kind: AttemptRecordKind,
        /// The byte received.
        value: u8,
    },
    /// `KELD-IPC-015`: a `KELD-AF1` class that is not admitted at this
    /// position (class `1` after `KELD-AB1`, class `2` before it).
    FailureClassNotAdmitted {
        /// Where the record was read.
        position: AttemptReadPosition,
        /// The class received.
        class: AttemptFailureClass,
    },
    /// `KELD-IPC-016`: the `KELD-AH1` installation ID is not the owner's.
    ForeignInstallation,
    /// `KELD-IPC-016`: the `KELD-AH1` client process ID is not the connected
    /// client's.
    ClientProcessMismatch {
        /// The process ID in the claim.
        claimed: u32,
        /// `GetNamedPipeClientProcessId`.
        connected: u32,
    },
    /// `KELD-IPC-016`: the `KELD-AC1` server process ID is not the connected
    /// server's.
    ServerProcessMismatch {
        /// The process ID in the challenge.
        challenged: u32,
        /// `GetNamedPipeServerProcessId`.
        connected: u32,
    },
    /// `KELD-IPC-016`: the locator over the claimed installation ID and the
    /// offered IDs refuses them or does not yield the rendezvous name.
    LocatorMismatch,
    /// `KELD-IPC-016`: the received `KELD-AA1` or `KELD-AR1` is not exactly the
    /// expected transcript.
    TranscriptMismatch,
    /// `KELD-IPC-016`: the received `KELD-AB1` is not exactly the expected
    /// boot acknowledgement.
    BootMismatch,
    /// `KELD-IPC-017`: end of file before a whole record, a read or write
    /// failure, or an expired deadline.
    Io {
        /// The I/O failure.
        source: io::Error,
    },
}

impl AttemptRecordError {
    const fn io(source: io::Error) -> Self {
        Self::Io { source }
    }
}

const MALFORMED_FIX: &str = "End the exchange without reading further: before KELD-AR1 \
     refuse the claimant, which then refuses its own start with WriterActive; after it, \
     the owner cannot commit health and rolls back, and a candidate that cannot read a \
     valid KELD-AK1 never arms its recovery gate.";
const MISMATCH_FIX: &str = "The peer is not bound to this attempt. End the exchange: \
     before KELD-AR1 refuse the claimant; after it, the owner rolls back.";
const IO_FIX: &str = "End the exchange; never retry or wait past the deadline: before \
     KELD-AR1 refuse the claimant; after it, the owner cannot commit health and rolls \
     back, and a candidate that cannot read a valid KELD-AK1 never arms its recovery gate.";

impl fmt::Display for AttemptRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MagicNotAdmitted { position, magic } => write!(
                f,
                "KELD-IPC-015: keld-attempt record refused (magic `{}` is not admitted {}). \
                 {MALFORMED_FIX}",
                magic.escape_ascii(),
                position.describe()
            ),
            Self::Truncated { expected, actual } => write!(
                f,
                "KELD-IPC-015: keld-attempt record refused (truncated: {actual} of {expected} \
                 bytes). {MALFORMED_FIX}"
            ),
            Self::TrailingBytes { expected, actual } => write!(
                f,
                "KELD-IPC-015: keld-attempt record refused ({actual} bytes where the record has \
                 {expected}). {MALFORMED_FIX}"
            ),
            Self::ValueOutOfSet { kind, value } => write!(
                f,
                "KELD-IPC-015: keld-attempt record refused ({kind} byte {value} is outside its \
                 closed set). {MALFORMED_FIX}"
            ),
            Self::FailureClassNotAdmitted { position, class } => write!(
                f,
                "KELD-IPC-015: keld-attempt record refused (KELD-AF1 class {} is not admitted \
                 {}). {MALFORMED_FIX}",
                *class as u8,
                position.describe()
            ),
            Self::ForeignInstallation => write!(
                f,
                "KELD-IPC-016: keld-attempt claim names another installation. {MISMATCH_FIX}"
            ),
            Self::ClientProcessMismatch { claimed, connected } => write!(
                f,
                "KELD-IPC-016: keld-attempt claim states process {claimed}, but the connected \
                 client is process {connected}. {MISMATCH_FIX}"
            ),
            Self::ServerProcessMismatch {
                challenged,
                connected,
            } => write!(
                f,
                "KELD-IPC-016: keld-attempt challenge states process {challenged}, but the \
                 connected server is process {connected}. {MISMATCH_FIX}"
            ),
            Self::LocatorMismatch => write!(
                f,
                "KELD-IPC-016: keld-attempt challenge IDs do not derive the rendezvous name. \
                 {MISMATCH_FIX}"
            ),
            Self::TranscriptMismatch => write!(
                f,
                "KELD-IPC-016: keld-attempt acknowledgement or receipt differs from the \
                 transcript. {MISMATCH_FIX}"
            ),
            Self::BootMismatch => write!(
                f,
                "KELD-IPC-016: keld-attempt boot acknowledgement differs from the journaled \
                 attempt. {MISMATCH_FIX}"
            ),
            Self::Io { source } => write!(
                f,
                "KELD-IPC-017: keld-attempt record I/O failed ({source}). {IO_FIX}"
            ),
        }
    }
}

impl std::error::Error for AttemptRecordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source } => Some(source),
            _ => None,
        }
    }
}

/// Raw-byte fuzz hook for the claim and health record codec (KEL-53 §6 S4b;
/// §7 "8 (keld-attempt codec)"). It exists only with the non-product
/// `fuzzing` feature, so the codec stays crate-private.
///
/// At every read position, decoding `bytes` terminates and either refuses
/// with a classified `KELD-IPC-*` error or admits a record whose canonical
/// encoding is the whole input; the stream reader agrees with the slice
/// decoder, consumes exactly one encoded record and never reads past a
/// refused magic. The locator check is Windows-only and covered by the
/// deterministic tests.
///
/// # Errors
///
/// The first violated property, which the fuzz target reports as a crash.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub fn fuzz_attempt_records(bytes: &[u8]) -> Result<(), String> {
    use AttemptReadPosition::{
        CandidateChallenge, CandidateHealthResult, CandidateReceipt, OwnerAcknowledgement,
        OwnerBoot, OwnerClaim, OwnerReady,
    };
    for position in [
        OwnerClaim,
        OwnerAcknowledgement,
        OwnerBoot,
        OwnerReady,
        CandidateChallenge,
        CandidateReceipt,
        CandidateHealthResult,
    ] {
        let decoded = AttemptRecord::decode(position, bytes);
        let mut stream = io::Cursor::new(bytes);
        let streamed = AttemptRecord::read_from(&mut stream, position);
        let consumed = usize::try_from(stream.position())
            .map_err(|_| format!("{position:?}: the cursor passed usize"))?;
        match (decoded, streamed) {
            (Ok(record), Ok(read)) => {
                let (encoded, len) = record.encode();
                if read != record {
                    return Err(format!("{position:?}: stream and slice decoders disagree"));
                }
                if encoded[..len] != *bytes {
                    return Err(format!("{position:?}: admitted bytes are not canonical"));
                }
                if consumed != bytes.len() {
                    return Err(format!("{position:?}: the stream read {consumed} bytes"));
                }
            }
            (Ok(record), Err(error)) => {
                return Err(format!(
                    "{position:?}: the slice admitted {record:?}, the stream refused: {error}"
                ));
            }
            (Err(error), Ok(read)) => {
                // Only a whole record followed by more bytes reads from a stream.
                let (encoded, len) = read.encode();
                if !matches!(error, AttemptRecordError::TrailingBytes { .. })
                    || consumed != len
                    || bytes.len() <= len
                    || !bytes.starts_with(&encoded[..len])
                {
                    return Err(format!(
                        "{position:?}: the stream admitted {read:?} after {consumed} bytes, the \
                         slice refused: {error}"
                    ));
                }
            }
            (Err(decode_error), Err(read_error)) => {
                for error in [&decode_error, &read_error] {
                    let text = error.to_string();
                    if !text.starts_with("KELD-IPC-01") {
                        return Err(format!("{position:?}: unclassified refusal: {text}"));
                    }
                }
                if matches!(read_error, AttemptRecordError::MagicNotAdmitted { .. })
                    && consumed != MAGIC_LEN
                {
                    return Err(format!(
                        "{position:?}: the stream read {consumed} bytes past a refused magic"
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
