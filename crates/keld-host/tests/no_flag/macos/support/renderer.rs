use crate::support::EVENT_DEADLINE;
use crate::support::MARKER;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::thread::JoinHandle;

pub(crate) struct NavigationBlocker {
    pub(crate) port: u16,
    pub(crate) connected: Receiver<()>,
    release: mpsc::Sender<()>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl NavigationBlocker {
    pub(crate) fn bind() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind navigation blocker");
        listener
            .set_nonblocking(true)
            .expect("nonblocking navigation fixture accept");
        let port = listener.local_addr().expect("blocker address").port();
        let (connected_tx, connected) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let _stream = loop {
                // Cancellation belongs to NavigationBlocker::Drop; the test's
                // existing receive deadline bounds its observation. Do not
                // add a second clock starting before Host spawn.
                if stopped.load(Ordering::Acquire) {
                    return;
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::park_timeout(std::time::Duration::from_millis(1));
                    }
                    Err(error) => panic!("navigation fixture accept failed: {:?}", error.kind()),
                }
            };
            if connected_tx.send(()).is_ok() {
                // The test controls release, even if the Host has already
                // exited. Drop sends release before joining this worker.
                let _ = release_rx.recv();
            }
        });
        Self {
            port,
            connected,
            stop,
            release,
            handle: Some(handle),
        }
    }

    /// Joins the only accepting worker on the success path as well as unwind.
    /// Callers preserve their original failure before inspecting this result.
    pub(crate) fn stop_and_join(&mut self) -> thread::Result<()> {
        self.stop.store(true, Ordering::Release);
        let _ = self.release.send(());
        self.handle.take().map_or(Ok(()), JoinHandle::join)
    }
}

impl Drop for NavigationBlocker {
    fn drop(&mut self) {
        let result = self.stop_and_join();
        if !thread::panicking() {
            result.expect("navigation blocker joins on drop");
        }
    }
}

pub(crate) struct Beacon {
    pub(crate) port: u16,
    pub(crate) request: Receiver<Vec<u8>>,
    pub(crate) handle: Option<JoinHandle<()>>,
}

impl Beacon {
    pub(crate) fn bind(marker: &'static str) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind renderer beacon");
        let port = listener.local_addr().expect("beacon address").port();
        let (request_tx, request) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept renderer beacon");
            stream
                .set_read_timeout(Some(EVENT_DEADLINE))
                .expect("beacon read deadline");
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 1024];
            while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).expect("read renderer beacon");
                if read == 0 {
                    request_tx
                        .send(bytes)
                        .expect("report closed renderer beacon");
                    return;
                }
                bytes.extend_from_slice(&chunk[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .expect("respond renderer beacon");
            request_tx.send(bytes).expect("report renderer request");
            assert_ne!(marker, "");
        });
        Self {
            port,
            request,
            handle: Some(handle),
        }
    }

    pub(crate) const fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn assert_exact(mut self) {
        let request = self
            .request
            .recv_timeout(EVENT_DEADLINE)
            .expect("WKWebView did not render the exact fixture beacon");
        let request = String::from_utf8_lossy(&request);
        assert!(request.starts_with(&format!("GET /{MARKER} ")), "{request}");
        self.handle
            .take()
            .expect("beacon thread")
            .join()
            .expect("beacon thread joins");
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = handle.join();
        }
    }
}
