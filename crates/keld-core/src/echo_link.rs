//! Host-owned platform app-link echo listener (KEL-30/KEL-101).
//!
//! Lives in `keld-core` so `keld-host` can mint/own the same link without
//! depending on `keld-cli`. CLI diagnostics re-export this module.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

use keld_ipc::{BootstrapListener, serve_echo_requests_until_stopped};
use keld_ipc::{EchoRequest, EchoResponse, echo_call, parse_app_link};
#[cfg(windows)]
use keld_ipc::{SessionToken, WindowsNamedPipeBootstrapStream, format_app_link};

/// Compatibility endpoint value for the legacy Windows diagnostic echo surface.
#[cfg(windows)]
#[derive(Debug, Clone)]
pub enum EchoEndpoint {
    /// Legacy client-only loopback TCP diagnostic port (Windows).
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
    /// The reusable platform listener is owned by kipc.
    bootstrap: Arc<BootstrapListener>,
    /// Stops an authenticated idle reader before joining its worker thread.
    stop: Arc<AtomicBool>,
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
        let stop = Arc::new(AtomicBool::new(false));
        let bootstrap = Arc::new(BootstrapListener::bind()?);
        ready.send(()).ok();
        let acceptor = Arc::clone(&bootstrap);
        let stop_for_worker = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let Some(mut stream) = acceptor.accept_authenticated()? else {
                return Err(keld_ipc::IpcError::Io(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "echo bootstrap listener stopped before authentication",
                )));
            };
            serve_echo_requests_until_stopped(&mut stream, stop_for_worker.as_ref())
        });
        Ok(Self {
            bootstrap,
            stop,
            handle: Some(handle),
        })
    }

    /// App-link value for clients (`<endpoint>#<64 hex chars>`).
    #[must_use]
    pub fn link(&self) -> String {
        self.bootstrap.app_link()
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
            self.stop.store(true, Ordering::Release);
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
        let _ = self.bootstrap.shutdown();
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
        if WindowsNamedPipeBootstrapStream::is_keld_endpoint(endpoint) {
            let mut stream = WindowsNamedPipeBootstrapStream::connect(endpoint)?;
            echo_call(&mut stream, request, &token)
        } else {
            let port = parse_decimal_diagnostic_port(endpoint)?;
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))?;
            echo_call(&mut stream, request, &token)
        }
    }
}

#[cfg(windows)]
fn parse_decimal_diagnostic_port(endpoint: &str) -> Result<u16, keld_ipc::IpcError> {
    if endpoint.is_empty()
        || endpoint.len() > 5
        || endpoint.starts_with('0')
        || !endpoint.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(keld_ipc::IpcError::HelloAuth {
            detail: "KELD_APP_LINK Windows endpoint must be an exact Keld pipe or decimal diagnostic port",
        });
    }
    endpoint
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or(keld_ipc::IpcError::HelloAuth {
            detail: "KELD_APP_LINK Windows endpoint must be an exact Keld pipe or decimal diagnostic port",
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::fs;
    #[cfg(windows)]
    use std::net::TcpListener;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::path::PathBuf;

    use keld_ipc::link::{AppLinkDeadlines, handshake_client};
    use keld_ipc::{
        APP_LINK_IO_DEADLINE, CorrelationId, SESSION_TOKEN_LEN, SessionToken, echo_invoke,
        format_app_link,
    };

    fn fixture_token() -> SessionToken {
        SessionToken::from_bytes([0x11; SESSION_TOKEN_LEN])
    }

    #[cfg(unix)]
    fn unix_socket_path(link: &str) -> PathBuf {
        let (endpoint, _) = parse_app_link(link).expect("KELD_APP_LINK");
        PathBuf::from(endpoint)
    }

    #[cfg(unix)]
    type PersistentClient = std::os::unix::net::UnixStream;
    #[cfg(windows)]
    type PersistentClient = WindowsNamedPipeBootstrapStream;

    fn open_persistent_client(server: &EchoServer) -> PersistentClient {
        let link = server.link();
        let (endpoint, token) = parse_app_link(&link).expect("KELD_APP_LINK");
        #[cfg(unix)]
        let mut stream = PersistentClient::connect(endpoint).expect("connect unix app link");
        #[cfg(windows)]
        let mut stream = PersistentClient::connect(endpoint).expect("connect named-pipe app link");
        stream
            .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
            .expect("set client deadlines");
        handshake_client(&mut stream, &token).expect("authenticated HELLO");
        let reply = echo_invoke(
            &mut stream,
            &EchoRequest {
                message: "quiet-client-ready".to_owned(),
                count: 1,
            },
            CorrelationId(1),
        )
        .expect("first echo");
        assert_eq!(reply.message, "quiet-client-ready");
        stream
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
        let endpoint = format!(r"\\.\pipe\keld-{}", "0".repeat(64));
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

    #[cfg(windows)]
    #[test]
    fn malformed_windows_endpoint_is_ipc_007_without_disclosure() {
        let token = fixture_token();
        for endpoint in [
            r"\\.\pipe\other-0123",
            r"\\.\pipe\keld-ABCDEF",
            "0",
            "01",
            "65536",
            "127.0.0.1:9000",
        ] {
            let link = format_app_link(endpoint, &token);
            let error = echo_roundtrip(
                &link,
                &EchoRequest {
                    message: "must-not-echo".to_owned(),
                    count: 1,
                },
            )
            .expect_err("unknown Windows endpoint must fail locally");
            let message = error.to_string();
            assert!(message.contains("KELD-IPC-007"), "{message}");
            assert!(!message.contains(&link), "full app link leaked: {message}");
            assert!(
                !message.contains(&token.to_hex()),
                "token leaked: {message}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn decimal_port_remains_explicit_client_only_diagnostic_compatibility() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind diagnostic TCP");
        let port = listener.local_addr().expect("diagnostic address").port();
        let token = fixture_token();
        let server_token = token;
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept diagnostic client");
            stream
                .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
                .expect("diagnostic deadlines");
            keld_ipc::link::handshake_server(&mut stream, &server_token).expect("diagnostic HELLO");
            keld_ipc::serve_echo_requests(&mut stream).expect("diagnostic echo");
        });
        let link = format_app_link(&port.to_string(), &token);
        let response = echo_roundtrip(
            &link,
            &EchoRequest {
                message: "diagnostic-only".to_owned(),
                count: 17,
            },
        )
        .expect("consume explicit decimal diagnostic link");
        assert_eq!(response.message, "diagnostic-only");
        assert_eq!(response.count, 17);
        worker.join().expect("join diagnostic server");
    }

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
    fn roundtrip_over_platform_app_link_copies_fields() {
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
        #[cfg(windows)]
        assert!(
            parse_app_link(&link)
                .expect("link")
                .0
                .starts_with(r"\\.\pipe\keld-"),
            "shipping Windows echo must mint a named pipe"
        );
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
    fn shutdown_interrupts_a_quiet_authenticated_client() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(&ready_tx).expect("bind echo server");
        ready_rx.recv().expect("server ready");
        let client = open_persistent_client(&server);

        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = server.shutdown();
            let _ = done_tx.send(result);
        });
        let result = done_rx.recv_timeout(std::time::Duration::from_secs(2)).expect(
            "shutdown must interrupt a quiet authenticated reader; waiting for the old 5-second \
             read deadline means the host cannot promptly reap the app-link",
        );
        assert!(
            result.is_ok(),
            "quiet authenticated shutdown failed: {result:?}"
        );
        drop(client);
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
