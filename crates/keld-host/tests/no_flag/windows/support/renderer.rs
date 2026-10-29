//! Renderer beacon observation, deadline activation and explicit worker completion.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Child;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::support::control::exited_host_output;
use crate::windows_renderer_http::{
    PendingRendererRequest, RendererRequestRead, accept_renderer_connection,
    read_renderer_request_line,
};
use crate::{DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE, RENDERER_ACCEPT_POLL, startup_kill_switch};

pub(crate) struct RendererBeacon {
    observed: mpsc::Receiver<Result<(), String>>,
    worker: thread::JoinHandle<()>,
    activate_deadline: mpsc::Sender<Instant>,
}

pub(crate) fn spawn_renderer_beacon(listener: TcpListener) -> RendererBeacon {
    let (observed_tx, observed) = mpsc::channel();
    let (activate_deadline, deadline_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = serve_renderer_beacon_loop(&listener, &deadline_rx);
        let _ = observed_tx.send(result);
    });
    RendererBeacon {
        observed,
        worker,
        activate_deadline,
    }
}

/// Awaits the beacon of a renderer whose session is already `Ready`.
pub(crate) fn expect_renderer_beacon(beacon: RendererBeacon, context: &str) {
    try_expect_renderer_beacon(beacon, PRODUCT_DEADLINE, context)
        .unwrap_or_else(|error| panic!("{error}"));
}

/// Awaits the beacon under `kill_switch`: `PRODUCT_DEADLINE` once the session is
/// `Ready`, or `startup_kill_switch` while the beacon still waits on the startup.
pub(crate) fn try_expect_renderer_beacon(
    beacon: RendererBeacon,
    kill_switch: Duration,
    context: &str,
) -> Result<(), String> {
    let RendererBeacon {
        observed,
        worker,
        activate_deadline,
    } = beacon;
    let deadline = Instant::now() + kill_switch;
    let _ = activate_deadline.send(deadline);
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .unwrap_or_default();
    let initial_result = observed.recv_timeout(remaining);
    finish_renderer_beacon(&observed, worker, initial_result, context)
}

pub(crate) fn finish_renderer_beacon(
    observed: &mpsc::Receiver<Result<(), String>>,
    worker: thread::JoinHandle<()>,
    initial_result: Result<Result<(), String>, mpsc::RecvTimeoutError>,
    context: &str,
) -> Result<(), String> {
    worker
        .join()
        .map_err(|_| format!("{context}: beacon worker panicked"))?;
    let result = match initial_result {
        Ok(result) => result,
        Err(receive_error) => match observed.try_recv() {
            Ok(Err(error)) => Err(error),
            Ok(Ok(())) | Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {
                return Err(format!(
                    "{context}: beacon worker did not report: {receive_error}"
                ));
            }
        },
    };
    result.map_err(|error| format!("{context}: {error}"))
}

pub(crate) fn serve_renderer_beacon_until(
    listener: &TcpListener,
    deadline: Instant,
) -> Result<(), String> {
    let (deadline_tx, deadline_rx) = mpsc::channel();
    deadline_tx
        .send(deadline)
        .expect("activate fixed renderer beacon deadline");
    serve_renderer_beacon_loop(listener, &deadline_rx)
}

fn serve_renderer_beacon_loop(
    listener: &TcpListener,
    deadline_rx: &mpsc::Receiver<Instant>,
) -> Result<(), String> {
    serve_renderer_beacon_loop_with_clock(listener, deadline_rx, Instant::now)
}

