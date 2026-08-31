//! Host-owned authenticated bootstrap listeners.
//!
//! This cold-path primitive owns a platform listener, a fresh `HELLO`
//! possession token, and cleanup. Unix uses an owner-only socket directory.
//! The shipping Windows constructor retains the explicitly unprivileged
//! loopback interim until KEL-101/T3 migrates consumers; KEL-101/T2 adds the
//! opt-in owner-DACL named-pipe bootstrap beside it. Both deliberately accept
//! another client after an invalid handshake so an
//! untrusted connector cannot consume the legitimate role's bootstrap.

#[cfg(windows)]
use core::fmt::Write as _;
#[cfg(unix)]
use std::fs;
use std::io;
#[cfg(windows)]
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
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
use crate::link::{AppLinkDeadlines, handshake_server_interruptible_until};
use crate::token::{SessionToken, format_app_link};
#[cfg(windows)]
use crate::windows_named_pipe::{
    WaitOutcome, WindowsNamedPipeCanceller, WindowsNamedPipeServer, WindowsNamedPipeStream,
};

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
#[cfg(unix)]
static UNIQUE_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Platform-selected connected stream returned after bootstrap authentication.
#[cfg(unix)]
pub type BootstrapStream = UnixStream;

/// Platform-selected connected stream returned after bootstrap authentication.
#[cfg(windows)]
pub type BootstrapStream = TcpStream;

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
    listener: Mutex<Option<TcpListener>>,
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(unix)]
    session_dir: PathBuf,
    #[cfg(windows)]
    port: u16,
    token: SessionToken,
    stopping: Arc<AtomicBool>,
    listening: Arc<AtomicBool>,
    active_stream: Arc<Mutex<Option<BootstrapStream>>>,
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
mod admission_type_tests {
    use super::BootstrapAdmission;

    #[test]
    fn terminal_variant_keeps_original_unconstrained_construction() {
        let admission = BootstrapAdmission::Cancelled;
        assert!(matches!(admission, BootstrapAdmission::Cancelled));
    }
}

struct NoopRejectionObserver;

impl BootstrapRejectionObserver for NoopRejectionObserver {
    fn rejected(&self, _rejection: BootstrapRejection) {}
}

