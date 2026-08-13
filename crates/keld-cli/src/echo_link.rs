//! Loopback app-link helpers shared by `ipc-echo` and `ipc-client`.

#[cfg(unix)]
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use keld_ipc::{
    EchoRequest, EchoResponse, SESSION_TOKEN_LEN, SessionToken, echo_call, format_app_link,
    parse_app_link, serve_echo_session,
};

/// Endpoint for the echo server (Unix socket path or TCP port).
#[derive(Debug, Clone)]
pub enum EchoEndpoint {
    /// Unix domain socket path.
    #[cfg(unix)]
    Unix(PathBuf),
    /// Loopback TCP port (Windows).
    #[cfg(windows)]
    Tcp(u16),
}

impl EchoEndpoint {
    fn endpoint_value(&self) -> String {
        #[cfg(unix)]
        {
            let EchoEndpoint::Unix(path) = self;
            path.display().to_string()
        }
        #[cfg(windows)]
        {
            let EchoEndpoint::Tcp(port) = self;
            port.to_string()
        }
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
    endpoint: EchoEndpoint,
    token: SessionToken,
    handle: Option<thread::JoinHandle<Result<(), keld_ipc::IpcError>>>,
    /// Owner-only directory that contains the Unix socket; removed on join/Drop.
    #[cfg(unix)]
    session_dir: PathBuf,
}

fn mint_session_token() -> io::Result<SessionToken> {
    let mut bytes = [0u8; SESSION_TOKEN_LEN];
    getrandom::fill(&mut bytes).map_err(|err| io::Error::other(err.to_string()))?;
    Ok(SessionToken::from_bytes(bytes))
}

#[cfg(unix)]
fn bind_unix_echo() -> io::Result<(PathBuf, PathBuf, std::os::unix::net::UnixListener)> {
    // `sockaddr_un.sun_path` is 104 bytes on macOS (108 on Linux). Long TMPDIR
    // plus `keld-echo-{pid}-{nanos}/echo.sock` overflows (ENAMETOOLONG).
    let session_dir = std::env::temp_dir().join(format!(
        "ke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::DirBuilder::new().mode(0o700).create(&session_dir)?;
    if let Err(err) = fs::set_permissions(&session_dir, fs::Permissions::from_mode(0o700)) {
        let _ = fs::remove_dir(&session_dir);
        return Err(err);
    }
    let path = session_dir.join("e.sock");
    match std::os::unix::net::UnixListener::bind(&path) {
        Ok(listener) => Ok((session_dir, path, listener)),
        Err(err) => {
            let _ = fs::remove_dir_all(&session_dir);
            Err(err)
        }
    }
}

impl EchoServer {
    /// Binds the endpoint and starts accepting one echo session on a worker thread.
    ///
    /// `ready` fires after the listener is bound (safe for clients to connect).
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the loopback listener cannot be bound.
    pub fn start(ready: &mpsc::Sender<()>) -> io::Result<Self> {
        let token = mint_session_token()?;
        #[cfg(unix)]
        {
            let (session_dir, path, listener) = bind_unix_echo()?;
            ready.send(()).ok();
            let serve_token = token;
            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept()?;
                serve_echo_session(&mut stream, &serve_token)
            });
            Ok(Self {
                endpoint: EchoEndpoint::Unix(path),
                token,
                handle: Some(handle),
                session_dir,
            })
        }
        #[cfg(windows)]
        {
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
        self.endpoint.link_env(&self.token)
    }

    /// Waits for the server thread and removes the Unix session directory.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the session failed or the worker panicked.
    pub fn join(mut self) -> io::Result<()> {
        self.finish(false)
    }

    /// Closes the listener so `accept` unblocks, then joins and unlinks.
    ///
    /// Used when the client never connects (failed Bun child, Drop).
    pub(crate) fn shutdown(mut self) -> io::Result<()> {
        self.finish(true)
    }

    fn finish(&mut self, interrupt: bool) -> io::Result<()> {
        if interrupt {
            self.interrupt_accept();
        }
        let result = if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(io::Error::other(err.to_string())),
                Err(_) => Err(io::Error::other("echo server thread panicked")),
            }
        } else {
            Ok(())
        };
        self.cleanup_socket();
        result
    }

    fn interrupt_accept(&self) {
        #[cfg(unix)]
        {
            let EchoEndpoint::Unix(path) = &self.endpoint;
            let _ = std::os::unix::net::UnixStream::connect(path);
        }
        #[cfg(windows)]
        {
            let EchoEndpoint::Tcp(port) = &self.endpoint;
            let _ = std::net::TcpStream::connect(("127.0.0.1", *port));
        }
    }

    #[cfg_attr(windows, allow(clippy::unused_self))] // Unix unlinks `self.session_dir`; Windows TCP has no socket file.
    fn cleanup_socket(&self) {
        #[cfg(unix)]
        {
            let _ = fs::remove_dir_all(&self.session_dir);
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
                .map(|d| d.as_nanos())
                .unwrap_or(0)
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

    #[test]
    fn wrong_token_is_ipc_007() {
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
        let server_msg = server
            .join()
            .expect_err("host must reject the foreign HELLO")
            .to_string();
        assert!(
            server_msg.contains("KELD-IPC-007"),
            "host must fail closed with 007, got {server_msg}"
        );
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
            Some("e.sock")
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
