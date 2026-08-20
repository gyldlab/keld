//! Host-owned loopback app-link echo listener (KEL-30).
//!
//! Lives in `keld-core` so `keld-host` can mint/own the same link without
//! depending on `keld-cli`. CLI diagnostics re-export this module.

use std::io;
#[cfg(unix)]
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

#[cfg(windows)]
use keld_ipc::serve_echo_session;
#[cfg(unix)]
use keld_ipc::{BootstrapListener, serve_echo_requests};
use keld_ipc::{EchoRequest, EchoResponse, echo_call, parse_app_link};
#[cfg(windows)]
use keld_ipc::{SessionToken, format_app_link};

/// Endpoint for the echo server (Unix socket path or TCP port).
#[cfg(windows)]
#[derive(Debug, Clone)]
pub enum EchoEndpoint {
    /// Loopback TCP port (Windows).
    Tcp(u16),
}

#[cfg(windows)]
impl EchoEndpoint {
    fn endpoint_value(&self) -> String {
        let Self::Tcp(port) = self;
        port.to_string()
    }

    /// String form passed to child processes via `KELD_APP_LINK`.
    #[must_use]
    pub fn link_env(&self, token: &SessionToken) -> String {
        format_app_link(&self.endpoint_value(), token)
    }
}

/// Handle to a background echo server thread.
#[derive(Debug)]
pub struct EchoServer {
    /// The Unix listener is reusable bootstrap infrastructure owned by kipc.
    #[cfg(unix)]
    bootstrap: Arc<BootstrapListener>,
    /// Windows retains the v0 loopback implementation until KEL-75 names a
    /// pipe/DACL transport slice.
    #[cfg(windows)]
    endpoint: EchoEndpoint,
    #[cfg(windows)]
    token: SessionToken,
    handle: Option<thread::JoinHandle<Result<(), keld_ipc::IpcError>>>,
}

impl EchoServer {
    /// Binds the endpoint and starts accepting one echo session on a worker thread.
    ///
    /// `ready` fires after the listener is bound (safe for clients to connect).
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the bootstrap endpoint cannot be bound.
    pub fn start(ready: &mpsc::Sender<()>) -> io::Result<Self> {
        #[cfg(unix)]
        {
            let bootstrap = Arc::new(BootstrapListener::bind()?);
            ready.send(()).ok();
            let acceptor = Arc::clone(&bootstrap);
            let handle = thread::spawn(move || {
                let Some(mut stream) = acceptor.accept_authenticated()? else {
                    return Err(keld_ipc::IpcError::Io(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "echo bootstrap listener stopped before authentication",
                    )));
                };
                serve_echo_requests(&mut stream)
            });
            Ok(Self {
                bootstrap,
                handle: Some(handle),
            })
        }
        #[cfg(windows)]
        {
            let token = SessionToken::random()?;
            // Destination: named pipe + current-user DACL (02-ipc §1). v0 is
            // loopback TCP; peer auth is the v2 HELLO token (KEL-60).
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            let port = listener.local_addr()?.port();
            ready.send(()).ok();
            let serve_token = token;
            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept()?;
                serve_echo_session(&mut stream, &serve_token)
            });
            Ok(Self {
                endpoint: EchoEndpoint::Tcp(port),
                token,
                handle: Some(handle),
            })
        }
    }

    /// App-link value for clients (`<endpoint>#<64 hex chars>`).
    #[must_use]
    pub fn link(&self) -> String {
        #[cfg(unix)]
        {
            self.bootstrap.app_link()
        }
        #[cfg(windows)]
        {
            self.endpoint.link_env(&self.token)
        }
    }

    /// Waits for the server thread.
    ///
    /// On Unix, [`BootstrapListener`] removes the session directory when its
    /// final owner drops after this method returns.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the session failed or the worker panicked.
    pub fn join(mut self) -> io::Result<()> {
        self.finish(false)
    }

    /// Closes the listener so `accept` unblocks, then joins and unlinks.
    ///
    /// Used when the client never connects (failed Bun child, Drop), or when
    /// the host tears down a concurrent hello session after the window closes.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the session failed or the worker panicked.
    pub fn shutdown(mut self) -> io::Result<()> {
        self.finish(true)
    }

    fn finish(&mut self, interrupt: bool) -> io::Result<()> {
        if interrupt {
            self.interrupt_accept();
        }
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(io::Error::other(err.to_string())),
                Err(_) => Err(io::Error::other("echo server thread panicked")),
            }
        } else {
            Ok(())
        }
    }

    fn interrupt_accept(&self) {
        #[cfg(unix)]
        {
            let _ = self.bootstrap.shutdown();
        }
        #[cfg(windows)]
        {
            let EchoEndpoint::Tcp(port) = &self.endpoint;
            let _ = std::net::TcpStream::connect(("127.0.0.1", *port));
        }
    }
}

