//! Physical macOS AC9 admission rows; framing and retry decisions stay in kipc.

#![allow(clippy::expect_used, clippy::panic)] // Test assertion oracles.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::{
    BootstrapAdmission, BootstrapListener, BootstrapRejection, BootstrapRejectionObserver,
    TestHandshakeWitness,
};
use crate::link::{AppLinkDeadlines, handshake_client, set_test_read_entry_witness};
use crate::{APP_LINK_IO_DEADLINE, SessionToken, serve_echo_requests_until_stopped};

#[path = "../../tests/support/receiver_corpus.rs"]
mod corpus;
#[path = "../../tests/support/macos_child.rs"]
mod macos_child;

const CHILD_SELECTOR: &str = "bootstrap::ac9_macos_tests::ac9_bootstrap_child";
const STEP_LIMIT: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug)]
struct RejectionFact {
    class: BootstrapRejection,
    at: Instant,
}

struct RejectionGate {
    observed: mpsc::SyncSender<RejectionFact>,
    release: Mutex<mpsc::Receiver<()>>,
    first: AtomicBool,
}

impl BootstrapRejectionObserver for RejectionGate {
    fn rejected(&self, class: BootstrapRejection) {
        self.observed
            .send(RejectionFact {
                class,
                at: Instant::now(),
            })
            .expect("record the actual production rejection");
        if self.first.swap(false, Ordering::AcqRel) {
            self.release
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .recv_timeout(STEP_LIMIT)
                .expect("release the already-classified rejected peer");
        }
    }
}

fn peer_close(stream: &mut UnixStream) -> String {
    let mut byte = [0_u8; 1];
    match stream.read(&mut byte) {
        Ok(0) => "eof".to_owned(),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::BrokenPipe
                    | ErrorKind::NotConnected
            ) =>
        {
            format!("io:{:?}", error.kind())
        }
        other => panic!("rejected peer must close without response bytes: {other:?}"),
    }
}

