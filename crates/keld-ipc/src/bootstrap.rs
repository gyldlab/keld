//! Host-owned authenticated Unix bootstrap listeners.
//!
//! This cold-path primitive owns an owner-only listener, a fresh `HELLO`
//! possession token, and cleanup. It deliberately accepts another client
//! after an invalid handshake so an untrusted connector cannot consume the
//! legitimate role's one bootstrap opportunity.

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::APP_LINK_IO_DEADLINE;
use crate::IpcError;
// Re-exported, not merely imported: these were public at
// `keld_ipc::bootstrap::{BootstrapRejection, BootstrapRejectionObserver}`
// before the taxonomy moved to `admission`, and a crate-root export does not
// preserve that path. Moving the owner must not break the published one.
pub use crate::admission::{BootstrapRejection, BootstrapRejectionObserver};
use crate::link::{AppLinkDeadlines, handshake_server};
use crate::token::{SessionToken, format_app_link};

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
static UNIQUE_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Owner-only Unix listener that authenticates one role bootstrap connection.
///
/// The listener is a cold setup mechanism, not a general application channel.
/// It remains available after rejected `HELLO` frames until a valid role
/// connects or [`Self::shutdown`] is requested.
#[derive(Debug)]
pub struct BootstrapListener {
    listener: Mutex<Option<UnixListener>>,
    path: PathBuf,
    session_dir: PathBuf,
    token: SessionToken,
    stopping: Arc<AtomicBool>,
    active_stream: Arc<Mutex<Option<UnixStream>>>,
}

/// Result of one bounded bootstrap admission attempt.
#[derive(Debug)]
pub enum BootstrapAdmission {
    /// A peer proved possession of the listener's token.
    Authenticated(UnixStream),
    /// The host cancelled admission before authentication completed.
    Cancelled,
    /// The generation-wide admission deadline elapsed before authentication.
    DeadlineElapsed,
}

struct NoopRejectionObserver;

impl BootstrapRejectionObserver for NoopRejectionObserver {
    fn rejected(&self, _rejection: BootstrapRejection) {}
}

/// Cancellation handle for a blocked bootstrap admission worker.
#[derive(Debug, Clone)]
pub struct BootstrapCancellation {
    path: PathBuf,
    stopping: Arc<AtomicBool>,
    active_stream: Arc<Mutex<Option<UnixStream>>>,
}

impl BootstrapListener {
    /// Binds a new owner-only Unix endpoint and mints its session token.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the random source, session directory, or Unix
    /// listener cannot be created.
    pub fn bind() -> io::Result<Self> {
        let token = SessionToken::random()?;
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
            active_stream: Arc::new(Mutex::new(None)),
        })
    }

    /// Canonical `KELD_APP_LINK` value for the one role this listener admits.
    #[must_use]
    pub fn app_link(&self) -> String {
        format_app_link(&self.path.display().to_string(), &self.token)
    }

    /// Filesystem endpoint for host-side assertions and cleanup diagnostics.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Handle that can cancel a blocked accept or active handshake.
    #[must_use]
    pub fn cancellation(&self) -> BootstrapCancellation {
        BootstrapCancellation {
            path: self.path.clone(),
            stopping: Arc::clone(&self.stopping),
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
    pub fn accept_authenticated(&self) -> io::Result<Option<UnixStream>> {
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
            // Both calls act on the ACCEPTED PEER's socket, not on the
            // listener, so a failure is a fact about that one peer. `?` made
            // them fatal to admission instead.
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
            if stream.set_app_link_deadlines(Some(timeout)).is_err() {
                observer.rejected(BootstrapRejection::Io);
                continue;
            }
            let Ok(active_stream) = stream.try_clone() else {
                observer.rejected(BootstrapRejection::Io);
                continue;
            };
            *lock_or_recover(&self.active_stream) = Some(active_stream);
            let _active = ActiveHandshake {
                active_stream: Arc::clone(&self.active_stream),
            };
            match handshake_server(&mut stream, &self.token) {
                Ok(()) => {
                    self.close_endpoint()?;
                    return Ok(BootstrapAdmission::Authenticated(stream));
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

    fn try_accept(&self) -> io::Result<Option<UnixStream>> {
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

    fn close_endpoint(&self) -> io::Result<()> {
        let _ = lock_or_recover(&self.listener).take();
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
}

struct ActiveHandshake {
    active_stream: Arc<Mutex<Option<UnixStream>>>,
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

#[cfg(test)]
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
