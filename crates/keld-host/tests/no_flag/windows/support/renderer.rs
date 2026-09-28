//! Renderer beacon observation, deadline activation and explicit worker completion.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::windows_renderer_http::{
    PendingRendererRequest, RendererRequestRead, accept_renderer_connection,
    read_renderer_request_line,
};
use crate::{PRODUCT_DEADLINE, RENDERER_ACCEPT_POLL};

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

pub(crate) fn expect_renderer_beacon(beacon: RendererBeacon, context: &str) {
    let RendererBeacon {
        observed,
        worker,
        activate_deadline,
    } = beacon;
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    let _ = activate_deadline.send(deadline);
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .unwrap_or_default();
    let initial_result = observed.recv_timeout(remaining);
    finish_renderer_beacon(&observed, worker, initial_result, context)
        .unwrap_or_else(|error| panic!("{error}"));
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
            .map(|deadline| renderer_beacon_remaining(deadline, now()))
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
                        .map(|deadline| renderer_beacon_remaining(deadline, now()))
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

pub(crate) fn renderer_beacon_remaining(
    deadline: Instant,
    now: Instant,
) -> Result<Duration, String> {
    deadline
        .checked_duration_since(now)
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| "renderer beacon deadline elapsed before the exact request".to_owned())
}
