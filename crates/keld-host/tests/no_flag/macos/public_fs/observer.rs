use crate::support::EVENT_DEADLINE;
use crate::support::control::{O_NONBLOCK, accept_before, file_status_flags};
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;

#[path = "observer/http.rs"]
mod http;
#[path = "observer/recovery.rs"]
mod recovery;
#[path = "observer/server.rs"]
mod server;
use http::{prepare_stream, read_observation, read_observation_observing};
pub(super) use recovery::{RecoveryCommand, RecoveryPhase};
pub(super) use server::Observation;

#[test]
fn idle_accepted_http_peer_cannot_block_a_released_public_quit() {
    // Observe the fixture's actual accept event, not merely a successful
    // connect: the regression requires an accepted but byte-silent peer.
    let (observer, accepts) = Observation::bind_with_accept_receipts();
    assert_eq!(
        observer
            .quit_progress
            .load(std::sync::atomic::Ordering::Acquire),
        0,
        "an idle accepted HTTP peer must not fabricate a public Quit receipt"
    );
    let address = ("127.0.0.1", observer.port);
    let idle = TcpStream::connect(address).expect("first real idle peer");
    assert_eq!(
        accepts
            .recv_timeout(EVENT_DEADLINE)
            .expect("observer accepted idle peer"),
        idle.local_addr().expect("owned idle peer address")
    );

    let (outcome_tx, outcome_rx) = mpsc::channel();
    let quit_client = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).expect("independent public Quit peer");
        stream
            .set_read_timeout(Some(EVENT_DEADLINE / 4))
            .expect("bounded quit response");
        stream
            .write_all(b"GET /quit HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("real completed Quit request");
        let mut response = Vec::new();
        let result = stream.read_to_end(&mut response);
        outcome_tx
            .send((result.is_ok(), response))
            .expect("Quit result observation");
        // On the original blocked implementation, keep this peer available
        // for the observer's eventual response during failure cleanup.
        let mut rest = Vec::new();
        let _ = stream.read_to_end(&mut rest);
    });
    observer
        .quit
        .send(())
        .expect("release actual public Quit barrier");
    let result = outcome_rx.recv_timeout(EVENT_DEADLINE / 2);
    // Keep cleanup independent of the assertion: the retained idle peer must
    // never keep a test-owned observation thread alive after a failed oracle.
    drop(idle);
    quit_client.join().expect("Quit request client cleanup");
    let (completed, response) = result.expect("bounded independent Quit response");
    assert_eq!(
        observer
            .quit_progress
            .load(std::sync::atomic::Ordering::Acquire),
        2,
        "the fixed redacted receipt must require a real replied HTTP Quit"
    );
    assert!(completed, "idle peer starved released Quit");
    assert!(
        response.ends_with(b"\r\n\r\nquit"),
        "actual released HTTP Quit response was not delivered"
    );
    drop(observer);
}