/// Cancellation handle for a blocked bootstrap admission worker.
#[derive(Debug, Clone)]
pub struct BootstrapCancellation {
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(windows)]
    port: u16,
    stopping: Arc<AtomicBool>,
    listening: Arc<AtomicBool>,
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
            })
        }
        #[cfg(windows)]
        {
            let listener = TcpListener::bind(("127.0.0.1", 0))?;
            let port = listener.local_addr()?.port();
            listener.set_nonblocking(true)?;
            Ok(Self {
                listener: Mutex::new(Some(listener)),
                port,
                token,
                stopping: Arc::new(AtomicBool::new(false)),
                listening: Arc::new(AtomicBool::new(true)),
                active_stream: Arc::new(Mutex::new(None)),
            })
        }
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
            format_app_link(&self.port.to_string(), &self.token)
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
            port: self.port,
            stopping: Arc::clone(&self.stopping),
            listening: Arc::clone(&self.listening),
            active_stream: Arc::clone(&self.active_stream),
        }
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
        match self.accept_loop(None, APP_LINK_IO_DEADLINE, &observer)? {
            BootstrapAdmission::Authenticated(stream) => Ok(Some(stream)),
            BootstrapAdmission::Cancelled | BootstrapAdmission::DeadlineElapsed => Ok(None),
        }
    }

    /// Waits until one client authenticates, this listener is cancelled, or
    /// `deadline` elapses for this whole bootstrap generation.
    ///
    /// Peer authentication failures are treated as untrusted input: the peer
    /// is closed, a redacted host-only record may be emitted through
    /// `observer`, and the listener keeps admitting the legitimate role until
    /// the generation-level deadline or cancellation wins.
    ///
    /// On successful authentication the socket path is unlinked immediately.
    /// The accepted stream remains live, but stale clients cannot reconnect
    /// through the bootstrap locator.
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
        self.accept_loop(Some(deadline), APP_LINK_IO_DEADLINE, observer)
    }

    /// Stops a blocked [`Self::accept_authenticated`] call.
    ///
    /// Connecting and immediately closing a local stream wakes the blocking
    /// `accept`; the receive-side handshake then observes the stop flag and
    /// returns without accepting an unauthenticated client.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the wake-up connection cannot be made while
    /// the listener is still live.
    pub fn shutdown(&self) -> io::Result<()> {
        let cancel = self.cancellation().cancel();
        let close = self.close_endpoint();
        cancel.and(close)
    }
}

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
            let timeout = deadline
                .and_then(|deadline| deadline.checked_duration_since(Instant::now()))
                .map_or(handshake_deadline, |remaining| {
                    remaining.min(handshake_deadline)
                });
            if timeout.is_zero() {
                self.close_endpoint()?;
                return Ok(BootstrapAdmission::DeadlineElapsed);
            }
            let Some(peer_deadline) = Instant::now().checked_add(timeout) else {
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
            *lock_or_recover(&self.active_stream) = Some(active_stream);
            let _active = ActiveHandshake {
                active_stream: Arc::clone(&self.active_stream),
            };
            match handshake_server_interruptible_until(
                &mut stream,
                &self.token,
                self.stopping.as_ref(),
                peer_deadline,
            ) {
                Ok(true) => {
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
            let guard = lock_or_recover(&self.listener);
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
            let guard = lock_or_recover(&self.listener);
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
        let listener = lock_or_recover(&self.listener).take();
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
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(stream) = lock_or_recover(&self.active_stream).take() {
            let _ = stream.shutdown_app_link();
        }
        if !self.listening.load(Ordering::Acquire) {
            return Ok(());
        }
        #[cfg(unix)]
        {
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
        {
            match TcpStream::connect(("127.0.0.1", self.port)) {
                Ok(stream) => match stream.shutdown_app_link() {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotConnected => Ok(()),
                    Err(error) => Err(error),
                },
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotConnected
                            | io::ErrorKind::ConnectionRefused
                            | io::ErrorKind::ConnectionAborted
                    ) =>
                {
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
    }
}

struct ActiveHandshake {
    active_stream: Arc<Mutex<Option<BootstrapStream>>>,
}

impl Drop for ActiveHandshake {
    fn drop(&mut self) {
        *lock_or_recover(&self.active_stream) = None;
    }
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

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

/// Opt-in Windows owner-DACL named-pipe bootstrap added by KEL-101/T2.
///
/// [`BootstrapListener::bind`] deliberately remains the live loopback-TCP
/// constructor until T3 migrates all consumers atomically. This type owns the
/// shared named-pipe transport and reuses the same safe HELLO and rejection
/// state machine without making a product-LIVE claim.
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsNamedPipeBootstrapListener {
    server: Mutex<Option<WindowsNamedPipeServer>>,
    admission: Mutex<()>,
    endpoint: String,
    token: SessionToken,
    stopping: Arc<AtomicBool>,
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
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(io::Error::other)?;
        let mut endpoint = String::from(r"\\.\pipe\keld-");
        for byte in nonce {
            write!(&mut endpoint, "{byte:02x}").map_err(io::Error::other)?;
        }
        let server = WindowsNamedPipeServer::bind(&endpoint)?;
        Ok(Self {
            server: Mutex::new(Some(server)),
            admission: Mutex::new(()),
            endpoint,
            token,
            stopping: Arc::new(AtomicBool::new(false)),
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
            server: lock_or_recover(&self.server).as_ref().map_or_else(
                WindowsNamedPipeCanceller::empty,
                WindowsNamedPipeServer::canceller,
            ),
            stopping: Arc::clone(&self.stopping),
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
        let _admission = lock_or_recover(&self.admission);
        self.accept_loop(deadline, APP_LINK_IO_DEADLINE, observer)
    }

    fn accept_loop(
        &self,
        deadline: Instant,
        handshake_deadline: Duration,
        observer: &dyn BootstrapRejectionObserver,
    ) -> io::Result<WindowsNamedPipeBootstrapAdmission> {
        loop {
            let server = lock_or_recover(&self.server)
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
                drop(lock_or_recover(&self.server).take());
                return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
            }
            if Instant::now() >= deadline {
                server.close_terminal()?;
                drop(lock_or_recover(&self.server).take());
                return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
            }
            match server.accept_until(Some(deadline))? {
                WaitOutcome::Cancelled => {
                    drop(lock_or_recover(&self.server).take());
                    return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
                }
                WaitOutcome::DeadlineElapsed => {
                    drop(lock_or_recover(&self.server).take());
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
                drop(lock_or_recover(&self.server).take());
                return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                server.close_terminal()?;
                drop(lock_or_recover(&self.server).take());
                return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
            };
            if remaining.is_zero() {
                server.close_terminal()?;
                drop(lock_or_recover(&self.server).take());
                return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
            }
            let peer_timeout = remaining.min(handshake_deadline);
            let Some(peer_deadline) = Instant::now().checked_add(peer_timeout) else {
                server.close_terminal()?;
                drop(lock_or_recover(&self.server).take());
                return Ok(WindowsNamedPipeBootstrapAdmission::DeadlineElapsed);
            };
            let mut stream = WindowsNamedPipeBootstrapStream(server.stream()?);
            stream.set_app_link_read_deadline(Some(APP_LINK_READER_POLL.min(peer_timeout)))?;
            stream.set_app_link_write_deadline(Some(peer_timeout))?;
            stream.0.set_absolute_deadline(Some(peer_deadline));
            match handshake_server_interruptible_until(
                &mut stream,
                &self.token,
                self.stopping.as_ref(),
                peer_deadline,
            ) {
                Ok(true) => {
                    stream.0.set_absolute_deadline(None);
                    server.consume();
                    drop(lock_or_recover(&self.server).take());
                    return Ok(WindowsNamedPipeBootstrapAdmission::Authenticated(stream));
                }
                Ok(false) => {
                    drop(stream);
                    server.close_terminal()?;
                    drop(lock_or_recover(&self.server).take());
                    return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
                }
                Err(_) if self.stopping.load(Ordering::Acquire) => {
                    drop(stream);
                    server.close_terminal()?;
                    drop(lock_or_recover(&self.server).take());
                    return Ok(WindowsNamedPipeBootstrapAdmission::Cancelled);
                }
                Err(IpcError::Timeout) if Instant::now() >= deadline => {
                    drop(stream);
                    server.close_terminal()?;
                    drop(lock_or_recover(&self.server).take());
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
        lock_or_recover(&self.server)
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "bootstrap consumed"))?
            .inspect_owned_pipe(inspect)
    }

    #[cfg(test)]
    fn is_connected(&self) -> bool {
        lock_or_recover(&self.server)
            .as_ref()
            .is_some_and(WindowsNamedPipeServer::is_connected)
    }

    #[cfg(test)]
    fn is_accept_pending(&self) -> bool {
        lock_or_recover(&self.server)
            .as_ref()
            .is_some_and(WindowsNamedPipeServer::is_accept_pending)
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
    /// Duplicates the Rust stream view while retaining the same owned pipe
    /// handle. Read and write deadline settings are copied, then independent.
    ///
    /// # Errors
    ///
    /// Reserved for parity with other platform streams.
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    /// Waits until the peer has consumed every written byte, then disconnects.
    ///
    /// This is the orderly reply-before-close path. [`AppLinkDeadlines::shutdown_app_link`]
    /// remains abortive and may discard unread pipe bytes.
    ///
    /// # Errors
    ///
    /// Returns a timeout if `deadline` wins; synchronous Windows flush is
    /// cancelled and joined before the pipe is disconnected.
    pub fn shutdown_after_drain(&self, deadline: Instant) -> io::Result<()> {
        self.0.shutdown_after_drain(deadline)
    }
}

#[cfg(all(test, windows))]
mod named_pipe_tests {
    #![allow(unsafe_code)] // test-only independent Win32 descriptor/handle oracle

    use std::io::{Read as _, Write as _};
    use std::os::windows::io::AsRawHandle as _;
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use windows_permissions::constants::{AceFlags, AceType, SeObjectType, SecurityInformation};
    use windows_permissions::utilities::current_process_sid;
    use windows_permissions::wrappers::GetSecurityInfo;
    use windows_sys::Win32::Foundation::{GetHandleInformation, HANDLE_FLAG_INHERIT};
    use windows_sys::Win32::System::Pipes::{GetNamedPipeInfo, PIPE_REJECT_REMOTE_CLIENTS};

    use crate::link::{AppLinkDeadlines, handshake_client};
    use crate::serve_echo_requests;
    use crate::token::{SessionToken, parse_app_link};
    use crate::windows_named_pipe::{WindowsNamedPipeServer, process_handle_count};
    use crate::{ChannelId, CorrelationId, FrameHeader, FrameKind, MAX_FRAME_LEN};

    use super::{
        BootstrapRejection, BootstrapRejectionObserver, WindowsNamedPipeBootstrapAdmission,
        WindowsNamedPipeBootstrapListener, WindowsNamedPipeBootstrapStream, lock_or_recover,
    };

    #[derive(Clone)]
    struct RecordingObserver {
        seen: Arc<Mutex<Vec<BootstrapRejection>>>,
        notify: Option<mpsc::Sender<BootstrapRejection>>,
    }

    impl BootstrapRejectionObserver for RecordingObserver {
        fn rejected(&self, rejection: BootstrapRejection) {
            lock_or_recover(&self.seen).push(rejection);
            if let Some(notify) = &self.notify {
                let _ = notify.send(rejection);
            }
        }
    }

    struct BlockingObserver {
        observed: mpsc::Sender<BootstrapRejection>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl BootstrapRejectionObserver for BlockingObserver {
        fn rejected(&self, rejection: BootstrapRejection) {
            let _ = self.observed.send(rejection);
            let _ = lock_or_recover(&self.release).recv();
        }
    }

    fn client(endpoint: &str) -> std::io::Result<WindowsNamedPipeBootstrapStream> {
        WindowsNamedPipeServer::connect_client_until(
            endpoint,
            Instant::now() + Duration::from_secs(2),
        )
        .map(WindowsNamedPipeBootstrapStream)
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
            assert_eq!(*lock_or_recover(&seen), vec![expected]);
            return;
        }
        assert_eq!(
            notify_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("rejection record"),
            expected
        );
        let mut reply = [0_u8; 1];
        assert!(
            hostile.read(&mut reply).is_err(),
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
        assert_eq!(*lock_or_recover(&seen), vec![expected]);
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
    fn pending_accept_cancellation_completes_and_joins() {
        let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
        let cancellation = listener.cancellation();
        let worker_listener = Arc::clone(&listener);
        let worker = thread::spawn(move || {
            worker_listener.accept_authenticated_until(
                Instant::now() + Duration::from_secs(30),
                &super::NoopRejectionObserver,
            )
        });
        let pending_deadline = Instant::now() + Duration::from_secs(1);
        while !listener.is_accept_pending() {
            assert!(
                Instant::now() < pending_deadline,
                "server never entered overlapped pending accept"
            );
            thread::yield_now();
        }
        cancellation.cancel().expect("cancel pending accept");
        let outcome = worker
            .join()
            .expect("join accept worker")
            .expect("admission");
        assert!(matches!(
            outcome,
            WindowsNamedPipeBootstrapAdmission::Cancelled
        ));
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
        let seen = lock_or_recover(&seen);
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
        assert_eq!(*lock_or_recover(&seen), vec![BootstrapRejection::HelloAuth]);
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
                Instant::now() + Duration::from_secs(3),
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
        assert_eq!(*lock_or_recover(&seen), vec![BootstrapRejection::Timeout]);
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
        run_rejection_then_authenticate(Some(&empty_hello), BootstrapRejection::HelloAuth);

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
        run_rejection_then_authenticate(Some(&short_hello), BootstrapRejection::HelloAuth);

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
        assert!(matches!(error.raw_os_error(), Some(995 | 109 | 233)));
        reader.join().expect("join blocked reader");
        let mut peer_byte = [0_u8; 1];
        let peer_error = role
            .read(&mut peer_byte)
            .expect_err("shutdown must also close the peer-facing connection");
        assert!(matches!(peer_error.raw_os_error(), Some(109 | 232 | 233)));
    }

    #[test]
    fn orderly_shutdown_waits_for_peer_read_then_preserves_bytes() {
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
        let WindowsNamedPipeBootstrapAdmission::Authenticated(mut server_stream) = outcome else {
            panic!("expected authenticated named-pipe session")
        };
        server_stream
            .write_all(b"reply")
            .expect("write orderly reply");
        let drain_stream = server_stream.try_clone().expect("clone drain stream");
        let (drain_tx, drain_rx) = mpsc::channel();
        let drain = thread::spawn(move || {
            let _ = drain_tx
                .send(drain_stream.shutdown_after_drain(Instant::now() + Duration::from_secs(2)));
        });
        let pending_deadline = Instant::now() + Duration::from_secs(1);
        while !server_stream.0.is_drain_pending() {
            assert!(
                Instant::now() < pending_deadline,
                "orderly drain never entered FlushFileBuffers"
            );
            thread::yield_now();
        }
        assert!(
            matches!(drain_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "drain must not complete before the peer consumes the reply"
        );
        let mut reply = [0_u8; 5];
        role.read_exact(&mut reply).expect("read orderly reply");
        assert_eq!(&reply, b"reply");
        drain_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("drain completion")
            .expect("orderly drain");
        drain.join().expect("join orderly drain");
    }

    #[test]
    fn orderly_shutdown_times_out_and_joins_for_non_reading_peer() {
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
        let WindowsNamedPipeBootstrapAdmission::Authenticated(mut server_stream) = outcome else {
            panic!("expected authenticated named-pipe session")
        };
        server_stream
            .write_all(b"reply")
            .expect("write orderly reply");
        let error = server_stream
            .shutdown_after_drain(Instant::now() + Duration::from_millis(50))
            .expect_err("non-reading peer must not wedge orderly shutdown");
        assert_eq!(error.raw_os_error(), Some(121));
        drop(role);
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
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::parse_app_link;

    use super::{BootstrapAdmission, BootstrapListener, lock_or_recover};

    #[test]
    fn cancellation_interrupts_active_silent_handshake_promptly() {
        let listener = Arc::new(BootstrapListener::bind().expect("bind Windows bootstrap"));
        let cancellation = listener.cancellation();
        let link = listener.app_link();
        let (endpoint, _) = parse_app_link(&link).expect("Windows app link");
        let port = endpoint.parse::<u16>().expect("Windows bootstrap port");
        let acceptor = Arc::clone(&listener);
        let (result_tx, result_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = acceptor.accept_authenticated_until(
                Instant::now() + Duration::from_secs(30),
                &super::NoopRejectionObserver,
            );
            let _ = result_tx.send(result);
        });
        let silent = TcpStream::connect(("127.0.0.1", port)).expect("connect silent peer");

        let active_deadline = Instant::now() + Duration::from_secs(2);
        while lock_or_recover(&cancellation.active_stream).is_none() {
            assert!(
                Instant::now() < active_deadline,
                "silent peer never became the active handshake"
            );
            thread::yield_now();
        }
        let cancel_started = Instant::now();
        cancellation.cancel().expect("cancel active handshake");
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
    fn consumed_windows_listener_close_preserves_outcome_and_rejects_reconnect() {
        let listener = BootstrapListener::bind().expect("bind Windows bootstrap");
        let link = listener.app_link();
        let (endpoint, _) = parse_app_link(&link).expect("Windows app link");
        let port = endpoint.parse::<u16>().expect("Windows bootstrap port");

        listener
            .close_endpoint()
            .expect("locator close cannot replace the selected admission outcome");
        let rebound = TcpListener::bind(("127.0.0.1", port))
            .expect("consumed listener must release its exact OS endpoint");
        assert_eq!(rebound.local_addr().expect("rebound address").port(), port);
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
    use std::sync::Arc;
    use std::sync::mpsc;
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
            super::lock_or_recover(&self.seen).push(rejection);
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

        let seen = super::lock_or_recover(&seen);
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

        let seen = super::lock_or_recover(&seen);
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
