//! Host-owned authenticated bootstrap listeners.
//!
//! This cold-path primitive owns a platform listener, a fresh `HELLO`
//! possession token, and cleanup. Unix uses an owner-only socket directory.
//! Windows uses a current-user-DACL named pipe; Unix uses an owner-only socket
//! directory. Both deliberately accept another client after an invalid
//! handshake so an untrusted connector cannot consume the legitimate role's
//! bootstrap.

#[cfg(windows)]
use core::fmt::Write as _;
#[cfg(unix)]
use std::fs;
use std::io;
#[cfg(windows)]
use std::io::{Read as _, Write as _};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

use crate::IpcError;
use crate::{APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL};
// Re-exported, not merely imported: these were public at
// `keld_ipc::bootstrap::{BootstrapRejection, BootstrapRejectionObserver}`
// before the taxonomy moved to `admission`, and a crate-root export does not
// preserve that path. Moving the owner must not break the published one.
pub use crate::admission::{BootstrapRejection, BootstrapRejectionObserver};
use crate::link::{
    AppLinkDeadlines, handshake_client_rendezvous, handshake_server_interruptible_until,
    handshake_server_rendezvous,
};
use crate::receive::AbsoluteDeadline;
use crate::token::{SessionToken, format_app_link};
#[cfg(windows)]
use crate::windows_named_pipe::{
    WaitOutcome, WindowsNamedPipeCanceller, WindowsNamedPipeServer, WindowsNamedPipeStream,
    WindowsPeerTokenFacts,
};

#[cfg(unix)]
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
#[cfg(unix)]
static UNIQUE_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Platform-selected connected stream returned after bootstrap authentication.
#[cfg(unix)]
pub type BootstrapStream = UnixStream;

/// Platform-selected connected stream returned after bootstrap authentication.
#[cfg(windows)]
pub type BootstrapStream = WindowsNamedPipeBootstrapStream;

/// Host-owned listener that authenticates one role bootstrap connection.
///
/// The listener is a cold setup mechanism, not a general application channel.
/// It remains available after rejected `HELLO` frames until a valid role
/// connects or [`Self::shutdown`] is requested.
#[derive(Debug)]
pub struct BootstrapListener {
    #[cfg(unix)]
    listener: Mutex<Option<UnixListener>>,
    #[cfg(windows)]
    listener: WindowsNamedPipeBootstrapListener,
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(unix)]
    session_dir: PathBuf,
    #[cfg(unix)]
    token: SessionToken,
    #[cfg(unix)]
    stopping: Arc<AtomicBool>,
    #[cfg(unix)]
    listening: Arc<AtomicBool>,
    #[cfg(unix)]
    active_stream: Arc<Mutex<Option<BootstrapStream>>>,
    #[cfg(all(test, unix))]
    handshake_witness: Mutex<Option<TestHandshakeWitness>>,
    #[cfg(all(test, unix))]
    before_consume: Mutex<Option<TestConsumeGate>>,
}

/// Result of one bounded bootstrap admission attempt.
#[derive(Debug)]
pub enum BootstrapAdmissionFor<S> {
    /// A peer proved possession of the listener's token.
    Authenticated(S),
    /// The host cancelled admission before authentication completed.
    Cancelled,
    /// The generation-wide admission deadline elapsed before authentication.
    DeadlineElapsed,
}

/// Admission result for the currently selected platform stream.
///
/// This concrete alias preserves the original public construction syntax,
/// including unconstrained terminal variants such as
/// `BootstrapAdmission::Cancelled`.
pub type BootstrapAdmission = BootstrapAdmissionFor<BootstrapStream>;

/// Admission result for the opt-in Windows named-pipe stream.
#[cfg(windows)]
pub type WindowsNamedPipeBootstrapAdmission =
    BootstrapAdmissionFor<WindowsNamedPipeBootstrapStream>;

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct TestHandshakeEntry {
    entered_at: Instant,
    generation_deadline: Option<Instant>,
    peer_deadline: Instant,
}

#[cfg(test)]
#[derive(Debug)]
struct TestHandshakeWitness {
    entered: std::sync::mpsc::SyncSender<TestHandshakeEntry>,
}

#[cfg(test)]
#[derive(Debug)]
struct TestConsumeGate {
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
fn report_test_handshake_entry(
    witness: &Mutex<Option<TestHandshakeWitness>>,
    generation_deadline: Option<Instant>,
    peer_deadline: Instant,
) {
    if let Some(witness) = witness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    {
        let _ = witness.entered.send(TestHandshakeEntry {
            entered_at: Instant::now(),
            generation_deadline,
            peer_deadline,
        });
    }
}

#[cfg(test)]
fn wait_at_test_consume_gate(gate: &Mutex<Option<TestConsumeGate>>) {
    if let Some(gate) = gate.lock().unwrap_or_else(PoisonError::into_inner).take() {
        let _ = gate.entered.send(());
        let _ = gate.release.recv();
    }
}

#[cfg(test)]
mod admission_type_tests {
    use super::BootstrapAdmission;

    #[test]
    fn terminal_variant_keeps_original_unconstrained_construction() {
        let admission = BootstrapAdmission::Cancelled;
        assert!(matches!(admission, BootstrapAdmission::Cancelled));
    }
}

#[cfg(test)]
mod peer_handshake_window_tests {
    use std::time::{Duration, Instant};

    use super::peer_handshake_window;

    #[test]
    fn generation_and_handshake_limits_share_one_start_in_both_orderings() {
        let started = Instant::now();
        let generation = started + Duration::from_millis(100);
        let (timeout, deadline) =
            peer_handshake_window(started, Some(generation), Duration::from_secs(5))
                .expect("generation window");
        assert_eq!(timeout, Duration::from_millis(100));
        assert_eq!(deadline.instant(), generation);

        let handshake_limit = Duration::from_millis(40);
        let (timeout, deadline) = peer_handshake_window(started, Some(generation), handshake_limit)
            .expect("handshake window");
        assert_eq!(timeout, handshake_limit);
        assert_eq!(deadline.instant(), started + handshake_limit);

        assert!(
            peer_handshake_window(started, Some(started), Duration::from_secs(5)).is_none(),
            "an expired generation must not mint a peer window"
        );
    }
}

#[cfg(test)]
mod admission_deadline_tests;

struct NoopRejectionObserver;

impl BootstrapRejectionObserver for NoopRejectionObserver {
    fn rejected(&self, _rejection: BootstrapRejection) {}
}

fn peer_handshake_window(
    started: Instant,
    generation_deadline: Option<Instant>,
    handshake_limit: Duration,
) -> Option<(Duration, AbsoluteDeadline)> {
    let timeout = match generation_deadline {
        Some(deadline) => deadline
            .checked_duration_since(started)?
            .min(handshake_limit),
        None => handshake_limit,
    };
    if timeout.is_zero() {
        return None;
    }
    Some((timeout, AbsoluteDeadline::at(started.checked_add(timeout)?)))
}

/// Cancellation handle for a blocked bootstrap admission worker.
#[derive(Debug, Clone)]
pub struct BootstrapCancellation {
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(windows)]
    cancellation: WindowsNamedPipeBootstrapCancellation,
    #[cfg(unix)]
    stopping: Arc<AtomicBool>,
    #[cfg(unix)]
    listening: Arc<AtomicBool>,
    #[cfg(unix)]
    active_stream: Arc<Mutex<Option<BootstrapStream>>>,
}

impl BootstrapListener {
    /// Binds a fresh platform endpoint and mints its session token.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the random source or platform listener cannot
    /// be created.
    pub fn bind() -> io::Result<Self> {
        #[cfg(unix)]
        let token = SessionToken::random()?;
        #[cfg(unix)]
        {
            let session_dir = unique_session_dir()?;
            let path = session_dir.join("app.sock");
            let listener = match UnixListener::bind(&path) {
                Ok(listener) => listener,
                Err(error) => {
                    let _ = fs::remove_dir_all(&session_dir);
                    return Err(error);
                }
            };
            listener.set_nonblocking(true)?;
            Ok(Self {
                listener: Mutex::new(Some(listener)),
                path,
                session_dir,
                token,
                stopping: Arc::new(AtomicBool::new(false)),
                listening: Arc::new(AtomicBool::new(true)),
                active_stream: Arc::new(Mutex::new(None)),
                #[cfg(test)]
                handshake_witness: Mutex::new(None),
                #[cfg(test)]
                before_consume: Mutex::new(None),
            })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                listener: WindowsNamedPipeBootstrapListener::bind()?,
            })
        }
    }

    /// Binds a stable Windows lifecycle locator from a trusted install/user
    /// identity. The existing HELLO token is only a bearer check; this method
    /// does not authenticate the connected process. The caller MUST validate
    /// its retained connected-peer process/token/image identity before moving
    /// any lifecycle or installation capability.
    ///
    /// # Errors
    ///
    /// Returns the underlying Windows listener creation/ACL validation error.
    #[cfg(windows)]
    pub fn bind_windows_rendezvous(
        install_user_locator: [u8; 32],
        token: SessionToken,
    ) -> io::Result<Self> {
        Ok(Self {
            listener: WindowsNamedPipeBootstrapListener::bind_rendezvous(
                install_user_locator,
                token,
            )?,
        })
    }

    /// Canonical `KELD_APP_LINK` value for the one role this listener admits.
    #[must_use]
    pub fn app_link(&self) -> String {
        #[cfg(unix)]
        {
            format_app_link(&self.path.display().to_string(), &self.token)
        }
        #[cfg(windows)]
        {
            self.listener.app_link()
        }
    }

    /// Filesystem endpoint for host-side assertions and cleanup diagnostics.
    #[must_use]
    #[cfg(unix)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Handle that can cancel a blocked accept or active handshake.
    #[must_use]
    pub fn cancellation(&self) -> BootstrapCancellation {
        BootstrapCancellation {
            #[cfg(unix)]
            path: self.path.clone(),
            #[cfg(windows)]
            cancellation: self.listener.cancellation(),
            #[cfg(unix)]
            stopping: Arc::clone(&self.stopping),
            #[cfg(unix)]
            listening: Arc::clone(&self.listening),
            #[cfg(unix)]
            active_stream: Arc::clone(&self.active_stream),
        }
    }

    #[cfg(test)]
    fn install_handshake_witness(&self, witness: TestHandshakeWitness) {
        #[cfg(unix)]
        {
            let replaced = self
                .handshake_witness
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .replace(witness);
            assert!(replaced.is_none(), "handshake witness already installed");
        }
        #[cfg(windows)]
        self.listener.install_handshake_witness(witness);
    }

    #[cfg(all(test, unix))]
    fn install_before_consume_gate(&self, gate: TestConsumeGate) {
        let replaced = self
            .before_consume
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(gate);
        assert!(replaced.is_none(), "before-consume gate already installed");
    }

    /// Waits until one client proves possession of this listener's token.
    ///
    /// A malformed, foreign, silent, or otherwise invalid client is closed and
    /// does not consume the bootstrap generation. Returns `Ok(None)` after
    /// [`Self::shutdown`] wakes a blocked `accept`.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] only when the listener itself cannot accept a
    /// connection. Peer handshake errors are untrusted input and are handled
    /// by continuing to accept.
    pub fn accept_authenticated(&self) -> io::Result<Option<BootstrapStream>> {
        let observer = NoopRejectionObserver;
        #[cfg(unix)]
        match self.accept_loop(None, APP_LINK_IO_DEADLINE, &observer)? {
            BootstrapAdmission::Authenticated(stream) => Ok(Some(stream)),
            BootstrapAdmission::Cancelled | BootstrapAdmission::DeadlineElapsed => Ok(None),
        }
        #[cfg(windows)]
        self.listener.accept_authenticated(&observer)
    }

    /// Waits until one client authenticates, this listener is cancelled, or
    /// `deadline` elapses for this whole bootstrap generation.
    ///
    /// Peer authentication failures are treated as untrusted input: the peer
    /// is closed, a redacted host-only record may be emitted through
    /// `observer`, and the listener keeps admitting the legitimate role until
    /// the generation-level deadline or cancellation wins.
    ///
    /// On successful authentication the bootstrap locator is consumed. The
    /// accepted stream remains live, but stale clients cannot reconnect.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] only for host-side listener or socket option
    /// failures. Peer handshake failures are not returned.
    pub fn accept_authenticated_until(
        &self,
        deadline: Instant,
        observer: &dyn BootstrapRejectionObserver,
    ) -> io::Result<BootstrapAdmission> {
        #[cfg(unix)]
        {
            self.accept_loop(Some(deadline), APP_LINK_IO_DEADLINE, observer)
        }
        #[cfg(windows)]
        {
            self.listener.accept_authenticated_until(deadline, observer)
        }
    }

    /// Stops a blocked [`Self::accept_authenticated`] call.
    ///
    /// The platform cancellation primitive wakes the blocked accept or active
    /// handshake, which then observes the stop flag without admitting an
    /// unauthenticated client.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if cancellation or endpoint close fails.
    pub fn shutdown(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let cancel = self.cancellation().cancel();
            let close = self.close_endpoint();
            cancel.and(close)
        }
        #[cfg(windows)]
        {
            self.listener.shutdown()
        }
    }
}

#[cfg(unix)]
impl BootstrapListener {
    fn accept_loop(
        &self,
        deadline: Option<Instant>,
        handshake_deadline: Duration,
        observer: &dyn BootstrapRejectionObserver,
    ) -> io::Result<BootstrapAdmission> {
        loop {
            if self.stopping.load(Ordering::SeqCst) {
                self.close_endpoint()?;
                return Ok(BootstrapAdmission::Cancelled);
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                self.close_endpoint()?;
                return Ok(BootstrapAdmission::DeadlineElapsed);
            }
            let Some(mut stream) = self.try_accept()? else {
                park_until_next_accept(deadline);
                continue;
            };
            if self.stopping.load(Ordering::SeqCst) {
                self.close_endpoint()?;
                return Ok(BootstrapAdmission::Cancelled);
            }
            let handshake_started = Instant::now();
            let Some((timeout, peer_deadline)) =
                peer_handshake_window(handshake_started, deadline, handshake_deadline)
            else {
                self.close_endpoint()?;
                return Ok(BootstrapAdmission::DeadlineElapsed);
            };
            // Setting the deadline is a fact about THIS PEER, not about the
            // listener: the call's outcome depends on the peer's state. `?`
            // made it fatal to admission instead.
            //
            // macOS returns EINVAL from SO_RCVTIMEO/SO_SNDTIMEO on an accepted
            // socket whose peer has already closed, so a bare connect-then-
            // close -- a port scan, a health check, a racing restart -- killed
            // the whole accept loop: the worker died with `listener I/O:
            // InvalidInput`, and the next legitimate client then blocked
            // forever in recvfrom waiting for a HELLO nobody would send.
            // Linux accepts the same setsockopt, which is why this only ever
            // reproduced on macOS (measured: 5ms there, >2640s here).
            //
            // Classified per peer and skipped, so the listener keeps accepting.
            // Setting the deadline earlier cannot fix this: a peer may close at
            // any point, including between accept() and setsockopt.
            if stream
                .set_app_link_read_deadline(Some(APP_LINK_READER_POLL.min(timeout)))
                .and_then(|()| stream.set_app_link_write_deadline(Some(timeout)))
                .is_err()
            {
                observer.rejected(BootstrapRejection::Io);
                continue;
            }
            // `try_clone` is NOT per-peer: it dups a local descriptor, so it
            // fails on host resource exhaustion (EMFILE/ENFILE), never because
            // of anything this peer did. Recording it as a rejection and
            // retrying would drop legitimate peers forever while the host fault
            // that caused it stayed invisible. It propagates.
            let active_stream = stream.try_clone()?;
            *self
                .active_stream
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(active_stream);
            let _active = ActiveHandshake {
                active_stream: Arc::clone(&self.active_stream),
            };
            #[cfg(test)]
            report_test_handshake_entry(&self.handshake_witness, deadline, peer_deadline.instant());
            match handshake_server_interruptible_until(
                &mut stream,
                &self.token,
                self.stopping.as_ref(),
                peer_deadline,
            ) {
                Ok(true) => {
                    #[cfg(test)]
                    wait_at_test_consume_gate(&self.before_consume);
                    if self.stopping.load(Ordering::SeqCst) {
                        drop(stream);
                        self.close_endpoint()?;
                        return Ok(BootstrapAdmission::Cancelled);
                    }
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        drop(stream);
                        self.close_endpoint()?;
                        return Ok(BootstrapAdmission::DeadlineElapsed);
                    }
                    self.close_endpoint()?;
                    return Ok(BootstrapAdmission::Authenticated(stream));
                }
                Ok(false) => {
                    self.close_endpoint()?;
                    return Ok(BootstrapAdmission::Cancelled);
                }
                Err(_) if self.stopping.load(Ordering::SeqCst) => {
                    self.close_endpoint()?;
                    return Ok(BootstrapAdmission::Cancelled);
                }
                Err(IpcError::Timeout) if deadline.is_some_and(|d| Instant::now() >= d) => {
                    self.close_endpoint()?;
                    return Ok(BootstrapAdmission::DeadlineElapsed);
                }
                // Every pre-authentication failure is recorded, not only token
                // failure. This arm used to be `Err(_) => {}`, so a peer that
                // failed on a bad header, an oversized envelope, or a partial
                // frame was indistinguishable from no peer at all and the host
                // saw an admission that simply never completed.
                Err(err) => {
                    observer.rejected(BootstrapRejection::classify(&err));
                }
            }
        }
    }

    fn try_accept(&self) -> io::Result<Option<BootstrapStream>> {
        #[cfg(unix)]
        {
            let guard = self.listener.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(listener) = guard.as_ref() else {
                return Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "bootstrap listener already consumed",
                ));
            };
            match listener.accept() {
                Ok((stream, _)) => {
                    // The bootstrap listener itself is non-blocking so generation
                    // deadline/cancellation can be polled without a helper
                    // accept-waker. The admitted app-link must be blocking:
                    // `APP_LINK_IO_DEADLINE` is the session contract, and a
                    // leaked non-blocking flag turns a quiet-but-live peer into an
                    // immediate `KELD-IPC-006`.
                    stream.set_nonblocking(false)?;
                    Ok(Some(stream))
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
                Err(error) => Err(error),
            }
        }
        #[cfg(windows)]
        {
            let guard = self.listener.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(listener) = guard.as_ref() else {
                return Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "bootstrap listener already consumed",
                ));
            };
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false)?;
                    Ok(Some(stream))
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
                Err(error) => Err(error),
            }
        }
    }

    #[cfg_attr(
        windows,
        expect(
            clippy::unnecessary_wraps,
            reason = "the shared lifecycle API is fallible on Unix; Windows listener close is deliberately outcome-preserving"
        )
    )]
    fn close_endpoint(&self) -> io::Result<()> {
        let listener = self
            .listener
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        self.listening.store(false, Ordering::Release);
        #[cfg(windows)]
        {
            // Authentication/cancellation/deadline has already selected the
            // admission outcome. A historical listener SO_ERROR must not
            // replace that outcome or discard an authenticated stream.
            drop(listener);
            Ok(())
        }
        #[cfg(unix)]
        {
            drop(listener);
            let mut first_error = None;
            if let Err(error) = fs::remove_file(&self.path)
                && error.kind() != io::ErrorKind::NotFound
            {
                first_error = Some(error);
            }
            if let Err(error) = fs::remove_dir(&self.session_dir)
                && error.kind() != io::ErrorKind::NotFound
            {
                first_error.get_or_insert(error);
            }
            match first_error {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
    }
}

impl BootstrapCancellation {
    /// Cancels a blocked accept or active handshake.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the wake connection fails while the endpoint
    /// still exists.
    pub fn cancel(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            self.stopping.store(true, Ordering::SeqCst);
            if let Some(stream) = self
                .active_stream
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
            {
                let _ = stream.shutdown_app_link();
            }
            if !self.listening.load(Ordering::Acquire) {
                return Ok(());
            }
            match UnixStream::connect(&self.path) {
                Ok(stream) => match stream.shutdown_app_link() {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotConnected => Ok(()),
                    Err(error) => Err(error),
                },
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotConnected
                            | io::ErrorKind::NotFound
                            | io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        #[cfg(windows)]
        self.cancellation.cancel()
    }
}

#[cfg(unix)]
struct ActiveHandshake {
    active_stream: Arc<Mutex<Option<BootstrapStream>>>,
}

#[cfg(unix)]
impl Drop for ActiveHandshake {
    fn drop(&mut self) {
        *self
            .active_stream
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }
}

#[cfg(unix)]
fn park_until_next_accept(deadline: Option<Instant>) {
    let timeout =
        match deadline.and_then(|deadline| deadline.checked_duration_since(Instant::now())) {
            Some(remaining) => remaining.min(ACCEPT_POLL_INTERVAL),
            None => ACCEPT_POLL_INTERVAL,
        };
    if !timeout.is_zero() {
        std::thread::park_timeout(timeout);
    }
}

impl Drop for BootstrapListener {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Windows owner-DACL named-pipe bootstrap.
///
/// [`BootstrapListener`] delegates its Windows transport to this single owner.
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsNamedPipeBootstrapListener {
    server: Mutex<Option<WindowsNamedPipeServer>>,
    admission: Mutex<()>,
    endpoint: String,
    token: SessionToken,
    stopping: Arc<AtomicBool>,
    #[cfg(test)]
    before_consume: Mutex<Option<TestConsumeGate>>,
    #[cfg(test)]
    handshake_witness: Mutex<Option<TestHandshakeWitness>>,
}

/// Connected authenticated stream from [`WindowsNamedPipeBootstrapListener`].
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsNamedPipeBootstrapStream(WindowsNamedPipeStream);

/// Cancellation handle for a pending named-pipe accept or HELLO.
#[cfg(windows)]
#[derive(Debug, Clone)]
pub struct WindowsNamedPipeBootstrapCancellation {
    server: WindowsNamedPipeCanceller,
    stopping: Arc<AtomicBool>,
}

/// Stable install/user rendezvous for the bounded Windows lifecycle keeper.
/// The endpoint is a locator only; caller callbacks must authenticate the exact
/// connected peer before any attempt capability is transferred.
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsLifecycleRendezvousListener {
    server: Mutex<Option<WindowsNamedPipeServer>>,
    admission: Mutex<()>,
    endpoint: String,
    binding: WindowsLifecycleBinding,
    #[cfg(test)]
    before_consume: Mutex<Option<TestConsumeGate>>,
    #[cfg(test)]
    before_receipt: Mutex<Option<TestConsumeGate>>,
}

/// Exact KEL-53 transaction scope authenticated on a lifecycle connection.
///
/// These identifiers are public values, not secrets. The pipe peer policy proves
/// identity; this transcript prevents cross-install, stale-attempt and wrong-purpose
/// handle handoffs on a freshly authenticated connection.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsLifecycleBinding {
    installation_id: [u8; 32],
    attempt_id: [u8; 32],
    lifecycle_channel_id: [u8; 32],
    purpose: WindowsLifecyclePurpose,
}

/// Client-side provenance for IDs learned from the authenticated keeper.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsLifecycleBindingKnowledge {
    /// Install, attempt, channel and purpose were independently known before connect.
    IndependentlyExpected,
    /// Keeper supplied attempt/channel IDs; revalidate them against the journal after lease acquisition.
    KeeperSuppliedAwaitingJournalRevalidation,
}

/// Context a lifecycle client must verify on the server's post-authentication challenge.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsLifecycleExpectation {
    installation_id: [u8; 32],
    attempt_id: Option<[u8; 32]>,
    lifecycle_channel_id: Option<[u8; 32]>,
    purpose: WindowsLifecyclePurpose,
}

