//! Real local HTTP token peer and exact exchange-count owner for the SDK case.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub(super) struct SingleflightPeer {
    pub(super) endpoint: String,
    pub(super) post: std::sync::mpsc::Receiver<Vec<u8>>,
    release: std::sync::mpsc::SyncSender<()>,
    done: std::sync::mpsc::SyncSender<()>,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<usize>>,
}
impl SingleflightPeer {
    pub(super) fn start(deadline: Instant) -> Self {
        use crate::auth::macos_http::tests as http_fixture;
        use std::io::Write as _;
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("real local token peer");
        listener.set_nonblocking(true).expect("bounded listener");
        let endpoint = format!(
            "http://{}/token",
            listener.local_addr().expect("owned port")
        );
        let (observed, post) = mpsc::sync_channel(1);
        let (release, held) = mpsc::sync_channel(1);
        let (done, finished) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let server = std::thread::spawn(move || {
            let mut exchanges = 0;
            let mut peer = http_fixture::accept(&listener);
            let body = http_fixture::request_body(&mut peer);
            exchanges += 1; // Only complete independently parsed POSTs count.
            observed.send(body).expect("one actual POST receipt");
            assert!(Instant::now() < deadline, "same peer hold clock");
            held.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("bounded response hold");
            if stopping.load(Ordering::Acquire) {
                drop(peer);
                return exchanges;
            }
            let body = br#"{"access_token":"SDK_SINGLEFLIGHT_ACTUAL_TOKEN","token_type":"Bearer","expires_in":300,"refresh_token":"ROTATED_FIXTURE_PRIVATE"}"#;
            write!(
                peer,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("valid response metadata");
            peer.write_all(body).expect("actual complete token JSON");
            drop(peer);
            assert!(Instant::now() < deadline, "same peer finish clock");
            finished
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("bounded broker drain");
            loop {
                match listener.accept() {
                    Ok((peer, _)) => {
                        let mut peer = http_fixture::prepare_stream(peer);
                        let _ = http_fixture::request_body(&mut peer);
                        exchanges += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => panic!("final token peer census: {error}"),
                }
            }
            exchanges
        });
        Self {
            endpoint,
            post,
            release,
            done,
            stop,
            server: Some(server),
        }
    }

    pub(super) fn release(&self, abort: bool) {
        if abort {
            self.stop.store(true, Ordering::Release);
        }
        let _ = self.release.try_send(());
    }

    pub(super) fn join(&mut self, deadline: Instant) -> Result<usize, &'static str> {
        let _ = self.done.try_send(());
        while self
            .server
            .as_ref()
            .is_some_and(|server| !server.is_finished())
        {
            if Instant::now() >= deadline {
                return Err("peer join unproved by same clock");
            }
            std::thread::yield_now();
        }
        self.server
            .take()
            .ok_or("peer owner missing")?
            .join()
            .map_err(|_| "peer exited with fixture fault")
    }
}
