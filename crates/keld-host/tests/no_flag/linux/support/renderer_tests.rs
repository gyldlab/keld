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
fn rejected_request_does_not_poison_a_healthy_follow_up() {
    let bad_server = TestServer::spawn(TEST_DEADLINE);
    let mut bad_client = TcpStream::connect(bad_server.address).expect("connect bad beacon");
    bad_client
        .write_all(b"GET /wrong.png HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("write bad beacon");
    assert!(
        bad_server.finish().is_err(),
        "first malformed request must fail"
    );

    let good_server = TestServer::spawn(TEST_DEADLINE);
    let mut good_client = TcpStream::connect(good_server.address).expect("connect healthy beacon");
    good_client
        .write_all(VALID_REQUEST)
        .expect("write healthy follow-up");
    assert_eq!(good_server.finish(), Ok(()));
}