/// Directional purpose for one lifecycle capability exchange.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WindowsLifecyclePurpose {
    /// Coordinator transfers the exact attempt Job and lock-retention handle to its keeper.
    CoordinatorToKeeper = 1,
    /// Keeper transfers a query-only zero witness to the selected successor.
    KeeperToSuccessor = 2,
}

#[cfg(windows)]
impl WindowsLifecycleBinding {
    /// Creates one nonempty install/attempt/channel binding with a closed purpose tag.
    ///
    /// # Errors
    ///
    /// Returns an error if any identifier is all-zero or any two identifiers alias.
    pub fn new(
        installation_id: [u8; 32],
        attempt_id: [u8; 32],
        lifecycle_channel_id: [u8; 32],
        purpose: WindowsLifecyclePurpose,
    ) -> io::Result<Self> {
        if installation_id == [0; 32]
            || attempt_id == [0; 32]
            || lifecycle_channel_id == [0; 32]
            || installation_id == attempt_id
            || installation_id == lifecycle_channel_id
            || attempt_id == lifecycle_channel_id
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "lifecycle identifiers must be nonzero and distinct",
            ));
        }
        Ok(Self {
            installation_id,
            attempt_id,
            lifecycle_channel_id,
            purpose,
        })
    }

    /// Directional lifecycle purpose authenticated on the connected pipe.
    #[must_use]
    pub const fn purpose(self) -> WindowsLifecyclePurpose {
        self.purpose
    }

    /// Installs the same IDs with another closed directional purpose.
    #[must_use]
    pub const fn with_purpose(self, purpose: WindowsLifecyclePurpose) -> Self {
        Self { purpose, ..self }
    }

    /// Immutable installation identity authenticated on this connection.
    #[must_use]
    pub const fn installation_id(&self) -> &[u8; 32] {
        &self.installation_id
    }

    /// Attempt identity authenticated on this connection.
    #[must_use]
    pub const fn attempt_id(&self) -> &[u8; 32] {
        &self.attempt_id
    }

    /// One-shot lifecycle channel identity authenticated on this connection.
    #[must_use]
    pub const fn lifecycle_channel_id(&self) -> &[u8; 32] {
        &self.lifecycle_channel_id
    }
}

#[cfg(windows)]
impl WindowsLifecycleExpectation {
    /// Requires an exact pre-known binding, as used for coordinator/keeper setup.
    #[must_use]
    pub const fn exact(binding: WindowsLifecycleBinding) -> Self {
        Self {
            installation_id: binding.installation_id,
            attempt_id: Some(binding.attempt_id),
            lifecycle_channel_id: Some(binding.lifecycle_channel_id),
            purpose: binding.purpose,
        }
    }

    /// Learns attempt/channel IDs from an authenticated keeper for later journal revalidation.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty installation ID. Discovery is only valid for
    /// keeper-to-successor retirement, where mutation remains blocked until the
    /// successor reacquires and validates the protected journal.
    pub fn from_keeper(installation_id: [u8; 32]) -> io::Result<Self> {
        if installation_id == [0; 32] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "lifecycle install identity must be nonzero",
            ));
        }
        Ok(Self {
            installation_id,
            attempt_id: None,
            lifecycle_channel_id: None,
            purpose: WindowsLifecyclePurpose::KeeperToSuccessor,
        })
    }

    fn accepts(
        &self,
        binding: WindowsLifecycleBinding,
    ) -> Option<WindowsLifecycleBindingKnowledge> {
        if self.installation_id != binding.installation_id || self.purpose != binding.purpose {
            return None;
        }
        match (self.attempt_id, self.lifecycle_channel_id) {
            (Some(attempt), Some(channel))
                if attempt == binding.attempt_id && channel == binding.lifecycle_channel_id =>
            {
                Some(WindowsLifecycleBindingKnowledge::IndependentlyExpected)
            }
            (None, None) if self.purpose == WindowsLifecyclePurpose::KeeperToSuccessor => {
                Some(WindowsLifecycleBindingKnowledge::KeeperSuppliedAwaitingJournalRevalidation)
            }
            _ => None,
        }
    }
}

/// Retained OS process-object pin supplied by the platform peer authenticator.
/// Implementations MUST keep the exact process object open for the rendezvous
/// lifetime, rather than only storing a numeric PID.
#[cfg(windows)]
pub trait WindowsLifecyclePeerPin {
    /// PID reported by the connected pipe and verified against the retained object.
    fn process_id(&self) -> u32;

    /// Session reported by the connected pipe and verified against the retained token.
    fn session_id(&self) -> u32;

    /// Reports whether the exact retained process object has exited.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the operating system cannot query the retained handle.
    fn has_exited(&self) -> io::Result<bool>;
}

/// Accepted lifecycle peer after PID/session, impersonated token and two-nonce
/// HELLO checks have completed on the same non-inheritable pipe connection.
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsLifecycleRendezvousPeer<P> {
    stream: WindowsNamedPipeBootstrapStream,
    process: P,
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    token_facts: WindowsPeerTokenFacts,
    binding: WindowsLifecycleBinding,
}

/// Connected client side after it authenticated the exact named-pipe server
/// process and completed the two-nonce HELLO exchange.
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsLifecycleRendezvousClient<P> {
    stream: WindowsNamedPipeBootstrapStream,
    process: P,
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    binding: WindowsLifecycleBinding,
    binding_knowledge: WindowsLifecycleBindingKnowledge,
}

/// One owner for the unguessable per-generation pipe name (32 random bytes,
/// hex) shared by the production listener and the test-only connected pair.
#[cfg(windows)]
fn random_pipe_endpoint() -> io::Result<String> {
    let mut nonce = [0_u8; 32];
    getrandom::fill(&mut nonce).map_err(io::Error::other)?;
    let mut endpoint = String::from(r"\\.\pipe\keld-");
    for byte in nonce {
        write!(&mut endpoint, "{byte:02x}").map_err(io::Error::other)?;
    }
    Ok(endpoint)
}

#[cfg(all(test, windows))]
fn random_lifecycle_pipe_endpoint() -> io::Result<String> {
    let endpoint = random_pipe_endpoint()?;
    let nonce = endpoint
        .strip_prefix(r"\\.\pipe\keld-")
        .ok_or_else(|| io::Error::other("random app-link endpoint prefix changed"))?;
    Ok(format!(r"\\.\pipe\keld-lifecycle-{nonce}"))
}

/// Test-only connected server/client pair on the shipped Windows transport,
/// with no HELLO exchanged: reader-clock contract tests (kel133 AC7/AC8,
/// windows-latest row) must prove the overlapped-wait + absolute-clamp clock
/// Keld ships, not loopback TCP's `SO_RCVTIMEO`. Ownership mirrors the
/// production accept loop: the server instance is consumed once its stream
/// exists, and the stream keeps the pipe alive through its shared inner.
///
/// # Errors
///
/// Returns the first I/O error from bind, accept, stream creation, or the
/// client connect.
#[cfg(all(test, windows))]
pub(crate) fn connected_named_pipe_pair() -> io::Result<(
    WindowsNamedPipeBootstrapStream,
    WindowsNamedPipeBootstrapStream,
)> {
    let endpoint = random_pipe_endpoint()?;
    let server = WindowsNamedPipeServer::bind(&endpoint)?;
    let connect_deadline = Instant::now() + Duration::from_secs(2);
    let client = std::thread::spawn(move || {
        WindowsNamedPipeServer::connect_client_until(&endpoint, connect_deadline)
    });
    match server.accept_until(Some(connect_deadline))? {
        WaitOutcome::Ready => {}
        WaitOutcome::PeerClosed => return Err(io::Error::other("pair accept: peer closed")),
        WaitOutcome::Cancelled => return Err(io::Error::other("pair accept: cancelled")),
        WaitOutcome::DeadlineElapsed => {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "pair accept: deadline",
            ));
        }
    }
    let server_stream = server.stream()?;
    server.consume();
    drop(server);
    let client_stream = client
        .join()
        .map_err(|_| io::Error::other("pair connect thread panicked"))??;
    Ok((
        WindowsNamedPipeBootstrapStream(server_stream),
        WindowsNamedPipeBootstrapStream(client_stream),
    ))
}

#[cfg(windows)]
impl WindowsLifecycleRendezvousListener {
    /// Binds a stable local endpoint from a trusted installation/user locator.
    /// The locator permits pre-lease discovery but conveys no attempt authority.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if endpoint validation, SID/DACL construction or
    /// first-instance pipe creation/readback fails.
    pub fn bind(
        install_user_locator: [u8; 32],
        binding: WindowsLifecycleBinding,
    ) -> io::Result<Self> {
        let endpoint =
            WindowsNamedPipeBootstrapStream::endpoint_for_lifecycle_install(&install_user_locator);
        let server = WindowsNamedPipeServer::bind(&endpoint)?;
        Ok(Self {
            server: Mutex::new(Some(server)),
            admission: Mutex::new(()),
            endpoint,
            binding,
            #[cfg(test)]
            before_consume: Mutex::new(None),
            #[cfg(test)]
            before_receipt: Mutex::new(None),
        })
    }

    /// Stable pipe locator used by a cold successor before mutable-journal reads.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    #[cfg(test)]
    fn install_before_consume_gate(&self, gate: TestConsumeGate) {
        *self
            .before_consume
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(gate);
    }

    #[cfg(test)]
    fn install_before_receipt_gate(&self, gate: TestConsumeGate) {
        *self
            .before_receipt
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(gate);
    }

    /// Accepts one client after the caller authenticates the actual process and
    /// token facts of the last HELLO writer. Rejected clients are disconnected
    /// and the first-instance listener remains available until `deadline`.
    /// The nonce echo establishes freshness/liveness only; caller policy proves
    /// process identity and role before any transaction handle is transferred.
    ///
    /// # Errors
    ///
    /// Returns host-side pipe or deadline setup errors. `Ok(None)` means the
    /// absolute deadline elapsed without an authenticated peer.
    #[expect(
        clippy::too_many_lines,
        reason = "linear pipe admission, impersonation, nonce and process-pin transitions stay auditable"
    )]
    pub fn accept_until<P: WindowsLifecyclePeerPin>(
        &self,
        deadline: Instant,
        mut authenticate_client: impl FnMut(u32, u32, &WindowsPeerTokenFacts) -> Option<P>,
    ) -> io::Result<Option<WindowsLifecycleRendezvousPeer<P>>> {
        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let server = self
            .server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "rendezvous consumed"))?;
        loop {
            if Instant::now() >= deadline {
                server.close_terminal()?;
                self.server
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                return Ok(None);
            }
            match server.accept_until(Some(deadline))? {
                WaitOutcome::Cancelled | WaitOutcome::DeadlineElapsed => {
                    server.close_terminal()?;
                    self.server
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    return Ok(None);
                }
                WaitOutcome::PeerClosed => {
                    server.disconnect_for_retry()?;
                    continue;
                }
                WaitOutcome::Ready => {}
            }
            let mut stream = WindowsNamedPipeBootstrapStream(server.stream()?);
            let started = Instant::now();
            let Some((peer_timeout, peer_deadline)) =
                peer_handshake_window(started, Some(deadline), APP_LINK_IO_DEADLINE)
            else {
                server.close_terminal()?;
                self.server
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                return Ok(None);
            };
            stream.set_app_link_read_deadline(Some(APP_LINK_READER_POLL.min(peer_timeout)))?;
            stream.set_app_link_write_deadline(Some(peer_timeout))?;
            stream
                .0
                .set_absolute_deadline(Some(peer_deadline.instant()));
            let Ok(peer_process_id) = stream.peer_process_id() else {
                drop(stream);
                server.disconnect_for_retry()?;
                continue;
            };
            let Ok(peer_session_id) = stream.peer_session_id() else {
                drop(stream);
                server.disconnect_for_retry()?;
                continue;
            };
            let peer_view = stream.try_clone()?;
            let peer_pin = std::cell::RefCell::new(None);
            let handshake = handshake_server_rendezvous(&mut stream, || {
                let token_facts = peer_view
                    .0
                    .last_client_token_facts()
                    .map_err(IpcError::from)?;
                let pin = authenticate_client(peer_process_id, peer_session_id, &token_facts)
                    .ok_or(IpcError::HelloAuth {
                        detail: "lifecycle peer rejected by process identity policy",
                    })?;
                if pin.process_id() != peer_process_id
                    || pin.session_id() != peer_session_id
                    || token_facts.session_id != peer_session_id
                    || pin.has_exited().map_err(IpcError::from)?
                {
                    return Err(IpcError::HelloAuth {
                        detail: "lifecycle peer pin does not match the connected endpoint",
                    });
                }
                *peer_pin.borrow_mut() = Some(pin);
                Ok(token_facts)
            });
            if let Ok((client_nonce, server_nonce, token_facts)) = handshake {
                let Some(process) = peer_pin.into_inner() else {
                    drop(stream);
                    server.disconnect_for_retry()?;
                    continue;
                };
                if process.has_exited()? {
                    drop(stream);
                    server.disconnect_for_retry()?;
                    continue;
                }
                let Ok(acceptance_receipt) = authenticate_lifecycle_binding_server(
                    &mut stream,
                    self.binding,
                    client_nonce,
                    server_nonce,
                    peer_process_id,
                    std::process::id(),
                ) else {
                    drop(stream);
                    server.disconnect_for_retry()?;
                    continue;
                };
                if Instant::now() >= deadline {
                    drop(stream);
                    server.close_terminal()?;
                    self.server
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    return Ok(None);
                }
                #[cfg(test)]
                wait_at_test_consume_gate(&self.before_consume);
                server.consume();
                #[cfg(test)]
                wait_at_test_consume_gate(&self.before_receipt);
                self.server
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                if let Err(error) = stream.write_all(&acceptance_receipt) {
                    if Instant::now() >= deadline {
                        return Ok(None);
                    }
                    return Err(error);
                }
                if Instant::now() >= deadline {
                    return Ok(None);
                }
                stream.0.set_absolute_deadline(None);
                return Ok(Some(WindowsLifecycleRendezvousPeer {
                    stream,
                    process,
                    client_nonce,
                    server_nonce,
                    token_facts,
                    binding: self.binding,
                }));
            }
            drop(stream);
            server.disconnect_for_retry()?;
        }
    }
}