#[test]
fn received_public_quit_has_no_reply_until_the_independent_release() {
    let observer = Observation::bind();
    let mut stream = TcpStream::connect(("127.0.0.1", observer.port))
        .expect("real independent public Quit peer");
    stream
        .write_all(b"GET /quit HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("complete native HTTP Quit request");
    let deadline = std::time::Instant::now() + EVENT_DEADLINE / 4;
    loop {
        let stage = observer
            .quit_progress
            .load(std::sync::atomic::Ordering::Acquire);
        if stage == 1 {
            break;
        }
        assert_eq!(stage, 0, "an unreleased HTTP Quit must not be answered");
        assert!(
            std::time::Instant::now() < deadline,
            "observer never recorded the real pending HTTP Quit request"
        );
        thread::yield_now();
    }
    stream
        .set_nonblocking(true)
        .expect("nonblocking check before independent release");
    let mut one = [0_u8; 1];
    assert!(
        matches!(stream.read(&mut one), Err(error)
            if error.kind() == std::io::ErrorKind::WouldBlock),
        "receipt 1 means request seen, not an HTTP response received"
    );
    prepare_stream(&stream);
    observer.quit.send(()).expect("release native Quit");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("bounded HTTP response after release");
    assert!(
        response.ends_with(b"\r\n\r\nquit"),
        "observer must reply with its exact public Quit body"
    );
    assert_eq!(
        observer
            .quit_progress
            .load(std::sync::atomic::Ordering::Acquire),
        2,
        "the final receipt requires the actual written HTTP Quit response"
    );
}

#[test]
fn split_report_body_does_not_block_quit_and_remains_byte_exact() {
    let (observer, accepts) = Observation::bind_with_accept_receipts();
    let address = ("127.0.0.1", observer.port);
    let mut held = TcpStream::connect(address).expect("real split report connection");
    assert_eq!(
        accepts
            .recv_timeout(EVENT_DEADLINE)
            .expect("report peer accepted"),
        held.local_addr().expect("report peer address")
    );
    let report = serde_json::json!({"phase":"split-peer","bytes":[0,255,65]});
    let body = serde_json::to_vec(&report).expect("real binary-safe report");
    write!(
        held,
        "POST /observe HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .expect("complete request headers");
    held.write_all(&body[..body.len() / 2])
        .expect("first half of body");

    let mut quit = TcpStream::connect(address).expect("independent Quit connection");
    prepare_stream(&quit);
    quit.write_all(b"GET /quit HTTP/1.1\r\n\r\n")
        .expect("complete Quit request");
    observer.quit.send(()).expect("release actual Quit");
    let mut reply = Vec::new();
    let result = quit.read_to_end(&mut reply);
    // The report remains owned independently and can complete after Quit.
    held.write_all(&body[body.len() / 2..])
        .expect("finish split report body");
    let mut report_reply = Vec::new();
    prepare_stream(&held);
    held.read_to_end(&mut report_reply)
        .expect("complete split report reply");
    assert!(result.is_ok(), "partial report starved an independent Quit");
    assert!(reply.ends_with(b"\r\n\r\nquit"));
    assert!(report_reply.ends_with(b"\r\n\r\nrecorded"));
    assert_eq!(
        observer
            .reports
            .recv_timeout(EVENT_DEADLINE)
            .expect("exact report"),
        report
    );
    drop(observer);
}
fn respond(stream: &mut TcpStream, body: &str) {
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}", body.len()).expect("respond to observation");
}

#[test]
fn accepted_observation_stream_preserves_split_header_and_body() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("real split-request listener");
    listener
        .set_nonblocking(true)
        .expect("same observer accept mode");
    // One original deadline covers connect and accept: a completed connect
    // does not mean the nonblocking listener can already dequeue the peer.
    let deadline = std::time::Instant::now() + EVENT_DEADLINE;
    let mut client = TcpStream::connect_timeout(
        &listener.local_addr().expect("listener address"),
        EVENT_DEADLINE,
    )
    .expect("real TCP client");
    let mut accepted = accept_before(&listener, deadline);
    // Force the failing starting state even on platforms that reset flags at accept.
    accepted
        .set_nonblocking(true)
        .expect("nonblocking accepted fixture");
    // Darwin SDK 26.5 sys/fcntl.h: O_NONBLOCK=4. Observe the kernel flag,
    // independently of the normalizer and before any scheduling/traffic race.
    assert_ne!(file_status_flags(&accepted) & O_NONBLOCK, 0);
    prepare_stream(&accepted);
    assert_eq!(
        file_status_flags(&accepted) & O_NONBLOCK,
        0,
        "accepted request reader stayed nonblocking"
    );
    prepare_stream(&client);
    let body = br#"{"phase":"split","bytes":[0,255]}"#;
    let prefix = b"POST /observe HTTP/1.1\r\nContent-Len";
    let remaining_header = format!("gth: {}\r\n\r\n", body.len());
    let (progress, observed) = mpsc::channel();
    thread::scope(|scope| {
        let writer = scope.spawn(move || {
            client
                .write_all(prefix)
                .expect("first real header fragment");
            await_byte_count(&observed, prefix.len());
            client
                .write_all(remaining_header.as_bytes())
                .expect("remaining header bytes");
            client
                .write_all(&body[..4])
                .expect("partial actual JSON body");
            await_byte_count(&observed, prefix.len() + remaining_header.len() + 4);
            client
                .write_all(&body[4..])
                .expect("remaining actual JSON body");
            await_byte_count(
                &observed,
                prefix.len() + remaining_header.len() + body.len(),
            );
        });
        let (path, actual) = read_observation_observing(&mut accepted, move |count| {
            progress
                .send(count)
                .expect("actual consumed TCP byte count");
        })
        .expect("complete split observation request");
        assert_eq!(path, "/observe");
        assert_eq!(actual, body);
        writer.join().expect("split-request writer completes");
    });
}