pub(crate) fn serve_renderer_beacon_loop_with_clock(
    listener: &TcpListener,
    deadline_rx: &mpsc::Receiver<Instant>,
    mut now: impl FnMut() -> Instant,
) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("set renderer beacon listener nonblocking: {error}"))?;
    let mut pending = Vec::<PendingRendererRequest>::new();
    let mut deadline = None;

    loop {
        if deadline.is_none() {
            match deadline_rx.try_recv() {
                Ok(activated) => deadline = Some(activated),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("renderer beacon deadline owner ended before awaiting".to_owned());
                }
            }
        }
        let remaining = deadline
            .map(|deadline| renderer_beacon_remaining(deadline, now(), BeaconPhase::IntakeWait))
            .transpose()?;

        let mut index = 0;
        while index < pending.len() {
            let request = {
                let PendingRendererRequest { stream, request } = &mut pending[index];
                read_renderer_request_line(request, |buffer| stream.read(buffer))
            }?;
            match request {
                RendererRequestRead::Pending => index += 1,
                RendererRequestRead::Empty => {
                    pending.swap_remove(index);
                }
                RendererRequestRead::Complete(request) => {
                    let mut matched = pending.swap_remove(index);
                    if !request.starts_with(b"GET /ready.png ") {
                        return Err(format!(
                            "renderer requested an unexpected beacon: {}",
                            String::from_utf8_lossy(&request)
                        ));
                    }
                    matched.stream.set_nonblocking(false).map_err(|error| {
                        format!("set renderer beacon reply stream blocking: {error}")
                    })?;
                    let write_timeout = deadline
                        .map(|deadline| {
                            renderer_beacon_remaining(deadline, now(), BeaconPhase::ReplyWrite)
                        })
                        .transpose()?
                        .unwrap_or(PRODUCT_DEADLINE);
                    matched
                        .stream
                        .set_write_timeout(Some(write_timeout))
                        .map_err(|error| format!("set renderer beacon write deadline: {error}"))?;
                    matched
                        .stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .map_err(|error| format!("reply renderer beacon: {error}"))?;
                    return Ok(());
                }
            }
        }

        if accept_renderer_connection(listener, &mut pending, "renderer beacon")? {
            continue;
        }

        // This only backs off the nonblocking kernel poll; socket readiness
        // and the parent-activated absolute deadline remain the observables.
        thread::park_timeout(remaining.map_or(RENDERER_ACCEPT_POLL, |remaining| {
            remaining.min(RENDERER_ACCEPT_POLL)
        }));
    }
}

/// Which wait an absolute beacon deadline elapsed in; it owns the error wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BeaconPhase {
    /// Accepting and reading connections until the exact request arrives.
    IntakeWait,
    /// The exact request matched; writing its reply.
    ReplyWrite,
}

pub(crate) fn renderer_beacon_remaining(
    deadline: Instant,
    now: Instant,
    phase: BeaconPhase,
) -> Result<Duration, String> {
    deadline
        .checked_duration_since(now)
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            match phase {
                BeaconPhase::IntakeWait => {
                    "renderer beacon deadline elapsed before the exact request; phase=intake_wait"
                }
                BeaconPhase::ReplyWrite => {
                    "renderer beacon deadline elapsed before replying to the exact request; phase=reply_write"
                }
            }
            .to_owned()
        })
}

/// A renderer subresource connection, accepted and never answered. While it is
/// held the document's `onload` cannot run, and `WebView2` raises
/// `NavigationCompleted` (which drives `Ready`) only after `onload`, so the
/// initial window cannot become `Ready`. The listener lives until release, so a
/// request on a later connection waits unanswered too.
pub(crate) struct HeldRendererRequest {
    pub(crate) port: u16,
    connected: mpsc::Receiver<bool>,
    release: mpsc::Sender<()>,
    worker: thread::JoinHandle<()>,
}

impl HeldRendererRequest {
    pub(crate) fn bind() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind held renderer");
        let port = listener.local_addr().expect("held renderer address").port();
        let (connected_tx, connected) = mpsc::channel();
        let (release, release_rx) = mpsc::channel::<()>();
        let worker = thread::spawn(move || {
            let accepted = listener.accept();
            let _ = connected_tx.send(accepted.is_ok());
            let _ = release_rx.recv();
            drop((accepted, listener));
        });
        Self {
            port,
            connected,
            release,
            worker,
        }
    }

    /// Writes `project`'s renderer: the product page with one image on this
    /// held listener.
    pub(crate) fn write_index(&self, project: &Path) {
        fs::write(
            project.join("index.html"),
            format!(
                "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{}/held.png\">\n",
                self.port
            ),
        )
        .expect("write held renderer");
    }

    /// Waits for the renderer's first connection under `startup_kill_switch`,
    /// which bounds the `WebView2` startup that makes it, and reports the host's
    /// output at once if the host exits first. The connection proves the window
    /// exists and its first navigation is in flight.
    pub(crate) fn await_connection(&self, child: &mut Child) {
        let deadline = Instant::now() + startup_kill_switch();
        loop {
            match self.connected.recv_timeout(RENDERER_ACCEPT_POLL) {
                Ok(accepted) => {
                    assert!(accepted, "held renderer accept failed");
                    return;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => panic!("held renderer worker ended"),
            }
            if let Some(status) = child
                .try_wait()
                .expect("observe host before held connection")
            {
                panic!(
                    "host exited before its renderer connected to the held resource: {status}\n{}",
                    exited_host_output(child)
                );
            }
            assert!(
                Instant::now() < deadline,
                "renderer did not connect to the held resource"
            );
        }
    }

    /// Answers nothing: closes the held connection and joins the worker.
    pub(crate) fn release(self) {
        self.release.send(()).expect("release held renderer");
        self.worker.join().expect("held renderer worker joins");
    }
}
