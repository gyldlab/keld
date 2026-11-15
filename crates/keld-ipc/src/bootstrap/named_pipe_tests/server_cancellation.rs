//! One-shot server cancellation and shutdown-race scenarios.

use super::{
    AppLinkDeadlines, Arc, Duration, Instant, PoisonError, TestConsumeGate, WaitOutcome,
    WindowsNamedPipeBootstrapAdmission, WindowsNamedPipeBootstrapListener, WindowsNamedPipeServer,
    client, handshake_client, parse_app_link,
};
use std::io::{self, Read as _, Write as _};
use std::sync::mpsc;
use std::thread;

/// A cancelled, not yet consumed server instance refuses I/O issued after
/// the cancel at once with `ERROR_OPERATION_ABORTED` (995), the code a
/// cancelled pending operation reports: `CancelIoEx` reaches only the
/// operations pending at the cancel, so without the gate such a read
/// would wait out its own deadline. The cancel also refuses the
/// instance's one-shot, so the gate is never lifted. An instance consumed
/// before a cancel is never refused: that cancel is a no-op and the
/// admitted stream reads on. Oracles: the raw code, the elapsed time
/// against the read deadline, and the byte the client wrote.
#[test]
fn a_cancelled_unconsumed_server_stream_refuses_new_io_and_its_one_shot() {
    let connected = || {
        let endpoint = super::super::random_pipe_endpoint().expect("mint pipe endpoint");
        let server = WindowsNamedPipeServer::bind(&endpoint).expect("bind pipe server");
        let client = WindowsNamedPipeServer::connect_client(&endpoint).expect("client");
        assert_eq!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(5)))
                .expect("accept the client"),
            WaitOutcome::Ready
        );
        let stream = server.stream().expect("server stream");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read deadline");
        (server, client, stream)
    };
    let (server, mut client, mut stream) = connected();
    server.cancel().expect("cancel the unconsumed instance");
    let refused_at = Instant::now();
    let mut byte = [0_u8; 1];
    let read = stream
        .read(&mut byte)
        .expect_err("a read issued after the cancel is refused");
    assert_eq!(read.raw_os_error(), Some(995), "{read}");
    let write = stream
        .write(&[1])
        .expect_err("a write issued after the cancel is refused");
    assert_eq!(write.raw_os_error(), Some(995), "{write}");
    assert!(
        refused_at.elapsed() < Duration::from_secs(1),
        "refused at once, not at the read deadline"
    );
    assert!(
        !server.consume(),
        "a cancel that acted first refuses the one-shot"
    );
    client.write_all(&[7]).expect("client write");
    let still = stream
        .read(&mut byte)
        .expect_err("a refused one-shot never lifts the gate");
    assert_eq!(still.raw_os_error(), Some(995), "{still}");

    let (server, mut client, mut stream) = connected();
    assert!(
        server.consume(),
        "an uncancelled instance takes its one-shot"
    );
    server
        .cancel()
        .expect("a cancel after the one-shot is a no-op");
    client.write_all(&[7]).expect("client write");
    assert_eq!(
        stream.read(&mut byte).expect("an admitted stream reads"),
        1,
        "a cancel after the one-shot never refuses the admitted stream"
    );
    assert_eq!(byte, [7]);
}

/// The failure the serialisation prevents, reported when admission
/// returned while `shutdown`'s cancel was held between its check and its
/// act: the admitted stream's first read is made pending, and only then is
/// the cancel let act.
fn late_shutdown_cancel_on_the_admitted_stream(
    admitted: io::Result<WindowsNamedPipeBootstrapAdmission>,
    server: &WindowsNamedPipeServer,
    act: &mpsc::Sender<()>,
    shutdown: thread::JoinHandle<io::Result<()>>,
    kill: Instant,
) -> String {
    let mut stream = match admitted {
        Ok(WindowsNamedPipeBootstrapAdmission::Authenticated(stream)) => stream,
        other => {
            let _ = act.send(());
            return format!("the one-shot did not wait for shutdown's cancel: {other:?}");
        }
    };
    // Shares the instance's test-only I/O census; it performs no I/O itself.
    let census = server.stream();
    let deadline = stream.set_app_link_read_deadline(Some(Duration::from_secs(20)));
    let reader = thread::spawn(move || {
        let mut byte = [0_u8; 1];
        stream.read(&mut byte)
    });
    let pending = census.map(|census| {
        while !census.has_active_io() && Instant::now() < kill {
            thread::yield_now();
        }
        census.has_active_io()
    });
    let _ = act.send(());
    let shut = shutdown.join();
    let read = reader.join();
    format!(
        "admission returned the authenticated stream while shutdown's cancel was in flight; \
         once the cancel acted, the stream's first read returned {read:?} (read pending: \
         {pending:?}; read deadline: {deadline:?}; shutdown: {shut:?})"
    )
}

