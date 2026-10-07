//! Fixtures for the `keld-attempt` exchange scenarios: a test claimant pin,
//! fresh connect-back IDs, a raw peer that writes and reads records directly
//! on the pipe, and a claimed owner/candidate pair built through the shipped
//! exchange. Fixture checks only validate prerequisites; each scenario owns
//! its oracle.

use std::io::{self, Read as _};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::oracle::{own_session_id, own_user_sid_bytes};
use crate::APP_LINK_IO_DEADLINE;
use crate::attempt::claim::{ClaimRefusal, ClaimSeams};
use crate::attempt::records::{
    AttemptClaim, AttemptReadPosition, AttemptRecord, AttemptRecordError,
};
use crate::attempt::{
    WindowsAttemptClaimantChannel, WindowsAttemptClient, WindowsAttemptEndpoint,
    WindowsAttemptEndpointSecurity, WindowsAttemptExchangeError, WindowsAttemptOwnerChannel,
};
use crate::bootstrap::WindowsLifecyclePeerPin;
use crate::token::SessionToken;
use crate::windows_named_pipe::{
    WindowsNamedPipeServer, WindowsNamedPipeStream, WindowsPeerTokenFacts,
};

/// A kill switch for every wait in these scenarios: far longer than any
/// expected step, never a synchronization point.
pub(super) const KILL_SWITCH: Duration = Duration::from_secs(20);

pub(super) fn kill_switch() -> Instant {
    Instant::now() + KILL_SWITCH
}

/// The health-receipt digest the health scenarios journal and acknowledge.
pub(super) const DIGEST: [u8; 32] = [0x66; 32];

/// A window short enough for a test to wait it out.
pub(super) const SHORT_WINDOW: Duration = Duration::from_millis(200);

/// The connect-back IDs of one test attempt: fresh random values, so
/// concurrent scenarios never derive the same endpoint name.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ids {
    pub(super) installation: [u8; 32],
    pub(super) attempt: [u8; 32],
    pub(super) channel: [u8; 32],
}

pub(super) fn fresh_ids() -> io::Result<Ids> {
    Ok(Ids {
        installation: *SessionToken::random()?.as_bytes(),
        attempt: *SessionToken::random()?.as_bytes(),
        channel: *SessionToken::random()?.as_bytes(),
    })
}

pub(super) fn per_user_security() -> io::Result<WindowsAttemptEndpointSecurity> {
    WindowsAttemptEndpointSecurity::per_user_connect_back(&own_user_sid_bytes()?)
        .map_err(io::Error::other)
}

pub(super) fn connect_back_endpoint(ids: Ids) -> io::Result<WindowsAttemptEndpoint> {
    WindowsAttemptEndpoint::create_connect_back(
        &ids.installation,
        &ids.attempt,
        &ids.channel,
        &per_user_security()?,
    )
    .map_err(io::Error::other)
}

/// The claimant pin that the caller's check returns. It stands in for
/// `keld-runtime`'s process pin, which S5 tests against real processes; here
/// it proves only that the exchange consults it.
#[derive(Debug, Clone)]
pub(super) struct TestPin {
    pub(super) process_id: u32,
    pub(super) session_id: u32,
    pub(super) exited: Arc<AtomicBool>,
}

