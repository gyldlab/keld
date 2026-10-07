//! The `keld-attempt` claim, `KELD-AH1` to `KELD-AR1` (KEL-53 §4 "Candidate
//! connect-back": *Acceptance*, *Transcript*, *Nonces and purpose* and
//! *Refusal*; approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06).
//!
//! The owner side runs inside [`WindowsAttemptEndpoint::accept_claim`]. Per
//! connection, under a per-connection deadline that never outlasts the claim
//! deadline, it reads the client process ID and session right after connect,
//! reads `KELD-AH1` magic first, requires its installation ID to be the
//! endpoint's own and its process ID the connected one, reads the claim
//! writer's token at identification level, and asks the caller's claimant
//! check for the claimant's retained process pin. Only then does it send
//! `KELD-AC1`, from the IDs the endpoint's name derives from and a fresh
//! server nonce. It requires `KELD-AA1` to equal the transcript exactly,
//! consumes the instance's one-shot, and only then sends `KELD-AR1`. Any
//! refusal disconnects that client and re-arms the same instance: it consumes
//! no one-shot and never extends the claim deadline.
//!
//! The claimant side runs inside [`WindowsAttemptClient::claim`]: a fresh
//! client nonce in `KELD-AH1`, then the locator check over `KELD-AC1` before
//! it sends `KELD-AA1`, so no caller can skip it, then `KELD-AR1` exactly.

use std::io;
use std::time::{Duration, Instant};

use super::channel::{
    WindowsAttemptClaimantChannel, WindowsAttemptExchangeError, WindowsAttemptOwnerChannel,
};
use super::records::{
    AttemptChallenge, AttemptClaim, AttemptReadPosition, AttemptRecord, AttemptRecordError,
    AttemptTranscript,
};
use super::{WindowsAttemptClient, WindowsAttemptEndpoint, WindowsAttemptEndpointError};
use crate::APP_LINK_IO_DEADLINE;
use crate::bootstrap::{WindowsLifecyclePeerPin, peer_handshake_window};
use crate::token::SessionToken;
use crate::windows_named_pipe::{WaitOutcome, WindowsNamedPipeStream, WindowsPeerTokenFacts};

/// Why the owner refused one connection. The owner disconnects that client
/// and re-arms the same instance.
#[derive(Debug)]
#[expect(
    dead_code,
    reason = "the causes are kept for the test observer and Debug: production disconnects and \
              re-arms on every refusal alike and reports none (KEL-53 §4 *Refusal*), and no test \
              reads an OS failure's payload"
)]
pub(super) enum ClaimRefusal {
    /// `GetNamedPipeClientProcessId` or `GetNamedPipeClientSessionId` failed.
    ClientIdentity(io::Error),
    /// A claim record was refused, mismatched, truncated or late, or its I/O
    /// failed: `KELD-AH1` (including a foreign installation or process ID),
    /// the `KELD-AC1` write or `KELD-AA1`.
    Record(AttemptRecordError),
    /// The claim writer's token could not be read.
    Token(io::Error),
    /// The caller's claimant check returned no pin.
    ClaimantCheck,
    /// The returned pin, or the token's session, is not the connected
    /// client's.
    PinMismatch,
    /// The pinned claimant has exited, or its state could not be read.
    ClaimantExited,
}

/// What [`WindowsAttemptEndpoint::accept_claim`] takes as given. Production
/// passes the landed per-connection deadline and the pipe's token reader;
/// tests shorten the one and substitute the other, and observe refusals.
pub(super) struct ClaimSeams<'a> {
    /// The longest one connection may take, clamped to the claim deadline.
    pub(super) per_connection: Duration,
    /// Reads the token facts of the client that wrote the last record.
    pub(super) read_token:
        &'a mut dyn FnMut(&WindowsNamedPipeStream) -> io::Result<WindowsPeerTokenFacts>,
    /// Observes each refusal before the owner re-arms.
    pub(super) refused: &'a mut dyn FnMut(ClaimRefusal),
}

