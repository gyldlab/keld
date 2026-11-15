//! One bounded HTTP request decoder for the blocking negative-control reader
//! and the independent, nonblocking multi-peer fixture observer.
use crate::support::EVENT_DEADLINE;
use std::io::Read;
use std::net::TcpStream;
use std::time::Instant;

const MAX_REQUEST_BYTES: usize = 16 * 1024;

pub(super) enum RequestPoll {
    Pending,
    Closed,
    Ready(String, Vec<u8>),
}

/// An incomplete TCP request cannot monopolize the observer's Quit command
/// lane. Each accepted peer retains its own incremental decoder and deadline.
pub(super) struct PendingHttpRequest {
    stream: TcpStream,
    bytes: Vec<u8>,
    deadline: Instant,
}

impl PendingHttpRequest {
    pub(super) fn new(stream: TcpStream) -> Self {
        stream
            .set_nonblocking(true)
            .expect("responsive native observation peer");
        Self {
            stream,
            bytes: Vec::new(),
            deadline: Instant::now() + EVENT_DEADLINE,
        }
    }

    pub(super) fn poll(&mut self) -> RequestPoll {
        if Instant::now() >= self.deadline {
            // Neither a silent keep-alive nor a trickle of partial bytes may
            // extend a single observation beyond the existing deadline.
            assert!(
                self.bytes.is_empty(),
                "partial HTTP request exceeded the existing observer deadline"
            );
            return RequestPoll::Closed;
        }
        let mut chunk = [0_u8; 1024];
        match self.stream.read(&mut chunk) {
            Ok(0) if self.bytes.is_empty() => RequestPoll::Closed,
            Ok(0) => panic!(
                "observer peer closed after {} bytes before complete request",
                self.bytes.len()
            ),
            Ok(count) => {
                self.bytes.extend_from_slice(&chunk[..count]);
                match decode_complete_request(&self.bytes) {
                    Some((path, body)) => RequestPoll::Ready(path, body),
                    None => RequestPoll::Pending,
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                RequestPoll::Pending
            }
            Err(error) => panic!("poll native observer peer: {error}"),
        }
    }

    pub(super) fn into_stream(self) -> TcpStream {
        prepare_stream(&self.stream);
        self.stream
    }
}

pub(super) fn prepare_stream(stream: &TcpStream) {
    // Direct reader tests use blocking sockets, and completed async requests
    // use bounded blocking writes. Never block on a partial accepted request.
    stream
        .set_nonblocking(false)
        .expect("normalize accepted observation stream");
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("observer read kill switch");
    stream
        .set_write_timeout(Some(EVENT_DEADLINE))
        .expect("observer write kill switch");
}

pub(super) fn read_observation(stream: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    read_observation_observing(stream, |_| {})
}

pub(super) fn read_observation_observing(
    stream: &mut TcpStream,
    mut observe: impl FnMut(usize),
) -> Option<(String, Vec<u8>)> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        if let Some(complete) = decode_complete_request(&bytes) {
            return Some(complete);
        }
        let count = stream
            .read(&mut chunk)
            .expect("bounded observation request");
        if count == 0 && bytes.is_empty() {
            return None;
        }
        assert_ne!(
            count,
            0,
            "observer peer closed after {} bytes before complete request",
            bytes.len()
        );
        bytes.extend_from_slice(&chunk[..count]);
        assert!(
            bytes.len() <= MAX_REQUEST_BYTES,
            "observation request oversized"
        );
        observe(bytes.len());
    }
}

fn decode_complete_request(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
    assert!(
        bytes.len() <= MAX_REQUEST_BYTES,
        "observation request oversized"
    );
    let end = bytes.windows(4).position(|part| part == b"\r\n\r\n")? + 4;
    let header = String::from_utf8(bytes[..end].to_vec()).expect("HTTP header UTF-8");
    let length = header
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map_or(0, |(_, value)| {
            value.trim().parse::<usize>().expect("body length")
        });
    assert!(
        end + length <= MAX_REQUEST_BYTES,
        "declared HTTP body oversized"
    );
    if bytes.len() < end + length {
        return None;
    }
    let path = header
        .lines()
        .next()
        .expect("request line")
        .split_whitespace()
        .nth(1)
        .expect("request target")
        .to_owned();
    Some((path, bytes[end..end + length].to_vec()))
}
