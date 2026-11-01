//! Owns the responsive HTTP observation thread and its finite peer lifetimes.
use super::http::{PendingHttpRequest, RequestPoll};
use super::recovery::{RecoveryCommand, RecoveryObserver};
use super::{respond, respond_html};
use crate::support::EVENT_DEADLINE;
use serde_json::Value;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

/// HTTP observes ordinary app/page data; the retained Quit stream owns no authority.
pub(in crate::public_fs) struct Observation {
    pub(in crate::public_fs) port: u16,
    pub(in crate::public_fs) reports: mpsc::Receiver<Value>,
    pub(in crate::public_fs) quit: mpsc::Sender<()>,
    /// Fixed test-only receipt: 0=no HTTP /quit, 1=received, 2=replied.
    /// No address, token, body, or real runtime protocol crosses this flag.
    pub(in crate::public_fs) quit_progress: Arc<AtomicU8>,
    recovery_commands: Option<mpsc::Sender<RecoveryCommand>>,
    page: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Observation {
    pub(in crate::public_fs) fn await_page_result(&self, journal: &mut Vec<Value>) {
        let deadline = std::time::Instant::now() + EVENT_DEADLINE;
        let mut started = false;
        loop {
            let report = self
                .reports
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .expect("actual page FS outcomes");
            eprintln!("KELD_KEL140_OBSERVATION {report}");
            let click_start = report["phase"] == "page-click-start";
            journal.push(report);
            if click_start && !started {
                started = true;
                continue;
            }
            // Existing exact page-result/effect assertions reject error,
            // duplicate click-start and every other unexpected final phase.
            return;
        }
    }

    pub(in crate::public_fs) fn bind() -> Self {
        Self::bind_with_recovery(false, false)
    }

    pub(in crate::public_fs) fn bind_recovery() -> Self {
        Self::bind_with_recovery(true, false)
    }

    pub(in crate::public_fs) fn bind_with_accept_receipts()
    -> (Self, mpsc::Receiver<std::net::SocketAddr>) {
        let (send, receives) = mpsc::channel();
        (
            Self::bind_with_recovery_and_receipts(false, false, Some(send), None),
            receives,
        )
    }

    /// The real app awaits its observation POST response before sending /quit.
    /// Withhold that response only in the adversarial native test until its
    /// independent idle peer has been *accepted*, then release app execution.
    pub(in crate::public_fs) fn bind_with_app_result_gate()
    -> (Self, mpsc::Receiver<std::net::SocketAddr>, mpsc::Sender<()>) {
        let (receipt_tx, receipt_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        (
            Self::bind_with_recovery_and_receipts(false, false, Some(receipt_tx), Some(release_rx)),
            receipt_rx,
            release_tx,
        )
    }

    /// Core/Runtime own Q/EOF; their G1/G2 ordinary app Quit waits park here.
    pub(in crate::public_fs) fn bind_staged_fixture() -> Self {
        Self::bind_with_recovery(false, true)
    }

    /// Serve `html` at `/successor-page`, the target of a page navigation.
    pub(in crate::public_fs) fn serve_page(&self, html: String) {
        *self
            .page
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(html);
    }

    pub(in crate::public_fs) fn send_recovery_command(&self, command: RecoveryCommand) {
        self.recovery_commands
            .as_ref()
            .expect("recovery observer owns these fixture commands")
            .send(command)
            .expect("recovery observer remains live");
    }

    fn bind_with_recovery(recovery_mode: bool, retain_fixture_quits: bool) -> Self {
        Self::bind_with_recovery_and_receipts(recovery_mode, retain_fixture_quits, None, None)
    }

    fn bind_with_recovery_and_receipts(
        recovery_mode: bool,
        retain_fixture_quits: bool,
        accept_receipts: Option<mpsc::Sender<std::net::SocketAddr>>,
        app_result_ack_release: Option<mpsc::Receiver<()>>,
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("prearm public FS observer");
        let port = listener.local_addr().expect("observer address").port();
        listener
            .set_nonblocking(true)
            .expect("responsive observer accept loop");
        let (send, reports) = mpsc::channel();
        let (quit, commands) = mpsc::channel();
        let (recovery_commands, recovery) = if recovery_mode {
            let (send, receive) = mpsc::channel();
            (Some(send), Some(RecoveryObserver::new(receive)))
        } else {
            (None, None)
        };
        let stop = Arc::new(AtomicBool::new(false));
        let quit_progress = Arc::new(AtomicU8::new(0));
        let page = Arc::new(Mutex::new(None));
        let server = ObserverServer {
            page: Arc::clone(&page),
            listener,
            send,
            commands,
            recovery,
            stopping: Arc::clone(&stop),
            quit_progress: Arc::clone(&quit_progress),
            accept_receipts,
            app_result_ack_release,
            retain_fixture_quits,
            pending_quit: None,
            fixture_quits: Vec::new(),
            released: false,
            held_app_result_reply: None,
            peers: Vec::new(),
        };
        let thread = thread::spawn(move || server.run());
        Self {
            port,
            reports,
            quit,
            quit_progress,
            recovery_commands,
            page,
            stop,
            thread: Some(thread),
        }
    }
}

impl Observation {
    /// Stop and join the sole HTTP owner before observing its final report queue.
    pub(in crate::public_fs) fn stop_and_join(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.expect("observer owns/join its worker");
            }
        }
    }
}

