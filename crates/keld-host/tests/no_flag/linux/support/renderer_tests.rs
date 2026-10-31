//! Native real-socket regressions for Linux renderer beacon support.

use super::*;
use std::{
    net::{Shutdown, SocketAddr, TcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const TEST_DEADLINE: Duration = Duration::from_secs(1);
const SHORT_DEADLINE: Duration = Duration::from_millis(80);
const VALID_REQUEST: &[u8] =
    b"GET /ready.png HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";

struct TestServer {
    address: SocketAddr,
    cancellation: mpsc::Sender<()>,
    result: mpsc::Receiver<Result<(), String>>,
    worker: JoinHandle<()>,
}

impl TestServer {
    fn spawn(timeout: Duration) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind renderer test server");
        let address = listener.local_addr().expect("renderer test address");
        let (cancellation, cancellation_rx) = mpsc::channel();
        let (result_tx, result) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result =
                serve_renderer_beacon_until(&listener, Instant::now() + timeout, &cancellation_rx);
            result_tx
                .send(result)
                .expect("publish renderer test result");
        });
        Self {
            address,
            cancellation,
            result,
            worker,
        }
    }

    fn finish(self) -> Result<(), String> {
        let result = self
            .result
            .recv_timeout(TEST_DEADLINE)
            .expect("renderer test server must report");
        self.worker.join().expect("renderer test worker joins");
        result
    }
}

#[test]
fn valid_request_accumulates_across_multiple_reads_and_replies() {
    assert!(
        VALID_REQUEST.len() > RENDERER_READ_CHUNK_BYTES,
        "fixture must require multiple reads"
    );
    let server = TestServer::spawn(TEST_DEADLINE);
    let mut client = TcpStream::connect(server.address).expect("connect renderer test server");
    client
        .set_read_timeout(Some(TEST_DEADLINE))
        .expect("set response read deadline");
    client
        .write_all(VALID_REQUEST)
        .expect("write complete renderer request");

    let mut response = Vec::new();
    client
        .read_to_end(&mut response)
        .expect("read renderer response");
    assert_eq!(response, RENDERER_RESPONSE);
    assert_eq!(server.finish(), Ok(()));
}

#[test]
fn truncated_request_cannot_publish_readiness() {
    let server = TestServer::spawn(TEST_DEADLINE);
    let mut client = TcpStream::connect(server.address).expect("connect truncated beacon");
    client
        .write_all(b"GET /ready.png ")
        .expect("write truncated request");
    client
        .shutdown(Shutdown::Write)
        .expect("close truncated request body");

    let error = server.finish().expect_err("truncated request must fail");
    assert!(
        error.contains("closed before complete HTTP headers"),
        "{error}"
    );
}

#[test]
fn malformed_request_is_rejected() {
    let server = TestServer::spawn(TEST_DEADLINE);
    let mut client = TcpStream::connect(server.address).expect("connect malformed beacon");
    client
        .write_all(b"GET /wrong.png HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("write malformed request");

    let error = server.finish().expect_err("wrong path must fail");
    assert!(error.contains("unexpected beacon"), "{error}");
}

#[test]
fn over_limit_request_is_rejected() {
    let server = TestServer::spawn(TEST_DEADLINE);
    let mut client = TcpStream::connect(server.address).expect("connect over-limit beacon");
    let over_limit = vec![b'A'; RENDERER_REQUEST_LIMIT];
    let _ = client.write_all(&over_limit);

    let error = server.finish().expect_err("over-limit request must fail");
    assert!(error.contains("exceeded"), "{error}");
}

#[test]
fn accept_deadline_is_finite_without_a_client() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind idle beacon");
    let (_cancellation, cancellation_rx) = mpsc::channel();
    let error =
        serve_renderer_beacon_until(&listener, Instant::now() + SHORT_DEADLINE, &cancellation_rx)
            .expect_err("idle accept must hit its deadline");
    assert!(error.contains("deadline elapsed"), "{error}");
}

#[test]
fn read_deadline_is_finite_for_a_stalled_client() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind stalled beacon");
    let address = listener.local_addr().expect("stalled beacon address");
    let mut client = TcpStream::connect(address).expect("connect stalled beacon");
    client
        .write_all(b"GET /ready")
        .expect("write stalled request prefix");
    let (_cancellation, cancellation_rx) = mpsc::channel();

    let error =
        serve_renderer_beacon_until(&listener, Instant::now() + SHORT_DEADLINE, &cancellation_rx)
            .expect_err("stalled read must hit its deadline");
    assert!(error.contains("deadline elapsed"), "{error}");
}

#[test]
fn cancellation_terminates_a_waiting_accept() {
    let server = TestServer::spawn(Duration::from_secs(10));
    let address = server.address;
    server
        .cancellation
        .send(())
        .expect("cancel renderer server");

    let error = server.finish().expect_err("cancellation must fail");
    assert_eq!(error, RENDERER_CANCELLED);
    TcpListener::bind(address).expect("cancelled server releases listener");
}

