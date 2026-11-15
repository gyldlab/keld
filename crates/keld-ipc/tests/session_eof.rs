//! macOS real-socket regression for the second IPC session entry changed by AC9.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use keld_ipc::link::AppLinkDeadlines;
use keld_ipc::{CorrelationId, EchoRequest, IpcError, echo_invoke, serve_echo_requests};

#[path = "support/receiver_corpus.rs"]
mod corpus;
#[path = "support/macos_child.rs"]
mod macos_child;

#[test]
fn plain_session_preserves_partial_eof_and_allows_frame_boundary_eof() {
    for case in [
        "truncated-header-8",
        "truncated-payload",
        "clean-frame-boundary",
    ] {
        let output =
            macos_child::run_case_child("plain_session_child", case, Duration::from_secs(20));
        let stdout = String::from_utf8(output.stdout).expect("plain-session evidence");
        assert!(stdout.contains(&format!("AC9_PLAIN row={case} passed=true")));
    }
}

#[test]
#[ignore = "isolated AC9 child; selected by the bounded parent runner"]
fn plain_session_child() {
    let case = macos_child::case_id();
    let (mut host, mut peer) = UnixStream::pair().expect("real macOS Unix socket pair");
    host.set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("host deadline");
    peer.set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("peer deadline");
    let worker = thread::spawn(move || serve_echo_requests(&mut host));
    let request = EchoRequest {
        message: "plain session healthy control".to_owned(),
        count: 1,
    };
    let reply = echo_invoke(&mut peer, &request, CorrelationId(1)).expect("healthy plain echo");
    assert_eq!(reply.message, request.message);
    assert_eq!(reply.count, request.count);
    if case != "clean-frame-boundary" {
        assert!(corpus::TRUNCATED_ROWS.contains(&case.as_str()));
        peer.write_all(&corpus::row(&case).bytes())
            .expect("canonical truncated frame");
    }
    peer.shutdown(Shutdown::Write).expect("real peer input EOF");
    let outcome = worker.join().expect("plain production session returned");
    println!("AC9_PLAIN row={case} host={outcome:?}");
    if case == "clean-frame-boundary" {
        outcome.expect("EOF after a complete frame remains successful");
    } else {
        let error = outcome.expect_err("partial EOF must not be swallowed");
        assert!(
            matches!(&error, IpcError::Io(io) if io.kind() == std::io::ErrorKind::UnexpectedEof)
        );
        assert!(
            error
                .to_string()
                .starts_with(corpus::row(&case).expected_code)
        );
    }
    // This test claims only the plain session result; the fixture owns this
    // pair, so its closure is not counted as product socket-close evidence.
    println!("AC9_PLAIN row={case} passed=true");
}