impl TestPin {
    /// This process, which every in-process claimant is.
    pub(super) fn own() -> io::Result<Self> {
        Ok(Self {
            process_id: std::process::id(),
            session_id: own_session_id()?,
            exited: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) fn report_exit(&self) {
        self.exited.store(true, Ordering::Release);
    }
}

impl WindowsLifecyclePeerPin for TestPin {
    fn process_id(&self) -> u32 {
        self.process_id
    }

    fn session_id(&self) -> u32 {
        self.session_id
    }

    fn has_exited(&self) -> io::Result<bool> {
        Ok(self.exited.load(Ordering::Acquire))
    }
}

/// The caller's claimant check: every claimant is this process.
pub(super) fn admit_own(
    pin: &TestPin,
) -> impl FnMut(u32, u32, &WindowsPeerTokenFacts) -> Option<TestPin> + use<> {
    let pin = pin.clone();
    move |_, _, _| Some(pin.clone())
}

/// What an owner thread returns: its result and every refusal it observed,
/// in order.
pub(super) type OwnerRun = (
    Result<WindowsAttemptOwnerChannel<TestPin>, WindowsAttemptExchangeError>,
    Vec<ClaimRefusal>,
);

/// Runs `accept_claim` on its own thread with `per_connection`, the token
/// reader `read_token` and the caller's check `check`, recording refusals and
/// reporting each one on `refusals` as it happens.
pub(super) fn spawn_owner<C, T>(
    endpoint: WindowsAttemptEndpoint,
    deadline: Instant,
    per_connection: Duration,
    check: C,
    mut read_token: T,
    refusals: mpsc::Sender<()>,
) -> thread::JoinHandle<OwnerRun>
where
    C: FnMut(u32, u32, &WindowsPeerTokenFacts) -> Option<TestPin> + Send + 'static,
    T: FnMut(&WindowsNamedPipeStream) -> io::Result<WindowsPeerTokenFacts> + Send + 'static,
{
    thread::spawn(move || {
        let mut observed = Vec::new();
        let mut refused = |refusal: ClaimRefusal| {
            observed.push(refusal);
            let _ = refusals.send(());
        };
        let result = endpoint.accept_claim_with(
            deadline,
            check,
            &mut ClaimSeams {
                per_connection,
                read_token: &mut read_token,
                refused: &mut refused,
            },
        );
        (result, observed)
    })
}

/// The production token reader.
pub(super) fn pipe_token(stream: &WindowsNamedPipeStream) -> io::Result<WindowsPeerTokenFacts> {
    stream.last_client_token_facts()
}

/// An owner thread with the production seams that admits this process.
pub(super) fn spawn_admitting_owner(
    endpoint: WindowsAttemptEndpoint,
    pin: &TestPin,
) -> (thread::JoinHandle<OwnerRun>, mpsc::Receiver<()>) {
    let (refusals, refused) = mpsc::channel();
    let owner = spawn_owner(
        endpoint,
        kill_switch(),
        APP_LINK_IO_DEADLINE,
        admit_own(pin),
        pipe_token,
        refusals,
    );
    (owner, refused)
}

pub(super) fn join(owner: thread::JoinHandle<OwnerRun>) -> io::Result<OwnerRun> {
    owner
        .join()
        .map_err(|_| io::Error::other("the owner thread panicked"))
}

/// Connects the shipped client to `endpoint` and claims it: the outer result
/// is the fixture's, the inner one the exchange's.
pub(super) fn claim(
    endpoint: &str,
    installation: &[u8; 32],
) -> io::Result<Result<WindowsAttemptClaimantChannel, WindowsAttemptExchangeError>> {
    let security = per_user_security()?;
    Ok(
        WindowsAttemptClient::connect_until(endpoint, &security, kill_switch())
            .map_err(WindowsAttemptExchangeError::from)
            .and_then(|client| client.claim(installation, kill_switch())),
    )
}

/// An owner and a candidate after `KELD-AR1`, through the shipped exchange.
pub(super) fn claimed_pair() -> io::Result<(
    WindowsAttemptOwnerChannel<TestPin>,
    WindowsAttemptClaimantChannel,
    TestPin,
)> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let pin = TestPin::own()?;
    let (owner, _refused) = spawn_admitting_owner(endpoint, &pin);
    let candidate = claim(&name, &ids.installation)?.map_err(io::Error::other)?;
    let (owner, refusals) = join(owner)?;
    if !refusals.is_empty() {
        return Err(io::Error::other(format!(
            "fixture claim refused: {refusals:?}"
        )));
    }
    Ok((owner.map_err(io::Error::other)?, candidate, pin))
}

/// A claimed pair whose candidate already sent `KELD-AB1` and `KELD-AY1` and
/// whose owner read both.
pub(super) fn ready_pair(
    digest: &[u8; 32],
) -> io::Result<(
    WindowsAttemptOwnerChannel<TestPin>,
    WindowsAttemptClaimantChannel,
    TestPin,
)> {
    let (mut owner, mut candidate, pin) = claimed_pair()?;
    candidate.send_boot(digest).map_err(io::Error::other)?;
    owner
        .read_boot(digest, kill_switch())
        .map_err(io::Error::other)?;
    candidate.send_ready().map_err(io::Error::other)?;
    owner.read_ready(kill_switch()).map_err(io::Error::other)?;
    Ok((owner, candidate, pin))
}

/// A raw claim from this process for `installation`, stating `pid`.
pub(super) fn raw_claim(installation: [u8; 32], pid: u32) -> io::Result<AttemptClaim> {
    Ok(AttemptClaim::new(
        installation,
        SessionToken::random()?,
        pid,
    ))
}

/// Plays one refused claimant on `name`: `send` writes what it dictates, and
/// the claimant then reads end of file, the owner's refusal.
pub(super) fn refused_claimant(
    name: &str,
    send: impl FnOnce(&mut WindowsNamedPipeStream) -> io::Result<()>,
) -> io::Result<()> {
    let mut client = raw_client(name)?;
    send(&mut client)?;
    expect_end_of_file(&mut client)
}

/// A raw client on `endpoint`: it opens the pipe as the shipped client does
/// and then writes and reads whatever a scenario dictates.
pub(super) fn raw_client(endpoint: &str) -> io::Result<WindowsNamedPipeStream> {
    let stream =
        WindowsNamedPipeServer::connect_identification_client_until(endpoint, kill_switch())?;
    stream.set_absolute_deadline(Some(kill_switch()));
    Ok(stream)
}

pub(super) fn write(stream: &mut WindowsNamedPipeStream, record: AttemptRecord) -> io::Result<()> {
    record.write_to(stream).map_err(io::Error::other)
}

pub(super) fn read(
    stream: &mut WindowsNamedPipeStream,
    position: AttemptReadPosition,
) -> Result<AttemptRecord, AttemptRecordError> {
    AttemptRecord::read_from(stream, position)
}

/// Reads until the peer closes: `Ok(())` for end of file with nothing before
/// it, so the peer sent nothing more.
pub(super) fn expect_end_of_file(stream: &mut WindowsNamedPipeStream) -> io::Result<()> {
    let mut byte = [0_u8; 1];
    match stream.read(&mut byte)? {
        0 => Ok(()),
        _ => Err(io::Error::other(format!(
            "the peer sent byte {:#04x} instead of closing",
            byte[0]
        ))),
    }
}

/// Whether `error` is end of file, as `read_exact` reports it.
pub(super) fn is_end_of_file(error: &WindowsAttemptExchangeError) -> bool {
    matches!(
        error,
        WindowsAttemptExchangeError::Record(AttemptRecordError::Io { source })
            if source.kind() == io::ErrorKind::UnexpectedEof
    )
}

/// Whether `error` is the deadline-bounded read's expiry, `ERROR_SEM_TIMEOUT`.
pub(super) fn is_deadline(error: &AttemptRecordError) -> bool {
    matches!(error, AttemptRecordError::Io { source } if source.raw_os_error() == Some(121))
}