/// One connection's result. The accepted channel is boxed: it is far larger
/// than a refusal, and a claim accepts at most once.
enum Connection<P> {
    Accepted(Box<WindowsAttemptOwnerChannel<P>>),
    Refused(ClaimRefusal),
    /// The claim deadline passed during this connection.
    Expired,
}

impl WindowsAttemptEndpoint {
    /// Accepts the one claimant of this endpoint before `deadline`, running the
    /// whole claim inside the endpoint (KEL-53 §4 "Candidate connect-back").
    ///
    /// `check_claimant` is the caller's claimant check. It receives the
    /// connected client's process ID (`GetNamedPipeClientProcessId`, read right
    /// after connect), its session (`GetNamedPipeClientSessionId`) and the
    /// claim writer's token facts, read by impersonating it at identification
    /// level and reverting before anything else. It returns the claimant's
    /// retained process pin only when that process is the launched and
    /// retained one and the token is the initiating one; the endpoint then
    /// requires the pin's process ID and session, and the token's session, to
    /// be the connected client's, and the pin to report the process running.
    ///
    /// Each connection runs under the landed per-connection deadline,
    /// [`APP_LINK_IO_DEADLINE`], clamped to `deadline`. A refusal of any kind
    /// disconnects that client and re-arms the same instance: only the
    /// accepted claimant consumes the instance's one-shot, which happens
    /// before `KELD-AR1` is sent, and no refusal extends `deadline`.
    ///
    /// # Errors
    ///
    /// - [`WindowsAttemptExchangeError::ClaimDeadline`] when `deadline` passes
    ///   first; the endpoint is closed;
    /// - [`WindowsAttemptExchangeError::Endpoint`] when a Windows call that no
    ///   client caused fails (waiting, re-arming, a nonce draw);
    /// - [`WindowsAttemptExchangeError::Record`] when sending `KELD-AR1` fails
    ///   after the one-shot was consumed.
    ///
    /// Every error leaves no accepted claimant: the owner rolls back.
    pub fn accept_claim<P: WindowsLifecyclePeerPin>(
        self,
        deadline: Instant,
        check_claimant: impl FnMut(u32, u32, &WindowsPeerTokenFacts) -> Option<P>,
    ) -> Result<WindowsAttemptOwnerChannel<P>, WindowsAttemptExchangeError> {
        let mut read_token = |stream: &WindowsNamedPipeStream| stream.last_client_token_facts();
        let mut refused = |_: ClaimRefusal| {};
        self.accept_claim_with(
            deadline,
            check_claimant,
            &mut ClaimSeams {
                per_connection: APP_LINK_IO_DEADLINE,
                read_token: &mut read_token,
                refused: &mut refused,
            },
        )
    }

    pub(super) fn accept_claim_with<P: WindowsLifecyclePeerPin>(
        self,
        deadline: Instant,
        mut check_claimant: impl FnMut(u32, u32, &WindowsPeerTokenFacts) -> Option<P>,
        seams: &mut ClaimSeams<'_>,
    ) -> Result<WindowsAttemptOwnerChannel<P>, WindowsAttemptExchangeError> {
        loop {
            if Instant::now() >= deadline {
                return Err(self.expire());
            }
            match self
                .server
                .accept_until(Some(deadline))
                .map_err(|source| os("wait for a claimant", source))?
            {
                WaitOutcome::Ready => {}
                WaitOutcome::PeerClosed => {
                    self.rearm()?;
                    continue;
                }
                // The wait closed the instance itself.
                WaitOutcome::Cancelled | WaitOutcome::DeadlineElapsed => {
                    return Err(WindowsAttemptExchangeError::ClaimDeadline);
                }
            }
            match self.exchange_one(deadline, &mut check_claimant, seams)? {
                Connection::Accepted(channel) => return Ok(*channel),
                Connection::Refused(refusal) => {
                    (seams.refused)(refusal);
                    self.rearm()?;
                }
                Connection::Expired => return Err(self.expire()),
            }
        }
    }