impl<P: WindowsLifecyclePeerPin> WindowsLifecycleRendezvousPeer<P> {
    /// Mutable stream for the attempt/install-bound lifecycle records.
    pub fn stream_mut(&mut self) -> &mut WindowsNamedPipeBootstrapStream {
        &mut self.stream
    }

    /// Applies one absolute deadline to subsequent cold-path handoff records.
    pub fn set_io_deadline(&mut self, deadline: Instant) {
        self.stream.0.set_absolute_deadline(Some(deadline));
    }

    /// PID observed from the actual connected named-pipe handle.
    #[must_use]
    pub fn process_id(&self) -> u32 {
        self.process.process_id()
    }

    /// Session observed from the pipe and corroborated by the last-writer token.
    #[must_use]
    pub fn session_id(&self) -> u32 {
        self.process.session_id()
    }

    /// Retained process-object pin validated by the caller's authentication policy.
    #[must_use]
    pub const fn process_pin(&self) -> &P {
        &self.process
    }

    /// Fresh client nonce, echoed by the server.
    #[must_use]
    pub const fn client_nonce(&self) -> &SessionToken {
        &self.client_nonce
    }

    /// Fresh server nonce, acknowledged by the client.
    #[must_use]
    pub const fn server_nonce(&self) -> &SessionToken {
        &self.server_nonce
    }

    /// Facts read from the last HELLO writer's identification token.
    #[must_use]
    pub const fn token_facts(&self) -> &WindowsPeerTokenFacts {
        &self.token_facts
    }

    /// Install, attempt, lifecycle channel and purpose authenticated on this pipe.
    #[must_use]
    pub const fn binding(&self) -> WindowsLifecycleBinding {
        self.binding
    }
}

/// Connects to a stable lifecycle locator, authenticates the exact server process
/// before sending a nonce, and completes the two-nonce HELLO exchange.
///
/// # Errors
///
/// Returns an I/O error if connect, process authentication or the bounded handshake fails.
#[cfg(windows)]
pub fn connect_windows_lifecycle_rendezvous_until<P: WindowsLifecyclePeerPin>(
    endpoint: &str,
    expectation: WindowsLifecycleExpectation,
    deadline: Instant,
    authenticate_server: impl FnOnce(u32, u32) -> Option<P>,
) -> io::Result<WindowsLifecycleRendezvousClient<P>> {
    if !WindowsNamedPipeBootstrapStream::is_lifecycle_endpoint(endpoint) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "lifecycle rendezvous endpoint is not in the lifecycle protocol namespace",
        ));
    }
    let mut stream = WindowsNamedPipeBootstrapStream(
        WindowsNamedPipeServer::connect_lifecycle_client_until(endpoint, deadline)?,
    );
    let process_id = stream.peer_process_id()?;
    let session_id = stream.peer_session_id()?;
    let process = authenticate_server(process_id, session_id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "lifecycle server process identity was not authenticated",
        )
    })?;
    if process.process_id() != process_id
        || process.session_id() != session_id
        || process.has_exited()?
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "retained lifecycle server process pin mismatches the connected pipe",
        ));
    }
    let started = Instant::now();
    let Some((peer_timeout, peer_deadline)) =
        peer_handshake_window(started, Some(deadline), APP_LINK_IO_DEADLINE)
    else {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "lifecycle rendezvous deadline elapsed before handshake",
        ));
    };
    stream.set_app_link_read_deadline(Some(APP_LINK_READER_POLL.min(peer_timeout)))?;
    stream.set_app_link_write_deadline(Some(peer_timeout))?;
    stream
        .0
        .set_absolute_deadline(Some(peer_deadline.instant()));
    let (client_nonce, server_nonce, ()) =
        handshake_client_rendezvous(&mut stream, || Ok(())).map_err(io::Error::other)?;
    let (binding, binding_knowledge) = authenticate_lifecycle_binding_client(
        &mut stream,
        expectation,
        client_nonce,
        server_nonce,
        std::process::id(),
        process_id,
    )?;
    if process.has_exited()? {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "authenticated lifecycle server exited during the nonce exchange",
        ));
    }
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "lifecycle rendezvous deadline elapsed after handshake",
        ));
    }
    stream.0.set_absolute_deadline(None);
    Ok(WindowsLifecycleRendezvousClient {
        stream,
        process,
        client_nonce,
        server_nonce,
        binding,
        binding_knowledge,
    })
}

impl<P: WindowsLifecyclePeerPin> WindowsLifecycleRendezvousClient<P> {
    /// Mutable stream for the attempt/install-bound lifecycle records.
    pub fn stream_mut(&mut self) -> &mut WindowsNamedPipeBootstrapStream {
        &mut self.stream
    }

    /// Applies one absolute deadline to subsequent cold-path handoff records.
    pub fn set_io_deadline(&mut self, deadline: Instant) {
        self.stream.0.set_absolute_deadline(Some(deadline));
    }

    /// Server PID observed from the actual connected pipe.
    #[must_use]
    pub fn process_id(&self) -> u32 {
        self.process.process_id()
    }

    /// Server session observed from the connected pipe.
    #[must_use]
    pub fn session_id(&self) -> u32 {
        self.process.session_id()
    }

    /// Retained server process-object pin validated before the nonce exchange.
    #[must_use]
    pub const fn process_pin(&self) -> &P {
        &self.process
    }

    /// Fresh client nonce echoed by the server.
    #[must_use]
    pub const fn client_nonce(&self) -> &SessionToken {
        &self.client_nonce
    }

    /// Fresh server nonce acknowledged by the client.
    #[must_use]
    pub const fn server_nonce(&self) -> &SessionToken {
        &self.server_nonce
    }

    /// Install, attempt, lifecycle channel and purpose authenticated on this pipe.
    #[must_use]
    pub const fn binding(&self) -> WindowsLifecycleBinding {
        self.binding
    }

    /// States whether transaction IDs were independently known or came from the keeper.
    #[must_use]
    pub const fn binding_knowledge(&self) -> WindowsLifecycleBindingKnowledge {
        self.binding_knowledge
    }
}

#[cfg(windows)]
const LIFECYCLE_BINDING_RECORD_LEN: usize = 8 + 1 + (32 * 3) + (32 * 2) + (4 * 2);

#[cfg(windows)]
fn lifecycle_binding_record(
    magic: [u8; 8],
    binding: WindowsLifecycleBinding,
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
) -> [u8; LIFECYCLE_BINDING_RECORD_LEN] {
    let mut record = [0; LIFECYCLE_BINDING_RECORD_LEN];
    let mut cursor = 0;
    record[cursor..cursor + magic.len()].copy_from_slice(&magic);
    cursor += magic.len();
    record[cursor] = binding.purpose as u8;
    cursor += 1;
    for identity in [
        binding.installation_id,
        binding.attempt_id,
        binding.lifecycle_channel_id,
        *client_nonce.as_bytes(),
        *server_nonce.as_bytes(),
    ] {
        record[cursor..cursor + identity.len()].copy_from_slice(&identity);
        cursor += identity.len();
    }
    for pid in [client_pid, server_pid] {
        record[cursor..cursor + 4].copy_from_slice(&pid.to_le_bytes());
        cursor += 4;
    }
    debug_assert_eq!(cursor, record.len());
    record
}

#[cfg(windows)]
fn authenticate_lifecycle_binding_server(
    stream: &mut WindowsNamedPipeBootstrapStream,
    binding: WindowsLifecycleBinding,
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
) -> io::Result<[u8; LIFECYCLE_BINDING_RECORD_LEN]> {
    let challenge = lifecycle_binding_record(
        *b"KELD-LC1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    );
    stream.write_all(&challenge)?;
    let mut acknowledgement = [0; LIFECYCLE_BINDING_RECORD_LEN];
    stream.read_exact(&mut acknowledgement)?;
    let expected_acknowledgement = lifecycle_binding_record(
        *b"KELD-LA1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    );
    if acknowledgement != expected_acknowledgement {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "lifecycle acknowledgement does not match the authenticated install/attempt/peer",
        ));
    }
    Ok(lifecycle_binding_record(
        *b"KELD-LR1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    ))
}

#[cfg(windows)]
fn authenticate_lifecycle_binding_client(
    stream: &mut WindowsNamedPipeBootstrapStream,
    expectation: WindowsLifecycleExpectation,
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
) -> io::Result<(WindowsLifecycleBinding, WindowsLifecycleBindingKnowledge)> {
    let mut received = [0; LIFECYCLE_BINDING_RECORD_LEN];
    stream.read_exact(&mut received)?;
    let binding = decode_lifecycle_challenge(
        &received,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    )?;
    let knowledge = expectation.accepts(binding).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "keeper lifecycle challenge changed install, attempt, channel or purpose",
        )
    })?;
    let acknowledgement = lifecycle_binding_record(
        *b"KELD-LA1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    );
    stream.write_all(&acknowledgement)?;
    let expected_receipt = lifecycle_binding_record(
        *b"KELD-LR1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    );
    let mut receipt = [0; LIFECYCLE_BINDING_RECORD_LEN];
    stream.read_exact(&mut receipt)?;
    if receipt != expected_receipt {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "lifecycle listener did not confirm one-use context consumption",
        ));
    }
    Ok((binding, knowledge))
}

#[cfg(windows)]
fn decode_lifecycle_challenge(
    record: &[u8; LIFECYCLE_BINDING_RECORD_LEN],
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
) -> io::Result<WindowsLifecycleBinding> {
    if record[..8] != *b"KELD-LC1"
        || record[105..137] != *client_nonce.as_bytes()
        || record[137..169] != *server_nonce.as_bytes()
        || record[169..173] != client_pid.to_le_bytes()
        || record[173..177] != server_pid.to_le_bytes()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "keeper challenge has wrong protocol, nonce or connected-process transcript",
        ));
    }
    let purpose = match record[8] {
        1 => WindowsLifecyclePurpose::CoordinatorToKeeper,
        2 => WindowsLifecyclePurpose::KeeperToSuccessor,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "keeper challenge has an unknown lifecycle purpose",
            ));
        }
    };
    let installation_id = record[9..41]
        .try_into()
        .map_err(|_| io::Error::other("installation ID record range changed"))?;
    let attempt_id = record[41..73]
        .try_into()
        .map_err(|_| io::Error::other("attempt ID record range changed"))?;
    let lifecycle_channel_id = record[73..105]
        .try_into()
        .map_err(|_| io::Error::other("lifecycle channel record range changed"))?;
    WindowsLifecycleBinding::new(installation_id, attempt_id, lifecycle_channel_id, purpose)
}

#[cfg(windows)]
impl WindowsNamedPipeBootstrapListener {
    /// Creates one first-instance, remote-rejecting named pipe protected by an
    /// explicit current-TokenUser DACL and mints an independent HELLO token.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if randomness, SID/DACL construction, pipe
    /// creation, handle validation, or descriptor readback fails.
    pub fn bind() -> io::Result<Self> {
        let token = SessionToken::random()?;
        let endpoint = random_pipe_endpoint()?;
        Self::bind_at(&endpoint, token)
    }

    /// Binds an install/user-derived keeper locator with its one-shot HELLO token.
    /// HELLO possession does not prove peer process identity.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the canonical endpoint or protected pipe cannot
    /// be created and validated.
    pub fn bind_rendezvous(
        install_user_locator: [u8; 32],
        token: SessionToken,
    ) -> io::Result<Self> {
        let endpoint = WindowsNamedPipeBootstrapStream::endpoint_for_install(&install_user_locator);
        Self::bind_at(&endpoint, token)
    }

    /// Binds one exact canonical Keld endpoint with the supplied one-shot token.
    ///
    /// # Errors
    ///
    /// Returns an input error for an invalid endpoint or a pipe creation/ACL
    /// validation error.
    fn bind_at(endpoint: &str, token: SessionToken) -> io::Result<Self> {
        if !WindowsNamedPipeBootstrapStream::is_keld_endpoint(endpoint) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows lifecycle endpoint is not an exact Keld named pipe",
            ));
        }
        let server = WindowsNamedPipeServer::bind(endpoint)?;
        Ok(Self {
            server: Mutex::new(Some(server)),
            admission: Mutex::new(()),
            endpoint: endpoint.to_owned(),
            token,
            stopping: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            before_consume: Mutex::new(None),
            #[cfg(test)]
            handshake_witness: Mutex::new(None),
        })
    }

    /// Canonical endpoint-plus-token value for this bootstrap generation.
    #[must_use]
    pub fn app_link(&self) -> String {
        format_app_link(&self.endpoint, &self.token)
    }

    /// Pipe endpoint without the HELLO token.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns a handle that cancels pending accept or handshake I/O.
    #[must_use]
    pub fn cancellation(&self) -> WindowsNamedPipeBootstrapCancellation {
        WindowsNamedPipeBootstrapCancellation {
            server: self
                .server
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                .map_or_else(
                    WindowsNamedPipeCanceller::empty,
                    WindowsNamedPipeServer::canceller,
                ),
            stopping: Arc::clone(&self.stopping),
        }
    }

    /// Waits until one client authenticates or cancellation wins.
    ///
    /// # Errors
    ///
    /// Returns only host-side pipe, deadline-configuration, or cleanup errors.
    /// Peer failures are classified through `observer` and do not consume the
    /// bootstrap generation.
    pub fn accept_authenticated(
        &self,
        observer: &dyn BootstrapRejectionObserver,
    ) -> io::Result<Option<WindowsNamedPipeBootstrapStream>> {
        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match self.accept_loop(None, APP_LINK_IO_DEADLINE, observer)? {
            WindowsNamedPipeBootstrapAdmission::Authenticated(stream) => Ok(Some(stream)),
            WindowsNamedPipeBootstrapAdmission::Cancelled
            | WindowsNamedPipeBootstrapAdmission::DeadlineElapsed => Ok(None),
        }
    }

    /// Waits until a client authenticates, cancellation wins, or the absolute
    /// generation deadline elapses.
    ///
    /// # Errors
    ///
    /// Returns only host-side pipe, deadline-configuration, or cleanup errors.
    /// Peer failures are classified once through `observer`, disconnected,
    /// and followed by another accept on the same pipe instance.
    pub fn accept_authenticated_until(
        &self,
        deadline: Instant,
        observer: &dyn BootstrapRejectionObserver,
    ) -> io::Result<WindowsNamedPipeBootstrapAdmission> {
        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.accept_loop(Some(deadline), APP_LINK_IO_DEADLINE, observer)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "linear admission state machine keeps deadline and handle transitions auditable"
    )]
    fn accept_loop(
        &self,
        deadline: Option<Instant>,
        handshake_deadline: Duration,
        observer: &dyn BootstrapRejectionObserver,
    ) -> io::Result<WindowsNamedPipeBootstrapAdmission> {
        loop {
            let server = self
                .server
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                .cloned()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotConnected,
                        "named-pipe bootstrap already consumed",
                    )
                })?;
            if self.stopping.load(Ordering::Acquire) {
                server.close_terminal()?;
                drop(
                    self.server
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take(),
                );
                return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                server.close_terminal()?;
                drop(
                    self.server
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take(),
                );
                return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
            }
            match server.accept_until(deadline)? {
                WaitOutcome::Cancelled => {
                    drop(
                        self.server
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take(),
                    );
                    return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
                }
                WaitOutcome::DeadlineElapsed => {
                    drop(
                        self.server
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take(),
                    );
                    return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
                }
                WaitOutcome::PeerClosed => {
                    observer.rejected(BootstrapRejection::Io);
                    server.disconnect_for_retry()?;
                    continue;
                }
                WaitOutcome::Ready => {}
            }
            if self.stopping.load(Ordering::Acquire) {
                server.close_terminal()?;
                drop(
                    self.server
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take(),
                );
                return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
            }
            let handshake_started = Instant::now();
            let Some((peer_timeout, peer_deadline)) =
                peer_handshake_window(handshake_started, deadline, handshake_deadline)
            else {
                server.close_terminal()?;
                drop(
                    self.server
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take(),
                );
                return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
            };
            let mut stream = WindowsNamedPipeBootstrapStream(server.stream()?);
            stream.set_app_link_read_deadline(Some(APP_LINK_READER_POLL.min(peer_timeout)))?;
            stream.set_app_link_write_deadline(Some(peer_timeout))?;
            stream
                .0
                .set_absolute_deadline(Some(peer_deadline.instant()));
            #[cfg(test)]
            report_test_handshake_entry(&self.handshake_witness, deadline, peer_deadline.instant());
            match handshake_server_interruptible_until(
                &mut stream,
                &self.token,
                self.stopping.as_ref(),
                peer_deadline,
            ) {
                Ok(true) => {
                    #[cfg(test)]
                    wait_at_test_consume_gate(&self.before_consume);
                    if self.stopping.load(Ordering::Acquire) {
                        drop(stream);
                        server.close_terminal()?;
                        drop(
                            self.server
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .take(),
                        );
                        return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
                    }
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        drop(stream);
                        server.close_terminal()?;
                        drop(
                            self.server
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .take(),
                        );
                        return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
                    }
                    stream.0.set_absolute_deadline(None);
                    server.consume();
                    drop(
                        self.server
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take(),
                    );
                    return Ok(WindowsNamedPipeBootstrapAdmission::Authenticated(stream));
                }
                Ok(false) => {
                    drop(stream);
                    server.close_terminal()?;
                    drop(
                        self.server
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take(),
                    );
                    return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
                }
                Err(_) if self.stopping.load(Ordering::Acquire) => {
                    drop(stream);
                    server.close_terminal()?;
                    drop(
                        self.server
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take(),
                    );
                    return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
                }
                Err(IpcError::Timeout)
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) =>
                {
                    drop(stream);
                    server.close_terminal()?;
                    drop(
                        self.server
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take(),
                    );
                    return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
                }
                Err(error) => {
                    observer.rejected(BootstrapRejection::classify(&error));
                    drop(stream);
                    server.disconnect_for_retry()?;
                }
            }
        }
    }

    #[cfg(test)]
    fn inspect_pipe_handle<T>(
        &self,
        inspect: impl FnOnce(&std::os::windows::io::OwnedHandle) -> io::Result<T>,
    ) -> io::Result<T> {
        self.server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "bootstrap consumed"))?
            .inspect_owned_pipe(inspect)
    }

    #[cfg(test)]
    fn is_connected(&self) -> bool {
        self.server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(WindowsNamedPipeServer::is_connected)
    }

    #[cfg(test)]
    fn is_accept_pending(&self) -> bool {
        self.server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(WindowsNamedPipeServer::is_accept_pending)
    }

    #[cfg(test)]
    fn install_before_consume_gate(&self, gate: TestConsumeGate) {
        *self
            .before_consume
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(gate);
    }

    #[cfg(test)]
    fn install_handshake_witness(&self, witness: TestHandshakeWitness) {
        let replaced = self
            .handshake_witness
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(witness);
        assert!(replaced.is_none(), "handshake witness already installed");
    }

    /// Cancels admission and closes the pipe locator.
    ///
    /// # Errors
    ///
    /// Returns the first cancellation or terminal-close error.
    pub fn shutdown(&self) -> io::Result<()> {
        let cancel_error = self.cancellation().cancel().err();
        // CancelIoEx only requests cancellation. The admission owner keeps
        // every stack OVERLAPPED/buffer live until it observes completion;
        // do not close the pipe handle until that owner releases this guard.
        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let close_error = self
            .server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .and_then(|server| server.close_terminal().err());
        close_error.or(cancel_error).map_or(Ok(()), Err)
    }
}

