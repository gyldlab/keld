//! Lifecycle cancellation and resource-ownership scenarios.

use super::{
    Arc, Command, Duration, Instant, PoisonError, ProcessIdToSessionId, Stdio, TestConsumeGate,
    TestPeerProcess, WindowsLifecycleRendezvousListener, WindowsNamedPipeBootstrapAdmission,
    WindowsNamedPipeBootstrapListener, WindowsNamedPipeServer, process_handle_count,
    test_lifecycle_binding,
};
use std::io::Write as _;
use std::sync::mpsc;
use std::thread;

/// The lifecycle rendezvous one-shot on a cancelled instance. No production
/// path cancels this listener's instance; if one did, the refused one-shot
/// fails closed. The cancel lands at the listener's final boundary, after
/// the peer authenticated and immediately before the one-shot: admission
/// returns `ERROR_OPERATION_ABORTED` (995) without sending the receipt, the
/// client fails without its accepted marker, and the locator is closed.
#[test]
fn a_cancelled_lifecycle_instance_refuses_its_one_shot_and_sends_no_receipt() {
    const ENDPOINT_ENV: &str = "KELD_TEST_LIFECYCLE_ENDPOINT";
    const SERVER_PID_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_PID";
    const SERVER_SESSION_ENV: &str = "KELD_TEST_LIFECYCLE_SERVER_SESSION";
    let listener = Arc::new(
        WindowsLifecycleRendezvousListener::bind([0x75; 32], test_lifecycle_binding())
            .expect("bind lifecycle endpoint"),
    );
    let endpoint = listener.endpoint().to_owned();
    let server = listener
        .server
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .cloned()
        .expect("the unconsumed instance");
    let mut server_session = 0_u32;
    // SAFETY: this live process PID and writable session output are valid.
    assert_ne!(
        unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
        0
    );
    let (gate_entered_tx, gate_entered_rx) = mpsc::sync_channel(1);
    let (gate_release_tx, gate_release_rx) = mpsc::channel();
    listener.install_before_consume_gate(TestConsumeGate {
        entered: gate_entered_tx,
        release: gate_release_rx,
    });
    let acceptor_listener = Arc::clone(&listener);
    let acceptor = thread::spawn(move || {
        acceptor_listener
            .accept_until(
                Instant::now() + Duration::from_secs(5),
                |pid, session, facts| {
                    if session != server_session || facts.session_id != session {
                        return None;
                    }
                    let pin = TestPeerProcess::open(pid, session)?;
                    (pin.token_facts == *facts).then_some(pin)
                },
            )
            .map(|peer| peer.is_some())
    });
    let mut client = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "bootstrap::named_pipe_tests::install_rendezvous_bootstrap_reports_exact_pipe_peer_after_hello",
            "--nocapture",
        ])
        .env(ENDPOINT_ENV, &endpoint)
        .env(SERVER_PID_ENV, std::process::id().to_string())
        .env(SERVER_SESSION_ENV, server_session.to_string())
        .env("KELD_TEST_LIFECYCLE_START_GATED", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lifecycle client");
    client
        .stdin
        .as_mut()
        .expect("client start gate")
        .write_all(&[1])
        .expect("release lifecycle client");
    gate_entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("server reached the final boundary before the one-shot");
    server
        .cancel()
        .expect("cancel the instance before the one-shot");
    gate_release_tx
        .send(())
        .expect("release the final boundary");
    let accepted = acceptor.join().expect("join lifecycle acceptor");
    let output = client
        .wait_with_output()
        .expect("wait the lifecycle client");
    assert!(
        matches!(&accepted, Err(error) if error.raw_os_error() == Some(995)),
        "a cancelled instance refuses its one-shot: {accepted:?}"
    );
    assert!(!output.status.success(), "the client requires LR1");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("CLIENT_LIFECYCLE_OK"),
        "the client emits no accepted marker without the receipt"
    );
    let stale = WindowsNamedPipeServer::connect_client(&endpoint)
        .expect_err("the refused one-shot closes the locator");
    assert_eq!(stale.raw_os_error(), Some(2));
}

#[test]
fn cancellation_handle_does_not_keep_dropped_listener_alive() {
    let listener = WindowsNamedPipeBootstrapListener::bind().expect("bind");
    let endpoint = listener.endpoint().to_owned();
    let cancellation = listener.cancellation();
    drop(listener);
    cancellation
        .cancel()
        .expect("cancel after owner drop is inert");
    let stale = WindowsNamedPipeServer::connect_client(&endpoint)
        .expect_err("non-owning cancellation view must not preserve pipe handle");
    assert_eq!(stale.raw_os_error(), Some(2));
}

#[expect(
    clippy::expect_used,
    reason = "test cycle helper reports the exact failed handle-lifecycle boundary"
)]
fn run_cancelled_accept_cycle() {
    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind cycle"));
    let cancellation = listener.cancellation();
    let worker_listener = Arc::clone(&listener);
    let worker = thread::spawn(move || {
        worker_listener.accept_authenticated_until(
            Instant::now() + Duration::from_secs(2),
            &super::NoopRejectionObserver,
        )
    });
    let pending_deadline = Instant::now() + Duration::from_secs(1);
    while !listener.is_accept_pending() {
        assert!(
            Instant::now() < pending_deadline,
            "cycle accept never became pending"
        );
        thread::yield_now();
    }
    cancellation.cancel().expect("cancel cycle");
    assert!(matches!(
        worker.join().expect("join cycle").expect("cycle result"),
        WindowsNamedPipeBootstrapAdmission::Cancelled
    ));
}

#[test]
fn repeated_cancellation_returns_process_handle_count_to_baseline() {
    const CHILD_ENV: &str = "KELD_TEST_PIPE_HANDLE_CENSUS_CHILD";
    if std::env::var_os(CHILD_ENV).is_none() {
        let output = Command::new(std::env::current_exe().expect("current test binary"))
            .args([
                "--exact",
                "bootstrap::named_pipe_tests::cancellation_ownership::repeated_cancellation_returns_process_handle_count_to_baseline",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .output()
            .expect("run isolated handle census");
        assert!(
            output.status.success(),
            "isolated handle census failed: status={:?}, stdout={}, stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("test result: ok. 1 passed; 0 failed;"),
            "isolated census must execute exactly one test: {stdout}"
        );
        return;
    }

    run_cancelled_accept_cycle();
    let baseline = process_handle_count().expect("baseline handle count");
    for _ in 0..32 {
        run_cancelled_accept_cycle();
    }
    let final_count = process_handle_count().expect("final handle count");
    assert_eq!(
        final_count, baseline,
        "pipe, cancel-event, or per-accept event handle leaked across cycles"
    );
}
