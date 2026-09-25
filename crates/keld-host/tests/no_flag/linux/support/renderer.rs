//! Observe the real renderer HTTP request before signaling readiness.

use super::PRODUCT_DEADLINE;
use std::{
    io::{Read as _, Write as _},
    net::TcpListener,
    sync::mpsc,
};

pub(crate) fn serve_renderer_beacon(listener: &TcpListener, observed: &mpsc::Sender<()>) {
    let (mut stream, _) = listener.accept().expect("accept renderer beacon");
    stream
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("beacon deadline");
    let mut request = [0_u8; 2048];
    let read = stream.read(&mut request).expect("read renderer beacon");
    assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /ready.png "));
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .expect("reply renderer beacon");
    observed.send(()).expect("publish renderer beacon");
}