#[cfg(windows)]
impl WindowsNamedPipeBootstrapCancellation {
    /// Cancels pending accept or stream I/O and wakes the admission worker.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if Windows cannot signal cancellation.
    pub fn cancel(&self) -> io::Result<()> {
        self.stopping.store(true, Ordering::Release);
        self.server.cancel()
    }
}

#[cfg(windows)]
impl io::Read for WindowsNamedPipeBootstrapStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

#[cfg(windows)]
impl io::Write for WindowsNamedPipeBootstrapStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(windows)]
impl AppLinkDeadlines for WindowsNamedPipeBootstrapStream {
    fn set_app_link_read_deadline(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_read_timeout(timeout)
    }

    fn set_app_link_write_deadline(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_write_timeout(timeout)
    }

    fn app_link_read_deadline(&self) -> io::Result<Option<Duration>> {
        Ok(self.0.read_timeout())
    }

    fn app_link_write_deadline(&self) -> io::Result<Option<Duration>> {
        Ok(self.0.write_timeout())
    }

    fn shutdown_app_link(&self) -> io::Result<()> {
        self.0.shutdown()
    }
}

#[cfg(windows)]
impl WindowsNamedPipeBootstrapStream {
    /// Derives the canonical keeper locator from a trusted install/user identity.
    /// It is discoverable before mutable journal reads and conveys no authority.
    #[must_use]
    pub fn endpoint_for_install(install_user_locator: &[u8; 32]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut endpoint = String::with_capacity(r"\\.\pipe\keld-".len() + 64);
        endpoint.push_str(r"\\.\pipe\keld-");
        for byte in install_user_locator {
            endpoint.push(char::from(HEX[usize::from(byte >> 4)]));
            endpoint.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        endpoint
    }

    /// Derives the distinct endpoint namespace for the KEL-53 lifecycle protocol.
    ///
    /// The prefix is a protocol discriminator checked before the nonce handshake;
    /// lifecycle peers cannot accidentally enter the ordinary app-link parser.
    #[must_use]
    pub fn endpoint_for_lifecycle_install(install_user_locator: &[u8; 32]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut endpoint = String::with_capacity(r"\\.\pipe\keld-lifecycle-".len() + 64);
        endpoint.push_str(r"\\.\pipe\keld-lifecycle-");
        for byte in install_user_locator {
            endpoint.push(char::from(HEX[usize::from(byte >> 4)]));
            endpoint.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        endpoint
    }

    /// Returns the operating-system PID of the process at the other end of this
    /// connected pipe. The caller must retain and authenticate that process
    /// identity before transferring lifecycle or installation capabilities.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the connected pipe handle cannot report its peer.
    pub fn peer_process_id(&self) -> io::Result<u32> {
        self.0.peer_process_id()
    }

    /// Returns the Windows Terminal Services session ID of the process at the
    /// other end of this connected pipe.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the connected pipe handle cannot report its peer.
    pub fn peer_session_id(&self) -> io::Result<u32> {
        self.0.peer_session_id()
    }

    /// Reports whether this connected pipe handle can be inherited by a child.
    /// Lifecycle endpoints must remain non-inheritable.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if Windows cannot read the handle flags.
    pub fn is_handle_inheritable(&self) -> io::Result<bool> {
        self.0.is_inheritable()
    }

    /// Returns whether `endpoint` has the exact host-minted Keld pipe shape.
    #[must_use]
    pub fn is_keld_endpoint(endpoint: &str) -> bool {
        endpoint
            .strip_prefix(r"\\.\pipe\keld-")
            .is_some_and(|nonce| {
                nonce.len() == 64
                    && nonce
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
    }

    /// Returns whether `endpoint` has the exact lifecycle-only pipe shape.
    #[must_use]
    pub fn is_lifecycle_endpoint(endpoint: &str) -> bool {
        endpoint
            .strip_prefix(r"\\.\pipe\keld-lifecycle-")
            .is_some_and(|nonce| {
                nonce.len() == 64
                    && nonce
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
    }

    /// Opens a client handle to an exact named-pipe endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if Windows cannot open the pipe or create the
    /// stream's manual-reset completion events.
    pub fn connect(endpoint: &str) -> io::Result<Self> {
        if !Self::is_keld_endpoint(endpoint) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows app-link endpoint is not an exact Keld named pipe",
            ));
        }
        let deadline = Instant::now()
            .checked_add(APP_LINK_IO_DEADLINE)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "deadline overflow"))?;
        WindowsNamedPipeServer::connect_client_until(endpoint, deadline).map(Self)
    }

    /// Duplicates the Rust stream view while retaining the same owned pipe
    /// handle. Read and write deadline settings are copied, then independent.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if either manual-reset event for the cloned
    /// stream cannot be created with `CreateEventW`.
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }
}

#[cfg(all(test, windows))]
mod named_pipe_tests {
    #![allow(unsafe_code)] // test-only independent Win32 descriptor/handle oracle

    use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::{Arc, Mutex, PoisonError, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use windows_permissions::constants::{AceFlags, AceType, SeObjectType, SecurityInformation};
    use windows_permissions::utilities::current_process_sid;
    use windows_permissions::wrappers::GetSecurityInfo;
    use windows_sys::Win32::Foundation::{
        GetHandleInformation, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Pipes::{GetNamedPipeInfo, PIPE_REJECT_REMOTE_CLIENTS};
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::{
        GetProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        WaitForSingleObject,
    };

    use crate::link::{AppLinkDeadlines, handshake_client};
    use crate::serve_echo_requests;
    use crate::token::{SessionToken, parse_app_link};
    use crate::windows_named_pipe::{WaitOutcome, WindowsNamedPipeServer, process_handle_count};
    use crate::{ChannelId, CorrelationId, FrameHeader, FrameKind, MAX_FRAME_LEN};

    use super::{
        BootstrapListener, BootstrapRejection, BootstrapRejectionObserver, TestConsumeGate,
        WindowsLifecycleBinding, WindowsLifecycleExpectation, WindowsLifecyclePeerPin,
        WindowsLifecyclePurpose, WindowsLifecycleRendezvousListener,
        WindowsNamedPipeBootstrapAdmission, WindowsNamedPipeBootstrapListener,
        WindowsNamedPipeBootstrapStream, WindowsPeerTokenFacts,
    };

    #[expect(
        clippy::expect_used,
        reason = "fixed nonzero, pairwise-distinct test IDs are a fixture invariant"
    )]
    fn test_lifecycle_binding() -> WindowsLifecycleBinding {
        WindowsLifecycleBinding::new(
            [0x11; 32],
            [0x22; 32],
            [0x33; 32],
            WindowsLifecyclePurpose::CoordinatorToKeeper,
        )
        .expect("distinct test lifecycle identities")
    }

    #[derive(Clone)]
    struct RecordingObserver {
        seen: Arc<Mutex<Vec<BootstrapRejection>>>,
        notify: Option<mpsc::Sender<BootstrapRejection>>,
    }

    impl BootstrapRejectionObserver for RecordingObserver {
        fn rejected(&self, rejection: BootstrapRejection) {
            self.seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(rejection);
            if let Some(notify) = &self.notify {
                let _ = notify.send(rejection);
            }
        }
    }

    struct BlockingObserver {
        observed: mpsc::Sender<BootstrapRejection>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    #[derive(Debug)]
    struct TestPeerProcess {
        process_id: u32,
        session_id: u32,
        process: OwnedHandle,
        token_facts: WindowsPeerTokenFacts,
    }