    /// One connection, from the connected instance to `KELD-AR1`.
    fn exchange_one<P: WindowsLifecyclePeerPin>(
        &self,
        deadline: Instant,
        check_claimant: &mut impl FnMut(u32, u32, &WindowsPeerTokenFacts) -> Option<P>,
        seams: &mut ClaimSeams<'_>,
    ) -> Result<Connection<P>, WindowsAttemptExchangeError> {
        let Some((_, per_connection)) =
            peer_handshake_window(Instant::now(), Some(deadline), seams.per_connection)
        else {
            return Ok(Connection::Expired);
        };
        let mut stream = self
            .server
            .stream()
            .map_err(|source| os("open the connected instance", source))?;
        stream.set_absolute_deadline(Some(per_connection.instant()));
        let client_pid = match stream.peer_process_id() {
            Ok(pid) => pid,
            Err(source) => return Ok(Connection::Refused(ClaimRefusal::ClientIdentity(source))),
        };
        let client_session = match stream.peer_session_id() {
            Ok(session) => session,
            Err(source) => return Ok(Connection::Refused(ClaimRefusal::ClientIdentity(source))),
        };
        let position = AttemptReadPosition::OwnerClaim;
        let claim = match AttemptRecord::read_from(&mut stream, position) {
            Ok(AttemptRecord::Claim(claim)) => claim,
            Ok(other) => return Ok(refused_record(other.not_admitted_at(position))),
            Err(error) => return Ok(refused_record(error)),
        };
        let server_nonce =
            SessionToken::random().map_err(|source| os("draw the server nonce", source))?;
        let challenge = AttemptChallenge::new(
            self.ids.attempt,
            self.ids.health_channel,
            server_nonce,
            std::process::id(),
        );
        let transcript = match AttemptTranscript::for_owner(
            &claim,
            &self.ids.installation,
            client_pid,
            &challenge,
        ) {
            Ok(transcript) => transcript,
            Err(error) => return Ok(refused_record(error)),
        };
        let token = match (seams.read_token)(&stream) {
            Ok(token) => token,
            Err(source) => return Ok(Connection::Refused(ClaimRefusal::Token(source))),
        };
        let Some(pin) = check_claimant(client_pid, client_session, &token) else {
            return Ok(Connection::Refused(ClaimRefusal::ClaimantCheck));
        };
        if pin.process_id() != client_pid
            || pin.session_id() != client_session
            || token.session_id != client_session
        {
            return Ok(Connection::Refused(ClaimRefusal::PinMismatch));
        }
        if !matches!(pin.has_exited(), Ok(false)) {
            return Ok(Connection::Refused(ClaimRefusal::ClaimantExited));
        }
        if let Err(error) = AttemptRecord::Challenge(challenge).write_to(&mut stream) {
            return Ok(refused_record(error));
        }
        let position = AttemptReadPosition::OwnerAcknowledgement;
        let acknowledged = match AttemptRecord::read_from(&mut stream, position) {
            Ok(AttemptRecord::Acknowledgement(acknowledged)) => acknowledged,
            Ok(other) => return Ok(refused_record(other.not_admitted_at(position))),
            Err(error) => return Ok(refused_record(error)),
        };
        if let Err(error) = transcript.require_match(&acknowledged) {
            return Ok(refused_record(error));
        }
        if !matches!(pin.has_exited(), Ok(false)) {
            return Ok(Connection::Refused(ClaimRefusal::ClaimantExited));
        }
        if Instant::now() >= deadline {
            return Ok(Connection::Expired);
        }
        self.server.consume();
        AttemptRecord::Receipt(transcript).write_to(&mut stream)?;
        stream.set_absolute_deadline(None);
        Ok(Connection::Accepted(Box::new(
            WindowsAttemptOwnerChannel::accepted(stream, pin, transcript),
        )))
    }