impl Drop for Observation {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Sole mutable owner of the test HTTP socket set and control/recovery signals.
/// Pending requests never block another socket or the release of a Quit barrier.
struct ObserverServer {
    page: Arc<Mutex<Option<String>>>,
    listener: TcpListener,
    send: mpsc::Sender<Value>,
    commands: mpsc::Receiver<()>,
    recovery: Option<RecoveryObserver>,
    stopping: Arc<AtomicBool>,
    quit_progress: Arc<AtomicU8>,
    accept_receipts: Option<mpsc::Sender<SocketAddr>>,
    app_result_ack_release: Option<mpsc::Receiver<()>>,
    retain_fixture_quits: bool,
    pending_quit: Option<TcpStream>,
    fixture_quits: Vec<TcpStream>,
    released: bool,
    held_app_result_reply: Option<TcpStream>,
    peers: Vec<PendingHttpRequest>,
}

impl ObserverServer {
    fn run(mut self) {
        while !self.stopping.load(Ordering::Acquire) {
            self.poll_controls();
            self.poll_peers();
            self.accept_one();
        }
    }

    fn poll_controls(&mut self) {
        if let Some(recovery) = self.recovery.as_mut() {
            recovery.poll();
        }
        self.released |= self.commands.try_recv().is_ok();
        if self
            .app_result_ack_release
            .as_ref()
            .is_some_and(|release| release.try_recv().is_ok())
        {
            let mut stream = self
                .held_app_result_reply
                .take()
                .expect("only a held app-result HTTP response can be released");
            respond(&mut stream, "recorded");
        }
        if self.released
            && let Some(mut stream) = self.pending_quit.take()
        {
            respond(&mut stream, "quit");
            self.quit_progress.store(2, Ordering::Release);
        }
    }

    fn poll_peers(&mut self) {
        // One silent keep-alive or split request must not prevent Quit,
        // another report, or the independent recovery command lane.
        let mut index = 0;
        while index < self.peers.len() {
            let (path, body) = match self.peers[index].poll() {
                RequestPoll::Pending => {
                    index += 1;
                    continue;
                }
                RequestPoll::Closed => {
                    self.peers.remove(index);
                    continue;
                }
                RequestPoll::Ready(path, body) => (path, body),
            };
            // Preserve accept order among peers that complete in this pass.
            let stream = self.peers.remove(index).into_stream();
            self.route_request(&path, &body, stream);
        }
    }

    fn route_request(&mut self, path: &str, body: &[u8], mut stream: TcpStream) {
        match path {
            "/observe" => {
                let report: Value = serde_json::from_slice(body).expect("actual app/page JSON");
                if self.app_result_ack_release.is_some() && report["phase"] == "app-result" {
                    assert!(
                        self.held_app_result_reply.is_none(),
                        "one gated app-result acknowledgement"
                    );
                    self.held_app_result_reply = Some(stream);
                    self.send
                        .send(report)
                        .expect("gated result reached independent test");
                } else {
                    respond(&mut stream, "recorded");
                    self.send.send(report).expect("report observed data");
                }
            }
            "/successor-page" => {
                let page = self
                    .page
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone()
                    .expect("a test served a successor page");
                respond_html(&mut stream, &page);
            }
            "/quit" if self.retain_fixture_quits => {
                self.quit_progress.store(1, Ordering::Release);
                assert!(
                    self.fixture_quits.len() < 2,
                    "only G1/G2 fixture Quit waits"
                );
                self.fixture_quits.push(stream);
            }
            "/quit" => {
                self.quit_progress.store(1, Ordering::Release);
                assert!(self.pending_quit.is_none(), "one public app Quit barrier");
                if self.released {
                    respond(&mut stream, "quit");
                    self.quit_progress.store(2, Ordering::Release);
                } else {
                    self.pending_quit = Some(stream);
                }
            }
            _ => match self.recovery.as_mut() {
                Some(recovery) => {
                    if let Some(report) = recovery.accept_wait(path, stream) {
                        self.send.send(report).expect("pending handler observation");
                    }
                }
                None => panic!("unexpected public FS observation route {path}"),
            },
        }
    }

    fn accept_one(&mut self) {
        match self.listener.accept() {
            Ok((stream, peer)) => {
                if let Some(receipt) = &self.accept_receipts {
                    receipt
                        .send(peer)
                        .expect("native observer peer receipt owner");
                }
                assert!(
                    self.peers.len() < 32,
                    "native fixture cannot retain an unbounded number of HTTP peers"
                );
                self.peers.push(PendingHttpRequest::new(stream));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::yield_now();
            }
            Err(error) => panic!("public FS observer accept: {error}"),
        }
    }
}