/// Bootstrap shutdown racing the one-shot: `shutdown`'s cancel has passed
/// its "not yet consumed" check and is held there, before it signals or
/// aborts anything, when the admission worker, already past its `stopping`
/// checks, takes the one-shot. Cancel and one-shot are serialised on the
/// instance, so the one-shot waits for that cancel and is then refused:
/// admission reports `Cancelled`, the authenticated client reads end of
/// file, and the locator is closed. Unserialised, admission would return
/// the authenticated stream and the cancel's `CancelIoEx` would then abort
/// that stream's first read with `ERROR_OPERATION_ABORTED` (995).
/// Oracles: the admission outcome, the client's end of file and the closed
/// name. Which way the worker went is read from the instance's test-only
/// witness that a thread waits for its lifecycle lock, or from the worker
/// having returned; a kill switch bounds both.
#[test]
fn shutdown_in_flight_at_the_bootstrap_one_shot_wins_and_admits_nothing() {
    const KILL: Duration = Duration::from_secs(20);
    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("bind"));
    let link = listener.app_link();
    let (endpoint, token) = parse_app_link(&link).expect("parse link");
    let endpoint = endpoint.to_owned();
    let server = listener
        .server
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .cloned()
        .expect("the unconsumed instance");
    let (reached, at_one_shot) = mpsc::sync_channel(1);
    let (take_one_shot, one_shot_released) = mpsc::channel();
    server.install_before_consume_gate(TestConsumeGate {
        entered: reached,
        release: one_shot_released,
    });
    let (checked, cancel_checked) = mpsc::sync_channel(1);
    let (act, acts) = mpsc::channel();
    server.install_cancel_gate(TestConsumeGate {
        entered: checked,
        release: acts,
    });
    let (waits, one_shot_waits) = mpsc::sync_channel(1);
    server.install_lifecycle_wait_witness(waits);
    let worker_listener = Arc::clone(&listener);
    let worker = thread::spawn(move || {
        worker_listener
            .accept_authenticated_until(Instant::now() + KILL, &super::NoopRejectionObserver)
    });
    let mut role = client(&endpoint).expect("open role");
    role.set_app_link_deadlines(Some(KILL))
        .expect("role deadlines");
    handshake_client(&mut role, &token).expect("HELLO authenticates");
    at_one_shot
        .recv_timeout(KILL)
        .expect("the worker reaches the one-shot past its stopping checks");
    let shutdown_listener = Arc::clone(&listener);
    let shutdown = thread::spawn(move || shutdown_listener.shutdown());
    cancel_checked
        .recv_timeout(KILL)
        .expect("shutdown's cancel passed its check");
    take_one_shot
        .send(())
        .expect("let the worker take the one-shot");
    let kill = Instant::now() + KILL;
    while one_shot_waits.try_recv().is_err() {
        if worker.is_finished() {
            let admitted = worker.join().expect("join admission");
            panic!(
                "{}",
                late_shutdown_cancel_on_the_admitted_stream(
                    admitted, &server, &act, shutdown, kill
                )
            );
        }
        assert!(
            Instant::now() < kill,
            "kill switch: the worker neither waited at the one-shot nor returned"
        );
        thread::yield_now();
    }
    act.send(()).expect("let the cancel act");
    shutdown.join().expect("join shutdown").expect("shutdown");
    let admitted = worker.join().expect("join admission").expect("admission");
    assert!(
        matches!(admitted, WindowsNamedPipeBootstrapAdmission::Cancelled),
        "shutdown in flight wins the one-shot: {admitted:?}"
    );
    let mut byte = [0_u8; 1];
    assert_eq!(
        role.read(&mut byte).expect("the client reads end of file"),
        0,
        "the client was not admitted"
    );
    let stale = WindowsNamedPipeServer::connect_client(&endpoint)
        .expect_err("the cancelled one-shot closes the locator");
    assert_eq!(stale.raw_os_error(), Some(2));
}