    impl TestPeerProcess {
        fn open(process_id: u32, session_id: u32) -> Option<Self> {
            // SAFETY: process_id came from a connected local pipe or current child.
            let raw = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    process_id,
                )
            };
            if raw.is_null() {
                return None;
            }
            // SAFETY: OpenProcess returned one fresh owning process handle.
            let process = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
            // SAFETY: process is retained; GetProcessId is read-only.
            if unsafe { GetProcessId(process.as_raw_handle().cast()) } != process_id {
                return None;
            }
            // SAFETY: process handle has PROCESS_SYNCHRONIZE and is retained.
            if unsafe { WaitForSingleObject(process.as_raw_handle().cast(), 0) } != WAIT_TIMEOUT {
                return None;
            }
            let mut raw_token = std::ptr::null_mut();
            // SAFETY: process is the retained exact child and token output is writable.
            if unsafe {
                windows_sys::Win32::System::Threading::OpenProcessToken(
                    process.as_raw_handle().cast(),
                    windows_sys::Win32::Security::TOKEN_QUERY,
                    &raw mut raw_token,
                )
            } == 0
            {
                return None;
            }
            // SAFETY: OpenProcessToken returned one fresh owned token handle.
            let token = unsafe { OwnedHandle::from_raw_handle(raw_token.cast()) };
            let token_facts = crate::query_windows_peer_token_facts(&token).ok()?;
            if token_facts.session_id != session_id {
                return None;
            }
            Some(Self {
                process_id,
                session_id,
                process,
                token_facts,
            })
        }
    }

    impl super::WindowsLifecyclePeerPin for TestPeerProcess {
        fn process_id(&self) -> u32 {
            self.process_id
        }

        fn session_id(&self) -> u32 {
            self.session_id
        }

        fn has_exited(&self) -> io::Result<bool> {
            // SAFETY: process is a retained process object with synchronize rights.
            match unsafe { WaitForSingleObject(self.process.as_raw_handle().cast(), 0) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                _ => Err(io::Error::last_os_error()),
            }
        }
    }

    impl BootstrapRejectionObserver for BlockingObserver {
        fn rejected(&self, rejection: BootstrapRejection) {
            let _ = self.observed.send(rejection);
            let _ = self
                .release
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .recv();
        }
    }

    fn client(endpoint: &str) -> std::io::Result<WindowsNamedPipeBootstrapStream> {
        WindowsNamedPipeServer::connect_client_until(
            endpoint,
            Instant::now() + Duration::from_secs(2),
        )
        .map(WindowsNamedPipeBootstrapStream)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the fixture exercises both the installed-rendezvous client child and the legacy token helper"
    )]
    #[test]
    fn install_rendezvous_bootstrap_reports_exact_pipe_peer_after_hello() {
        const LIFECYCLE_ENDPOINT_ENV: &str = "KELD_TEST_LIFECYCLE_ENDPOINT";
        const LIFECYCLE_SERVER_PID_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_PID";
        const LIFECYCLE_SERVER_SESSION_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_SESSION";
        const ENDPOINT_ENV: &str = "KELD_TEST_KEEPER_ENDPOINT";
        const TOKEN_ENV: &str = "KELD_TEST_KEEPER_TOKEN";
        if let (Some(endpoint), Some(server_pid), Some(server_session)) = (
            std::env::var_os(LIFECYCLE_ENDPOINT_ENV),
            std::env::var_os(LIFECYCLE_SERVER_PID_ENV),
            std::env::var_os(LIFECYCLE_SERVER_SESSION_ENV),
        ) {
            let endpoint = endpoint.to_string_lossy();
            let server_pid = server_pid
                .to_string_lossy()
                .parse::<u32>()
                .expect("valid server PID");
            let server_session = server_session
                .to_string_lossy()
                .parse::<u32>()
                .expect("valid server session");
            let start_gated = std::env::var_os("KELD_TEST_LIFECYCLE_START_GATED").is_some();
            if start_gated {
                let mut start = [0_u8; 1];
                std::io::stdin()
                    .read_exact(&mut start)
                    .expect("wait for test-controlled lifecycle connect gate");
                assert_eq!(start, [1]);
            }
            let expectation = if std::env::var_os("KELD_TEST_LIFECYCLE_DISCOVERY").is_some() {
                WindowsLifecycleExpectation::from_keeper([0x11; 32])
                    .expect("known install identity for cold successor")
            } else {
                let binding = if std::env::var_os("KELD_TEST_LIFECYCLE_STALE_BINDING").is_some() {
                    WindowsLifecycleBinding::new(
                        [0x11; 32],
                        [0x44; 32],
                        [0x33; 32],
                        WindowsLifecyclePurpose::CoordinatorToKeeper,
                    )
                    .expect("distinct stale-attempt identities")
                } else {
                    test_lifecycle_binding()
                };
                WindowsLifecycleExpectation::exact(binding)
            };
            let connect_timeout = std::env::var("KELD_TEST_LIFECYCLE_CONNECT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .map_or(Duration::from_secs(5), Duration::from_millis);
            if std::env::var_os("KELD_TEST_LIFECYCLE_BUSY_WITNESS").is_some() {
                let (busy_tx, busy_rx) = mpsc::channel();
                WindowsNamedPipeServer::install_connect_busy_witness(busy_tx);
                thread::spawn(move || {
                    if busy_rx.recv().is_ok() {
                        println!("CLIENT_PIPE_BUSY");
                        let _ = std::io::stdout().flush();
                    }
                });
            }
            let mut connection = crate::connect_windows_lifecycle_rendezvous_until(
                &endpoint,
                expectation,
                Instant::now() + connect_timeout,
                |pid, session| {
                    if pid != server_pid || session != server_session {
                        return None;
                    }
                    TestPeerProcess::open(pid, session)
                },
            )
            .expect("authenticate actual lifecycle server process");
            assert!(
                !connection
                    .stream_mut()
                    .is_handle_inheritable()
                    .expect("read client pipe inheritance"),
                "lifecycle client handle must not be inheritable"
            );
            assert!(
                !connection
                    .process_pin()
                    .has_exited()
                    .expect("check retained server process object"),
                "server process pin must survive the nonce exchange"
            );
            println!(
                "CLIENT_LIFECYCLE_OK client={} server={} nonceC={} nonceS={} knowledge={:?}",
                std::process::id(),
                server_pid,
                connection.client_nonce().to_hex(),
                connection.server_nonce().to_hex(),
                connection.binding_knowledge()
            );
            std::io::stdout()
                .flush()
                .expect("flush client nonce transcript");
            if start_gated {
                let mut exit = String::new();
                let stdin = std::io::stdin();
                let mut input = BufReader::new(stdin);
                std::io::BufRead::read_line(&mut input, &mut exit)
                    .expect("wait for parent to release authenticated client");
                assert_eq!(exit.trim_end(), "EXIT");
            }
            return;
        }

        if let (Some(endpoint), Some(token_hex)) =
            (std::env::var_os(ENDPOINT_ENV), std::env::var_os(TOKEN_ENV))
        {
            let endpoint = endpoint.to_string_lossy();
            let token = SessionToken::from_hex(&token_hex.to_string_lossy())
                .expect("valid keeper bootstrap token");
            let mut stream = WindowsNamedPipeBootstrapStream::connect(&endpoint)
                .expect("connect keeper bootstrap endpoint");
            let server_pid = stream.peer_process_id().expect("read keeper process PID");
            let server_session = stream
                .peer_session_id()
                .expect("read keeper process session");
            handshake_client(&mut stream, &token).expect("authenticate keeper HELLO");
            println!("KEEPER_SERVER_PID={server_pid} SESSION={server_session}");
            return;
        }

        let mut install_user_locator = [0_u8; 32];
        getrandom::fill(&mut install_user_locator).expect("install/user locator randomness");
        let token = SessionToken::random().expect("keeper token randomness");
        let endpoint = WindowsNamedPipeBootstrapStream::endpoint_for_install(&install_user_locator);
        assert!(WindowsNamedPipeBootstrapStream::is_keld_endpoint(&endpoint));
        assert_eq!(
            endpoint,
            WindowsNamedPipeBootstrapStream::endpoint_for_install(&install_user_locator),
            "the same trusted install/user identity has one deterministic locator"
        );
        let listener = BootstrapListener::bind_windows_rendezvous(install_user_locator, token)
            .expect("bind install/user-scoped keeper listener");
        let child = Command::new(std::env::current_exe().expect("current test binary"))
            .args([
                "--exact",
                "bootstrap::named_pipe_tests::install_rendezvous_bootstrap_reports_exact_pipe_peer_after_hello",
                "--nocapture",
            ])
            .env(ENDPOINT_ENV, &endpoint)
            .env(TOKEN_ENV, token.to_hex())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn authenticated keeper peer");
        let stream = listener
            .accept_authenticated()
            .expect("accept keeper peer")
            .expect("peer authenticated before deadline");
        assert_eq!(
            stream
                .peer_process_id()
                .expect("read exact keeper peer PID"),
            child.id(),
            "the accepted process handle must bind to the actual client"
        );
        let client_session = stream
            .peer_session_id()
            .expect("read exact keeper peer session");
        let mut server_session = 0_u32;
        // SAFETY: current PID and writable session output are valid for this
        // synchronous process-session query.
        assert_ne!(
            unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
            0,
            "query server session"
        );
        assert_eq!(client_session, server_session);
        let output = child.wait_with_output().expect("wait authenticated client");
        assert!(
            output.status.success(),
            "client failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(&format!(
                "KEEPER_SERVER_PID={} SESSION={server_session}",
                std::process::id()
            )),
            "client must bind to the actual server process"
        );
        drop(stream);
        listener
            .shutdown()
            .expect("consume and close keeper listener");
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the subprocess controls wrong-peer rejection, retry, exact nonce binding and process-pin retention"
    )]
    #[test]
    fn lifecycle_rendezvous_rejects_wrong_process_then_authenticates_fresh_peer() {
        use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

        const LIFECYCLE_ENDPOINT_ENV: &str = "KELD_TEST_LIFECYCLE_ENDPOINT";
        const SERVER_PID_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_PID";
        const SERVER_SESSION_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_SESSION";
        let locator = [0x61; 32];
        let listener = Arc::new(
            WindowsLifecycleRendezvousListener::bind(locator, test_lifecycle_binding())
                .expect("bind install/user lifecycle locator"),
        );
        let endpoint = listener.endpoint().to_owned();
        let mut server_session = 0_u32;
        // SAFETY: this test process is live and the session output is writable.
        assert_ne!(
            unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
            0
        );
        let allowed_client = Arc::new(AtomicU32::new(0));
        let allowed_for_server = Arc::clone(&allowed_client);
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let listener_for_acceptor = Arc::clone(&listener);
        let acceptor = thread::spawn(move || {
            let accepted = listener_for_acceptor
                .accept_until(
                    Instant::now() + Duration::from_secs(5),
                    |pid, session, facts| {
                        if pid != allowed_for_server.load(AtomicOrdering::Acquire)
                            || session != server_session
                            || facts.session_id != session
                            || facts.user_sid.is_empty()
                            || facts.integrity_rid == 0
                        {
                            return None;
                        }
                        let pin = TestPeerProcess::open(pid, session)?;
                        (&pin.token_facts == facts).then_some(pin)
                    },
                )
                .expect("accept lifecycle peer");
            accepted_tx.send(accepted).expect("publish accepted peer");
        });

        let spawn_client = |start_gated: bool,
                            stale_binding: bool,
                            connect_timeout_ms: Option<u64>| {
            let mut command = Command::new(std::env::current_exe().expect("current test binary"));
            command
                .args([
                    "--exact",
                    "bootstrap::named_pipe_tests::install_rendezvous_bootstrap_reports_exact_pipe_peer_after_hello",
                    "--nocapture",
                ])
                .env(LIFECYCLE_ENDPOINT_ENV, &endpoint)
                .env(SERVER_PID_ENV, std::process::id().to_string())
                .env(SERVER_SESSION_ENV, server_session.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            if start_gated {
                command
                    .env("KELD_TEST_LIFECYCLE_START_GATED", "1")
                    .stdin(Stdio::piped());
            } else {
                command.stdin(Stdio::null());
            }
            if stale_binding {
                command.env("KELD_TEST_LIFECYCLE_STALE_BINDING", "1");
            }
            if let Some(timeout_ms) = connect_timeout_ms {
                command.env("KELD_TEST_LIFECYCLE_CONNECT_MS", timeout_ms.to_string());
            }
            command.spawn().expect("spawn lifecycle client")
        };

        let rejected_client = spawn_client(false, false, None);
        let rejected_id = rejected_client.id();
        let rejected_output = rejected_client
            .wait_with_output()
            .expect("wait rejected peer");
        assert!(
            !rejected_output.status.success(),
            "wrong client must not complete rendezvous"
        );
        assert!(
            !String::from_utf8_lossy(&rejected_output.stdout).contains("CLIENT_LIFECYCLE_OK"),
            "rejected client must receive no nonce echo or completion record"
        );
        assert!(
            accepted_rx.try_recv().is_err(),
            "wrong peer identity must not consume the one-shot listener"
        );

        let mut stale_client = spawn_client(true, true, None);
        let stale_id = stale_client.id();
        allowed_client.store(stale_id, AtomicOrdering::Release);
        stale_client
            .stdin
            .as_mut()
            .expect("stale-client gate pipe")
            .write_all(&[1])
            .expect("release stale-attempt client");
        let stale_output = stale_client
            .wait_with_output()
            .expect("wait stale-attempt client");
        assert!(
            !stale_output.status.success(),
            "stale attempt binding must be rejected before handoff"
        );
        assert!(
            !String::from_utf8_lossy(&stale_output.stdout).contains("CLIENT_LIFECYCLE_OK"),
            "stale attempt cannot receive a binding acknowledgement"
        );
        assert!(
            accepted_rx.try_recv().is_err(),
            "stale attempt must not consume this attempt's listener"
        );

        let mut accepted_client = spawn_client(true, false, None);
        let accepted_id = accepted_client.id();
        allowed_client.store(accepted_id, AtomicOrdering::Release);
        accepted_client
            .stdin
            .as_mut()
            .expect("accepted-client gate pipe")
            .write_all(&[1])
            .expect("release accepted-client gate");
        let accepted_stdout = accepted_client
            .stdout
            .take()
            .expect("accepted client stdout");
        let mut accepted_lines = BufReader::new(accepted_stdout).lines();
        let client_line = accepted_lines
            .find_map(|line| match line {
                Ok(line) if line.starts_with("CLIENT_LIFECYCLE_OK ") => Some(Ok(line)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .expect("client result line iterator ended")
            .expect("read client result line");
        let mut peer = accepted_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("authorized client accepted before deadline")
            .expect("authorized peer result");
        acceptor.join().expect("join lifecycle acceptor");
        assert_eq!(peer.process_id(), accepted_id);
        assert_ne!(peer.process_id(), rejected_id);
        assert_eq!(peer.session_id(), server_session);
        assert_eq!(peer.token_facts().session_id, server_session);
        assert!(!peer.token_facts().user_sid.is_empty());
        assert!(
            !peer
                .process_pin()
                .has_exited()
                .expect("check retained client process object"),
            "client process pin must survive until the test retains the rendezvous"
        );
        assert_ne!(peer.client_nonce(), peer.server_nonce());
        assert!(
            !peer
                .stream_mut()
                .is_handle_inheritable()
                .expect("server pipe inheritance"),
            "lifecycle server handle must not be inheritable"
        );
        assert!(
            client_line.contains(&peer.client_nonce().to_hex())
                && client_line.contains(&peer.server_nonce().to_hex()),
            "both peers must agree on the exact fresh nonce pair: {client_line}"
        );
        writeln!(
            accepted_client
                .stdin
                .as_mut()
                .expect("accepted client exit gate"),
            "EXIT"
        )
        .expect("release accepted client after peer-pin assertions");
        drop(accepted_client.stdin.take());
        assert!(
            accepted_client
                .wait()
                .expect("wait accepted client")
                .success(),
            "accepted lifecycle client exits cleanly"
        );
        for line in accepted_lines {
            line.expect("drain accepted client output");
        }
        drop(peer);
        let second_accept = listener.accept_until(
            Instant::now() + Duration::from_millis(250),
            |_, _, _| -> Option<TestPeerProcess> {
                panic!("a consumed listener must reject before peer authorization")
            },
        );
        assert!(
            matches!(&second_accept, Err(error) if error.kind() == io::ErrorKind::NotConnected),
            "the retained listener itself must reject a second authorized acceptance after the first connection closes: {second_accept:?}"
        );
        let competing_client = spawn_client(false, false, Some(250));
        let competing_output = competing_client
            .wait_with_output()
            .expect("wait one-shot listener competitor");
        assert!(
            !competing_output.status.success(),
            "a consumed attempt listener must reject a second successor"
        );
        assert!(
            !String::from_utf8_lossy(&competing_output.stdout).contains("CLIENT_LIFECYCLE_OK"),
            "competing successor must receive no acceptance transcript"
        );
    }

    #[expect(
        clippy::too_many_lines,
        reason = "simultaneous authorized contenders and the accepted peer transcript form one adversarial oracle"
    )]
    #[test]
    fn lifecycle_rendezvous_race_admits_at_most_one_authorized_client() {
        const ENDPOINT_ENV: &str = "KELD_TEST_LIFECYCLE_ENDPOINT";
        const SERVER_PID_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_PID";
        const SERVER_SESSION_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_SESSION";
        const CLIENT_TEST: &str = "bootstrap::named_pipe_tests::install_rendezvous_bootstrap_reports_exact_pipe_peer_after_hello";
        let listener = Arc::new(
            WindowsLifecycleRendezvousListener::bind([0x64; 32], test_lifecycle_binding())
                .expect("bind one-shot lifecycle endpoint"),
        );
        let endpoint = listener.endpoint().to_owned();
        let mut server_session = 0_u32;
        // SAFETY: this test process is live and the session output is writable.
        assert_ne!(
            unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
            0
        );
        let authorized_ids = Arc::new(Mutex::new([0_u32; 2]));
        let (gate_entered_tx, gate_entered_rx) = mpsc::sync_channel(1);
        let (gate_release_tx, gate_release_rx) = mpsc::channel();
        listener.install_before_consume_gate(TestConsumeGate {
            entered: gate_entered_tx,
            release: gate_release_rx,
        });
        let listener_for_acceptor = Arc::clone(&listener);
        let allowed_for_server = Arc::clone(&authorized_ids);
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let acceptor = thread::spawn(move || {
            let accepted = listener_for_acceptor
                .accept_until(
                    Instant::now() + Duration::from_secs(5),
                    move |pid, session, facts| {
                        let authorized = allowed_for_server
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .contains(&pid);
                        if !authorized || session != server_session || facts.session_id != session {
                            return None;
                        }
                        let pin = TestPeerProcess::open(pid, session)?;
                        (pin.token_facts == *facts).then_some(pin)
                    },
                )
                .expect("accept one authorized contender before deadline");
            accepted_tx
                .send(accepted)
                .expect("publish the one accepted lifecycle peer");
        });
        let spawn_client = |busy_witness: bool| {
            let mut command = Command::new(std::env::current_exe().expect("current test binary"));
            command
                .args(["--exact", CLIENT_TEST, "--nocapture"])
                .env(ENDPOINT_ENV, &endpoint)
                .env(SERVER_PID_ENV, std::process::id().to_string())
                .env(SERVER_SESSION_ENV, server_session.to_string())
                .env("KELD_TEST_LIFECYCLE_START_GATED", "1")
                .env("KELD_TEST_LIFECYCLE_CONNECT_MS", "1500")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            if busy_witness {
                command.env("KELD_TEST_LIFECYCLE_BUSY_WITNESS", "1");
            }
            command.spawn().expect("spawn authorized contender")
        };
        let mut first = spawn_client(false);
        authorized_ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)[0] = first.id();
        first
            .stdin
            .as_mut()
            .expect("first contender start gate")
            .write_all(&[1])
            .expect("release first contender");
        gate_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("first authorized contender reaches post-authentication barrier");

        let mut second = spawn_client(true);
        authorized_ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)[1] = second.id();
        let (second_output_tx, second_output_rx) = mpsc::channel();
        let second_stdout = second.stdout.take().expect("second contender stdout");
        let output_reader = thread::spawn(move || {
            for line in BufReader::new(second_stdout).lines() {
                if second_output_tx
                    .send(line.expect("read contender output"))
                    .is_err()
                {
                    break;
                }
            }
        });
        second
            .stdin
            .as_mut()
            .expect("second contender start gate")
            .write_all(&[1])
            .expect("release second contender while first admission is held");
        let mut saw_busy = false;
        while !saw_busy {
            let line = second_output_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("second contender must reach the occupied pipe while the first is held");
            saw_busy = line == "CLIENT_PIPE_BUSY";
        }
        gate_release_tx
            .send(())
            .expect("release first contender after second observed ERROR_PIPE_BUSY");
        let accepted = accepted_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("receive the exactly-once listener result")
            .expect("first contender completes the one-shot handshake");
        acceptor.join().expect("join one-shot acceptor");
        if let Some(mut stdin) = first.stdin.take() {
            writeln!(stdin, "EXIT").expect("release accepted first contender");
        }
        let first_output = first.wait_with_output().expect("wait first contender");
        if let Some(mut stdin) = second.stdin.take() {
            let _ = writeln!(stdin, "EXIT");
        }
        let second_status = second.wait().expect("wait second contender");
        output_reader.join().expect("join contender output reader");
        let second_lines = second_output_rx.try_iter().collect::<Vec<_>>();
        assert!(
            !second_status.success(),
            "the authorized contender that observed ERROR_PIPE_BUSY cannot complete after one-shot consumption: {second_lines:?}"
        );
        assert!(
            !second_lines
                .iter()
                .any(|line| line.starts_with("CLIENT_LIFECYCLE_OK ")),
            "losing authorized contender receives no success transcript: {second_lines:?}"
        );
        assert!(
            first_output.status.success(),
            "the first admitted contender exits cleanly: {}",
            String::from_utf8_lossy(&first_output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&first_output.stdout).contains("CLIENT_LIFECYCLE_OK "),
            "the accepted contender completed the exact nonce transcript"
        );
        let accepted_pid = accepted.process_id();
        assert!(
            authorized_ids
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains(&accepted_pid),
            "accepted peer must be one of the two authorized contenders"
        );
        drop(accepted);
    }

    #[test]
    fn authenticated_keeper_supplies_attempt_to_cold_successor() {
        const ENDPOINT_ENV: &str = "KELD_TEST_LIFECYCLE_ENDPOINT";
        const SERVER_PID_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_PID";
        const SERVER_SESSION_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_SESSION";
        let binding = WindowsLifecycleBinding::new(
            [0x11; 32],
            [0x22; 32],
            [0x33; 32],
            WindowsLifecyclePurpose::KeeperToSuccessor,
        )
        .expect("independent keeper attempt context");
        let listener = WindowsLifecycleRendezvousListener::bind([0x62; 32], binding)
            .expect("bind lifecycle keeper endpoint");
        let endpoint = listener.endpoint().to_owned();
        let mut server_session = 0_u32;
        // SAFETY: this test process is live and the session output is writable.
        assert_ne!(
            unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
            0
        );
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "bootstrap::named_pipe_tests::install_rendezvous_bootstrap_reports_exact_pipe_peer_after_hello",
                "--nocapture",
            ])
            .env(ENDPOINT_ENV, &endpoint)
            .env(SERVER_PID_ENV, std::process::id().to_string())
            .env(SERVER_SESSION_ENV, server_session.to_string())
            .env("KELD_TEST_LIFECYCLE_DISCOVERY", "1")
            .env("KELD_TEST_LIFECYCLE_START_GATED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut successor = command.spawn().expect("spawn cold successor fixture");
        successor
            .stdin
            .as_mut()
            .expect("successor start gate")
            .write_all(&[1])
            .expect("release successor connect");
        let peer = listener
            .accept_until(
                Instant::now() + Duration::from_secs(5),
                |pid, session, facts| {
                    if session != server_session || facts.session_id != session {
                        return None;
                    }
                    let pin = TestPeerProcess::open(pid, session)?;
                    (pin.token_facts == *facts).then_some(pin)
                },
            )
            .expect("accept authenticated cold successor")
            .expect("successor connected before deadline");
        assert_eq!(peer.binding(), binding);
        let stdout = successor.stdout.take().expect("successor stdout");
        let mut lines = BufReader::new(stdout).lines();
        let record = lines
            .find_map(|line| match line {
                Ok(line) if line.starts_with("CLIENT_LIFECYCLE_OK ") => Some(Ok(line)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .expect("cold successor result line")
            .expect("read cold successor result");
        assert!(
            record.contains("knowledge=KeeperSuppliedAwaitingJournalRevalidation"),
            "discovered attempt must remain untrusted until lease/journal revalidation: {record}"
        );
        writeln!(
            successor.stdin.as_mut().expect("successor release gate"),
            "EXIT"
        )
        .expect("release authenticated successor");
        drop(successor.stdin.take());
        assert!(successor.wait().expect("wait successor").success());
        assert!(
            peer.process_pin()
                .has_exited()
                .expect("successor process exit"),
            "keeper retains exact successor process object through release"
        );
    }

    #[test]
    fn expired_final_receipt_does_not_admit_lifecycle_peer() {
        const ENDPOINT_ENV: &str = "KELD_TEST_LIFECYCLE_ENDPOINT";
        const SERVER_PID_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_PID";
        const SERVER_SESSION_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_SESSION";
        let binding = test_lifecycle_binding();
        let listener = WindowsLifecycleRendezvousListener::bind([0x73; 32], binding)
            .expect("bind lifecycle deadline endpoint");
        let endpoint = listener.endpoint().to_owned();
        let mut server_session = 0_u32;
        // SAFETY: this live process PID and writable session output are valid.
        assert_ne!(
            unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
            0
        );
        let (gate_entered_tx, gate_entered_rx) = mpsc::sync_channel(1);
        let (gate_release_tx, gate_release_rx) = mpsc::channel();
        listener.install_before_receipt_gate(TestConsumeGate {
            entered: gate_entered_tx,
            release: gate_release_rx,
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let acceptor = thread::spawn(move || {
            let accepted = listener
                .accept_until(deadline, |pid, session, facts| {
                    if session != server_session || facts.session_id != session {
                        return None;
                    }
                    let pin = TestPeerProcess::open(pid, session)?;
                    (pin.token_facts == *facts).then_some(pin)
                })
                .expect("accept lifecycle peer through deadline");
            accepted_tx
                .send(accepted.is_some())
                .expect("publish lifecycle admission result");
        });
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "bootstrap::named_pipe_tests::install_rendezvous_bootstrap_reports_exact_pipe_peer_after_hello",
                "--nocapture",
            ])
            .env(ENDPOINT_ENV, &endpoint)
            .env(SERVER_PID_ENV, std::process::id().to_string())
            .env(SERVER_SESSION_ENV, server_session.to_string())
            .env("KELD_TEST_LIFECYCLE_START_GATED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut client = command.spawn().expect("spawn deadline-bound client");
        client
            .stdin
            .as_mut()
            .expect("client start gate")
            .write_all(&[1])
            .expect("release deadline-bound client");
        gate_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("server reached post-consume receipt gate");
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (_timer_tx, timer_rx) = mpsc::channel::<()>();
        assert_eq!(
            timer_rx.recv_timeout(remaining + Duration::from_millis(25)),
            Err(mpsc::RecvTimeoutError::Timeout),
            "timer must expire after the handshake deadline"
        );
        gate_release_tx
            .send(())
            .expect("release expired receipt gate");
        assert!(
            !accepted_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("server reports deadline result"),
            "receipt missing before deadline must not admit peer"
        );
        acceptor.join().expect("join deadline-bound acceptor");
        let output = client
            .wait_with_output()
            .expect("wait client whose receipt was refused");
        assert!(!output.status.success(), "client must require LR1");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("CLIENT_LIFECYCLE_OK"),
            "client emits no accepted marker without the final receipt"
        );
    }

    #[test]
    fn lifecycle_client_rejects_fake_server_before_sending_nonce() {
        const ENDPOINT_ENV: &str = "KELD_TEST_LIFECYCLE_FAKE_SERVER_ENDPOINT";
        if let Some(endpoint) = std::env::var_os(ENDPOINT_ENV) {
            let result = crate::connect_windows_lifecycle_rendezvous_until(
                &endpoint.to_string_lossy(),
                WindowsLifecycleExpectation::exact(test_lifecycle_binding()),
                Instant::now() + Duration::from_secs(5),
                |_, _| None::<TestPeerProcess>,
            );
            assert!(
                matches!(result, Err(error) if error.kind() == io::ErrorKind::PermissionDenied),
                "an untrusted endpoint process must be rejected before HELLO"
            );
            return;
        }

        let endpoint = super::random_lifecycle_pipe_endpoint().expect("mint fake-server endpoint");
        let server = WindowsNamedPipeServer::bind(&endpoint).expect("bind fake server");
        let child = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "bootstrap::named_pipe_tests::lifecycle_client_rejects_fake_server_before_sending_nonce",
                "--nocapture",
            ])
            .env(ENDPOINT_ENV, &endpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn lifecycle client against fake endpoint");
        assert_eq!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(5)))
                .expect("accept fake endpoint client"),
            WaitOutcome::Ready
        );
        let mut stream = WindowsNamedPipeBootstrapStream(server.stream().expect("fake stream"));
        let mut byte = [0_u8; 1];
        let read = stream.read(&mut byte);
        assert!(
            matches!(read, Ok(0) | Err(_)),
            "fake endpoint must receive no HELLO byte, got {read:?}"
        );
        server.consume();
        drop(stream);
        let output = child.wait_with_output().expect("wait fake-endpoint client");
        assert!(
            output.status.success(),
            "client did not reject the fake endpoint: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn ordinary_and_lifecycle_pipe_namespaces_refuse_cross_protocol_connects() {
        let locator = [0x72; 32];
        let app_link = WindowsNamedPipeBootstrapStream::endpoint_for_install(&locator);
        let lifecycle = WindowsNamedPipeBootstrapStream::endpoint_for_lifecycle_install(&locator);
        assert!(WindowsNamedPipeBootstrapStream::is_keld_endpoint(&app_link));
        assert!(!WindowsNamedPipeBootstrapStream::is_lifecycle_endpoint(
            &app_link
        ));
        assert!(WindowsNamedPipeBootstrapStream::is_lifecycle_endpoint(
            &lifecycle
        ));
        assert!(!WindowsNamedPipeBootstrapStream::is_keld_endpoint(
            &lifecycle
        ));

        let app_to_lifecycle = WindowsNamedPipeBootstrapStream::connect(&lifecycle)
            .expect_err("ordinary app-link client must refuse lifecycle namespace");
        assert_eq!(app_to_lifecycle.kind(), io::ErrorKind::InvalidInput);
        let lifecycle_to_app = crate::connect_windows_lifecycle_rendezvous_until(
            &app_link,
            WindowsLifecycleExpectation::exact(test_lifecycle_binding()),
            Instant::now() + Duration::from_secs(1),
            |_, _| None::<TestPeerProcess>,
        )
        .expect_err("lifecycle client must refuse ordinary app-link namespace");
        assert_eq!(lifecycle_to_app.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn lifecycle_context_record_has_exact_shape_and_binds_nonce_pair_and_roles() {
        let binding = test_lifecycle_binding();
        let client_nonce = SessionToken::from_bytes([0x44; 32]);
        let server_nonce = SessionToken::from_bytes([0x55; 32]);
        let challenge = super::lifecycle_binding_record(
            *b"KELD-LC1",
            binding,
            client_nonce,
            server_nonce,
            0x1122_3344,
            0x5566_7788,
        );
        assert_eq!(challenge.len(), 177);
        assert_eq!(&challenge[..8], b"KELD-LC1");
        assert_eq!(
            challenge[8],
            WindowsLifecyclePurpose::CoordinatorToKeeper as u8
        );
        assert_eq!(&challenge[9..41], &[0x11; 32]);
        assert_eq!(&challenge[41..73], &[0x22; 32]);
        assert_eq!(&challenge[73..105], &[0x33; 32]);
        assert_eq!(&challenge[105..137], &[0x44; 32]);
        assert_eq!(&challenge[137..169], &[0x55; 32]);
        assert_eq!(&challenge[169..173], &0x1122_3344_u32.to_le_bytes());
        assert_eq!(&challenge[173..177], &0x5566_7788_u32.to_le_bytes());
        let acknowledgement = super::lifecycle_binding_record(
            *b"KELD-LA1",
            binding,
            client_nonce,
            server_nonce,
            0x1122_3344,
            0x5566_7788,
        );
        assert_eq!(&acknowledgement[..8], b"KELD-LA1");
        let acceptance_receipt = super::lifecycle_binding_record(
            *b"KELD-LR1",
            binding,
            client_nonce,
            server_nonce,
            0x1122_3344,
            0x5566_7788,
        );
        assert_eq!(&acceptance_receipt[..8], b"KELD-LR1");
        assert_ne!(
            challenge,
            super::lifecycle_binding_record(
                *b"KELD-LC1",
                binding,
                server_nonce,
                client_nonce,
                0x1122_3344,
                0x5566_7788,
            ),
            "replayed or swapped nonce transcript differs"
        );
        let wrong_purpose = WindowsLifecycleBinding::new(
            [0x11; 32],
            [0x22; 32],
            [0x33; 32],
            WindowsLifecyclePurpose::KeeperToSuccessor,
        )
        .expect("distinct wrong-purpose context");
        assert_ne!(
            challenge,
            super::lifecycle_binding_record(
                *b"KELD-LC1",
                wrong_purpose,
                client_nonce,
                server_nonce,
                0x1122_3344,
                0x5566_7788,
            ),
            "role-direction mismatch differs"
        );
    }

    #[test]
    fn connected_pipe_reports_exact_opposite_process_pid() {
        const ENDPOINT_ENV: &str = "KELD_TEST_PEER_PID_ENDPOINT";
        if let Some(endpoint) = std::env::var_os(ENDPOINT_ENV) {
            let stream = WindowsNamedPipeBootstrapStream::connect(&endpoint.to_string_lossy())
                .expect("connect peer PID child");
            let server_pid = stream.peer_process_id().expect("read server process PID");
            println!("CLIENT_PEER_PID={server_pid}");
            return;
        }

        let endpoint = super::random_pipe_endpoint().expect("mint pipe endpoint");
        let server = WindowsNamedPipeServer::bind(&endpoint).expect("bind pipe server");
        let child = Command::new(std::env::current_exe().expect("current test binary"))
            .args([
                "--exact",
                "bootstrap::named_pipe_tests::connected_pipe_reports_exact_opposite_process_pid",
                "--nocapture",
            ])
            .env(ENDPOINT_ENV, &endpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn pipe client child");
        match server
            .accept_until(Some(Instant::now() + Duration::from_secs(2)))
            .expect("accept pipe client")
        {
            WaitOutcome::Ready => {}
            outcome => panic!("pipe client did not connect: {outcome:?}"),
        }
        let stream = WindowsNamedPipeBootstrapStream(server.stream().expect("server stream"));
        assert_eq!(
            stream.peer_process_id().expect("read client process PID"),
            child.id(),
            "server must report the exact connecting process"
        );
        server.consume();
        drop(stream);
        drop(server);
        let output = child.wait_with_output().expect("wait for pipe client");
        assert!(
            output.status.success(),
            "client failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!("CLIENT_PEER_PID={}", std::process::id())),
            "client must report the exact server PID, got {stdout:?}"
        );
    }

    fn frame(header: FrameHeader, payload: &[u8]) -> Vec<u8> {
        let mut bytes = header.encode().to_vec();
        bytes.extend_from_slice(payload);
        bytes
    }

    const BUN_NAMED_PIPE_ECHO: &str = r##"
import { createConnection } from "node:net";

const link = process.env.KELD_APP_LINK;
if (!link) throw new Error("missing KELD_APP_LINK");
const split = link.lastIndexOf("#");
if (split <= 0) throw new Error("invalid app link");
const endpoint = link.slice(0, split);
const token = Buffer.from(link.slice(split + 1), "hex");
if (token.length !== 32) throw new Error("invalid token length");

const socket = createConnection({ path: endpoint });
let buffered = Buffer.alloc(0);
const readers = [];
let terminalError = null;

function pump() {
  while (readers.length > 0 && buffered.length >= readers[0].length) {
    const { length, resolve } = readers.shift();
    const value = buffered.subarray(0, length);
    buffered = buffered.subarray(length);
    resolve(value);
  }
}

socket.on("data", (chunk) => {
  buffered = Buffer.concat([buffered, Buffer.from(chunk)]);
  pump();
});
socket.on("error", (error) => {
  terminalError = error;
  while (readers.length > 0) readers.shift().reject(error);
});
socket.on("close", () => {
  if (!terminalError) terminalError = new Error("pipe closed before reply");
  while (readers.length > 0) readers.shift().reject(terminalError);
});

function readExact(length) {
  if (terminalError) return Promise.reject(terminalError);
  return new Promise((resolve, reject) => {
    readers.push({ length, resolve, reject });
    pump();
  });
}

function makeFrame(kind, channel, corr, payload) {
  const header = Buffer.alloc(16);
  header.writeUInt16LE(0x494b, 0);
  header[2] = 2;
  header[3] = kind;
  header.writeUInt16LE(0, 4);
  header.writeUInt16LE(channel, 6);
  header.writeUInt32LE(corr, 8);
  header.writeUInt32LE(payload.length, 12);
  return Buffer.concat([header, payload]);
}

async function readFrame() {
  const header = await readExact(16);
  if (header.readUInt16LE(0) !== 0x494b || header[2] !== 2) {
    throw new Error("invalid kipc header");
  }
  return {
    kind: header[3],
    channel: header.readUInt16LE(6),
    corr: header.readUInt32LE(8),
    payload: await readExact(header.readUInt32LE(12)),
  };
}

await new Promise((resolve, reject) => {
  socket.once("connect", resolve);
  socket.once("error", reject);
});
socket.write(makeFrame(0, 0, 0, token));
const hello = await readFrame();
if (hello.kind !== 0 || hello.channel !== 0 || hello.corr !== 0 || !hello.payload.equals(token)) {
  throw new Error("HELLO mismatch");
}

const message = Buffer.from("bun-pipe", "utf8");
const echoPayload = Buffer.concat([Buffer.from([message.length]), message, Buffer.from([42])]);
socket.write(makeFrame(1, 1, 7, echoPayload));
const reply = await readFrame();
if (reply.kind !== 2 || reply.channel !== 1 || reply.corr !== 7 || !reply.payload.equals(echoPayload)) {
  throw new Error("echo reply mismatch");
}
console.log("KELD_BUN_PIPE_ECHO_OK");
socket.end();
"##;

    fn wait_child_output(mut child: Child, deadline: Instant) -> std::io::Result<Output> {
        loop {
            if child.try_wait()?.is_some() {
                return child.wait_with_output();
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Bun named-pipe fixture exceeded deadline",
                ));
            }
            thread::yield_now();
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "test helper reports the exact failed boundary at each assertion"
    )]
    fn run_rejection_then_authenticate(hostile_bytes: Option<&[u8]>, expected: BootstrapRejection) {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (notify_tx, notify_rx) = mpsc::channel();
        let observer = RecordingObserver {
            seen: Arc::clone(&seen),
            notify: Some(notify_tx),
        };
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener
                .accept_authenticated_until(Instant::now() + Duration::from_secs(3), &observer)
        });

        let mut hostile = client(&endpoint).expect("open hostile client");
        hostile
            .set_app_link_deadlines(Some(Duration::from_millis(250)))
            .expect("hostile deadlines");
        if let Some(bytes) = hostile_bytes {
            hostile.write_all(bytes).expect("write hostile input");
        } else {
            drop(hostile);
            assert_eq!(
                notify_rx
                    .recv_timeout(Duration::from_secs(1))
                    .expect("EOF record"),
                expected
            );
            let mut legitimate = client(&endpoint).expect("open legitimate client");
            legitimate
                .set_app_link_deadlines(Some(Duration::from_millis(500)))
                .expect("legitimate deadlines");
            handshake_client(&mut legitimate, &token).expect("legitimate HELLO");
            let outcome = worker.join().expect("join").expect("admission");
            assert!(matches!(
                outcome,
                WindowsNamedPipeBootstrapAdmission::Authenticated(_)
            ));
            assert_eq!(
                *seen.lock().unwrap_or_else(PoisonError::into_inner),
                vec![expected]
            );
            return;
        }
        assert_eq!(
            notify_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("rejection record"),
            expected
        );
        let mut reply = [0_u8; 1];
        assert_eq!(
            hostile
                .read(&mut reply)
                .expect("pre-auth rejection must produce EOF"),
            0,
            "pre-auth rejection must close without a host frame"
        );
        drop(hostile);

        let mut legitimate = client(&endpoint).expect("open legitimate client");
        legitimate
            .set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("legitimate deadlines");
        handshake_client(&mut legitimate, &token).expect("legitimate HELLO");
        let outcome = worker.join().expect("join").expect("admission");
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::Authenticated(_)
        ));
        assert_eq!(
            *seen.lock().unwrap_or_else(PoisonError::into_inner),
            vec![expected]
        );
    }

    #[test]
    fn pipe_name_dacl_and_non_inheritance_match_exact_contract() {
        let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind named pipe");
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse app link");
        let suffix = endpoint
            .strip_prefix(r"\\.\pipe\keld-")
            .expect("canonical named-pipe prefix");
        assert_eq!(suffix.len(), 64);
        assert!(
            suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "pipe nonce must be lowercase hex"
        );
        assert_ne!(
            suffix,
            token.to_hex(),
            "pipe namespace nonce must be minted independently from the HELLO token"
        );
        listener
            .inspect_pipe_handle(|handle| {
                let current_sid = current_process_sid()?;
                let descriptor = GetSecurityInfo(
                    handle,
                    SeObjectType::SE_KERNEL_OBJECT,
                    SecurityInformation::Dacl,
                )?;
                let sddl = descriptor.as_sddl()?;
                assert!(sddl.to_string_lossy().contains("D:P"));
                let dacl = descriptor
                    .dacl()
                    .ok_or_else(|| std::io::Error::other("test readback found no DACL"))?;
                assert_eq!(dacl.len(), 1);
                let ace = dacl
                    .get_ace(0)
                    .ok_or_else(|| std::io::Error::other("test readback found no ACE"))?;
                assert_eq!(ace.sid(), Some(&*current_sid));
                assert_eq!(ace.ace_type(), AceType::ACCESS_ALLOWED_ACE_TYPE);
                assert_eq!(ace.flags(), AceFlags::empty());
                assert_eq!(ace.mask().bits(), 0x0012_019B);
                assert_eq!(ace.mask().bits() & 0x4, 0);

                let mut handle_flags = 0;
                // SAFETY: the borrowed server handle is live for this closure
                // and `handle_flags` is a valid writable u32.
                assert_ne!(
                    unsafe { GetHandleInformation(handle.as_raw_handle(), &raw mut handle_flags) },
                    0
                );
                assert_eq!(handle_flags & HANDLE_FLAG_INHERIT, 0);

                let mut pipe_flags = 0;
                // SAFETY: the borrowed server handle is live, `pipe_flags` is
                // writable, and the remaining outputs are documented optional.
                assert_ne!(
                    unsafe {
                        GetNamedPipeInfo(
                            handle.as_raw_handle(),
                            &raw mut pipe_flags,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                        )
                    },
                    0
                );
                assert_ne!(pipe_flags & PIPE_REJECT_REMOTE_CLIENTS, 0);
                Ok(())
            })
            .expect("independent pipe security readback");

        let collision = WindowsNamedPipeServer::bind(endpoint)
            .expect_err("FILE_FLAG_FIRST_PIPE_INSTANCE must reject a second server");
        assert_eq!(collision.raw_os_error(), Some(5));
    }

    #[test]
    fn shipping_client_waits_for_busy_instance_then_connects() {
        let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind");
        let endpoint = listener.endpoint().to_owned();
        let server = listener
            .server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .cloned()
            .expect("live server");
        let first = WindowsNamedPipeServer::connect_client(&endpoint).expect("first client");
        assert!(matches!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(1)))
                .expect("accept first client"),
            WaitOutcome::Ready
        ));
        let busy = WindowsNamedPipeServer::connect_client(&endpoint)
            .expect_err("one-shot open must expose the busy-instance control");
        assert_eq!(busy.raw_os_error(), Some(231));

        let (busy_tx, busy_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let connector = thread::spawn(move || {
            WindowsNamedPipeServer::install_connect_busy_witness(busy_tx);
            let _ = result_tx.send(WindowsNamedPipeBootstrapStream::connect(&endpoint));
        });
        busy_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("shipping client must causally observe ERROR_PIPE_BUSY");

        server.disconnect_for_retry().expect("release first client");
        assert!(matches!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(1)))
                .expect("rearm server for shipping client"),
            WaitOutcome::Ready
        ));
        let second = result_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("shipping client must finish after rearm")
            .expect("shipping client must connect after rearm");
        connector.join().expect("join shipping connector");
        drop(first);
        drop(second);
        server.close_terminal().expect("close test server");
    }

    #[test]
    fn busy_client_deadline_is_timeout_and_past_deadline_never_connects() {
        let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind busy listener");
        let endpoint = listener.endpoint().to_owned();
        let server = listener
            .server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .cloned()
            .expect("live busy server");
        let first = WindowsNamedPipeServer::connect_client(&endpoint).expect("first client");
        assert!(matches!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(1)))
                .expect("accept first client"),
            WaitOutcome::Ready
        ));
        let started = Instant::now();
        let timeout = WindowsNamedPipeServer::connect_client_until(
            &endpoint,
            started + Duration::from_millis(40),
        )
        .expect_err("permanently busy instance must hit its deadline");
        assert_eq!(timeout.kind(), io::ErrorKind::TimedOut);
        assert_eq!(timeout.raw_os_error(), Some(121));
        assert!(
            started.elapsed() >= Duration::from_millis(30),
            "busy-instance wait returned materially before its 40 ms deadline"
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "busy-instance deadline exceeded its kill bound"
        );
        drop(first);
        server.close_terminal().expect("close busy server");

        let available = WindowsNamedPipeBootstrapListener::bind().expect("bind available listener");
        let past = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("representable past deadline");
        let timeout = WindowsNamedPipeServer::connect_client_until(available.endpoint(), past)
            .expect_err("past deadline must not open an available instance");
        assert_eq!(timeout.kind(), io::ErrorKind::TimedOut);
        let available_server = available
            .server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .cloned()
            .expect("live available server");
        assert!(matches!(
            available_server
                .accept_until(Some(Instant::now() + Duration::from_millis(40)))
                .expect("observe available server after past-deadline call"),
            WaitOutcome::DeadlineElapsed
        ));
        available.shutdown().expect("close available listener");
    }

    #[test]
    fn pending_accept_cancellation_completes_and_joins() {
        prove_pending_accept_cancellation(false).expect("pending cancellation");
    }

    #[test]
    fn pending_accept_cancellation_with_package_acl_completes_and_joins() {
        prove_pending_accept_cancellation(true).expect("package ACL pending cancellation");
    }

    fn prove_pending_accept_cancellation(package_acl: bool) -> io::Result<()> {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind()?);
        if package_acl {
            // Descriptor-only fixture: actual LPAC identity is proved in runtime tests.
            let original = listener.inspect_pipe_handle(|handle| {
                Ok(GetSecurityInfo(
                    handle,
                    SeObjectType::SE_KERNEL_OBJECT,
                    SecurityInformation::Dacl,
                )?
                .as_sddl()?
                .to_string_lossy()
                .into_owned())
            })?;
            let expected = format!("{original}(A;;0x12019b;;;S-1-15-2-1-2-3-4-5-6-7)");
            let descriptor: windows_permissions::LocalBox<windows_permissions::SecurityDescriptor> =
                expected.parse()?;
            windows_permissions::wrappers::SetNamedSecurityInfo(
                listener.endpoint(),
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
                None,
                None,
                Some(
                    descriptor
                        .dacl()
                        .ok_or_else(|| io::Error::other("missing fixture DACL"))?,
                ),
                None,
            )?;
            listener.inspect_pipe_handle(|handle| {
                let observed = GetSecurityInfo(
                    handle,
                    SeObjectType::SE_KERNEL_OBJECT,
                    SecurityInformation::Dacl,
                )?;
                // SetNamedSecurityInfo can add SE_DACL_AUTO_INHERITED. Compare
                // authority-bearing ACEs and require protection independently.
                assert!(observed.as_sddl()?.to_string_lossy().starts_with("D:P"));
                let actual = observed
                    .dacl()
                    .ok_or_else(|| io::Error::other("missing readback DACL"))?;
                let intended = descriptor
                    .dacl()
                    .ok_or_else(|| io::Error::other("missing fixture DACL"))?;
                assert_eq!(actual.len(), 2);
                for index in 0..2 {
                    let actual = actual
                        .get_ace(index)
                        .ok_or_else(|| io::Error::other("missing readback ACE"))?;
                    let intended = intended
                        .get_ace(index)
                        .ok_or_else(|| io::Error::other("missing fixture ACE"))?;
                    assert_eq!(actual.sid(), intended.sid());
                    assert_eq!(actual.ace_type(), intended.ace_type());
                    assert_eq!(actual.flags(), intended.flags());
                    assert_eq!(actual.mask(), intended.mask());
                }
                Ok(())
            })?;
        }
        let cancellation = listener.cancellation();
        let worker_listener = Arc::clone(&listener);
        let (completed, completion) = mpsc::channel();
        let admission_deadline = Instant::now() + Duration::from_secs(5);
        let worker = thread::spawn(move || {
            let outcome = worker_listener
                .accept_authenticated_until(admission_deadline, &super::NoopRejectionObserver);
            let _ = completed.send(());
            outcome
        });
        let pending_deadline = Instant::now() + Duration::from_secs(1);
        while !listener.is_accept_pending() && Instant::now() < pending_deadline {
            thread::yield_now();
        }
        // This witness is set only after ConnectNamedPipe reports ERROR_IO_PENDING.
        let was_pending = listener.is_accept_pending();
        let completion_deadline = (Instant::now() + Duration::from_secs(2)).min(admission_deadline);
        let cancelled = cancellation.cancel();
        let completed_promptly =
            completion.recv_timeout(completion_deadline.saturating_duration_since(Instant::now()));
        let completed_at = Instant::now();
        // Join even on an oracle failure; the independent admission deadline bounds cleanup.
        let outcome = worker
            .join()
            .map_err(|_| io::Error::other("accept worker panicked"))?;
        assert!(
            was_pending,
            "server never entered overlapped pending accept"
        );
        cancelled?;
        completed_promptly.map_err(|error| {
            io::Error::other(format!(
                "cancellation must finish before admission deadline: {error}"
            ))
        })?;
        assert!(
            completed_at < completion_deadline,
            "synchronous cancel exceeded completion deadline"
        );
        assert!(matches!(
            outcome?,
            WindowsNamedPipeBootstrapAdmission::Cancelled
        ));
        Ok(())
    }

    #[test]
    fn active_partial_hello_cancellation_completes_and_closes_locator() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let endpoint = listener.endpoint().to_owned();
        let cancellation = listener.cancellation();
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_authenticated_until(
                Instant::now() + Duration::from_secs(30),
                &super::NoopRejectionObserver,
            )
        });
        let mut partial = client(&endpoint).expect("open partial client");
        partial.write_all(b"K").expect("start partial HELLO");
        let connected_deadline = Instant::now() + Duration::from_secs(1);
        while !listener.is_connected() {
            assert!(
                Instant::now() < connected_deadline,
                "server never entered the connected handshake state"
            );
            thread::yield_now();
        }
        cancellation.cancel().expect("cancel active HELLO");
        let outcome = worker.join().expect("join").expect("admission");
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::Cancelled
        ));
        drop(partial);
        let stale = WindowsNamedPipeServer::connect_client(&endpoint)
            .expect_err("cancelled generation must close its pipe handle");
        assert_eq!(stale.raw_os_error(), Some(2));
    }

    #[test]
    fn foreign_hello_is_redacted_then_same_instance_authenticates() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let observer = RecordingObserver {
            seen: Arc::clone(&seen),
            notify: None,
        };
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener
                .accept_authenticated_until(Instant::now() + Duration::from_secs(3), &observer)
        });

        let mut foreign_bytes = *token.as_bytes();
        foreign_bytes[0] ^= 1;
        let foreign = SessionToken::from_bytes(foreign_bytes);
        let mut hostile = client(&endpoint).expect("open hostile client");
        hostile
            .set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("hostile deadlines");
        let error = handshake_client(&mut hostile, &foreign).expect_err("foreign HELLO denied");
        assert!(matches!(
            error,
            crate::IpcError::Io(_) | crate::IpcError::Timeout
        ));
        drop(hostile);

        let mut legitimate = client(&endpoint).expect("open legitimate client");
        legitimate
            .set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("legitimate deadlines");
        handshake_client(&mut legitimate, &token).expect("matching HELLO accepted");
        let outcome = worker.join().expect("join").expect("admission");
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::Authenticated(_)
        ));
        let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(*seen, vec![BootstrapRejection::HelloAuth]);
        assert_eq!(seen[0].code(), "KELD-IPC-007");
    }

    #[test]
    fn rejected_peer_then_bun_node_net_child_completes_hello_and_echo() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (notify_tx, notify_rx) = mpsc::channel();
        let observer = RecordingObserver {
            seen: Arc::clone(&seen),
            notify: Some(notify_tx),
        };
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || -> Result<(), String> {
            let outcome = worker_listener
                .accept_authenticated_until(Instant::now() + Duration::from_secs(10), &observer)
                .map_err(|error| error.to_string())?;
            let WindowsNamedPipeBootstrapAdmission::Authenticated(mut stream) = outcome else {
                return Err("Bun child did not authenticate before terminal admission".to_owned());
            };
            match serve_echo_requests(&mut stream) {
                Ok(()) => Ok(()),
                Err(crate::IpcError::Io(error)) if error.raw_os_error() == Some(109) => Ok(()),
                Err(error) => Err(error.to_string()),
            }
        });

        let mut foreign_bytes = *token.as_bytes();
        foreign_bytes[0] ^= 1;
        let mut hostile = client(&endpoint).expect("open hostile client");
        hostile
            .set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("hostile deadlines");
        let _ = handshake_client(&mut hostile, &SessionToken::from_bytes(foreign_bytes))
            .expect_err("foreign HELLO denied");
        drop(hostile);
        assert_eq!(
            notify_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("HELLO rejection record"),
            BootstrapRejection::HelloAuth
        );
        let pending_deadline = Instant::now() + Duration::from_secs(1);
        while !listener.is_accept_pending() {
            assert!(
                Instant::now() < pending_deadline,
                "server did not re-enter named-pipe accept after rejection"
            );
            thread::yield_now();
        }

        let child = Command::new("bun")
            .args(["-e", BUN_NAMED_PIPE_ECHO])
            .env("KELD_APP_LINK", &link)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn Bun named-pipe fixture");
        let output = wait_child_output(child, Instant::now() + Duration::from_secs(5))
            .expect("bounded Bun fixture");
        assert!(
            output.status.success(),
            "Bun named-pipe fixture failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "KELD_BUN_PIPE_ECHO_OK"
        );
        worker
            .join()
            .expect("join Bun echo server")
            .expect("serve Bun echo");
        assert_eq!(
            *seen.lock().unwrap_or_else(PoisonError::into_inner),
            vec![BootstrapRejection::HelloAuth]
        );
    }

    #[test]
    fn partial_hello_timeout_reaccepts_same_pipe_then_authenticates() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (notify_tx, notify_rx) = mpsc::channel();
        let observer = RecordingObserver {
            seen: Arc::clone(&seen),
            notify: Some(notify_tx),
        };
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_loop(
                Some(Instant::now() + Duration::from_secs(3)),
                Duration::from_millis(100),
                &observer,
            )
        });

        let mut silent_partial = client(&endpoint).expect("open partial client");
        silent_partial.write_all(b"K").expect("start partial frame");
        assert_eq!(
            notify_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("timeout rejection"),
            BootstrapRejection::Timeout
        );
        drop(silent_partial);

        let mut legitimate = client(&endpoint).expect("open legitimate client");
        legitimate
            .set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("legitimate deadlines");
        handshake_client(&mut legitimate, &token).expect("same pipe reaccepted");
        let outcome = worker.join().expect("join").expect("admission");
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::Authenticated(_)
        ));
        assert_eq!(
            *seen.lock().unwrap_or_else(PoisonError::into_inner),
            vec![BootstrapRejection::Timeout]
        );
    }

    #[test]
    fn generation_deadline_is_terminal_without_a_client() {
        let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind");
        let outcome = listener
            .accept_authenticated_until(
                Instant::now() + Duration::from_millis(30),
                &super::NoopRejectionObserver,
            )
            .expect("deadline admission");
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::DeadlineElapsed
        ));
        let error = WindowsNamedPipeServer::connect_client(listener.endpoint())
            .expect_err("terminal generation must not accept a late connector");
        assert!(matches!(error.raw_os_error(), Some(2 | 231)));
    }

    #[test]
    fn already_expired_generation_never_enters_pipe_accept() {
        let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind");
        let deadline = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("representable expired deadline");
        let outcome = listener
            .accept_authenticated_until(deadline, &super::NoopRejectionObserver)
            .expect("expired admission");
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::DeadlineElapsed
        ));
        let stale = WindowsNamedPipeServer::connect_client(listener.endpoint())
            .expect_err("expired generation must close before accepting");
        assert_eq!(stale.raw_os_error(), Some(2));
    }

    #[test]
    fn generation_expiry_during_rejection_blocks_reaccept() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let endpoint = listener.endpoint().to_owned();
        let cancellation = listener.cancellation();
        let deadline = Instant::now() + Duration::from_millis(100);
        let (observed_tx, observed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let observer = BlockingObserver {
            observed: observed_tx,
            release: Mutex::new(release_rx),
        };
        let worker_listener = Arc::clone(&listener);
        let (result_tx, result_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let _ = result_tx.send(worker_listener.accept_authenticated_until(deadline, &observer));
        });

        let mut hostile = client(&endpoint).expect("open hostile client");
        hostile.write_all(&[0_u8; 16]).expect("bad header");
        assert_eq!(
            observed_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("header rejection"),
            BootstrapRejection::Header
        );
        while Instant::now() < deadline {
            thread::yield_now();
        }
        release_tx.send(()).expect("release observer after expiry");

        let outcome = match result_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(result) => result.expect("admission result"),
            Err(error) => {
                cancellation.cancel().expect("cancel wedged reaccept");
                worker.join().expect("join cancelled reaccept");
                panic!("expired rejection path re-entered an unbounded accept: {error}");
            }
        };
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::DeadlineElapsed
        ));
        drop(hostile);
        worker.join().expect("join expired rejection worker");
    }

    #[test]
    fn generation_expiry_at_final_auth_boundary_is_not_consumed() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        listener.install_before_consume_gate(TestConsumeGate {
            entered: entered_tx,
            release: release_rx,
        });
        let deadline = Instant::now() + Duration::from_millis(100);
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_authenticated_until(deadline, &super::NoopRejectionObserver)
        });
        let mut role = client(&endpoint).expect("open role");
        role.set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("role deadlines");
        handshake_client(&mut role, &token).expect("HELLO reaches final boundary");
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("final authentication boundary");
        while Instant::now() < deadline {
            thread::yield_now();
        }
        release_tx.send(()).expect("release final boundary");
        let outcome = worker.join().expect("join admission").expect("admission");
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::DeadlineElapsed
        ));
        let stale = WindowsNamedPipeServer::connect_client(&endpoint)
            .expect_err("expired final authentication must not consume a session");
        assert_eq!(stale.raw_os_error(), Some(2));
        drop(role);
    }

    #[test]
    fn every_pre_auth_failure_uses_shared_redacted_taxonomy_and_reaccepts() {
        run_rejection_then_authenticate(None, BootstrapRejection::Io);

        let bad_header = [0_u8; 16];
        run_rejection_then_authenticate(Some(&bad_header), BootstrapRejection::Header);

        let oversized = frame(
            FrameHeader {
                kind: FrameKind::Hello,
                flags: 0,
                channel: ChannelId(0),
                corr: CorrelationId(0),
                len: u32::try_from(MAX_FRAME_LEN + 1).expect("test length fits u32"),
            },
            &[],
        );
        run_rejection_then_authenticate(Some(&oversized), BootstrapRejection::PayloadTooLarge);

        let non_hello = frame(
            FrameHeader {
                kind: FrameKind::Call,
                flags: 0,
                channel: ChannelId(0),
                corr: CorrelationId(0),
                len: 0,
            },
            &[],
        );
        run_rejection_then_authenticate(Some(&non_hello), BootstrapRejection::Protocol);

        let empty_hello = frame(
            FrameHeader {
                kind: FrameKind::Hello,
                flags: 0,
                channel: ChannelId(0),
                corr: CorrelationId(0),
                len: 0,
            },
            &[],
        );
        run_rejection_then_authenticate(Some(&empty_hello), BootstrapRejection::Protocol);

        let short_hello = frame(
            FrameHeader {
                kind: FrameKind::Hello,
                flags: 0,
                channel: ChannelId(0),
                corr: CorrelationId(0),
                len: 31,
            },
            &[0xA5; 31],
        );
        run_rejection_then_authenticate(Some(&short_hello), BootstrapRejection::Protocol);

        // kel133 AC4 split: the same foreign bytes in an exactly shaped HELLO
        // are the one remaining HelloAuth class — shape failures above must
        // never collapse into it, and this row must never collapse into 005.
        let foreign_hello = frame(
            FrameHeader {
                kind: FrameKind::Hello,
                flags: 0,
                channel: ChannelId(0),
                corr: CorrelationId(0),
                len: 32,
            },
            &[0xA5; 32],
        );
        run_rejection_then_authenticate(Some(&foreign_hello), BootstrapRejection::HelloAuth);

        let reserved_hello = frame(
            FrameHeader {
                kind: FrameKind::Hello,
                flags: 0,
                channel: ChannelId(1),
                corr: CorrelationId(0),
                len: 32,
            },
            &[0xA5; 32],
        );
        run_rejection_then_authenticate(Some(&reserved_hello), BootstrapRejection::Protocol);
    }

    #[test]
    fn authenticated_session_drop_removes_locator_and_successor_is_fresh() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let first_link = listener.app_link();
        let stale_cancellation = listener.cancellation();
        let (endpoint, token) = parse_app_link(&first_link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_authenticated_until(
                Instant::now() + Duration::from_secs(2),
                &super::NoopRejectionObserver,
            )
        });
        let mut role = client(&endpoint).expect("open role");
        role.set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("deadlines");
        handshake_client(&mut role, &token).expect("authenticate");
        let outcome = worker.join().expect("join").expect("admission");
        let WindowsNamedPipeBootstrapAdmission::Authenticated(mut server_stream) = outcome else {
            panic!("expected authenticated named-pipe session")
        };
        stale_cancellation
            .cancel()
            .expect("consumed bootstrap cancellation is inert");
        role.write_all(b"X").expect("session write after bootstrap");
        let mut received = [0_u8; 1];
        server_stream
            .read_exact(&mut received)
            .expect("session remains live after stale cancellation");
        assert_eq!(received, *b"X");
        drop(role);
        drop(server_stream);
        let stale = WindowsNamedPipeServer::connect_client(&endpoint)
            .expect_err("dropping the session must remove its locator");
        assert_eq!(stale.raw_os_error(), Some(2));

        let successor = WindowsNamedPipeBootstrapListener::bind().expect("bind successor");
        assert_ne!(successor.app_link(), first_link);
        assert_ne!(successor.endpoint(), endpoint);
    }

    #[test]
    fn overlapped_write_to_non_reading_peer_hits_configured_deadline() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_authenticated_until(
                Instant::now() + Duration::from_secs(2),
                &super::NoopRejectionObserver,
            )
        });
        let mut role = client(&endpoint).expect("open role");
        role.set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("role deadlines");
        handshake_client(&mut role, &token).expect("authenticate");
        let outcome = worker.join().expect("join").expect("admission");
        let WindowsNamedPipeBootstrapAdmission::Authenticated(mut server_stream) = outcome else {
            panic!("expected authenticated named-pipe session")
        };
        server_stream
            .set_app_link_write_deadline(Some(Duration::from_millis(30)))
            .expect("short write deadline");
        let payload = vec![0_u8; 1024 * 1024];
        let error = server_stream
            .write_all(&payload)
            .expect_err("non-reading peer must not wedge an overlapped write");
        assert_eq!(error.raw_os_error(), Some(121));
    }

    #[test]
    fn stream_shutdown_cancels_pending_read_and_joins_without_peer_input() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_authenticated_until(
                Instant::now() + Duration::from_secs(2),
                &super::NoopRejectionObserver,
            )
        });
        let mut role = client(&endpoint).expect("open role");
        role.set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("role deadlines");
        handshake_client(&mut role, &token).expect("authenticate");
        let outcome = worker.join().expect("join admission").expect("admission");
        let WindowsNamedPipeBootstrapAdmission::Authenticated(server_stream) = outcome else {
            panic!("expected authenticated named-pipe session")
        };
        let mut blocked_reader = server_stream.try_clone().expect("clone server stream");
        blocked_reader
            .set_app_link_read_deadline(Some(Duration::from_secs(5)))
            .expect("reader deadline");
        let (result_tx, result_rx) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut byte = [0_u8; 1];
            let _ = result_tx.send(blocked_reader.read_exact(&mut byte));
        });
        let active_deadline = Instant::now() + Duration::from_secs(1);
        while !server_stream.0.has_active_io() {
            assert!(
                Instant::now() < active_deadline,
                "reader never entered overlapped I/O"
            );
            thread::yield_now();
        }
        server_stream
            .shutdown_app_link()
            .expect("shutdown connected pipe");
        let error = result_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("shutdown must wake local read")
            .expect_err("pending read must not succeed without peer bytes");
        assert!(
            error.kind() == std::io::ErrorKind::UnexpectedEof
                || matches!(error.raw_os_error(), Some(995 | 109 | 233))
        );
        reader.join().expect("join blocked reader");
        let mut peer_byte = [0_u8; 1];
        assert_eq!(
            role.read(&mut peer_byte)
                .expect("shutdown must produce peer EOF"),
            0
        );
    }

    #[test]
    fn stream_shutdown_disconnects_peer_even_when_cancellation_reports_error() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("parse link");
        let endpoint = endpoint.to_owned();
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_authenticated_until(
                Instant::now() + Duration::from_secs(2),
                &super::NoopRejectionObserver,
            )
        });
        let mut role = client(&endpoint).expect("open role");
        role.set_app_link_deadlines(Some(Duration::from_millis(500)))
            .expect("role deadlines");
        handshake_client(&mut role, &token).expect("authenticate");
        let outcome = worker.join().expect("join admission").expect("admission");
        let WindowsNamedPipeBootstrapAdmission::Authenticated(server_stream) = outcome else {
            panic!("expected authenticated named-pipe session")
        };
        server_stream.0.force_cancel_error();
        let shutdown_error = server_stream
            .shutdown_app_link()
            .expect_err("injected cancellation failure must be reported");
        assert_eq!(shutdown_error.raw_os_error(), Some(5));

        let mut peer_byte = [0_u8; 1];
        assert_eq!(
            role.read(&mut peer_byte)
                .expect("disconnect must still close the peer-facing connection"),
            0
        );
    }

    #[test]
    fn cancellation_handle_does_not_keep_dropped_listener_alive() {
        let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind");
        let endpoint = listener.endpoint().to_owned();
        let cancellation = listener.cancellation();
        drop(listener);
        cancellation
            .cancel()
            .expect("cancel after owner drop is inert");
        let stale = WindowsNamedPipeServer::connect_client(&endpoint)
            .expect_err("non-owning cancellation view must not preserve pipe handle");
        assert_eq!(stale.raw_os_error(), Some(2));
    }

    #[expect(
        clippy::expect_used,
        reason = "test cycle helper reports the exact failed handle-lifecycle boundary"
    )]
    fn run_cancelled_accept_cycle() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind cycle"));
        let cancellation = listener.cancellation();
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_authenticated_until(
                Instant::now() + Duration::from_secs(2),
                &super::NoopRejectionObserver,
            )
        });
        let pending_deadline = Instant::now() + Duration::from_secs(1);
        while !listener.is_accept_pending() {
            assert!(
                Instant::now() < pending_deadline,
                "cycle accept never became pending"
            );
            thread::yield_now();
        }
        cancellation.cancel().expect("cancel cycle");
        assert!(matches!(
            worker.join().expect("join cycle").expect("cycle result"),
            WindowsNamedPipeBootstrapAdmission::Cancelled
        ));
    }

    #[test]
    fn repeated_cancellation_returns_process_handle_count_to_baseline() {
        const CHILD_ENV: &str = "KELD_TEST_PIPE_HANDLE_CENSUS_CHILD";
        if std::env::var_os(CHILD_ENV).is_none() {
            let output = Command::new(std::env::current_exe().expect("current test binary"))
                .args([
                    "--exact",
                    "bootstrap::named_pipe_tests::repeated_cancellation_returns_process_handle_count_to_baseline",
                    "--nocapture",
                ])
                .env(CHILD_ENV, "1")
                .output()
                .expect("run isolated handle census");
            assert!(
                output.status.success(),
                "isolated handle census failed: status={:?}, stdout={}, stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains("test result: ok. 1 passed; 0 failed;"),
                "isolated census must execute exactly one test: {stdout}"
            );
            return;
        }

        run_cancelled_accept_cycle();
        let baseline = process_handle_count().expect("baseline handle count");
        for _ in 0..32 {
            run_cancelled_accept_cycle();
        }
        let final_count = process_handle_count().expect("final handle count");
        assert_eq!(
            final_count, baseline,
            "pipe, cancel-event, or per-accept event handle leaked across cycles"
        );
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::parse_app_link;

    use crate::windows_named_pipe::WindowsNamedPipeServer;

    use super::{BootstrapAdmission, BootstrapListener, WindowsNamedPipeBootstrapStream};

    #[test]
    fn shipping_shutdown_waits_for_active_handshake_cancellation_observation() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind Windows bootstrap"));
        let link = listener.app_link();
        let (endpoint, _) = parse_app_link(&link).expect("Windows app link");
        let endpoint = endpoint.to_owned();
        let acceptor = Arc::clone(&listener);
        let (result_tx, result_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = acceptor.accept_authenticated_until(
                Instant::now() + Duration::from_secs(30),
                &super::NoopRejectionObserver,
            );
            let _ = result_tx.send(result);
        });
        let silent = WindowsNamedPipeBootstrapStream(
            WindowsNamedPipeServer::connect_client(&endpoint).expect("connect silent peer"),
        );

        let active_deadline = Instant::now() + Duration::from_secs(2);
        while !listener.listener.is_connected() {
            assert!(
                Instant::now() < active_deadline,
                "silent peer never became the active handshake"
            );
            thread::yield_now();
        }
        let cancel_started = Instant::now();
        listener.shutdown().expect("shutdown active handshake");
        let result = result_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("cancellation must beat the five-second peer deadline")
            .expect("listener cancellation result");
        assert!(matches!(result, BootstrapAdmission::Cancelled));
        assert!(
            cancel_started.elapsed() < Duration::from_secs(1),
            "active Windows handshake cancellation exceeded one second"
        );
        worker.join().expect("admission worker");
        drop(silent);
    }

    #[test]
    fn shipping_windows_listener_closes_pipe_locator_without_tcp_fallback() {
        let listener = BootstrapListener::bind().expect("bind Windows bootstrap");
        let link = listener.app_link();
        let (endpoint, _) = parse_app_link(&link).expect("Windows app link");
        assert!(endpoint.starts_with(r"\\.\pipe\keld-"));
        assert!(endpoint.parse::<u16>().is_err(), "new host minted TCP port");

        listener.shutdown().expect("close named-pipe locator");
        let error = WindowsNamedPipeServer::connect_client(endpoint)
            .expect_err("closed shipping pipe must reject reconnect");
        assert!(matches!(error.raw_os_error(), Some(2 | 231)));
    }
}

