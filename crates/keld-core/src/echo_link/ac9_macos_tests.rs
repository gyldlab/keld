//! Physical macOS AC9 observations at the production accepted-stream owner.
//! The IPC fixture owns bytes and outcomes; the existing worker owns closure.

#![allow(clippy::expect_used, clippy::panic)] // Test assertion oracles.

use std::cell::RefCell;
use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use keld_ipc::link::{AppLinkDeadlines, handshake_client};
use keld_ipc::{APP_LINK_IO_DEADLINE, CorrelationId, EchoRequest, echo_invoke, parse_app_link};

use super::EchoServer;

#[path = "../../../keld-ipc/tests/support/receiver_corpus.rs"]
mod corpus;
#[path = "../../../keld-ipc/tests/support/macos_child.rs"]
mod macos_child;
#[path = "../../../keld-ipc/tests/support/socket_probe.rs"]
mod socket_probe;

use socket_probe::{Boundary, Observed, Probe};

const STEP_LIMIT: Duration = Duration::from_secs(10);

thread_local! {
    static NEXT_PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
}

// Captured on the caller, then moved into the existing production worker.
// Other EchoServer tests have no installed probe.
pub(super) fn take_probe() -> Option<Probe> {
    NEXT_PROBE.with(|slot| slot.borrow_mut().take())
}

pub(super) fn observe_stream<S>(stream: S, probe: Option<Probe>) -> Observed<S> {
    Observed::new(stream, probe)
}

fn prove_row(id: &str) {
    let case = corpus::row(id);
    assert_eq!(case.policy, "echo-receiver");
    assert_eq!(case.link_action, "close");
    assert_eq!(case.handler_effects, 0); // Fixture declaration, not a handler counter.
    let fault = case.bytes();
    let truncated = corpus::TRUNCATED_ROWS.contains(&id);
    let probe = Probe::default();
    NEXT_PROBE.with(|slot| {
        assert!(slot.borrow_mut().replace(probe.clone()).is_none());
    });
    let (ready_tx, ready_rx) = mpsc::channel();
    let mut server = EchoServer::start(&ready_tx).expect("production echo owner");
    ready_rx
        .recv_timeout(STEP_LIMIT)
        .expect("production listener bound");
    let link = server.link();
    let (endpoint, token) = parse_app_link(&link).expect("production app link");
    assert!(
        Path::new(endpoint).starts_with(macos_child::capture_root()),
        "bootstrap must stay inside the parent's short owned root"
    );
    let mut peer = UnixStream::connect(endpoint).expect("real Unix peer");
    peer.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
        .expect("bounded peer I/O");
    handshake_client(&mut peer, &token).expect("production HELLO");
    let request = EchoRequest {
        message: format!("AC9 healthy control {id}"),
        count: 1,
    };
    let response =
        echo_invoke(&mut peer, &request, CorrelationId(1)).expect("healthy admitted echo");
    assert_eq!(response.message, request.message);
    assert_eq!(response.count, request.count);
    println!("AC9 row={id} control=healthy authenticated_echo=true");

    let boundary = if truncated {
        Boundary::Eof
    } else {
        Boundary::Bytes(fault.len())
    };
    let (observed_rx, release_tx) = probe.arm(boundary);
    peer.write_all(&fault)
        .expect("send exact canonical fault bytes");
    if truncated {
        // Input EOF only. Retry is observed at the receiver, never inferred
        // from inability to write after this local write-half shutdown.
        peer.shutdown(Shutdown::Write)
            .expect("deliver partial-frame EOF");
    }
    let boundary = observed_rx
        .recv_timeout(STEP_LIMIT)
        .expect("actual underlying fault-byte/EOF observation");
    assert_eq!(boundary.bytes, fault.len());
    assert_eq!(boundary.eof_reads, usize::from(truncated));
    if !truncated {
        // Paused after actual fault bytes were read, before returning them
        // to the unchanged reader. A following frame is physically queued.
        peer.write_all(&corpus::row("echo-call-valid").bytes())
            .expect("queue a valid same-stream retry challenge");
    }
    release_tx
        .send(())
        .expect("release the unchanged read result");

    let mut byte = [0u8; 1];
    let peer_close = match peer.read(&mut byte) {
        Ok(0) => "eof".to_owned(),
        Err(error) if error.kind() == ErrorKind::ConnectionReset => {
            format!("io:{:?}", error.kind())
        }
        other => panic!("production must close without response bytes: {other:?}"),
    };
    println!("AC9 row={id} atom=peer result={peer_close}");

    observe_session_completion(id, &server);
    observe_host_rejection(id, &mut server, case.expected_code);

    let observed = probe.snapshot();
    assert!(observed.boundary_seen);
    assert_eq!(
        observed.bytes,
        fault.len(),
        "no retry bytes may be consumed"
    );
    assert_eq!(observed.eof_reads, usize::from(truncated));
    assert_eq!(
        observed.reads_after_boundary, 0,
        "receiver retried the rejected stream"
    );
    println!(
        "AC9 row={id} atom=retry consumed_bytes={} eof_reads={} reads_after_boundary={} queued_canary={} before_server_drop=true",
        observed.bytes, observed.eof_reads, observed.reads_after_boundary, !truncated
    );
    assert!(
        !Path::new(endpoint).exists(),
        "admission must have consumed the locator"
    );
    drop(server);
    println!("AC9 row={id} complete=true");
}

