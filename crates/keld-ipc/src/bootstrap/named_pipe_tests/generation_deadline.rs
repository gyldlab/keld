//! Generation deadline lifecycle scenarios for Windows named pipes.
//! Native observers verify no late authorization or reaccept after expiry.

use super::{
    AppLinkDeadlines, Arc, BlockingObserver, BootstrapRejection, Duration, Instant, Mutex,
    TestConsumeGate, WindowsNamedPipeBootstrapAdmission, WindowsNamedPipeBootstrapListener,
    WindowsNamedPipeServer, client, handshake_client, parse_app_link,
};
use std::io::Write as _;
use std::sync::mpsc;
use std::thread;

#[test]
fn generation_deadline_is_terminal_without_a_client() {
    let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind");
    let outcome = listener
        .accept_authenticated_until(
            Instant::now() + Duration::from_millis(30),
            &super::NoopRejectionObserver,
        )
        .expect("deadline admission");
    assert!(matches!(
        outcome,
        WindowsNamedPipeBootstrapAdmission::DeadlineElapsed
    ));
    let error = WindowsNamedPipeServer::connect_client(listener.endpoint())
        .expect_err("terminal generation must not accept a late connector");
    assert!(matches!(error.raw_os_error(), Some(2 | 231)));
}

#[test]
fn already_expired_generation_never_enters_pipe_accept() {
    let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind");
    let deadline = Instant::now()
        .checked_sub(Duration::from_millis(1))
        .expect("representable expired deadline");
    let outcome = listener
        .accept_authenticated_until(deadline, &super::NoopRejectionObserver)
        .expect("expired admission");
    assert!(matches!(
        outcome,
        WindowsNamedPipeBootstrapAdmission::DeadlineElapsed
    ));
    let stale = WindowsNamedPipeServer::connect_client(listener.endpoint())
        .expect_err("expired generation must close before accepting");
    assert_eq!(stale.raw_os_error(), Some(2));
}

#[test]
fn generation_expiry_during_rejection_blocks_reaccept() {
    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
    let endpoint = listener.endpoint().to_owned();
    let cancellation = listener.cancellation();
    let deadline = Instant::now() + Duration::from_millis(100);
    let (observed_tx, observed_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let observer = BlockingObserver {
        observed: observed_tx,
        release: Mutex::new(release_rx),
    };
    let worker_listener = Arc::clone(&listener);
    let (result_tx, result_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = result_tx.send(worker_listener.accept_authenticated_until(deadline, &observer));
    });

    let mut hostile = client(&endpoint).expect("open hostile client");
    hostile.write_all(&[0_u8; 16]).expect("bad header");
    assert_eq!(
        observed_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("header rejection"),
        BootstrapRejection::Header
    );
    while Instant::now() < deadline {
        thread::yield_now();
    }
    release_tx.send(()).expect("release observer after expiry");

    let outcome = match result_rx.recv_timeout(Duration::from_secs(1)) {
        Ok(result) => result.expect("admission result"),
        Err(error) => {
            cancellation.cancel().expect("cancel wedged reaccept");
            worker.join().expect("join cancelled reaccept");
            panic!("expired rejection path re-entered an unbounded accept: {error}");
        }
    };
    assert!(matches!(
        outcome,
        WindowsNamedPipeBootstrapAdmission::DeadlineElapsed
    ));
    drop(hostile);
    worker.join().expect("join expired rejection worker");
}

#[test]
fn generation_expiry_at_final_auth_boundary_is_not_consumed() {
    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
    let link = listener.app_link();
    let (endpoint, token) = parse_app_link(&link).expect("parse link");
    let endpoint = endpoint.to_owned();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    listener.install_before_consume_gate(TestConsumeGate {
        entered: entered_tx,
        release: release_rx,
    });
    let deadline = Instant::now() + Duration::from_millis(100);
    let worker_listener = Arc::clone(&listener);
    let worker = thread::spawn(move || {
        worker_listener.accept_authenticated_until(deadline, &super::NoopRejectionObserver)
    });
    let mut role = client(&endpoint).expect("open role");
    role.set_app_link_deadlines(Some(Duration::from_millis(500)))
        .expect("role deadlines");
    handshake_client(&mut role, &token).expect("HELLO reaches final boundary");
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("final authentication boundary");
    while Instant::now() < deadline {
        thread::yield_now();
    }
    release_tx.send(()).expect("release final boundary");
    let outcome = worker.join().expect("join admission").expect("admission");
    assert!(matches!(
        outcome,
        WindowsNamedPipeBootstrapAdmission::DeadlineElapsed
    ));
    let stale = WindowsNamedPipeServer::connect_client(&endpoint)
        .expect_err("expired final authentication must not consume a session");
    assert_eq!(stale.raw_os_error(), Some(2));
    drop(role);
}
