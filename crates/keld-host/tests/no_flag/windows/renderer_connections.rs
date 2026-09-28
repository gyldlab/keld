//! Renderer beacon connection admission and exact-request contracts.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::support::renderer::{
    expect_renderer_beacon, serve_renderer_beacon_until, spawn_renderer_beacon,
};
use crate::windows_renderer_http::RENDERER_CONNECTION_LIMIT;

#[test]
fn renderer_beacon_ignores_an_empty_preconnect_before_the_exact_request() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind beacon regression listener");
    let address = listener.local_addr().expect("beacon regression address");
    let beacon = spawn_renderer_beacon(listener);

    drop(TcpStream::connect(address).expect("connect empty preconnect"));
    let mut target = TcpStream::connect(address).expect("connect target renderer request");
    target
        .write_all(b"GET /ready.png HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write target renderer request");

    expect_renderer_beacon(
        beacon,
        "exact renderer request observed after empty preconnect",
    );
}

#[test]
fn renderer_beacon_does_not_serialize_idle_preconnects_before_the_exact_request() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind idle-preconnect listener");
    let address = listener.local_addr().expect("idle-preconnect address");
    let idle = (0..(RENDERER_CONNECTION_LIMIT - 1))
        .map(|_| TcpStream::connect(address).expect("queue idle preconnect"))
        .collect::<Vec<_>>();
    let deadline = Instant::now() + Duration::from_secs(2);
    let (observed_tx, observed_rx) = mpsc::channel();
    let beacon = thread::spawn(move || {
        let result = serve_renderer_beacon_until(&listener, deadline);
        let _ = observed_tx.send(result);
    });

    let mut target = TcpStream::connect(address).expect("connect exact request behind idle peer");
    target
        .write_all(b"GET /ready.png HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write exact request behind idle peer");

    let observed = observed_rx.recv_timeout(Duration::from_secs(1));
    drop(idle);
    beacon.join().expect("idle-preconnect beacon thread");
    let result = observed.expect("idle peers serialized ahead of the exact request");
    assert_eq!(result, Ok(()));
}

#[test]
fn renderer_beacon_reaps_closed_pending_connections_before_applying_the_live_cap() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind stale-pending listener");
    let address = listener.local_addr().expect("stale-pending address");
    let stale = (0..RENDERER_CONNECTION_LIMIT)
        .map(|_| TcpStream::connect(address).expect("queue stale pending connection"))
        .collect::<Vec<_>>();
    drop(stale);

    let mut target = TcpStream::connect(address).expect("connect behind stale pending peers");
    target
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("stale-pending response kill switch");
    target
        .write_all(b"GET /ready.png HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write exact request behind stale pending peers");

    let result = serve_renderer_beacon_until(&listener, Instant::now() + Duration::from_secs(1));
    assert_eq!(result, Ok(()));
    let mut response = Vec::new();
    target
        .read_to_end(&mut response)
        .expect("read exact response behind stale pending peers");
    assert_eq!(
        response,
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
}

#[test]
fn renderer_beacon_rejects_maximum_plus_one_pending_connections() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind connection-limit listener");
    let address = listener.local_addr().expect("connection-limit address");
    let pending = (0..=RENDERER_CONNECTION_LIMIT)
        .map(|_| TcpStream::connect(address).expect("queue pending connection"))
        .collect::<Vec<_>>();
    let result = serve_renderer_beacon_until(&listener, Instant::now() + Duration::from_secs(1));
    drop(pending);

    assert_eq!(
        result,
        Err(format!(
            "renderer beacon exceeded {RENDERER_CONNECTION_LIMIT} pending connections"
        ))
    );
}

#[test]
fn renderer_beacon_preserves_the_unexpected_path_failure() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind wrong-path listener");
    let address = listener.local_addr().expect("wrong-path address");
    let deadline = Instant::now() + Duration::from_secs(1);
    let beacon = thread::spawn(move || serve_renderer_beacon_until(&listener, deadline));
    let mut request = TcpStream::connect(address).expect("connect wrong-path request");
    request
        .write_all(b"GET /leaked.png HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write wrong-path request");

    let error = beacon
        .join()
        .expect("wrong-path beacon thread")
        .expect_err("wrong renderer path must fail");
    assert_eq!(
        error,
        "renderer requested an unexpected beacon: GET /leaked.png HTTP/1.1"
    );
}
