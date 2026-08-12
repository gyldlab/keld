//! Loopback app-link helpers shared by `ipc-echo` and `ipc-client`.

use std::io;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use keld_ipc::{EchoRequest, EchoResponse, echo_call, serve_echo_session};

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
    /// String form passed to child processes via `KELD_APP_LINK`.
    #[must_use]
    pub fn link_env(&self) -> String {
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
}

/// Handle to a background echo server thread.
#[derive(Debug)]
pub struct EchoServer {
    endpoint: EchoEndpoint,
    handle: Option<thread::JoinHandle<Result<(), keld_ipc::IpcError>>>,
}

impl EchoServer {
    /// Binds the endpoint and starts accepting one echo session on a worker thread.
    ///
    /// `ready` fires after the listener is bound (safe for clients to connect).
    #[must_use]
    #[allow(clippy::expect_used)] // ephemeral loopback bind is a CLI process invariant
    pub fn start(ready: mpsc::Sender<()>) -> Self {
        #[cfg(unix)]
        {
            let endpoint = EchoEndpoint::Unix(std::env::temp_dir().join(format!(
                "keld-echo-{}-{}.sock",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            )));
            let endpoint_for_thread = endpoint.clone();
            let handle = thread::spawn(move || serve_unix(endpoint_for_thread, &ready));
            Self {
                endpoint,
                handle: Some(handle),
            }
        }
        #[cfg(windows)]
        {
            let listener =
                std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback for echo server");
            let port = listener.local_addr().expect("local addr").port();
            let handle = thread::spawn(move || serve_tcp(listener, &ready));
            Self {
                endpoint: EchoEndpoint::Tcp(port),
                handle: Some(handle),
            }
        }
    }

    /// App-link path/port for clients.
    #[must_use]
    pub fn link(&self) -> String {
        self.endpoint.link_env()
    }

    /// Waits for the server thread and removes any Unix socket file.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the session failed.
    ///
    /// # Panics
    ///
    /// If the server thread panicked (should not happen in normal operation).
    pub fn join(mut self) -> io::Result<()> {
        if let Some(handle) = self.handle.take() {
            #[allow(clippy::expect_used)]
            let result = handle.join().expect("echo server thread");
            if let Err(err) = result {
                self.cleanup_socket();
                return Err(io::Error::other(err.to_string()));
            }
        }
        self.cleanup_socket();
        Ok(())
    }

    fn cleanup_socket(&self) {
        #[cfg(unix)]
        {
            let EchoEndpoint::Unix(path) = &self.endpoint;
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for EchoServer {
    fn drop(&mut self) {
        if self.handle.take().is_some() {
            self.cleanup_socket();
        }
    }
}

#[cfg(unix)]
fn serve_unix(endpoint: EchoEndpoint, ready: &mpsc::Sender<()>) -> Result<(), keld_ipc::IpcError> {
    let EchoEndpoint::Unix(path) = endpoint;
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path)?;
    ready.send(()).ok();
    let (mut stream, _) = listener.accept()?;
    serve_echo_session(&mut stream)
}

#[cfg(windows)]
fn serve_tcp(
    listener: std::net::TcpListener,
    ready: &mpsc::Sender<()>,
) -> Result<(), keld_ipc::IpcError> {
    ready.send(()).ok();
    let (mut stream, _) = listener.accept()?;
    serve_echo_session(&mut stream)
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
    #[cfg(unix)]
    {
        let mut stream = std::os::unix::net::UnixStream::connect(link)?;
        echo_call(&mut stream, request)
    }
    #[cfg(windows)]
    {
        let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{link}"))?;
        echo_call(&mut stream, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_socket_is_ipc_001() {
        #[cfg(unix)]
        let link = format!(
            "/no/such/keld-echo-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        #[cfg(windows)]
        let link = "1"; // port 1: connection refused on loopback
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
    }

    #[test]
    fn roundtrip_over_loopback_copies_fields() {
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = EchoServer::start(ready_tx);
        ready_rx.recv().expect("server ready");
        let req = EchoRequest {
            message: "cli-loopback".to_owned(),
            count: 11,
        };
        let response = echo_roundtrip(&server.link(), &req).expect("echo");
        server.join().expect("join");
        assert_eq!(response.message, "cli-loopback");
        assert_eq!(response.count, 11);
    }
}
