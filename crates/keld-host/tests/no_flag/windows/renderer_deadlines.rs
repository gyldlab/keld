//! Renderer beacon activation and absolute-deadline contracts.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::support::renderer::{
    expect_renderer_beacon, serve_renderer_beacon_loop_with_clock, serve_renderer_beacon_until,
    spawn_renderer_beacon,
};

#[test]
fn renderer_beacon_rechecks_the_absolute_deadline_before_replying() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind reply-deadline listener");
    let address = listener.local_addr().expect("reply-deadline address");
    let mut target = TcpStream::connect(address).expect("connect reply-deadline request");
    target
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("reply-deadline response kill switch");
    target
        .write_all(b"GET /ready.png HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write reply-deadline request");

    let before = Instant::now();
    let deadline = before + Duration::from_secs(1);
    let after = deadline + Duration::from_nanos(1);
    let (deadline_tx, deadline_rx) = mpsc::channel();
    deadline_tx
        .send(deadline)
        .expect("activate injected reply deadline");
    let mut observations = [before, before, after].into_iter();
    let result = serve_renderer_beacon_loop_with_clock(&listener, &deadline_rx, || {
        observations
            .next()
            .expect("renderer beacon sampled the clock beyond the reply boundary")
    });

    assert_eq!(
        result,
        Err("renderer beacon deadline elapsed before the exact request".to_owned())
    );
    assert!(
        observations.next().is_none(),
        "the reply boundary did not consume its fresh clock observation"
    );
    let mut response = Vec::new();
    target
        .read_to_end(&mut response)
        .expect("read reply-deadline response");
    assert!(response.is_empty(), "late response escaped: {response:?}");
}

#[test]
fn renderer_beacon_serves_before_the_parent_activates_its_wait_deadline() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind early beacon listener");
    let address = listener.local_addr().expect("early beacon address");
    let beacon = spawn_renderer_beacon(listener);
    let mut target = TcpStream::connect(address).expect("connect early renderer request");
    target
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("early response kill switch");
    target
        .write_all(b"GET /ready.png HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write early renderer request");
    let mut response = [0_u8; 128];
    let read = target
        .read(&mut response)
        .expect("worker replied before parent await");
    assert!(
        response[..read].starts_with(b"HTTP/1.1 200 OK\r\n"),
        "{}",
        String::from_utf8_lossy(&response[..read])
    );

    expect_renderer_beacon(beacon, "early renderer request");
}

#[test]
fn renderer_beacon_reports_its_absolute_deadline_instead_of_disconnect() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind beacon deadline listener");
    let deadline = Instant::now() + Duration::from_millis(50);
    let (observed_tx, observed_rx) = mpsc::channel();
    let beacon = thread::spawn(move || {
        let result = serve_renderer_beacon_until(&listener, deadline);
        observed_tx
            .send(result)
            .expect("publish beacon deadline result");
    });

    let result = observed_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("beacon worker honored its absolute deadline");
    assert_eq!(
        result,
        Err("renderer beacon deadline elapsed before the exact request".to_owned())
    );
    beacon.join().expect("beacon deadline thread");
}

#[test]
fn renderer_beacon_empty_preconnect_does_not_renew_the_deadline() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind preconnect deadline listener");
    let address = listener.local_addr().expect("preconnect deadline address");
    drop(TcpStream::connect(address).expect("queue empty preconnect before deadline"));

    let deadline = Instant::now() + Duration::from_millis(50);
    let (observed_tx, observed_rx) = mpsc::channel();
    let beacon = thread::spawn(move || {
        let result = serve_renderer_beacon_until(&listener, deadline);
        observed_tx
            .send(result)
            .expect("publish preconnect deadline result");
    });

    let result = observed_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("empty preconnect retained the original absolute deadline");
    assert_eq!(
        result,
        Err("renderer beacon deadline elapsed before the exact request".to_owned())
    );
    beacon.join().expect("preconnect deadline thread");
}