    /// Disconnects a refused or vanished client and re-arms the instance.
    fn rearm(&self) -> Result<(), WindowsAttemptExchangeError> {
        self.server
            .disconnect_for_retry()
            .map_err(|source| os("disconnect a refused claimant", source))
    }

    /// Closes the instance at the claim deadline.
    fn expire(&self) -> WindowsAttemptExchangeError {
        match self.server.close_terminal() {
            Ok(()) => WindowsAttemptExchangeError::ClaimDeadline,
            Err(source) => os("close the endpoint at its deadline", source),
        }
    }
}

impl WindowsAttemptClient {
    /// Claims the endpoint this client opened, before `deadline`, running the
    /// whole claim inside the client (KEL-53 §4 "Candidate connect-back").
    ///
    /// It sends `KELD-AH1` with `installation_id`, which the caller learned
    /// only from its immutable provenance record, a fresh nonce and this
    /// process's ID. It requires `KELD-AC1` to state the connected server's
    /// process ID (`GetNamedPipeServerProcessId`) and the locator over
    /// `installation_id` and the offered IDs to yield exactly the rendezvous
    /// name this client opened, before it sends `KELD-AA1`, and then requires
    /// `KELD-AR1` to equal that transcript exactly. A caller that checks the
    /// server's image does so on [`Self::server_process_id`] before this call,
    /// which sends the first byte.
    ///
    /// # Errors
    ///
    /// - [`WindowsAttemptExchangeError::Record`] for a refused, mismatched or
    ///   truncated record, end of file (the owner refused this claimant), a
    ///   failed write or the deadline;
    /// - [`WindowsAttemptExchangeError::Endpoint`] when the server process ID
    ///   query or the nonce draw fails.
    ///
    /// Every error is a refusal: the candidate refuses its own start with a
    /// typed `WriterActive` before app code.
    pub fn claim(
        mut self,
        installation_id: &[u8; 32],
        deadline: Instant,
    ) -> Result<WindowsAttemptClaimantChannel, WindowsAttemptExchangeError> {
        self.stream.set_absolute_deadline(Some(deadline));
        let server_pid = self.server_process_id()?;
        let client_nonce =
            SessionToken::random().map_err(|source| os("draw the client nonce", source))?;
        let claim = AttemptClaim::new(*installation_id, client_nonce, std::process::id());
        AttemptRecord::Claim(claim).write_to(&mut self.stream)?;
        let position = AttemptReadPosition::CandidateChallenge;
        let challenge = match AttemptRecord::read_from(&mut self.stream, position)? {
            AttemptRecord::Challenge(challenge) => challenge,
            other => return Err(other.not_admitted_at(position).into()),
        };
        let transcript =
            AttemptTranscript::for_claimant(&claim, &challenge, &self.endpoint, server_pid)?;
        AttemptRecord::Acknowledgement(transcript).write_to(&mut self.stream)?;
        let position = AttemptReadPosition::CandidateReceipt;
        let receipt = match AttemptRecord::read_from(&mut self.stream, position)? {
            AttemptRecord::Receipt(receipt) => receipt,
            other => return Err(other.not_admitted_at(position).into()),
        };
        transcript.require_match(&receipt)?;
        self.stream.set_absolute_deadline(None);
        Ok(WindowsAttemptClaimantChannel::accepted(
            self.stream,
            transcript,
        ))
    }
}

fn refused_record<P>(error: AttemptRecordError) -> Connection<P> {
    Connection::Refused(ClaimRefusal::Record(error))
}

fn os(operation: &'static str, source: io::Error) -> WindowsAttemptExchangeError {
    WindowsAttemptExchangeError::Endpoint(WindowsAttemptEndpointError::Os { operation, source })
}
