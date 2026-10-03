//! Observe the real renderer HTTP request before signaling readiness.

use super::PRODUCT_DEADLINE;
use std::{
    io::{Read as _, Write as _},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const RENDERER_REQUEST_LIMIT: usize = 2048;
const RENDERER_READ_CHUNK_BYTES: usize = 32;
const RENDERER_IO_POLL: Duration = Duration::from_millis(10);
const RENDERER_RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const RENDERER_CANCELLED: &str = "renderer beacon cancelled";

pub(crate) struct RendererBeacon {
    deadline: Option<mpsc::Sender<Instant>>,
    cancellation: mpsc::Sender<()>,
    observed: mpsc::Receiver<Result<(), String>>,
    worker: Option<JoinHandle<()>>,
}

pub(crate) fn spawn_renderer_beacon(listener: TcpListener) -> RendererBeacon {
    let (deadline_tx, deadline_rx) = mpsc::channel();
    let (cancellation, cancellation_rx) = mpsc::channel();
    let (observed_tx, observed) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = match deadline_rx.recv() {
            Ok(deadline) => serve_renderer_beacon_until(&listener, deadline, &cancellation_rx),
            Err(_) => Err("renderer beacon deadline owner ended before awaiting".to_owned()),
        };
        let _ = observed_tx.send(result);
    });

    RendererBeacon {
        deadline: Some(deadline_tx),
        cancellation,
        observed,
        worker: Some(worker),
    }
}

pub(crate) fn expect_renderer_beacon(beacon: RendererBeacon, context: &str) {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    expect_renderer_beacon_until(beacon, deadline, context)
        .unwrap_or_else(|error| panic!("{error}"));
}

fn expect_renderer_beacon_until(
    mut beacon: RendererBeacon,
    deadline: Instant,
    context: &str,
) -> Result<(), String> {
    let deadline_tx = beacon
        .deadline
        .take()
        .ok_or_else(|| format!("{context}: renderer beacon deadline already activated"))?;
    deadline_tx
        .send(deadline)
        .map_err(|_| format!("{context}: renderer beacon worker ended before activation"))?;
    drop(deadline_tx);

    let remaining = renderer_beacon_remaining(deadline, Instant::now())
        .map_err(|error| format!("{context}: {error}"))?;
    let initial_result = beacon.observed.recv_timeout(remaining);
    if initial_result.is_err() {
        let _ = beacon.cancellation.send(());
    }

    beacon
        .worker
        .take()
        .expect("renderer beacon worker")
        .join()
        .map_err(|_| format!("{context}: renderer beacon worker panicked"))?;

    let result = match initial_result {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Once the parent deadline expires, a worker result racing in after that
            // boundary cannot reclassify the timed-out observation as success.
            let _ = beacon.observed.try_recv();
            Err("renderer beacon deadline elapsed before the exact request".to_owned())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("renderer beacon worker disconnected before reporting".to_owned())
        }
    };

    result.map_err(|error| format!("{context}: {error}"))
}

impl Drop for RendererBeacon {
    fn drop(&mut self) {
        self.deadline.take();
        let _ = self.cancellation.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_renderer_beacon_until(
    listener: &TcpListener,
    deadline: Instant,
    cancellation: &mpsc::Receiver<()>,
) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("set renderer beacon listener nonblocking: {error}"))?;

    let (mut stream, _) = loop {
        check_renderer_cancellation(cancellation)?;
        renderer_beacon_remaining(deadline, Instant::now())?;
        match listener.accept() {
            Ok(connection) => break connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                wait_for_renderer_io(cancellation, deadline)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("accept renderer beacon: {error}")),
        }
    };

    stream
        .set_nonblocking(true)
        .map_err(|error| format!("set renderer beacon stream nonblocking: {error}"))?;

    let request = read_renderer_request(&mut stream, deadline, cancellation)?;
    validate_renderer_request(&request)?;
    write_renderer_response(&mut stream, deadline, cancellation)
}

