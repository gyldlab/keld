//! Stream write-deadline and peer shutdown scenarios.

use super::{
    AppLinkDeadlines, Arc, Duration, Instant, WindowsNamedPipeBootstrapAdmission,
    WindowsNamedPipeBootstrapListener, client, handshake_client, parse_app_link,
};
use std::io::{Read as _, Write as _};
use std::sync::mpsc;
use std::thread;

#[test]
fn overlapped_write_to_non_reading_peer_hits_configured_deadline() {
    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
    let link = listener.app_link();
    let (endpoint, token) = parse_app_link(&link).expect("parse link");
    let endpoint = endpoint.to_owned();
    let worker_listener = Arc::clone(&listener);
    let worker = thread::spawn(move || {
        worker_listener.accept_authenticated_until(
            Instant::now() + Duration::from_secs(2),
            &super::NoopRejectionObserver,
        )
    });
    let mut role = client(&endpoint).expect("open role");
    role.set_app_link_deadlines(Some(Duration::from_millis(500)))
        .expect("role deadlines");
    handshake_client(&mut role, &token).expect("authenticate");
    let outcome = worker.join().expect("join").expect("admission");
    let WindowsNamedPipeBootstrapAdmission::Authenticated(mut server_stream) = outcome else {
        panic!("expected authenticated named-pipe session")
    };
    server_stream
        .set_app_link_write_deadline(Some(Duration::from_millis(30)))
        .expect("short write deadline");
    let payload = vec![0_u8; 1024 * 1024];
    let error = server_stream
        .write_all(&payload)
        .expect_err("non-reading peer must not wedge an overlapped write");
    assert_eq!(error.raw_os_error(), Some(121));
}

#[test]
fn stream_shutdown_cancels_pending_read_and_joins_without_peer_input() {
    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
    let link = listener.app_link();
    let (endpoint, token) = parse_app_link(&link).expect("parse link");
    let endpoint = endpoint.to_owned();
    let worker_listener = Arc::clone(&listener);
    let worker = thread::spawn(move || {
        worker_listener.accept_authenticated_until(
            Instant::now() + Duration::from_secs(2),
            &super::NoopRejectionObserver,
        )
    });
    let mut role = client(&endpoint).expect("open role");
    role.set_app_link_deadlines(Some(Duration::from_millis(500)))
        .expect("role deadlines");
    handshake_client(&mut role, &token).expect("authenticate");
    let outcome = worker.join().expect("join admission").expect("admission");
    let WindowsNamedPipeBootstrapAdmission::Authenticated(server_stream) = outcome else {
        panic!("expected authenticated named-pipe session")
    };
    let mut blocked_reader = server_stream.try_clone().expect("clone server stream");
    blocked_reader
        .set_app_link_read_deadline(Some(Duration::from_secs(5)))
        .expect("reader deadline");
    let (result_tx, result_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut byte = [0_u8; 1];
        let _ = result_tx.send(blocked_reader.read_exact(&mut byte));
    });
    let active_deadline = Instant::now() + Duration::from_secs(1);
    while !server_stream.0.has_active_io() {
        assert!(
            Instant::now() < active_deadline,
            "reader never entered overlapped I/O"
        );
        thread::yield_now();
    }
    server_stream
        .shutdown_app_link()
        .expect("shutdown connected pipe");
    let error = result_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("shutdown must wake local read")
        .expect_err("pending read must not succeed without peer bytes");
    assert!(
        error.kind() == std::io::ErrorKind::UnexpectedEof
            || matches!(error.raw_os_error(), Some(995 | 109 | 233))
    );
    reader.join().expect("join blocked reader");
    let mut peer_byte = [0_u8; 1];
    assert_eq!(
        role.read(&mut peer_byte)
            .expect("shutdown must produce peer EOF"),
        0
    );
}

#[test]
fn stream_shutdown_disconnects_peer_even_when_cancellation_reports_error() {
    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
    let link = listener.app_link();
    let (endpoint, token) = parse_app_link(&link).expect("parse link");
    let endpoint = endpoint.to_owned();
    let worker_listener = Arc::clone(&listener);
    let worker = thread::spawn(move || {
        worker_listener.accept_authenticated_until(
            Instant::now() + Duration::from_secs(2),
            &super::NoopRejectionObserver,
        )
    });
    let mut role = client(&endpoint).expect("open role");
    role.set_app_link_deadlines(Some(Duration::from_millis(500)))
        .expect("role deadlines");
    handshake_client(&mut role, &token).expect("authenticate");
    let outcome = worker.join().expect("join admission").expect("admission");
    let WindowsNamedPipeBootstrapAdmission::Authenticated(server_stream) = outcome else {
        panic!("expected authenticated named-pipe session")
    };
    server_stream.0.force_cancel_error();
    let shutdown_error = server_stream
        .shutdown_app_link()
        .expect_err("injected cancellation failure must be reported");
    assert_eq!(shutdown_error.raw_os_error(), Some(5));

    let mut peer_byte = [0_u8; 1];
    assert_eq!(
        role.read(&mut peer_byte)
            .expect("disconnect must still close the peer-facing connection"),
        0
    );
}