// Cohesion exception: keld-ipc owns this physical macOS AC9 scenario (KEL-133).
// Its two canonical rows share one ordered admission/rejection/recovery flow.
// Keep the fault, same-peer challenge, independent observations, complete read
// drain, next allowed operation, and explicit cleanup together for review.
// Revisit if another row or phase is added, or observer/resource ownership changes.
#[allow(
    clippy::too_many_lines,
    reason = "KEL-133: ordered real-socket acceptance scenario; see cohesion exception above"
)]
fn prove_row(case: &str, capture_root: &Path) {
    let row = corpus::row(case);
    let expected_class = match case {
        "started-frame-stall" => BootstrapRejection::Timeout,
        "hello-foreign-token" => BootstrapRejection::HelloAuth,
        _ => panic!("unsupported AC9 bootstrap case"),
    };
    let (fault_bytes, generation_limit) =
        if let Some(milliseconds) = row.policy.strip_prefix("trace:generation-deadline-ms=") {
            // The canonical v1 traces are HELLO admissions. This selected trace
            // has one arrival at zero; no general timing model is copied.
            let arrival = row
                .header_or_trace
                .strip_prefix("at0=")
                .expect("selected trace starts at zero");
            assert!(!arrival.contains(';'), "selected trace has one arrival");
            let close_ms: u64 = row
                .link_action
                .strip_prefix("close-at-")
                .expect("started-frame trace has a close instant")
                .parse()
                .expect("canonical close milliseconds");
            assert_eq!(Duration::from_millis(close_ms), APP_LINK_IO_DEADLINE);
            (
                corpus::unhex(arrival),
                Duration::from_millis(milliseconds.parse().expect("generation milliseconds")),
            )
        } else {
            assert_eq!(row.policy, "server-pre-auth-hello");
            assert_eq!(row.link_action, "close-reaccept");
            (row.bytes(), Duration::from_secs(10))
        };
    assert_eq!(row.handler_effects, 0);
    let hello = corpus::row("hello-valid").bytes();

    let mut bound = BootstrapListener::bind().expect("bind production bootstrap");
    // Fixture setup only: the corpus declares this public, non-secret token.
    bound.token = SessionToken::from_bytes(corpus::fixture_token_bytes());
    let endpoint = bound.path().to_path_buf();
    let directory = endpoint
        .parent()
        .expect("bootstrap directory")
        .to_path_buf();
    assert!(
        endpoint.starts_with(capture_root),
        "bootstrap escaped owned child root"
    );
    let token = bound.token;
    let app_link = bound.app_link();
    let listener = Arc::new(bound);

    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    listener.install_handshake_witness(TestHandshakeWitness {
        entered: entered_tx,
    });
    let (observed_tx, observed_rx) = mpsc::sync_channel(8);
    let (release_tx, release_rx) = mpsc::channel();
    let observer = RejectionGate {
        observed: observed_tx,
        release: Mutex::new(release_rx),
        first: AtomicBool::new(true),
    };
    let (read_tx, read_rx) = mpsc::channel();
    let (start_tx, start_rx) = mpsc::sync_channel(1);
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let acceptor = Arc::clone(&listener);
    let worker = thread::spawn(move || {
        let deadline = start_rx
            .recv_timeout(STEP_LIMIT)
            .expect("fixed generation deadline");
        set_test_read_entry_witness(Some(read_tx));
        let admission = acceptor.accept_authenticated_until(deadline, &observer);
        set_test_read_entry_witness(None);
        let admission = admission.expect("host listener I/O");
        let BootstrapAdmission::Authenticated(mut stream) = admission else {
            panic!("rejection ended admission instead of reaccepting: {admission:?}");
        };
        accepted_tx
            .send(())
            .expect("record actual authenticated result");
        let never_stopped = AtomicBool::new(false);
        let result = serve_echo_requests_until_stopped(&mut stream, &never_stopped);
        // This is only the healthy follow-up. The rejected peer's product
        // closure was observed before the legitimate peer connected.
        drop(stream);
        finished_tx
            .send(())
            .expect("record production session return");
        result
    });

    let mut hostile = UnixStream::connect(&endpoint).expect("connect raw hostile peer");
    hostile
        .set_app_link_deadlines(Some(STEP_LIMIT))
        .expect("peer kill-switch deadlines");
    let deadline = Instant::now()
        .checked_add(generation_limit)
        .expect("bounded generation");
    start_tx.send(deadline).expect("start one fixed generation");
    let first = entered_rx
        .recv_timeout(STEP_LIMIT)
        .expect("production handshake entry");
    assert_eq!(first.generation_deadline, Some(deadline));
    assert!(first.entered_at < deadline);
    let first_read = read_rx
        .recv_timeout(STEP_LIMIT)
        .expect("actual production read entry");
    assert!(first_read < deadline);
    hostile
        .write_all(&fault_bytes)
        .expect("submit canonical fault bytes");

    let rejection = observed_rx
        .recv_timeout(STEP_LIMIT)
        .expect("host rejection class");
    assert_eq!(rejection.class, expected_class);
    assert_eq!(rejection.class.code(), row.expected_code);
    assert!(
        rejection.at < deadline,
        "recoverable rejection must leave generation time"
    );
    if expected_class == BootstrapRejection::Timeout {
        assert!(
            first.peer_deadline < deadline,
            "peer expiry, not generation expiry"
        );
        assert!(
            rejection.at >= first.peer_deadline,
            "a short idle poll is not terminal expiry"
        );
    }
    let host_record = format!("{:?} {}", rejection.class, rejection.class.code());
    assert!(!host_record.contains(&token.to_hex()));
    assert!(!host_record.contains(&app_link));
    assert!(!host_record.contains(&endpoint.to_string_lossy().into_owned()));
    println!(
        "AC9 row={case} atom=host class={:?} code={}",
        rejection.class,
        rejection.class.code()
    );

    // Pause after the production rejection decision; keep the write half
    // open and queue a next-operation challenge before releasing this peer.
    if expected_class == BootstrapRejection::Timeout {
        assert!(!fault_bytes.is_empty() && fault_bytes.len() < hello.len());
        assert!(hello.starts_with(&fault_bytes));
        hostile
            .write_all(&hello[fault_bytes.len()..])
            .expect("queue HELLO remainder");
    }
    hostile
        .write_all(&hello)
        .expect("queue same-peer fresh HELLO challenge");
    release_tx.send(()).expect("release rejected peer");
    let peer = peer_close(&mut hostile);
    println!("AC9 row={case} atom=peer result={peer}");

    // Before a new peer connects, later reads can only retry the old peer.
    let before_new_peer_reads = read_rx.try_iter().filter(|at| *at >= rejection.at).count();
    assert_eq!(
        before_new_peer_reads, 0,
        "production retried the rejected stream"
    );
    assert!(
        matches!(observed_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "one rejected peer must produce exactly one class"
    );
    let bound = listener
        .listener
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some();
    let active_clear = listener
        .active_stream
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_none();
    let listening = listener.listening.load(Ordering::Acquire);
    let stopping = listener.stopping.load(Ordering::Acquire);
    let awaiting_admission = matches!(accepted_rx.try_recv(), Err(mpsc::TryRecvError::Empty));
    let worker_running = !worker.is_finished();
    assert!(
        bound && active_clear && listening && !stopping && awaiting_admission && worker_running,
        "rejected peer must close while the same admission remains available"
    );
    println!(
        "AC9 row={case} atom=session listener_bound={bound} active_clear={active_clear} listening={listening} stopping={stopping} awaiting_admission={awaiting_admission} worker_running={worker_running}"
    );

    // The handshake witness is one-shot; reinstall it before the next peer.
    let (next_tx, next_rx) = mpsc::sync_channel(1);
    listener.install_handshake_witness(TestHandshakeWitness { entered: next_tx });
    let mut legitimate = UnixStream::connect(&endpoint).expect("next peer uses same generation");
    legitimate
        .set_app_link_deadlines(Some(STEP_LIMIT))
        .expect("legitimate deadlines");
    handshake_client(&mut legitimate, &token).expect("next allowed HELLO authenticates");
    let next = next_rx
        .recv_timeout(STEP_LIMIT)
        .expect("next production handshake entry");
    assert_eq!(
        next.generation_deadline,
        Some(deadline),
        "reaccept must not mint another D"
    );
    assert!(next.entered_at < deadline);
    accepted_rx
        .recv_timeout(STEP_LIMIT)
        .expect("actual authenticated admission result");
    // The worker cleared its TLS witness before sending accepted. The closed
    // channel makes this complete; the second handshake separates new reads.
    let late_old_peer_reads = read_rx
        .iter()
        .filter(|at| *at >= rejection.at && *at < next.entered_at)
        .count();
    let after_rejection_reads = before_new_peer_reads + late_old_peer_reads;
    assert_eq!(
        after_rejection_reads, 0,
        "production retried the rejected stream"
    );
    legitimate
        .write_all(&corpus::row("echo-call-valid").bytes())
        .expect("healthy canonical echo");
    let expected_reply = corpus::row("echo-reply-valid").bytes();
    let mut reply = vec![0; expected_reply.len()];
    legitimate
        .read_exact(&mut reply)
        .expect("healthy correlated echo reply");
    assert_eq!(
        reply, expected_reply,
        "next operation preserves exact golden reply"
    );
    assert!(
        !endpoint.exists() && !directory.exists(),
        "successful admission consumes locator before harness cleanup"
    );
    assert!(
        matches!(
            observed_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ),
        "legitimate peer must not add a rejection"
    );
    println!(
        "AC9 row={case} atom=retry same_peer_post_rejection_reads={after_rejection_reads} fresh_peer_authenticated=true same_generation=true healthy_reply_bytes={} locator_consumed=true",
        reply.len()
    );

    drop(legitimate);
    drop(hostile);
    finished_rx
        .recv_timeout(STEP_LIMIT)
        .expect("healthy production session completes");
    worker
        .join()
        .expect("join admission worker")
        .expect("healthy session result");
    drop(listener);
}

#[test]
fn real_unix_admission_rows_prove_ac9() {
    for case in ["started-frame-stall", "hello-foreign-token"] {
        let output = macos_child::run_case_child(CHILD_SELECTOR, case, Duration::from_secs(20));
        let stdout = String::from_utf8(output.stdout).expect("child evidence is UTF-8");
        for atom in ["host", "peer", "session", "retry"] {
            let marker = format!("AC9 row={case} atom={atom} ");
            assert_eq!(
                stdout
                    .lines()
                    .filter(|line| line.starts_with(&marker))
                    .count(),
                1,
                "each independently asserted atom must appear exactly once: {marker}"
            );
        }
    }
}

#[test]
#[ignore = "isolated AC9 child; selected by the bounded parent runner"]
fn ac9_bootstrap_child() {
    let case = macos_child::case_id();
    let root = macos_child::capture_root();
    prove_row(&case, &root);
}