fn read_renderer_request(
    stream: &mut TcpStream,
    deadline: Instant,
    cancellation: &mpsc::Receiver<()>,
) -> Result<Vec<u8>, String> {
    let mut request = Vec::with_capacity(512);
    let mut chunk = [0_u8; RENDERER_READ_CHUNK_BYTES];

    loop {
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(request);
        }
        if request.len() >= RENDERER_REQUEST_LIMIT {
            return Err(format!(
                "renderer beacon request exceeded {RENDERER_REQUEST_LIMIT} bytes"
            ));
        }

        check_renderer_cancellation(cancellation)?;
        renderer_beacon_remaining(deadline, Instant::now())?;

        match stream.read(&mut chunk) {
            Ok(0) => {
                return Err(
                    "renderer beacon request closed before complete HTTP headers".to_owned(),
                );
            }
            Ok(read) => {
                if request.len() + read > RENDERER_REQUEST_LIMIT {
                    return Err(format!(
                        "renderer beacon request exceeded {RENDERER_REQUEST_LIMIT} bytes"
                    ));
                }
                request.extend_from_slice(&chunk[..read]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                wait_for_renderer_io(cancellation, deadline)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("read renderer beacon: {error}")),
        }
    }
}

fn validate_renderer_request(request: &[u8]) -> Result<(), String> {
    let line_end = request
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or_else(|| {
            "renderer beacon request is missing its request-line terminator".to_owned()
        })?;
    let request_line = &request[..line_end];
    if matches!(
        request_line,
        b"GET /ready.png HTTP/1.1" | b"GET /ready.png HTTP/1.0"
    ) {
        return Ok(());
    }

    Err(format!(
        "renderer requested an unexpected beacon: {}",
        String::from_utf8_lossy(request_line)
    ))
}

fn write_renderer_response(
    stream: &mut TcpStream,
    deadline: Instant,
    cancellation: &mpsc::Receiver<()>,
) -> Result<(), String> {
    let mut written = 0;
    while written < RENDERER_RESPONSE.len() {
        check_renderer_cancellation(cancellation)?;
        renderer_beacon_remaining(deadline, Instant::now())?;

        match stream.write(&RENDERER_RESPONSE[written..]) {
            Ok(0) => return Err("renderer beacon response stream closed before reply".to_owned()),
            Ok(count) => written += count,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                wait_for_renderer_io(cancellation, deadline)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("reply renderer beacon: {error}")),
        }
    }
    Ok(())
}

fn check_renderer_cancellation(cancellation: &mpsc::Receiver<()>) -> Result<(), String> {
    match cancellation.try_recv() {
        Ok(()) => Err(RENDERER_CANCELLED.to_owned()),
        Err(mpsc::TryRecvError::Empty) => Ok(()),
        Err(mpsc::TryRecvError::Disconnected) => {
            Err("renderer beacon cancellation owner ended".to_owned())
        }
    }
}

fn wait_for_renderer_io(
    cancellation: &mpsc::Receiver<()>,
    deadline: Instant,
) -> Result<(), String> {
    let remaining = renderer_beacon_remaining(deadline, Instant::now())?;
    // This is the bounded wakeup for nonblocking socket progress and cancellation,
    // not a readiness sleep; the absolute deadline is never extended.
    let wait = remaining.min(RENDERER_IO_POLL);
    match cancellation.recv_timeout(wait) {
        Ok(()) => Err(RENDERER_CANCELLED.to_owned()),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(()),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("renderer beacon cancellation owner ended".to_owned())
        }
    }
}

fn renderer_beacon_remaining(deadline: Instant, now: Instant) -> Result<Duration, String> {
    deadline
        .checked_duration_since(now)
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| "renderer beacon deadline elapsed before the exact request".to_owned())
}

#[cfg(test)]
#[path = "renderer_tests.rs"]
mod tests;