#[cfg(unix)]
fn unique_session_dir() -> io::Result<PathBuf> {
    // `sockaddr_un.sun_path` is 104 bytes on macOS (108 on Linux). Keep the
    // generated component short, and fall back to short sticky temp roots if
    // the process temp dir itself is too long.
    let (nonce_secs, nonce_nanos) = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or((0, 0), |duration| {
            (duration.as_secs(), duration.subsec_nanos())
        });
    let bases = [
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/var/tmp"),
    ];
    for base in bases {
        for _ in 0..128 {
            let counter = UNIQUE_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let session_dir = base.join(format!(
                "kb-{:x}-{nonce_secs:x}{nonce_nanos:x}-{counter:x}",
                std::process::id(),
            ));
            if session_dir
                .join("app.sock")
                .as_os_str()
                .as_encoded_bytes()
                .len()
                >= 100
            {
                continue;
            }
            match fs::DirBuilder::new().mode(0o700).create(&session_dir) {
                Ok(()) => {
                    if let Err(error) =
                        fs::set_permissions(&session_dir, fs::Permissions::from_mode(0o700))
                    {
                        let _ = fs::remove_dir(&session_dir);
                        return Err(error);
                    }
                    return Ok(session_dir);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "could not allocate a short unique bootstrap session directory",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::ErrorKind;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::sync::{Arc, PoisonError};
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::link::{AppLinkDeadlines, handshake_client};
    use crate::token::{SessionToken, parse_app_link};

    use super::{
        BootstrapAdmission, BootstrapListener, BootstrapRejection, BootstrapRejectionObserver,
    };

    struct RecordingObserver {
        seen: Arc<std::sync::Mutex<Vec<BootstrapRejection>>>,
    }

    impl BootstrapRejectionObserver for RecordingObserver {
        fn rejected(&self, rejection: BootstrapRejection) {
            self.seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(rejection);
        }
    }

    #[test]
    fn listener_ignores_foreign_hello_then_accepts_legitimate_role() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("link");
        let mut foreign = *token.as_bytes();
        foreign[0] ^= 1;
        let foreign = SessionToken::from_bytes(foreign);

        let acceptor = Arc::clone(&listener);
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let stream = acceptor
                .accept_authenticated()
                .expect("listener I/O")
                .expect("must not be stopped");
            accepted_tx.send(()).expect("notify accepted");
            drop(stream);
        });

        let mut hostile = UnixStream::connect(endpoint).expect("hostile connect");
        hostile
            .set_app_link_deadlines(Some(Duration::from_millis(250)))
            .expect("deadline");
        let error = handshake_client(&mut hostile, &foreign).expect_err("foreign token denied");
        assert!(
            error.to_string().contains("KELD-IPC-007") || matches!(error, crate::IpcError::Io(_))
        );

        let mut role = UnixStream::connect(endpoint).expect("legitimate connect");
        handshake_client(&mut role, &token).expect("legitimate token accepted");
        accepted_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("foreign client must not consume listener");
        drop(role);
        server.join().expect("server join");
    }

    #[test]
    fn accept_until_deadline_elapses_without_a_client() {
        let listener = BootstrapListener::bind().expect("bind");
        let link = listener.app_link();
        let (endpoint, _) = parse_app_link(&link).expect("link");
        let observer = RecordingObserver {
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        };
        let result = listener
            .accept_authenticated_until(Instant::now() + Duration::from_millis(30), &observer)
            .expect("listener I/O");
        assert!(matches!(result, BootstrapAdmission::DeadlineElapsed));
        let error = UnixStream::connect(endpoint).expect_err("deadline must close locator");
        assert!(
            matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ),
            "deadline must close stale locator through the OS, got {error}"
        );
    }

    #[test]
    fn generation_deadline_is_not_renewed_by_silent_peers() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, _) = parse_app_link(&link).expect("link");
        let observer = RecordingObserver {
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        };
        let acceptor = Arc::clone(&listener);
        let (result_tx, result_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            result_tx
                .send(
                    acceptor
                        .accept_authenticated_until(
                            Instant::now() + Duration::from_millis(80),
                            &observer,
                        )
                        .expect("listener I/O"),
                )
                .expect("send result");
        });

        let silent = UnixStream::connect(endpoint).expect("silent connect");
        let result = result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("generation deadline must elapse despite connected peer");
        assert!(matches!(result, BootstrapAdmission::DeadlineElapsed));
        drop(silent);
        server.join().expect("server join");
    }

    #[test]
    fn cancellation_interrupts_active_silent_handshake() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind"));
        let cancellation = listener.cancellation();
        let link = listener.app_link();
        let (endpoint, _) = parse_app_link(&link).expect("link");
        let observer = RecordingObserver {
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        };
        let acceptor = Arc::clone(&listener);
        let (result_tx, result_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let result = acceptor
                .accept_authenticated_until(Instant::now() + Duration::from_secs(30), &observer);
            result_tx.send(result).expect("send result");
        });

        let _silent = UnixStream::connect(endpoint).expect("silent connect");
        cancellation.cancel().expect("cancel");
        let result = result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("active handshake must be cancelled")
            .expect("listener I/O");
        assert!(matches!(result, BootstrapAdmission::Cancelled));
        server.join().expect("server join");
    }

    #[test]
    fn observer_reports_redacted_hello_auth_rejection() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("link");
        let mut foreign = *token.as_bytes();
        foreign[0] ^= 1;
        let foreign = SessionToken::from_bytes(foreign);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observer = RecordingObserver {
            seen: Arc::clone(&seen),
        };

        let acceptor = Arc::clone(&listener);
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let result = acceptor
                .accept_authenticated_until(Instant::now() + Duration::from_secs(2), &observer)
                .expect("listener I/O");
            accepted_tx.send(result).expect("notify accepted");
        });

        let mut hostile = UnixStream::connect(endpoint).expect("hostile connect");
        hostile
            .set_app_link_deadlines(Some(Duration::from_millis(250)))
            .expect("deadline");
        let error = handshake_client(&mut hostile, &foreign).expect_err("foreign token denied");
        assert!(
            error.to_string().contains("KELD-IPC-007") || matches!(error, crate::IpcError::Io(_))
        );

        let mut role = UnixStream::connect(endpoint).expect("legitimate connect");
        handshake_client(&mut role, &token).expect("legitimate token accepted");
        let result = accepted_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("legitimate client must still bind");
        assert!(matches!(result, BootstrapAdmission::Authenticated(_)));
        drop(role);
        server.join().expect("server join");

        let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(*seen, vec![BootstrapRejection::HelloAuth]);
        assert_eq!(seen[0].code(), "KELD-IPC-007");
    }

    /// A peer that connects and closes without sending anything must not be
    /// able to take down admission.
    ///
    /// On macOS `set_app_link_deadlines` returns `EINVAL` on an accepted socket
    /// whose peer has already closed. That error used to propagate out of
    /// `accept_loop` with `?` as fatal listener I/O, killing the worker; the
    /// next legitimate client then blocked forever in `recvfrom` waiting for a
    /// HELLO from a dead thread. A port scan, a health check or a racing
    /// restart was enough -- no malformed bytes required.
    ///
    /// FALSIFICATION IS PLATFORM-SPECIFIC, and that is a real limit of this
    /// test: reverting the fix fails it on macOS in ~30s and it still passes on
    /// Linux, because Linux accepts the same setsockopt. The defect only exists
    /// where the OS refuses the call.
    #[test]
    fn a_peer_that_connects_and_closes_does_not_kill_admission() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("link");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observer = RecordingObserver {
            seen: Arc::clone(&seen),
        };

        let acceptor = Arc::clone(&listener);
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let result = acceptor
                .accept_authenticated_until(Instant::now() + Duration::from_secs(2), &observer)
                .expect("a closed peer must not surface as listener I/O");
            accepted_tx.send(result).expect("notify accepted");
        });

        // No bytes at all: connect, then close. This is the whole trigger.
        drop(UnixStream::connect(endpoint).expect("transient connect"));

        let mut role = UnixStream::connect(endpoint).expect("legitimate connect");
        handshake_client(&mut role, &token).expect("legitimate token accepted");
        let result = accepted_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("a closed peer must not consume the bootstrap opportunity");
        assert!(matches!(result, BootstrapAdmission::Authenticated(_)));
        drop(role);
        server.join().expect("server join");

        let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(
            !seen.contains(&BootstrapRejection::HelloAuth),
            "a peer that sent no token must not be recorded as token failure, got {seen:?}"
        );
    }

    #[test]
    fn authenticated_bind_unlinks_stale_locator() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("link");
        let observer = RecordingObserver {
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        };

        let acceptor = Arc::clone(&listener);
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let result = acceptor
                .accept_authenticated_until(Instant::now() + Duration::from_secs(2), &observer)
                .expect("listener I/O");
            accepted_tx.send(result).expect("notify accepted");
        });

        let mut role = UnixStream::connect(endpoint).expect("legitimate connect");
        handshake_client(&mut role, &token).expect("legitimate token accepted");
        let result = accepted_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("accepted");
        assert!(matches!(result, BootstrapAdmission::Authenticated(_)));

        let error = UnixStream::connect(endpoint).expect_err("stale locator must be unlinked");
        assert!(
            matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ),
            "stale locator must fail through the OS, got {error}"
        );
        drop(role);
        server.join().expect("server join");
    }

    #[test]
    fn shutdown_unblocks_accept_and_removes_owner_only_directory() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind"));
        let path = listener.path().to_path_buf();
        let directory = path.parent().expect("parent").to_path_buf();
        assert_eq!(
            std::fs::metadata(&directory)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );

        let acceptor = Arc::clone(&listener);
        let (result_tx, result_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            result_tx
                .send(acceptor.accept_authenticated())
                .expect("notify result");
        });
        listener.shutdown().expect("shutdown");
        assert!(
            result_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("accept must unblock")
                .expect("listener I/O")
                .is_none(),
            "shutdown wake connection must not authenticate"
        );
        server.join().expect("server join");
        drop(listener);
        assert!(
            !directory.exists(),
            "drop must remove owner-only bootstrap directory: {}",
            directory.display()
        );
    }

    #[test]
    fn silent_client_times_out_without_consuming_legitimate_bootstrap() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind"));
        let link = listener.app_link();
        let (endpoint, token) = parse_app_link(&link).expect("link");

        let acceptor = Arc::clone(&listener);
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let observer = RecordingObserver {
                seen: Arc::new(std::sync::Mutex::new(Vec::new())),
            };
            let stream = match acceptor
                .accept_loop(None, Duration::from_millis(100), &observer)
                .expect("listener I/O")
            {
                BootstrapAdmission::Authenticated(stream) => stream,
                other => panic!("must authenticate after silent timeout, got {other:?}"),
            };
            accepted_tx.send(()).expect("notify accepted");
            drop(stream);
        });

        let silent = UnixStream::connect(endpoint).expect("silent connect");
        let mut legitimate = UnixStream::connect(endpoint).expect("legitimate connect");
        handshake_client(&mut legitimate, &token)
            .expect("deadline must advance to legitimate client");
        accepted_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("silent client must not consume bootstrap listener");
        drop(silent);
        drop(legitimate);
        server.join().expect("server join");
    }

    #[test]
    fn stale_locator_fails_after_listener_drop() {
        let listener = BootstrapListener::bind().expect("bind");
        let link = listener.app_link();
        let (endpoint, _) = parse_app_link(&link).expect("link");
        drop(listener);
        let error = UnixStream::connect(endpoint).expect_err("stale endpoint must be removed");
        assert!(
            matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ),
            "stale locator must fail through the OS, got {error}"
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
mod ac9_macos_tests;