fn observe_session_completion(id: &str, server: &EchoServer) {
    let deadline = Instant::now() + STEP_LIMIT;
    let worker = server.handle.as_ref().expect("production worker handle");
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "production worker did not terminate"
        );
        thread::yield_now();
    }
    assert!(
        !server.stop.load(Ordering::Acquire),
        "test shutdown cannot close the session"
    );
    println!(
        "AC9 row={id} atom=session worker_finished=true stop_requested=false before_server_drop=true"
    );
}

fn observe_host_rejection(id: &str, server: &mut EchoServer, expected_code: &str) {
    let error = server
        .handle
        .take()
        .expect("production worker handle")
        .join()
        .expect("production worker did not panic")
        .expect_err("canonical rejected frame must retain the typed host failure");
    println!("AC9 row={id} atom=host typed={error:?} code={error}");
    assert!(
        error.to_string().starts_with(expected_code),
        "expected {expected_code}, got {error:?}"
    );
}

fn prove_cases(selector: &str, cases: &[&str]) {
    for id in cases {
        let output = macos_child::run_case_child(selector, id, Duration::from_secs(20));
        let stdout = String::from_utf8(output.stdout).expect("child evidence is UTF-8");
        for atom in ["host", "peer", "session", "retry"] {
            let marker = format!("AC9 row={id} atom={atom} ");
            assert_eq!(
                stdout
                    .lines()
                    .filter(|line| line.starts_with(&marker))
                    .count(),
                1,
                "each independently asserted atom must appear exactly once"
            );
        }
        assert!(stdout.contains(&format!("AC9 row={id} complete=true")));
    }
}

#[test]
fn authenticated_truncated_corpus_rows_preserve_host_io_error() {
    prove_cases(
        "echo_link::ac9_macos_tests::truncated_frame_child",
        &corpus::TRUNCATED_ROWS,
    );
}

#[test]
fn authenticated_corpus_rows_observe_all_rejection_dimensions() {
    prove_cases(
        "echo_link::ac9_macos_tests::authenticated_frame_child",
        &corpus::MACOS_ECHO_ROWS,
    );
}

#[test]
#[ignore = "subprocess entry; run only through the bounded AC9 parent"]
fn truncated_frame_child() {
    let case = macos_child::case_id();
    assert!(corpus::TRUNCATED_ROWS.contains(&case.as_str()));
    prove_row(&case);
}

#[test]
#[ignore = "isolated AC9 child; selected by the bounded parent runner"]
fn authenticated_frame_child() {
    let case = macos_child::case_id();
    assert!(corpus::MACOS_ECHO_ROWS.contains(&case.as_str()));
    prove_row(&case);
}
