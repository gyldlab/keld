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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::link::handshake_server;
use crate::token::{SessionToken, format_app_link};

/// Owner-only Unix listener that authenticates one role bootstrap connection.
///
/// The listener is a cold setup mechanism, not a general application channel.
/// It remains available after rejected `HELLO` frames until a valid role
/// connects or [`Self::shutdown`] is requested.
#[derive(Debug)]
pub struct BootstrapListener {
    listener: UnixListener,
    path: PathBuf,
    session_dir: PathBuf,
    token: SessionToken,
    stopping: Arc<AtomicBool>,
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
        Ok(Self {
            listener,
            path,
            session_dir,
            token,
            stopping: Arc::new(AtomicBool::new(false)),
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
        loop {
            let (mut stream, _) = self.listener.accept()?;
            match handshake_server(&mut stream, &self.token) {
                Ok(()) => return Ok(Some(stream)),
                Err(_) if self.stopping.load(Ordering::SeqCst) => return Ok(None),
                Err(_) => {}
            }
        }
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
        self.stopping.store(true, Ordering::SeqCst);
        match UnixStream::connect(&self.path) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl Drop for BootstrapListener {
    fn drop(&mut self) {
        let _ = self.shutdown();
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.session_dir);
    }
}

fn unique_session_dir() -> io::Result<PathBuf> {
    // `sockaddr_un.sun_path` is 104 bytes on macOS (108 on Linux). Keep the
    // generated component short so a long TMPDIR cannot overflow it.
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let session_dir = std::env::temp_dir().join(format!("kb-{}-{nonce}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&session_dir)?;
    if let Err(error) = fs::set_permissions(&session_dir, fs::Permissions::from_mode(0o700)) {
        let _ = fs::remove_dir(&session_dir);
        return Err(error);
    }
    Ok(session_dir)
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use crate::link::{AppLinkDeadlines, handshake_client};
    use crate::token::{SessionToken, parse_app_link};

    use super::BootstrapListener;

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