#[test]
fn parent_timeout_cannot_be_reclassified_by_late_worker_success() {
    let (deadline_tx, deadline_rx) = mpsc::channel();
    let (cancellation, cancellation_rx) = mpsc::channel();
    let (observed_tx, observed) = mpsc::channel();
    let worker = thread::spawn(move || {
        deadline_rx.recv().expect("receive renderer deadline");
        cancellation_rx.recv().expect("receive parent cancellation");
        observed_tx
            .send(Ok(()))
            .expect("publish deliberately late success");
    });
    let beacon = RendererBeacon {
        deadline: Some(deadline_tx),
        cancellation,
        observed,
        worker: Some(worker),
    };

    let error =
        expect_renderer_beacon_until(beacon, Instant::now() + SHORT_DEADLINE, "late renderer")
            .expect_err("success after the parent deadline must stay failed");
    assert!(error.contains("deadline elapsed"), "{error}");
}

#[test]
fn dropping_an_unawaited_beacon_joins_and_releases_its_listener() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind owned beacon");
    let address = listener.local_addr().expect("owned beacon address");
    let beacon = spawn_renderer_beacon(listener);

    drop(beacon);

    TcpListener::bind(address).expect("dropped beacon releases listener after joining worker");
}

#[test]
fn activated_timeout_cancels_joins_and_releases_its_listener() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind timed beacon");
    let address = listener.local_addr().expect("timed beacon address");
    let beacon = spawn_renderer_beacon(listener);

    let error =
        expect_renderer_beacon_until(beacon, Instant::now() + SHORT_DEADLINE, "timed renderer")
            .expect_err("missing renderer must fail");
    assert!(error.contains("deadline elapsed"), "{error}");
    TcpListener::bind(address).expect("timed-out beacon releases listener after joining worker");
}

#[test]
fn rejected_request_does_not_poison_a_healthy_follow_up_on_the_same_listener() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind shared beacon listener");
    let address = listener.local_addr().expect("shared beacon address");
    // Queue both peers on the one listener before either observation runs, so the
    // follow-up shares the listener, its backlog and the rejected peer's aftermath.
    // The order is load-bearing: a loopback connect() returns only after the handshake
    // completes, and Linux accept() takes completed connections from the head of the
    // listen queue (FIFO), so the first observation accepts the bad peer. Were the
    // order ever reversed, the first observation would serve the healthy peer and its
    // `expect_err` would fail loudly; the row cannot pass vacuously.
    let mut bad_client = TcpStream::connect(address).expect("connect bad beacon");
    bad_client
        .write_all(b"GET /wrong.png HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("write bad beacon");
    let mut good_client = TcpStream::connect(address).expect("connect healthy beacon");
    good_client
        .write_all(VALID_REQUEST)
        .expect("write healthy follow-up");
    for client in [&bad_client, &good_client] {
        client
            .set_read_timeout(Some(TEST_DEADLINE))
            .expect("set reply read deadline");
    }
    let (_cancellation, cancellation_rx) = mpsc::channel();

    let rejected =
        serve_renderer_beacon_until(&listener, Instant::now() + TEST_DEADLINE, &cancellation_rx)
            .expect_err("first malformed request must fail");
    assert!(rejected.contains("unexpected beacon"), "{rejected}");
    let mut bad_reply = Vec::new();
    bad_client
        .read_to_end(&mut bad_reply)
        .expect("rejected peer sees its stream close");
    assert!(
        bad_reply.is_empty(),
        "a rejected request must not be acknowledged: {bad_reply:?}"
    );

    assert_eq!(
        serve_renderer_beacon_until(&listener, Instant::now() + TEST_DEADLINE, &cancellation_rx),
        Ok(()),
        "the queued healthy follow-up on the same listener must still be observed"
    );
    let mut good_reply = Vec::new();
    good_client
        .read_to_end(&mut good_reply)
        .expect("read healthy reply");
    assert_eq!(good_reply, RENDERER_RESPONSE);
}

#[test]
fn empty_preconnect_is_skipped_before_the_exact_request() {
    let server = TestServer::spawn(TEST_DEADLINE);
    drop(TcpStream::connect(server.address).expect("connect empty preconnect"));
    let mut client = TcpStream::connect(server.address).expect("connect exact request");
    client
        .set_read_timeout(Some(TEST_DEADLINE))
        .expect("set response read deadline");
    client
        .write_all(VALID_REQUEST)
        .expect("write exact request after empty preconnect");

    let mut response = Vec::new();
    client
        .read_to_end(&mut response)
        .expect("read renderer response");
    assert_eq!(response, RENDERER_RESPONSE);
    assert_eq!(server.finish(), Ok(()));
}

#[test]
fn empty_preconnect_alone_still_ends_at_the_original_deadline() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind preconnect-only beacon");
    let address = listener.local_addr().expect("preconnect-only address");
    drop(TcpStream::connect(address).expect("queue empty preconnect"));
    let (_cancellation, cancellation_rx) = mpsc::channel();

    let error =
        serve_renderer_beacon_until(&listener, Instant::now() + SHORT_DEADLINE, &cancellation_rx)
            .expect_err("an empty preconnect is not a renderer request");
    assert!(error.contains("deadline elapsed"), "{error}");
}