fn await_byte_count(observed: &mpsc::Receiver<usize>, expected: usize) {
    let deadline = std::time::Instant::now() + EVENT_DEADLINE;
    loop {
        let actual = observed
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .expect("reader consumed the released fragment");
        assert!(actual <= expected, "unreleased fragment reached the reader");
        if actual == expected {
            return;
        }
    }
}

#[test]
fn empty_observation_connection_is_not_request_or_worker_failure() {
    let observation = Observation::bind();
    let address = ("127.0.0.1", observation.port);
    let empty = TcpStream::connect(address).expect("real empty TCP connection");
    empty
        .shutdown(std::net::Shutdown::Write)
        .expect("clean EOF before any request byte");
    drop(empty);

    let report = serde_json::json!({"phase":"after-empty-connection","bytes":[0,255]});
    let body = serde_json::to_vec(&report).expect("actual report bytes");
    let mut client = TcpStream::connect(address).expect("next real observer connection");
    prepare_stream(&client);
    write!(
        client,
        "POST /observe HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .expect("actual next request header");
    client.write_all(&body).expect("actual next request body");
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("complete request EOF");
    let mut response = Vec::new();
    client
        .read_to_end(&mut response)
        .expect("bounded exact response");
    assert_eq!(
        response,
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 8\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n\r\nrecorded"
    );
    assert_eq!(
        observation
            .reports
            .recv_timeout(EVENT_DEADLINE)
            .expect("only complete report observed"),
        report
    );
    assert!(
        matches!(
            observation.reports.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ),
        "empty TCP connection became application evidence"
    );
    drop(observation); // Join the real listener owner; a panic cannot count as health.
}

#[test]
fn empty_accepted_observation_does_not_consume_request_bytes() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("own empty-request listener");
    let client = TcpStream::connect(listener.local_addr().expect("listener address"))
        .expect("actual zero-byte client");
    let (mut accepted, _) = listener.accept().expect("accepted exact empty peer");
    prepare_stream(&accepted);
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("clean zero-byte EOF");
    let mut consumed = Vec::new();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_observation_observing(&mut accepted, |count| consumed.push(count))
    }));
    assert!(
        outcome.is_ok(),
        "clean empty transport close killed request reader"
    );
    assert!(
        consumed.is_empty(),
        "empty transport produced request bytes"
    );
}

#[test]
fn nonempty_incomplete_malformed_and_oversized_observations_still_reject() {
    let oversized_header = vec![b'x'; 16 * 1024 + 1];
    let controls: &[(&str, &[u8])] = &[
        ("partial header", b"POST /observe HTTP/1.1\r\nContent-Len"),
        (
            "partial body",
            b"POST /observe HTTP/1.1\r\nContent-Length: 3\r\n\r\nab",
        ),
        ("invalid header UTF-8", b"\xff\r\n\r\n"),
        (
            "invalid content length",
            b"POST /observe HTTP/1.1\r\nContent-Length: nope\r\n\r\n",
        ),
        ("oversized header", &oversized_header),
        (
            "oversized declared body",
            b"POST /observe HTTP/1.1\r\nContent-Length: 16384\r\n\r\n",
        ),
    ];
    for (label, bytes) in controls {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("owned hostile request listener");
        let mut client = TcpStream::connect(listener.local_addr().expect("listener address"))
            .expect("actual hostile request peer");
        let (mut accepted, _) = listener.accept().expect("accepted exact hostile peer");
        prepare_stream(&accepted);
        prepare_stream(&client);
        thread::scope(|scope| {
            let writer = scope.spawn(move || {
                client.write_all(bytes).expect("bounded hostile bytes");
                client
                    .shutdown(std::net::Shutdown::Write)
                    .expect("terminal hostile EOF");
            });
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                read_observation(&mut accepted)
            }));
            writer.join().expect("hostile peer release/join");
            assert!(outcome.is_err(), "accepted or ignored {label}");
        });
    }
}