impl Drop for EchoServer {
    fn drop(&mut self) {
        if self.handle.is_some() {
            let _ = self.finish(true);
        }
    }
}

/// Performs one echo round-trip against `link` (`KELD_APP_LINK` value).
///
/// # Errors
///
/// Returns [`keld_ipc::IpcError`] on connect, protocol, or codec failure.
pub fn echo_roundtrip(
    link: &str,
    request: &EchoRequest,
) -> Result<EchoResponse, keld_ipc::IpcError> {
    let (endpoint, token) = parse_app_link(link)?;
    #[cfg(unix)]
    {
        let mut stream = std::os::unix::net::UnixStream::connect(endpoint)?;
        echo_call(&mut stream, request, &token)
    }
    #[cfg(windows)]
    {
        let port: u16 = endpoint
            .parse()
            .map_err(|_| keld_ipc::IpcError::HelloAuth {
                detail: "KELD_APP_LINK Windows endpoint must be a TCP port",
            })?;
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))?;
        echo_call(&mut stream, request, &token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::path::PathBuf;

    use keld_ipc::{SESSION_TOKEN_LEN, SessionToken, format_app_link};

    fn fixture_token() -> SessionToken {
        SessionToken::from_bytes([0x11; SESSION_TOKEN_LEN])
    }

    #[cfg(unix)]
    fn unix_socket_path(link: &str) -> PathBuf {
        let (endpoint, _) = parse_app_link(link).expect("KELD_APP_LINK");
        PathBuf::from(endpoint)
    }

    #[test]
    fn missing_hash_is_ipc_007_not_echo() {
        let err = echo_roundtrip(
            "/no/such/keld-echo.sock",
            &EchoRequest {
                message: "missing".to_owned(),
                count: 1,
            },
        )
        .expect_err("link without token must fail closed");
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-007"), "{msg}");
        assert!(!msg.contains("KELD-IPC-001"), "{msg}");
        assert!(
            !msg.contains("message=\"missing\""),
            "must not fabricate an echo reply: {msg}"
        );
    }

    #[test]
    fn missing_socket_is_ipc_001() {
        let token = fixture_token();
        #[cfg(unix)]
        let endpoint = format!(
            "/no/such/keld-echo-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        #[cfg(windows)]
        let endpoint = "1".to_owned(); // port 1: connection refused on loopback
        let link = format_app_link(&endpoint, &token);
        let err = echo_roundtrip(
            &link,
            &EchoRequest {
                message: "missing".to_owned(),
                count: 1,
            },
        )
        .expect_err("connect must fail");
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-001"), "{msg}");
        assert!(
            !msg.contains("message=\"missing\""),
            "must not fabricate an echo reply: {msg}"
        );
        assert!(
            !msg.contains(&token.to_hex()),
            "must not leak the session token: {msg}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn wrong_token_is_ipc_007_and_does_not_consume_echo_listener() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(&ready_tx).expect("bind echo server");
        ready_rx.recv().expect("server ready");
        let link = server.link();
        let (endpoint, token) = parse_app_link(&link).expect("link");
        let mut foreign = *token.as_bytes();
        foreign[0] ^= 1;
        let bad_link = format_app_link(endpoint, &SessionToken::from_bytes(foreign));
        let err = echo_roundtrip(
            &bad_link,
            &EchoRequest {
                message: "stolen".to_owned(),
                count: 1,
            },
        )
        .expect_err("foreign token must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("KELD-IPC-001") || msg.contains("KELD-IPC-007"),
            "client must not complete echo; host closes without sending the token: {msg}"
        );
        assert!(
            !msg.contains("stolen"),
            "must not fabricate an echo reply: {msg}"
        );
        assert!(
            !msg.contains(&token.to_hex()),
            "must not leak the session token: {msg}"
        );

        let response = echo_roundtrip(
            &link,
            &EchoRequest {
                message: "legitimate".to_owned(),
                count: 2,
            },
        )
        .expect("foreign connector must not consume the legitimate echo session");
        assert_eq!(response.message, "legitimate");
        assert_eq!(response.count, 2);
        server.join().expect("legitimate session finishes");
    }

    #[cfg(windows)]
    #[test]
    fn wrong_token_is_ipc_007_on_the_one_accept_loopback_server() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(&ready_tx).expect("bind echo server");
        ready_rx.recv().expect("server ready");
        let link = server.link();
        let (endpoint, token) = parse_app_link(&link).expect("link");
        let mut foreign = *token.as_bytes();
        foreign[0] ^= 1;
        let bad_link = format_app_link(endpoint, &SessionToken::from_bytes(foreign));
        let error = echo_roundtrip(
            &bad_link,
            &EchoRequest {
                message: "stolen".to_owned(),
                count: 1,
            },
        )
        .expect_err("foreign token must fail");
        assert!(
            error.to_string().contains("KELD-IPC-001")
                || error.to_string().contains("KELD-IPC-007"),
            "foreign caller must not receive an echo result: {error}"
        );
        let server_error = server
            .join()
            .expect_err("v0 Windows one-accept listener closes after the rejected HELLO")
            .to_string();
        assert!(server_error.contains("KELD-IPC-007"), "{server_error}");
    }

    #[test]
    fn minted_session_tokens_differ() {
        let (ready_a, rx_a) = mpsc::channel();
        let a = EchoServer::start(&ready_a).expect("bind a");
        rx_a.recv().expect("a ready");
        let (ready_b, rx_b) = mpsc::channel();
        let b = EchoServer::start(&ready_b).expect("bind b");
        rx_b.recv().expect("b ready");
        let (_, token_a) = parse_app_link(&a.link()).expect("a");
        let (_, token_b) = parse_app_link(&b.link()).expect("b");
        assert_ne!(token_a, token_b, "each session must mint its own token");
        let _ = a.shutdown();
        let _ = b.shutdown();
    }

    #[test]
    fn roundtrip_over_loopback_copies_fields() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(&ready_tx).expect("bind echo server");
        ready_rx.recv().expect("server ready");
        let req = EchoRequest {
            message: "cli-loopback".to_owned(),
            count: 11,
        };
        let response = echo_roundtrip(&server.link(), &req).expect("echo");
        let link = server.link();
        assert!(
            link.contains('#'),
            "KELD_APP_LINK must carry the session token: {link}"
        );
        let (_, token) = parse_app_link(&link).expect("link");
        assert_eq!(token.to_hex().len(), 64);
        server.join().expect("join");
        assert_eq!(response.message, "cli-loopback");
        assert_eq!(response.count, 11);
    }

    #[test]
    fn shutdown_without_client_unblocks() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(&ready_tx).expect("bind echo server");
        ready_rx.recv().expect("server ready");
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = server.shutdown();
            let _ = done_tx.send(result);
        });
        let result = done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("shutdown must close the listener and join; timeout means accept() leaked");
        assert!(
            result.is_err(),
            "interrupt connect is not a kipc session: {result:?}"
        );
    }

    #[test]
    fn drop_without_client_unblocks() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(&ready_tx).expect("bind echo server");
        ready_rx.recv().expect("server ready");
        #[cfg(unix)]
        let session_dir = unix_socket_path(&server.link())
            .parent()
            .expect("socket lives in a session dir")
            .to_path_buf();
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            drop(server);
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("Drop must close the listener and join; timeout means accept() leaked");
        #[cfg(unix)]
        assert!(
            !session_dir.exists(),
            "Drop must remove the owner-only session dir: {}",
            session_dir.display()
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_session_dir_is_owner_only_and_removed_on_join() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(&ready_tx).expect("bind echo server");
        ready_rx.recv().expect("server ready");
        let link = server.link();
        let socket = unix_socket_path(&link);
        let session_dir = socket
            .parent()
            .expect("socket lives in a session dir")
            .to_path_buf();

        assert_eq!(
            socket.file_name().and_then(|name| name.to_str()),
            Some("app.sock")
        );
        assert_ne!(session_dir, std::env::temp_dir());
        assert!(
            session_dir.starts_with(std::env::temp_dir()),
            "session dir must be under temp: {}",
            session_dir.display()
        );
        let mode = fs::metadata(&session_dir)
            .expect("session dir metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "session dir must be owner-only, got {mode:#o}");

        let req = EchoRequest {
            message: "session-dir".to_owned(),
            count: 1,
        };
        echo_roundtrip(&link, &req).expect("echo");
        server.join().expect("join");
        assert!(
            !socket.exists(),
            "socket must be removed: {}",
            socket.display()
        );
        assert!(
            !session_dir.exists(),
            "session dir must be removed: {}",
            session_dir.display()
        );
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_removes_unix_session_dir() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(&ready_tx).expect("bind echo server");
        ready_rx.recv().expect("server ready");
        let session_dir = unix_socket_path(&server.link())
            .parent()
            .expect("socket lives in a session dir")
            .to_path_buf();
        let _ = server.shutdown();
        assert!(
            !session_dir.exists(),
            "shutdown must remove the owner-only session dir: {}",
            session_dir.display()
        );
    }
}
