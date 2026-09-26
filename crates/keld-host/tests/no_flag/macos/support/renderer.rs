use crate::support::EVENT_DEADLINE;
use crate::support::MARKER;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::thread::JoinHandle;

pub(crate) struct NavigationBlocker {
    pub(crate) port: u16,
    pub(crate) connected: Receiver<()>,
    pub(crate) release: mpsc::Sender<()>,
    pub(crate) handle: JoinHandle<()>,
}

impl NavigationBlocker {
    pub(crate) fn bind() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind navigation blocker");
        let port = listener.local_addr().expect("blocker address").port();
        let (connected_tx, connected) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept blocked navigation");
            connected_tx.send(()).expect("report blocked navigation");
            release_rx.recv().expect("release blocked navigation");
        });
        Self {
            port,
            connected,
            release,
            handle,
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
            assert!(!marker.is_empty());
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
