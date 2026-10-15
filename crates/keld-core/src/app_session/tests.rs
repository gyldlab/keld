#[cfg(windows)]
use std::cell::Cell;
use std::path::Path;

use std::fs;
#[cfg(windows)]
use std::io::{BufRead as _, BufReader};
#[cfg(target_os = "macos")]
use std::os::unix::fs::{PermissionsExt, symlink};
#[cfg(windows)]
use std::process::{Command, Stdio};

use super::*;

const DIGEST: &str = "sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356";

#[cfg(any(target_os = "macos", target_os = "linux"))]
struct ReapedRouterProbe(std::process::Child);

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Drop for ReapedRouterProbe {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
const RECOVERED_CHILD_ENV: &str = "KELD_CORE_TEST_RECOVERED_EVENT_ORDER_CHILD";
#[cfg(any(target_os = "macos", target_os = "linux"))]
const RECOVERED_CHILD_OBSERVED: &str = "KELD_CORE_RECOVERED_EVENT_ORDER_OBSERVED";

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn assert_bounded_recovered_event_order_child() {
    use std::io::{Read as _, Seek as _, SeekFrom};
    use std::process::{Command, Stdio};

    const SELECTOR: &str =
        "app_session::tests::recovered_generation_orders_window_events_once_across_installation";
    let mut stdout = tempfile::tempfile().expect("private child stdout");
    let mut stderr = tempfile::tempfile().expect("private child stderr");
    let mut child = ReapedRouterProbe(
        Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", SELECTOR, "--nocapture"])
            .env(RECOVERED_CHILD_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone().expect("child stdout clone")))
            .stderr(Stdio::from(stderr.try_clone().expect("child stderr clone")))
            .spawn()
            .expect("spawn isolated event-order test"),
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("inspect event-order child") {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(10) {
            child.0.kill().expect("kill blocked event-order child");
            let status = child.0.wait().expect("reap blocked event-order child");
            stdout
                .seek(SeekFrom::Start(0))
                .expect("rewind child stdout");
            stderr
                .seek(SeekFrom::Start(0))
                .expect("rewind child stderr");
            let mut output = String::new();
            let mut detail = String::new();
            stdout
                .read_to_string(&mut output)
                .expect("read child stdout");
            stderr
                .read_to_string(&mut detail)
                .expect("read child stderr");
            panic!(
                "event-order child exceeded 10-second cleanup bound: status={status}, stdout={output}, stderr={detail}"
            );
        }
        std::thread::park_timeout(Duration::from_millis(10));
    };
    stdout
        .seek(SeekFrom::Start(0))
        .expect("rewind child stdout");
    stderr
        .seek(SeekFrom::Start(0))
        .expect("rewind child stderr");
    let mut output = String::new();
    let mut detail = String::new();
    stdout
        .read_to_string(&mut output)
        .expect("read child stdout");
    stderr
        .read_to_string(&mut detail)
        .expect("read child stderr");
    assert!(
        status.success()
            && output.contains(RECOVERED_CHILD_OBSERVED)
            && output.contains("test result: ok. 1 passed; 0 failed;"),
        "isolated event-order proof failed: status={status}, stdout={output}, stderr={detail}"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn stale_and_foreign_link_contexts_are_inert_before_echo_decode() {
    use std::os::unix::net::UnixStream;
    let (writer, _peer) = UnixStream::pair().expect("actual app link pair");
    let current_stop = Arc::new(AtomicBool::new(false));
    let current = Mutex::new(Some(ActivePrimaryGeneration {
        identity: PrimaryLinkIdentity::Fixture,
        attempt: 2,
        writer,
        reader_stop: Arc::clone(&current_stop),
        pending_quit: None,
    }));
    let shutdown = SessionShutdownState::new();
    for caller in [
        PrimaryCallerContext {
            attempt: 1,
            identity: PrimaryLinkIdentity::Fixture,
            reader_stop: Arc::clone(&current_stop),
        },
        PrimaryCallerContext {
            attempt: 2,
            identity: PrimaryLinkIdentity::Fixture,
            reader_stop: Arc::new(AtomicBool::new(false)),
        },
    ] {
        // Literal malformed Echo is a direct decoder-entry falsifier: deleting
        // either corresponding context guard must surface IPC003 instead of Ok.
        dispatch_primary_echo(&current, &shutdown, &caller, CorrelationId(71), &[0xff])
            .expect("retired or foreign context never reaches unprivileged decoder");
    }
}

#[test]
#[cfg(target_os = "macos")]
fn current_g2_context_dispatches_original_echo_and_stopped_context_is_inert() {
    use keld_ipc::link::{AppLinkDeadlines as _, read_frame};
    use std::os::unix::net::UnixStream;
    let (writer, mut peer) = UnixStream::pair().expect("actual g2 app link");
    peer.set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("existing observer bound");
    let stop = Arc::new(AtomicBool::new(false));
    let current = Mutex::new(Some(ActivePrimaryGeneration {
        identity: PrimaryLinkIdentity::Fixture,
        attempt: 2,
        writer,
        reader_stop: Arc::clone(&stop),
        pending_quit: None,
    }));
    let caller = PrimaryCallerContext {
        attempt: 2,
        identity: PrimaryLinkIdentity::Fixture,
        reader_stop: Arc::clone(&stop),
    };
    let shutdown = SessionShutdownState::new();
    let payload = encode(&keld_ipc::echo::EchoRequest {
        message: "current-g2".to_owned(),
        count: 72,
    })
    .expect("owning codec");
    dispatch_primary_echo(&current, &shutdown, &caller, CorrelationId(72), &payload)
        .expect("current original Echo");
    let (header, bytes) = read_frame(&mut peer).expect("actual g2 terminal wire");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, ECHO_CHANNEL, CorrelationId(72))
    );
    let reply: keld_ipc::echo::EchoResponse = decode(&bytes).expect("owning reply codec");
    assert_eq!((reply.message.as_str(), reply.count), ("current-g2", 72));
    stop.store(true, Ordering::Release);
    dispatch_primary_echo(&current, &shutdown, &caller, CorrelationId(73), &[0xff])
        .expect("stopped context remains inert before decoder");
}

#[test]
#[cfg(target_os = "macos")]
fn macos_publisher_and_signed_identifier_both_separate_profiles() {
    let first = MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "com.example.app")
        .expect("verified signing facts");
    let publisher_changed =
        MacValidatedAppIdentity::from_verified_parts("KLMNOPQRST", "com.example.app")
            .expect("verified publisher change");
    let app_changed =
        MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "com.example.other")
            .expect("verified signing identifier change");
    assert_ne!(first.profile_identity, publisher_changed.profile_identity);
    assert_ne!(first.profile_identity, app_changed.profile_identity);
    assert!(MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "Com.Example.App").is_err());
    assert!(MacValidatedAppIdentity::from_verified_parts("", "com.example.app").is_err());
}

#[test]
#[cfg(target_os = "macos")]
fn macos_dev_selection_never_reads_release_identity_and_changes_each_launch() {
    let first = select_macos_profile_mode_with(true, || {
        panic!("explicit dev lease must bypass release signing identity")
    })
    .expect("first ephemeral selection");
    let second = select_macos_profile_mode_with(true, || {
        panic!("explicit dev lease must bypass release signing identity")
    })
    .expect("second ephemeral selection");
    let (WebProfileSelection::EphemeralDev(first), WebProfileSelection::EphemeralDev(second)) =
        (first, second)
    else {
        panic!("dev lease selected a persistent profile")
    };
    assert_ne!(first.launch_nonce(), second.launch_nonce());
}

#[test]
#[cfg(target_os = "macos")]
fn macos_release_selection_rejects_signature_failure() {
    let error = select_macos_profile_mode_with(false, || {
        Err(macos_identity_error(
            "synthetic signature validation failure",
        ))
    })
    .expect_err("release mode must fail closed");
    assert_eq!(error.code(), "KELD-WV-009");
    assert!(
        error
            .to_string()
            .contains("synthetic signature validation failure")
    );
}

#[cfg(windows)]
struct ReapedTestChild(std::process::Child);

#[cfg(windows)]
impl Drop for ReapedTestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn valid_boot(entry: &str, renderer: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "name": "Fixture",
        "entry": entry,
        "renderer": renderer,
        "permissions": {
            "file": "keld.permissions.jsonc",
            "content_sha256": DIGEST,
        }
    }))
    .expect("serialize fixture boot")
}

fn must_err<T>(result: Result<T, HostAppError>, message: &str) -> HostAppError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

#[test]
#[cfg(windows)]
fn synthetic_windows_verified_parts_keep_publisher_and_app_separate() {
    let first = ValidatedAppIdentity::from_verified_parts([1; 32], "com.example.app")
        .expect("synthetic verified identity");
    let publisher_changed = ValidatedAppIdentity::from_verified_parts([2; 32], "com.example.app")
        .expect("synthetic publisher change");
    let app_changed = ValidatedAppIdentity::from_verified_parts([1; 32], "com.example.other")
        .expect("synthetic app change");
    assert_ne!(first.profile_identity, publisher_changed.profile_identity);
    assert_ne!(first.profile_identity, app_changed.profile_identity);
    for noncanonical in ["Com.Example.App", "com.example.app\n"] {
        let error = ValidatedAppIdentity::from_verified_parts([1; 32], noncanonical)
            .expect_err("a noncanonical verified app id has no profile identity");
        assert_eq!(error.code(), "KELD-WV-009");
    }
}

/// The session loop runs another session only after an `Ok` session that a
/// requester asked to follow: a lifecycle-Quit session, or one a handover ended
/// under either of its causes; every other outcome exits.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn direct_session_loop_runs_another_session_only_for_a_requested_lifecycle_quit() {
    assert_eq!(
        next_direct_session(true, SESSION_LIFECYCLE_QUIT, false, true),
        DirectSessionNext::RunAnother
    );
    // Production has no requester: one session, whatever the cause or the end.
    for cause in [
        SESSION_RUNNING,
        SESSION_LIFECYCLE_QUIT,
        SESSION_CLI_LEASE_LOST,
        SESSION_HANDOVER,
    ] {
        for ended_by_handover in [false, true] {
            assert_eq!(
                next_direct_session(true, cause, ended_by_handover, false),
                DirectSessionNext::Exit,
                "cause {cause} handover {ended_by_handover}"
            );
            assert_eq!(
                next_direct_session(false, cause, ended_by_handover, false),
                DirectSessionNext::Exit,
                "cause {cause} handover {ended_by_handover}"
            );
        }
    }
    // A requested session never follows a failed session, a CLI lease loss or a
    // session that returned without claiming a cause.
    assert_eq!(
        next_direct_session(false, SESSION_LIFECYCLE_QUIT, false, true),
        DirectSessionNext::Exit
    );
    assert_eq!(
        next_direct_session(true, SESSION_CLI_LEASE_LOST, false, true),
        DirectSessionNext::Exit
    );
    assert_eq!(
        next_direct_session(true, SESSION_RUNNING, false, true),
        DirectSessionNext::Exit
    );
}

/// A handover-ended `Ok` session may be followed under either of its causes, but
/// only when the session result carries the handover fact: the handover cause alone
/// never admits another session, nor does the fact with a failed session, a CLI
/// lease loss or no request.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn direct_session_loop_runs_another_session_after_a_requested_handover() {
    assert_eq!(
        next_direct_session(true, SESSION_HANDOVER, true, true),
        DirectSessionNext::RunAnother
    );
    assert_eq!(
        next_direct_session(true, SESSION_LIFECYCLE_QUIT, true, true),
        DirectSessionNext::RunAnother
    );
    assert_eq!(
        next_direct_session(true, SESSION_HANDOVER, false, true),
        DirectSessionNext::Exit,
        "the handover cause alone is not the handover fact"
    );
    assert_eq!(
        next_direct_session(false, SESSION_HANDOVER, true, true),
        DirectSessionNext::Exit
    );
    assert_eq!(
        next_direct_session(true, SESSION_CLI_LEASE_LOST, true, true),
        DirectSessionNext::Exit
    );
    assert_eq!(
        next_direct_session(true, SESSION_HANDOVER, true, false),
        DirectSessionNext::Exit
    );
    assert!(is_forced_cause(SESSION_HANDOVER));
    assert!(is_forced_cause(SESSION_CLI_LEASE_LOST));
    assert!(!is_forced_cause(SESSION_LIFECYCLE_QUIT));
    assert!(!is_forced_cause(SESSION_RUNNING));
}

/// The handover claim is one more claimant of the single session cause: the
/// first claim wins in either order, and a handover claim leaves the tail to be
/// started by this thread.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn session_shutdown_handover_claim_is_first_come_first_served() {
    let quit_first = SessionShutdownState::new();
    assert!(quit_first.reserve_guarded(SESSION_LIFECYCLE_QUIT));
    assert!(!quit_first.claim_guarded(SESSION_HANDOVER));
    assert_eq!(
        quit_first.cause(),
        SESSION_RUNNING,
        "reserved, not yet accepted"
    );
    assert!(quit_first.accept_reserved_guarded(SESSION_LIFECYCLE_QUIT));
    assert_eq!(quit_first.cause(), SESSION_LIFECYCLE_QUIT);

    let handover_first = SessionShutdownState::new();
    assert!(handover_first.claim_guarded(SESSION_HANDOVER));
    assert_eq!(handover_first.cause(), SESSION_HANDOVER);
    assert!(!handover_first.reserve_guarded(SESSION_LIFECYCLE_QUIT));
    assert!(!handover_first.is_running());
    assert!(
        handover_first.reader_stop.load(Ordering::Acquire),
        "the handover claim stops the readers as every claim does"
    );
}

/// The main thread's bounded wait (step 3b) returns as soon as another claimant
/// reserved the cause, and at the deadline claims the handover under the same
/// guard hold as its last running check (step 3c).
#[test]
#[cfg(any(target_os = "linux", windows))]
fn handover_wait_ends_on_a_claim_or_claims_at_the_deadline() {
    let shutdown = SessionShutdownState::new();
    let claimant = shutdown.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let quit = thread::spawn(move || {
        started_rx.recv().expect("the waiter started");
        claimant.claim(SESSION_LIFECYCLE_QUIT)
    });
    started_tx.send(()).expect("start the claimant");
    let waited = Instant::now();
    let wait = shutdown.await_quit_or_claim_handover(Instant::now() + Duration::from_secs(20));
    assert_eq!(wait, HandoverWait::ClaimedByOther);
    assert!(
        waited.elapsed() < Duration::from_secs(10),
        "the claim's notification ended the wait, not the deadline"
    );
    assert!(quit.join().expect("claimant joins"));
    assert_eq!(shutdown.cause(), SESSION_LIFECYCLE_QUIT);

    let expired = SessionShutdownState::new();
    assert_eq!(
        expired.await_quit_or_claim_handover(Instant::now()),
        HandoverWait::ClaimedHandover
    );
    assert_eq!(expired.cause(), SESSION_HANDOVER);
    assert!(!expired.reserve_guarded(SESSION_LIFECYCLE_QUIT));
    assert_eq!(
        expired.await_quit_or_claim_handover(Instant::now()),
        HandoverWait::ClaimedByOther,
        "a session already ended has nothing left to claim"
    );
}

/// After the application's `Quit` claimed the session, the main thread waits
/// for that tail's end, which a `TailRun` guard signals on every exit path; a
/// CLI-lease-loss claim has no tail to await.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn handover_waits_for_the_quit_tail_to_end() {
    let shutdown = SessionShutdownState::new();
    assert!(shutdown.claim(SESSION_LIFECYCLE_QUIT));
    assert!(!shutdown.tail_ended.load(Ordering::Acquire));
    let runner = shutdown.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let tail = thread::spawn(move || {
        let _tail = runner.tail_run();
        started_rx.recv().expect("the waiter started");
        // The guard's drop ends the tail, even when the runner returns an error.
    });
    started_tx.send(()).expect("start the tail");
    shutdown.await_tail_end();
    assert!(shutdown.tail_ended.load(Ordering::Acquire));
    tail.join().expect("tail joins");

    let lease = SessionShutdownState::new();
    assert!(lease.claim(SESSION_CLI_LEASE_LOST));
    let waited = Instant::now();
    lease.await_tail_end();
    assert!(
        waited.elapsed() < Duration::from_secs(5),
        "a CLI-lease-loss claim is not awaited"
    );
}

/// A thread that saw the handover's `Quit` arrive, standing in for the main thread
/// after the UI loop returned, reads the handover fact under `transition`. It can
/// therefore never see the `Quit` without the fact, which is the post-loop
/// classification of a handover end.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn a_reader_that_saw_the_handover_quit_reads_the_handover_fact() {
    let (handle, _client, owner_rx, window_rx) = initial_ready_router_with_window_loop();
    drop(owner_rx);
    let shutdown = handle.shutdown.clone();
    let ui_loop = thread::spawn(move || {
        let command = window_rx.recv_timeout(Duration::from_secs(2));
        (command, shutdown.handover_requested_guarded())
    });
    assert!(handle.request_handover().expect("a running session"));
    let (command, observed) = ui_loop.join().expect("UI loop stand-in");
    assert_eq!(command, Ok(AppWindowCommand::Quit));
    assert!(
        observed,
        "a reader that saw the handover's Quit must see the handover fact"
    );
}

/// `request_handover` sends the loop exactly one `Quit` while the session runs;
/// from then on `finish_tail`'s UI step is a no-op even though the loop's command
/// receiver is gone, whereas without a handover the same closed channel stays the
/// typed failure it was. A request after a claim, or a second request, sends
/// nothing; a request with no loop fails typed.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn request_handover_ends_the_loop_once_and_finish_tail_then_sends_nothing() {
    let (handle, _client, owner_rx, window_rx) = initial_ready_router_with_window_loop();
    drop(owner_rx); // `finish_tail`'s guardian step succeeds at once without an owner.
    assert!(handle.request_handover().expect("a running session"));
    assert_eq!(
        window_rx.recv_timeout(Duration::from_secs(2)),
        Ok(AppWindowCommand::Quit)
    );
    assert!(handle.shutdown.handover_requested());
    assert!(
        handle.shutdown.handover_clock.requested.get().is_some(),
        "the request instant is recorded"
    );
    assert!(
        !handle
            .request_handover()
            .expect("a second request is refused, not failed")
    );
    assert!(
        window_rx.try_recv().is_err(),
        "nothing else reaches the loop"
    );
    // The wake bridge returned on the first terminal command: the receiver is gone.
    drop(window_rx);
    handle
        .finish_tail("lifecycle Quit")
        .expect("the UI step is a no-op once the handover ended the loop");

    let (plain, _plain_client, plain_owner_rx, plain_window_rx) =
        initial_ready_router_with_window_loop();
    drop(plain_owner_rx);
    drop(plain_window_rx);
    let error = plain
        .finish_tail("lifecycle Quit")
        .expect_err("without a handover the closed loop channel is a failure");
    assert!(
        error.to_string().contains("UI event loop is unavailable"),
        "{error}"
    );
    let error = plain
        .request_handover()
        .expect_err("a handover needs a live loop");
    assert!(
        error.to_string().contains("UI event loop is unavailable"),
        "{error}"
    );
    assert!(!plain.shutdown.handover_requested());

    let (ended, _ended_client, _ended_owner_rx, ended_window_rx) =
        initial_ready_router_with_window_loop();
    assert!(ended.shutdown.claim(SESSION_LIFECYCLE_QUIT));
    assert!(!ended.request_handover().expect("refused after a claim"));
    assert!(ended_window_rx.try_recv().is_err());
}

/// A session under a CLI dev lease fails the handover typed and leaves the session
/// untouched: nothing reaches the loop, no handover is recorded and the session
/// still runs, so its lease tail stays the only forced end.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn request_handover_refuses_a_session_under_a_dev_lease() {
    let (handle, _client, _owner_rx, window_rx) = initial_ready_router_with_window_loop();
    handle.shutdown.mark_under_dev_lease();
    let error = handle
        .request_handover()
        .expect_err("a dev-lease session never hands over");
    assert_eq!(error.code, "KELD-CORE-037");
    assert!(error.to_string().contains("CLI dev lease"), "{error}");
    assert!(!handle.shutdown.handover_requested());
    assert!(handle.shutdown.handover_clock.requested.get().is_none());
    assert!(handle.shutdown.is_running(), "the session still runs");
    assert!(window_rx.try_recv().is_err(), "nothing reaches the loop");
}

/// When the quit deadline D starts and what keeps a `Quit` admitted inside it its
/// claim (KEL-53 §4 "Profile handover", steps 3b and 3c).
#[cfg(any(target_os = "linux", windows))]
mod handover_quit_deadline;

/// The lease fact comes from the session's lease itself: attaching a dev lease's
/// tail to the router makes the handover refuse, and a session without a lease
/// attaches none and still hands over.
#[test]
#[cfg(windows)]
fn attaching_a_dev_lease_tail_makes_the_handover_refuse() {
    let (leased, _client, _owner_rx, leased_window_rx) = initial_ready_router_with_window_loop();
    let (observation_tx, observations) = mpsc::channel();
    let monitor = WindowsDevLeaseMonitor {
        observations,
        #[cfg(debug_assertions)]
        raw_handle_value: 0,
    };
    let lease_errors =
        start_direct_dev_lease_tail(Some(monitor), leased.clone()).expect("start the lease tail");
    assert!(lease_errors.is_some());
    let error = leased
        .request_handover()
        .expect_err("the leased session refuses");
    assert_eq!(error.code, "KELD-CORE-037");
    assert!(leased_window_rx.try_recv().is_err());
    // Ends the tail thread without an observation: its receive fails and it returns.
    drop(observation_tx);

    let (plain, _plain_client, _plain_owner_rx, plain_window_rx) =
        initial_ready_router_with_window_loop();
    assert!(
        start_direct_dev_lease_tail(None, plain.clone())
            .expect("no lease, no tail")
            .is_none()
    );
    assert!(plain.request_handover().expect("a lease-less session"));
    assert_eq!(
        plain_window_rx.recv_timeout(Duration::from_secs(2)),
        Ok(AppWindowCommand::Quit)
    );
}

/// Each application session of a process takes the sole broker once; the next
/// session gets a fresh one only after the previous transferred its own and
/// drained, with the dispatch state back at its initial shape.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn guard_snapshot_prepares_filesystem_state_for_each_session() {
    let temp = tempfile::tempdir().expect("FS session temp root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("allowed root");
    let scope = allowed.display().to_string().replace('\\', "/");
    let manifest_text = format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"]}}}}}}"#);
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, &manifest_text).expect("write FS manifest");
    // The loader's integrity refusal names the digest of the exact bytes it read.
    let Err(ManifestError::IntegrityMismatch { actual: digest, .. }) = load_verified_manifest(
        File::open(&manifest_path).expect("open FS manifest for its digest"),
        manifest_path.clone(),
        [0_u8; 32],
    ) else {
        panic!("a zero digest must be refused with the actual digest");
    };
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open FS manifest"),
        manifest_path,
        digest,
    )
    .expect("verify FS manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare the first session");

    // An untransferred broker means the previous session never ran its router.
    let error = snapshot
        .prepare_next_session()
        .expect_err("a session that never took its broker cannot be followed");
    assert!(
        error.to_string().contains("never transferred its broker"),
        "{error}"
    );
    drop(
        snapshot
            .take_initial_broker()
            .expect("first session broker"),
    );
    let error = snapshot
        .take_initial_broker()
        .expect_err("the sole broker transfers once per session");
    assert!(error.to_string().contains("already transferred"), "{error}");

    // Undrained calls refuse the reset and leave no broker behind.
    snapshot.fs.state.lock().expect("dispatch state").in_flight = 1;
    let error = snapshot
        .prepare_next_session()
        .expect_err("an undrained session cannot be followed");
    assert!(error.to_string().contains("have not drained"), "{error}");
    assert!(
        snapshot.take_initial_broker().is_err(),
        "a refused preparation leaves no broker"
    );
    {
        let mut state = snapshot.fs.state.lock().expect("dispatch state");
        state.in_flight = 0;
        state.prepared_attempt = Some(3);
        state.accepting = false;
    }

    snapshot
        .prepare_next_session()
        .expect("the drained session is followed by a prepared one");
    {
        let state = snapshot.fs.state.lock().expect("dispatch state");
        assert!(state.accepting, "the next session accepts from its start");
        assert_eq!(
            state.prepared_attempt,
            Some(1),
            "attempts restart per session"
        );
        assert_eq!(state.in_flight, 0);
    }
    drop(
        snapshot
            .take_initial_broker()
            .expect("second session broker"),
    );
    assert!(
        snapshot.take_initial_broker().is_err(),
        "the second session's broker also transfers once"
    );
}

/// The debug seam grants exactly one extra session, never under a dev lease.
#[test]
#[cfg(all(debug_assertions, windows))]
fn second_session_seam_requests_one_session_without_a_dev_lease() {
    let mut seam = TestSessionSeam::from_request(true, false, false)
        .expect("an installed host may request one extra session");
    assert!(
        !seam.take_handover_at_ready(),
        "no handover without its trigger"
    );
    assert!(seam.take_request());
    assert!(!seam.take_request(), "exactly one extra session");
    assert!(!seam.take_request());

    let mut absent =
        TestSessionSeam::from_request(false, false, false).expect("no request, no error");
    assert!(!absent.take_request());
    assert!(!absent.take_handover_at_ready());
    let mut absent_dev =
        TestSessionSeam::from_request(false, false, true).expect("a dev lease alone is fine");
    assert!(!absent_dev.take_request());

    let error = TestSessionSeam::from_request(true, false, true)
        .expect_err("a dev-stage host cannot run a second session");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(error.to_string().contains("under a dev lease"), "{error}");
}

/// The handover seam requests one handover, for the first session only, and one
/// extra session after it; never under a dev lease, never beside the
/// second-session trigger.
#[test]
#[cfg(all(debug_assertions, windows))]
fn handover_seam_requests_one_handover_and_one_session_without_a_dev_lease() {
    let mut seam = TestSessionSeam::from_request(false, true, false)
        .expect("an installed host may request one handover");
    assert!(seam.take_handover_at_ready());
    assert!(
        !seam.take_handover_at_ready(),
        "the handover belongs to the first session only"
    );
    assert!(seam.take_request());
    assert!(!seam.take_request(), "exactly one extra session");

    let error = TestSessionSeam::from_request(false, true, true)
        .expect_err("a dev-stage host cannot hand over");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(
        error
            .to_string()
            .contains("test handover requested under a dev lease"),
        "{error}"
    );
    let error = TestSessionSeam::from_request(true, true, false)
        .expect_err("one evidence trigger per process");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(error.to_string().contains("requested together"), "{error}");
}

#[test]
#[cfg(windows)]
fn windows_profile_follows_the_boot_route() {
    let development =
        select_windows_profile_mode(None, true).expect("a dev stage runs under its lease");
    assert!(matches!(development, WindowsProfileMode::EphemeralDev));

    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
        .expect("synthetic verified identity");
    let verified_profile = identity.profile_identity;
    let installed = select_windows_profile_mode(Some(identity), false)
        .expect("an installed package runs without a lease");
    let WindowsProfileMode::Persistent(selection) = installed else {
        panic!("an installed package selected an ephemeral profile")
    };
    assert_eq!(
        selection,
        WebProfileSelection::Persistent(verified_profile),
        "the profile is the identity verified at boot"
    );

    let error = select_windows_profile_mode(None, false)
        .expect_err("a dev stage without a live lease must not run");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(error.to_string().contains("no live dev lease"), "{error}");
    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
        .expect("synthetic verified identity");
    let error = select_windows_profile_mode(Some(identity), true)
        .expect_err("an installed package must not run under a dev lease");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(
        error.to_string().contains("presented a dev lease"),
        "{error}"
    );
}

#[test]
#[cfg(windows)]
fn windows_dev_lease_routes_only_to_the_dev_stage_validator() {
    let error = select_windows_boot_route_with(
        true,
        || Err(target_error("app root", "synthetic dev-stage refusal")),
        || panic!("a dev lease must not reach installed-package boot"),
    )
    .expect_err("the synthetic dev-stage validator decides the dev route");
    assert_eq!(error.code(), "KELD-CORE-036");
    assert!(error.to_string().contains("synthetic dev-stage refusal"));
}

#[test]
#[cfg(windows)]
fn windows_lease_less_boot_reaches_only_installed_package_boot() {
    let unsigned = select_windows_boot_route_with(
        false,
        || panic!("a lease-less launch must not reach the dev-stage validator"),
        || {
            Err(windows_identity_error(
                "synthetic verifier rejection",
                "Use a real signed package fixture.",
            ))
        },
    )
    .expect_err("an unverified lease-less executable is refused");
    assert_eq!(unsigned.code(), "KELD-WV-009");

    let unselected = select_windows_boot_route_with(
        false,
        || panic!("a lease-less launch must not reach the dev-stage validator"),
        || {
            Err(installed_package_error(
                &keld_update::UpdateError::ExecutableBinding {
                    image: keld_update::WindowsLocatedImage::Host,
                    step: "locator",
                    detail: "synthetic staged layout".to_owned(),
                },
            ))
        },
    )
    .expect_err("a verified host outside an installation is refused");
    assert_eq!(unselected.code(), "KELD-UPDATE-018");
    assert!(
        unselected.to_string().contains("synthetic staged layout"),
        "{unselected}"
    );
}

#[test]
#[cfg(windows)]
fn installed_package_refusal_carries_its_code_once() {
    let error = installed_package_error(&keld_update::UpdateError::ExecutableBinding {
        image: keld_update::WindowsLocatedImage::Host,
        step: "locator",
        detail: "synthetic staged layout".to_owned(),
    });
    let rendered = error.to_string();
    println!("KELD_KEL254_INSTALLED_PACKAGE_REFUSAL {rendered}");
    // The resource snapshot reads process-wide counters other tests advance.
    let (message, _resources) = rendered
        .split_once(" [startup-resource-attempts ")
        .expect("a host error ends with its startup-resource snapshot");
    assert_eq!(
        message,
        "KELD-UPDATE-018: no-flag host failed during Windows installed package selection \
             — installed executable locator refused (synthetic staged layout). Launch \
             keld-host.exe from its installation's selected version tree; repair or reinstall \
             through the trusted installer if the layout is damaged. Then relaunch the \
             installed host."
    );
    assert_eq!(rendered.matches("KELD-UPDATE-").count(), 1, "{rendered}");
    assert!(!rendered.contains(".."), "{rendered}");
}

#[test]
#[cfg(windows)]
fn installed_boot_of_an_unsigned_host_refuses_before_resources() {
    let before = startup_resource_snapshot();
    let error = validate_installed_current_exe()
        .expect_err("the unsigned test executable is not an installed package");
    assert_eq!(error.code(), "KELD-WV-009");
    // The running image opened as a pinned trust image and was refused by its owner.
    assert!(
        error
            .to_string()
            .contains("rejected the current executable with status"),
        "{error}"
    );
    assert_eq!(
        error.resources, before,
        "installed boot advanced app resources"
    );
}

#[test]
#[cfg(windows)]
fn installed_boot_reads_the_expectation_from_the_handle_before_selection() {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let executable = std::env::current_exe().expect("test executable path");
    // The test executable carries no `.keldeai` container and lies in no installation.
    let image = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&executable)
        .expect("open the test executable as the verified-image owner does");
    let locator = executable.canonicalize().expect("canonical locator");
    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
        .expect("synthetic verified identity");
    let before = startup_resource_snapshot();
    let error = validate_installed_from_verified(&locator, &image, identity)
        .expect_err("a host without an embedded expectation is refused");
    // KELD-UPDATE-019, not the locator's KELD-UPDATE-018: the expectation is read from
    // the handle before any installation is located.
    assert_eq!(error.code(), "KELD-UPDATE-019");
    assert!(error.to_string().contains("KELD-PACK-007"), "{error}");
    assert_eq!(
        error.resources, before,
        "installed boot advanced app resources"
    );
}

#[test]
#[cfg(windows)]
fn installed_boot_reads_the_verified_handle_never_the_locator_path() {
    use std::io::{Seek as _, SeekFrom, Write as _};

    // The locator names this test executable, a valid PE image without a container
    // (KELD-PACK-007 if anything reopened it). The handle is an anonymous file, with
    // no path to reopen and its cursor at end of file, holding no PE image at all.
    let locator = std::env::current_exe()
        .expect("test executable path")
        .canonicalize()
        .expect("canonical locator");
    let mut image = tempfile::tempfile().expect("anonymous image handle");
    image
        .write_all(b"MZ is not a host image")
        .expect("write the malformed image");
    image.seek(SeekFrom::End(0)).expect("cursor to end of file");
    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
        .expect("synthetic verified identity");
    let before = startup_resource_snapshot();
    let error = validate_installed_from_verified(&locator, &image, identity)
        .expect_err("a malformed verified image is refused");
    // KELD-PACK-006 is the handle's malformed image; a locator reopen would report the
    // test executable's missing container instead.
    assert_eq!(error.code(), "KELD-UPDATE-019");
    assert!(error.to_string().contains("KELD-PACK-006"), "{error}");
    assert_eq!(
        error.resources, before,
        "installed boot advanced app resources"
    );
}

#[test]
#[cfg(windows)]
fn windows_boot_files_validate_without_the_dev_stage_dacl() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("tree");
    fs::create_dir(&root).expect("tree");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join("index.html"), "<p>installed</p>\n").expect("renderer");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");

    // The inherited temporary-directory DACL is not owner-private: the dev route
    // still refuses this tree, so its predicate is unchanged.
    let error = must_err(validate_from_root(&root), "a non-private stage must fail");
    assert_eq!(error.code(), "KELD-CORE-036");
    assert!(error.to_string().contains("app root DACL"), "{error}");

    let selection = validate_windows_boot_files(root.clone(), WindowsBootMode::DevStage)
        .expect("the selected tree's boot files validate");
    assert_eq!(selection.app.root, root);
    assert_eq!(selection.app.name, "Fixture");
    assert_eq!(selection.app.entry_path, Path::new("main.ts"));
    assert_eq!(selection.app.renderer_html, b"<p>installed</p>\n");
    assert_eq!(
        selection.permissions_digest,
        decode_digest(DIGEST).expect("fixture digest")
    );
}

#[test]
#[cfg(windows)]
fn windows_boot_files_keep_the_strict_descriptor_and_target_rules() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("tree");
    fs::create_dir(&root).expect("tree");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.join(BOOT_FILE), br#"{"schema":1,"foreign":true}"#).expect("boot");
    let error = must_err(
        validate_windows_boot_files(root.clone(), WindowsBootMode::DevStage),
        "a malformed descriptor must fail",
    );
    assert_eq!(error.code(), "KELD-CORE-035");

    let outside = temp.path().join("outside.html");
    fs::write(&outside, "<p>outside</p>\n").expect("outside renderer");
    std::os::windows::fs::symlink_file(&outside, root.join("index.html"))
        .expect("renderer symlink");
    fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");
    let error = must_err(
        validate_windows_boot_files(root, WindowsBootMode::DevStage),
        "a reparse-point renderer must fail",
    );
    assert_eq!(error.code(), "KELD-CORE-036");
    assert!(error.to_string().contains("reparse point"), "{error}");
}

#[test]
#[cfg(windows)]
fn windows_trust_image_refusal_is_reported_as_wv009() {
    let directory = tempfile::tempdir().expect("trust-image fixture directory");
    let target = directory.path().join("target.exe");
    let link = directory.path().join("link.exe");
    fs::write(&target, b"trust image fixture").expect("write trust-image target");
    std::os::windows::fs::symlink_file(&target, &link).expect("create a leaf symlink");

    // keld-guard refuses the leaf reparse point; keld-core owns the code.
    let error = verified_windows_image(&link).expect_err("a leaf reparse point is refused");
    assert_eq!(error.code(), "KELD-WV-009");
    assert!(error.to_string().contains("reparse point"), "{error}");
}

/// Authenticode FFI names that only the `keld-guard` owner may use. Split
/// literals keep these needles from matching this test's own source.
const AUTHENTICODE_FFI_NEEDLES: [&str; 8] = [
    concat!("WinTrust", "::"),
    concat!("Cryptography", "::"),
    concat!("WinVerify", "Trust"),
    concat!("WT", "Helper"),
    concat!("CryptDecode", "Object"),
    concat!("CryptEncode", "Object"),
    concat!("CryptQuery", "Object"),
    concat!("Crypt", "Msg"),
];

fn authenticode_ffi_needles_in(source: &str) -> Vec<&'static str> {
    AUTHENTICODE_FFI_NEEDLES
        .into_iter()
        .filter(|needle| source.contains(needle))
        .collect()
}

/// The KEL-135 Authenticode FFI has one owner in `keld-guard` (KEL-270 D4).
#[test]
fn keld_core_source_holds_no_authenticode_ffi() {
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut scanned = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("read a keld-core source directory") {
            let path = entry.expect("read a keld-core source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = fs::read_to_string(&path).expect("read a keld-core source file");
                scanned += 1;
                let found = authenticode_ffi_needles_in(&source);
                assert!(
                    found.is_empty(),
                    "{} names {found:?}; call the keld-guard Authenticode owner instead",
                    path.display()
                );
            }
        }
    }
    assert!(scanned > 1, "no keld-core source files were scanned");
}

#[test]
fn authenticode_ffi_scan_flags_each_needle() {
    assert_eq!(
        authenticode_ffi_needles_in("fn clean() {}"),
        [] as [&str; 0]
    );
    for needle in AUTHENTICODE_FFI_NEEDLES {
        let planted = format!("fn planted() {{ let _ = {needle}; }}");
        assert_eq!(authenticode_ffi_needles_in(&planted), [needle]);
    }
}

#[test]
#[cfg(windows)]
fn current_test_executable_is_rejected_as_a_release_carrier() {
    let executable = current_windows_executable().expect("test executable path");
    let error = verified_windows_image(&executable)
        .expect_err("the test executable is not an approved signed Keld package carrier");
    assert_eq!(error.code(), "KELD-WV-009");
    // The running image opened as a pinned trust image and was refused by its owner.
    assert!(
        error
            .to_string()
            .contains("rejected the current executable with status"),
        "{error}"
    );
}

/// Verifies the specified host carrier, or this signed libtest for older fixtures.
#[test]
#[ignore = "requires a trusted Authenticode-signed fixture executable"]
#[cfg(windows)]
fn kel135_signed_package_acceptance_fixture() {
    let carrier = std::env::var_os("KELD_KEL135_CARRIER_UNDER_TEST")
        .map_or_else(current_windows_executable, |path| Ok(PathBuf::from(path)))
        .expect("the signed fixture carrier path");
    let (_verified, identity) = verified_windows_image(&carrier)
        .expect("the signed fixture must produce a verified Windows identity");
    let app_id = identity.app_id.clone();
    let publisher_scope = identity.publisher_scope.iter().fold(
        String::with_capacity(identity.publisher_scope.len() * 2),
        |mut text, byte| {
            use std::fmt::Write as _;
            write!(&mut text, "{byte:02x}").expect("format publisher scope");
            text
        },
    );
    let profile_namespace = identity.profile_identity.namespace_segment();
    let mode = select_windows_profile_mode(Some(identity), false)
        .expect("the verified signed fixture must select a persistent profile");
    assert!(matches!(mode, WindowsProfileMode::Persistent(_)));
    println!(
        "KELD_KEL135_SIGNED_IDENTITY app_id={app_id} publisher_scope={publisher_scope} profile_namespace={profile_namespace}"
    );
}

/// Proves the signed acceptance fixture binary includes the isolated-profile-root build.
#[test]
#[ignore = "probes only the debug KEL-135 profile test-root feature"]
#[cfg(all(windows, feature = "kel135-profile-test-root"))]
fn kel135_profile_test_root_probe_fixture() {
    let root = std::env::var_os("KELD_PROFILE_TEST_ROOT")
        .expect("the profile test-root probe needs the requested path");
    println!("KELD_PROFILE_TEST_ROOT_FEATURE {}", root.to_string_lossy());
}

/// Runs only from a deliberately signed copy of this libtest binary.
#[test]
#[ignore = "requires a trusted Authenticode-signed fixture executable"]
#[cfg(windows)]
fn kel135_signed_package_purge_acceptance_fixture() {
    let executable = current_windows_executable().expect("signed fixture path");
    let (_verified, identity) = verified_windows_image(&executable)
        .expect("the signed fixture must produce a verified Windows identity");
    let profile_namespace = identity.profile_identity.namespace_segment();
    WebView2Engine::purge_persistent_profile(identity.profile_identity)
        .expect("the signed fixture must purge its authenticated idle profile");
    println!("KELD_KEL135_SIGNED_PURGE profile_namespace={profile_namespace}");
}

#[test]
#[cfg(windows)]
fn quit_peer_close_wait_is_bounded_for_a_non_closing_client() {
    let (mut server, client) = primary_test_stream_pair();
    server
        .set_app_link_read_deadline(Some(Duration::from_millis(10)))
        .expect("server poll deadline");

    let started = Instant::now();
    let error = await_windows_quit_peer_close_until(
        &mut server,
        Instant::now() + Duration::from_millis(50),
    )
    .expect_err("non-closing client must hit the peer-close deadline");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "peer-close deadline did not bound the wait"
    );
    assert!(error.to_string().contains("did not close"), "{error}");
    drop(client);
}

#[test]
#[cfg(windows)]
fn dev_stage_cleanup_retries_only_transient_final_directory_sharing() {
    let temp = tempfile::tempdir().expect("cleanup fixture root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("cleanup stage");
    fs::write(root.join("entry"), b"fixture").expect("cleanup stage entry");

    let mut blocker = ReapedTestChild(
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Write-Output (Get-Location).Path; $null = [Console]::In.ReadLine()",
            ])
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn final-directory holder"),
    );
    let mut blocker_stdin = blocker.0.stdin.take();
    let mut ready = String::new();
    BufReader::new(blocker.0.stdout.take().expect("holder readiness pipe"))
        .read_line(&mut ready)
        .expect("holder readiness");
    let observed_ready = Path::new(ready.trim_end())
        .canonicalize()
        .expect("canonical holder working directory");
    let expected_root = root.canonicalize().expect("canonical cleanup stage");
    assert_eq!(observed_ready, expected_root);

    let bare_error = fs::remove_dir_all(&root)
        .expect_err("bare Windows removal must expose the transient sharing violation");
    assert_eq!(
        bare_error.raw_os_error(),
        Some(WINDOWS_SHARING_VIOLATION),
        "{bare_error}"
    );
    assert!(root.is_dir(), "sharing failure removed the held directory");

    let mut exhausted_retry_count = 0;
    let exhausted = remove_windows_dev_stage_with(&root, || false, || exhausted_retry_count += 1)
        .expect_err("expired cleanup deadline must preserve the sharing violation");
    assert_eq!(
        exhausted.raw_os_error(),
        Some(WINDOWS_SHARING_VIOLATION),
        "{exhausted}"
    );
    assert_eq!(
        exhausted_retry_count, 0,
        "expired cleanup deadline entered the retry path"
    );

    let mut retry_count = 0;
    let retry_budget = Cell::new(10_000_u32);
    remove_windows_dev_stage_with(
        &root,
        || {
            let remaining = retry_budget.get();
            if remaining == 0 {
                false
            } else {
                retry_budget.set(remaining - 1);
                true
            }
        },
        || {
            retry_count += 1;
            if let Some(stdin) = blocker_stdin.take() {
                drop(stdin);
                blocker.0.wait().expect("reap final-directory holder");
            }
            thread::yield_now();
        },
    )
    .expect("bounded cleanup after holder exit");

    assert!(retry_count > 0, "sharing violation did not exercise retry");
    assert!(!root.exists(), "cleanup left the final directory");

    let file = temp.path().join("not-a-directory");
    fs::write(&file, b"fixture").expect("non-directory fixture");
    let mut unrelated_retry_count = 0;
    let unrelated = remove_windows_dev_stage_with(&file, || true, || unrelated_retry_count += 1)
        .expect_err("non-directory cleanup must fail");
    assert_ne!(
        unrelated.raw_os_error(),
        Some(WINDOWS_SHARING_VIOLATION),
        "{unrelated}"
    );
    assert_eq!(
        unrelated_retry_count, 0,
        "non-sharing error entered the retry path"
    );
}

#[test]
fn strict_boot_schema_accepts_exact_v1() {
    let parsed = parse_boot_bytes(&valid_boot("src/main.ts", "index.html"))
        .expect("exact schema-v1 descriptor");
    assert_eq!(parsed.name, "Fixture");
    assert_eq!(parsed.entry, Path::new("src/main.ts"));
    assert_eq!(parsed.renderer, Path::new("index.html"));
    assert_eq!(parsed.permissions_digest.len(), 32);
}

#[test]
fn strict_schema_v1_boot_carries_auth_without_filtering_profiles() {
    let bytes = br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"fixedRedirect":"http://localhost:53161/callback"},"future-provider":{}}}"#;
    let parsed = parse_boot_bytes(bytes).expect("schema-v1 auth carrier");
    assert_eq!(
        parsed.auth_configuration.len(),
        2,
        "Core must not filter provider keys"
    );
    let entra = parsed
        .auth_configuration
        .get("entra-common")
        .expect("known profile");
    assert_eq!(
        entra.client_id, None,
        "missing clientId remains data until Native post-Allow"
    );
    assert_eq!(
        entra.fixed_redirect.as_deref(),
        Some("http://localhost:53161/callback")
    );
    assert!(parsed.auth_configuration.contains_key("future-provider"));
}

#[test]
fn strict_schema_v1_boot_rejects_unknown_auth_profile_fields() {
    let cases = [
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientID":"operator-client"}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientId":"operator-client","fixedRedirct":"http://localhost:53161/callback"}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"future-provider":{"clientID":"unused"}}}"#.as_slice(),
        ];
    for bytes in cases {
        let error = must_err(
            parse_boot_bytes(bytes),
            "unknown inner auth field must refuse",
        );
        assert_eq!(error.code(), "KELD-CORE-035");
    }
}

#[test]
fn strict_schema_v1_auth_null_fields_preserve_absence() {
    let bytes = br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientId":null,"fixedRedirect":null}}}"#;
    let parsed = parse_boot_bytes(bytes).expect("existing Option null compatibility");
    let profile = parsed
        .auth_configuration
        .get("entra-common")
        .expect("known profile");
    assert!(profile.client_id.is_none());
    assert!(profile.fixed_redirect.is_none());
}

#[test]
fn strict_schema_v1_carries_optional_as_return_data() {
    let mut value = serde_json::json!({
        "schema": 1, "name": "Fixture", "entry": "src/main.ts",
        "renderer": "index.html", "permissions": {
            "file": "keld.permissions.jsonc",
            "content_sha256": "sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"
        }, "auth": { "entra-common": { "asRedirect": "not-a-valid-registration" } }
    });
    for binding in [
        serde_json::json!("not-a-valid-registration"),
        serde_json::Value::Null,
    ] {
        value["auth"]["entra-common"]["asRedirect"] = binding.clone();
        let bytes = serde_json::to_vec(&value).expect("boot fixture bytes");
        let parsed = parse_boot_bytes(&bytes).expect("AS binding is boot data");
        let profile = parsed
            .auth_configuration
            .get("entra-common")
            .expect("profile");
        assert_eq!(profile.as_redirect.as_deref(), binding.as_str());
        assert!(profile.client_id.is_none());
        assert!(profile.fixed_redirect.is_none());
    }
}

#[test]
fn strict_schema_v1_boot_without_auth_defaults_to_empty() {
    let bytes = br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#;
    let parsed = parse_boot_bytes(bytes).expect("legacy schema-v1 boot descriptor");
    assert!(parsed.auth_configuration.is_empty());
}

#[test]
fn strict_schema_v1_boot_rejects_duplicate_auth_shape() {
    let cases = [
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{},"auth":{"entra-common":{}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientId":"first","clientId":"second"}}}"#.as_slice(),
            // The map owner must reject repeated decoded profile keys as well
            // as duplicate struct fields. These are independent raw JSON literals.
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientId":"first"},"entra-common":{"clientId":"second"}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{},"entra\u002dcommon":{}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"unknown-profile":{},"unknown-profile":{}}}"#.as_slice(),
        ];
    for bytes in cases {
        let error = must_err(parse_boot_bytes(bytes), "duplicate auth shape must fail");
        assert_eq!(error.code(), "KELD-CORE-035");
    }
}

#[cfg(unix)]
#[test]
fn schema_v1_auth_carrier_survives_actual_root_validation() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = tempfile::tempdir().expect("temporary parent");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage root");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("private mode");
    fs::create_dir(root.join("src")).expect("source directory");
    fs::write(root.join("src/main.ts"), b"export {};\n").expect("entry");
    fs::write(root.join("index.html"), b"<p>fixture</p>\n").expect("renderer");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    // Independent literal: this is the exact serialized schema-v1 boot carrier.
    fs::write(
            root.join(BOOT_FILE),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"fixedRedirect":"http://localhost:53161/callback"},"future-provider":{}}}"#,
        )
        .expect("boot descriptor");

    let selection = validate_from_root(&root).expect("validate real staged boot files");
    assert_eq!(selection.app.auth_configuration.len(), 2);
    let entra = selection
        .app
        .auth_configuration
        .get("entra-common")
        .expect("configured profile");
    assert_eq!(
        entra.client_id, None,
        "clientId remains for Native post-Allow handling"
    );
    assert_eq!(
        entra.fixed_redirect.as_deref(),
        Some("http://localhost:53161/callback")
    );
    assert!(
        selection
            .app
            .auth_configuration
            .contains_key("future-provider")
    );
}

#[test]
fn bounded_boot_rejects_zero_limit_plus_one_and_non_utf8() {
    assert!(parse_boot_bytes(&[]).is_err());
    let exact = vec![b' '; MAX_BOOT_BYTES];
    let exact_error = must_err(parse_boot_bytes(&exact), "spaces are not JSON");
    assert_eq!(exact_error.code(), "KELD-CORE-035");
    let over = vec![b' '; MAX_BOOT_BYTES + 1];
    let over_error = must_err(parse_boot_bytes(&over), "64 KiB + 1 must fail");
    assert!(over_error.to_string().contains("64 KiB"), "{over_error}");
    let utf8_error = must_err(parse_boot_bytes(&[0xff]), "non-UTF-8 must fail");
    assert!(utf8_error.to_string().contains("UTF-8"), "{utf8_error}");
}

#[test]
fn duplicate_unknown_version_name_and_permissions_fields_fail_closed() {
    let cases = [
        r#"{"schema":1,"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"x","entry":"a","renderer":"b","unknown":1,"permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":2,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"wrong.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"SHA256:CA3D163BAB055381827226140568F3BEF7EAAC187CEBD76878E0B63E9E442356"}}"#,
    ];
    for bytes in cases {
        assert!(
            parse_boot_bytes(bytes.as_bytes()).is_err(),
            "accepted: {bytes}"
        );
    }
}

#[test]
fn kipc_transport_sidecar_admission_skips_absent_and_accepts_regular_file() {
    let temp = tempfile::tempdir().expect("sidecar root");
    let sidecar = temp.path().join("kipc-transport.ts");
    assert!(
        !admit_kipc_transport_sidecar(&sidecar).expect("absent sidecar"),
        "NotFound must allow self-contained entries"
    );
    fs::write(&sidecar, "export {}\n").expect("regular sidecar");
    assert!(
        admit_kipc_transport_sidecar(&sidecar).expect("regular sidecar"),
        "regular file must be eligible to bind"
    );
}

#[test]
fn kipc_transport_sidecar_admission_rejects_directory() {
    let temp = tempfile::tempdir().expect("sidecar root");
    let sidecar = temp.path().join("kipc-transport.ts");
    fs::create_dir(&sidecar).expect("directory sidecar");
    let error = must_err(
        admit_kipc_transport_sidecar(&sidecar),
        "directory sidecar must fail closed",
    );
    assert!(error.to_string().contains("not a regular file"), "{error}");
}

#[test]
fn fs_route_keeps_guard_evaluation_out_of_core_production_source() {
    let source = include_str!("../app_session.rs");
    let production = source
        .split_once("\n#[cfg(test)]\nmod tests")
        .map(|(production, _)| production)
        .expect("tests module marker");
    let forbidden_dispatch = ["dispatch", "_privileged("].concat();
    let forbidden_evaluate = ["keld_guard::", "evaluate("].concat();
    assert!(
        !production.contains(&forbidden_dispatch),
        "keld-core must route to native rather than call the privileged dispatcher"
    );
    assert!(
        !production.contains(&forbidden_evaluate),
        "keld-core must never become a second guard evaluator"
    );
}

#[test]
fn kipc_transport_sidecar_admission_uses_lstat_not_follow() {
    let source = include_str!("../app_session.rs");
    let start = source
        .find("fn admit_kipc_transport_sidecar")
        .expect("admission helper");
    let body = source
        .get(start..start.saturating_add(900))
        .expect("admission helper body");
    assert!(
        body.contains("symlink_metadata"),
        "admission must lstat so a sidecar symlink is not followed into --ro-bind"
    );
    assert!(
        !body.contains(".metadata()"),
        "Path::metadata follows symlinks and would reintroduce CWE-59"
    );
    assert!(
        body.contains("is_symlink()"),
        "admission must reject a symlink before readonly_runtime"
    );
}

#[test]
#[cfg(unix)]
fn kipc_transport_sidecar_admission_rejects_symlink_to_regular_file() {
    let temp = tempfile::tempdir().expect("sidecar root");
    let outside = temp.path().join("outside.ts");
    fs::write(&outside, "export const steal = 1;\n").expect("external target");
    let sidecar = temp.path().join("kipc-transport.ts");
    std::os::unix::fs::symlink(&outside, &sidecar).expect("sidecar symlink");
    // `metadata()` follows and would admit this as a regular file — CWE-59.
    assert!(
        fs::metadata(&sidecar).expect("follow").is_file(),
        "negative control: following the symlink sees a regular file"
    );
    let error = must_err(
        admit_kipc_transport_sidecar(&sidecar),
        "symlink sidecar must fail closed before --ro-bind",
    );
    assert!(error.to_string().contains("symbolic link"), "{error}");
}

#[test]
fn portable_relative_paths_reject_normalized_escape_and_empty_components() {
    for path in [
        "",
        ".",
        "..",
        "/abs",
        "a/../b",
        "a/./b",
        "a//b",
        "a/",
        "C:\\boot.ts",
        "C:/boot.ts",
        "\\\\server\\share",
        "a\\b",
    ] {
        let error = must_err(
            parse_boot_bytes(&valid_boot(path, "index.html")),
            "unsafe entry path must fail",
        );
        assert!(error.to_string().contains("entry"), "{path}: {error}");
    }
}

#[test]
#[cfg(target_os = "macos")]
fn validated_root_opens_same_handle_targets_and_rejects_symlink_escape() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    fs::create_dir(root.join("src")).expect("src");
    fs::write(root.join("src/main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join("index.html"), "<p id=fixture>exact</p>\n").expect("renderer");
    fs::write(root.join("keld.permissions.jsonc"), b"{}\n").expect("permissions");
    fs::write(
        root.join("keld.boot.json"),
        valid_boot("src/main.ts", "index.html"),
    )
    .expect("boot");

    let selection = validate_from_root(&root).expect("validated selection");
    assert_eq!(selection.app.renderer_html, b"<p id=fixture>exact</p>\n");
    assert_eq!(selection.app.name, "Fixture");

    let mut substituted: serde_json::Value =
        serde_json::from_slice(&valid_boot("src/main.ts", "index.html"))
            .expect("substitute boot JSON");
    substituted["name"] = serde_json::Value::String("Substituted".to_owned());
    fs::write(
        root.join("keld.boot.json"),
        serde_json::to_vec(&substituted).expect("substitute boot bytes"),
    )
    .expect("replace sidecar after selection");

    let outside = temp.path().join("outside.html");
    fs::write(&outside, "outside").expect("outside");
    fs::remove_file(root.join("index.html")).expect("remove renderer");
    symlink(&outside, root.join("index.html")).expect("escape symlink");
    assert_eq!(
        selection.app.renderer_html, b"<p id=fixture>exact</p>\n",
        "post-selection renderer substitution changed consumed bytes"
    );
    assert_eq!(
        selection.app.name, "Fixture",
        "post-selection sidecar substitution changed owned fields"
    );
    let error = must_err(validate_from_root(&root), "symlink escape must fail");
    assert!(error.to_string().contains("renderer"), "{error}");
}

#[test]
#[cfg(target_os = "macos")]
fn guarded_preflight_preserves_read_error_before_resources() {
    use std::fs::OpenOptions;

    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join("index.html"), "<p>guarded</p>\n").expect("renderer");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");

    let mut selection = validate_from_root(&root).expect("validated selection");
    selection.permissions_file = OpenOptions::new()
        .write(true)
        .open(root.join(PERMISSIONS_FILE))
        .expect("write-only retained handle");
    let before = startup_resource_snapshot();

    let error = run_guarded(selection).expect_err("read failure must fail preflight");
    assert_eq!(error.code(), "KELD-GUARD004");
    assert_eq!(error.resources, before, "preflight advanced app resources");
    let message = error.to_string();
    assert!(message.contains("Check the path"), "{message}");
    assert!(
        message.contains("rebuild the staged boot artifact"),
        "{message}"
    );
    let source = std::error::Error::source(&error).expect("manifest source");
    assert!(source.downcast_ref::<ManifestError>().is_some());
    assert_eq!(
        source.to_string(),
        error.manifest_source.as_ref().unwrap().to_string()
    );
}

#[test]
#[cfg(target_os = "macos")]
fn guarded_primary_route_reaches_native_fs_broker_with_allow_and_deny() {
    use std::os::unix::net::UnixStream;

    use keld_ipc::link::{read_frame, write_frame};
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("FS route temp root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("allowed root");
    let scope = allowed.display().to_string().replace('\\', "/");
    let manifest_text =
        format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, &manifest_text).expect("write FS manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open FS manifest"),
        manifest_path,
        digest,
    )
    .expect("verify FS manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare FS broker");

    let (server, mut client) = UnixStream::pair().expect("primary FS pair");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("primary FS read deadline");
    let (window_tx, _window_rx) = mpsc::channel();
    let (guardian_tx, _guardian_rx) = mpsc::channel();
    let router = PrimaryRouter::start_with_fs(
        server,
        window_tx,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
        Some(&snapshot),
    )
    .expect("guarded primary router");

    let target = allowed.join("route.txt");
    let target_wire = target.display().to_string().replace('\\', "/");
    let write = FsRequest::Write {
        path: target_wire.clone(),
        bytes: b"through-primary".to_vec(),
    };
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(41),
        &encode(&write).expect("encode allowed write"),
    )
    .expect("send allowed write");
    let (write_header, write_payload) = read_frame(&mut client).expect("allowed write reply");
    assert_eq!(write_header.kind, FrameKind::Reply);
    assert_eq!(write_header.channel, FS_CHANNEL);
    assert_eq!(write_header.corr, CorrelationId(41));
    assert!(matches!(
        decode::<FsResponse>(&write_payload).expect("decode write response"),
        FsResponse::Write
    ));
    assert_eq!(
        fs::read(&target).expect("read written bytes"),
        b"through-primary"
    );

    let read = FsRequest::Read { path: target_wire };
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(42),
        &encode(&read).expect("encode allowed read"),
    )
    .expect("send allowed read");
    let (read_header, read_payload) = read_frame(&mut client).expect("allowed read reply");
    assert_eq!(read_header.kind, FrameKind::Reply);
    assert_eq!(read_header.channel, FS_CHANNEL);
    assert_eq!(read_header.corr, CorrelationId(42));
    match decode::<FsResponse>(&read_payload).expect("decode read response") {
        FsResponse::Read { bytes } => assert_eq!(bytes, b"through-primary"),
        FsResponse::Write => panic!("read returned write response"),
    }

    let denied = temp.path().join("denied.txt");
    let denied_wire = denied.display().to_string().replace('\\', "/");
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(43),
        &encode(&FsRequest::Write {
            path: denied_wire,
            bytes: b"must-not-write".to_vec(),
        })
        .expect("encode denied write"),
    )
    .expect("send denied write");
    let (deny_header, deny_payload) = read_frame(&mut client).expect("denied write Err");
    assert_eq!(deny_header.kind, FrameKind::Err);
    assert_eq!(deny_header.channel, FS_CHANNEL);
    assert_eq!(deny_header.corr, CorrelationId(43));
    let error: CallError = decode(&deny_payload).expect("decode guard denial");
    assert_eq!(error.code, "KELD-GUARD002");
    assert!(!denied.exists(), "denied primary route created its target");

    router.shutdown().expect("guarded primary router shutdown");
}

#[test]
#[cfg(target_os = "macos")]
fn fs_worker_reply_write_failure_retires_generation_and_notifies_owner() {
    use std::os::unix::net::UnixStream;

    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("FS worker write-failure root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("FS worker write-failure allowed root");
    let target = allowed.join("input.txt");
    fs::write(&target, b"reply write failure").expect("FS worker write-failure input");
    let scope = allowed.display().to_string().replace('\\', "/");
    let manifest_text = format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"]}}}}}}"#);
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, &manifest_text).expect("write FS worker write-failure manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open FS worker write-failure manifest"),
        manifest_path,
        digest,
    )
    .expect("verify FS worker write-failure manifest");
    let snapshot =
        GuardSnapshot::prepare(verified).expect("prepare FS worker write-failure broker");

    let (server, peer) = UnixStream::pair().expect("FS worker write-failure pair");
    drop(peer);
    let reader_stop = Arc::new(AtomicBool::new(false));
    let current = Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
        identity: PrimaryLinkIdentity::Fixture,
        attempt: 1,
        writer: server,
        reader_stop: Arc::clone(&reader_stop),
        pending_quit: None,
    })));
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let (window_tx, window_rx) = mpsc::channel();
    let shutdown = SessionShutdownState::new();
    let handle = PrimaryRouterHandle {
        current: Arc::clone(&current),
        readers: Arc::new(Mutex::new(HashMap::new())),
        pending_echo: Arc::new(Mutex::new(None)),
        pending_echo_attempt: Arc::new(AtomicU32::new(0)),
        pending_echo_corr: Arc::new(AtomicU32::new(0)),
        next_host_corr: Arc::new(AtomicU32::new(1)),
        window_ready: Arc::new(AtomicBool::new(true)),
        last_window_closed: Arc::new(AtomicBool::new(false)),
        recovery_armed: Arc::new(AtomicBool::new(true)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        shutdown,
        fs: Some(snapshot.fs_weak()),
        fs_worker_commands: None,
        #[cfg(target_os = "macos")]
        auth: None,
        #[cfg(target_os = "macos")]
        auth_commands: None,
        guardian: GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        window_commands: window_tx,
        #[cfg(target_os = "macos")]
        quit_drain_hooks: Arc::default(),
    };

    let pending = PendingFsCall {
        attempt: 1,
        correlation: CorrelationId(90),
    };
    let admitted = snapshot
        .fs
        .begin_call(pending)
        .expect("admit FS worker write-failure request")
        .expect("FS worker write-failure admission remains open");
    let work = FsWorkItem {
        admitted,
        request: FsRequest::Read {
            path: target.display().to_string().replace('\\', "/"),
        },
        attempt: 1,
        correlation: CorrelationId(90),
        cancellation: Arc::new(AtomicBool::new(false)),
    };
    let (commands, receiver) = mpsc::channel();
    commands
        .send(FsWorkerCommand::Work(work))
        .expect("queue FS worker write-failure work");
    commands
        .send(FsWorkerCommand::Stop)
        .expect("queue worker stop after failed reply");

    let owner = thread::spawn(move || {
        let command = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("retired FS generation recovery request");
        let GuardianOwnerCommand::FailRetiredGeneration(attempt, reply) = command else {
            panic!("failed FS reply used the wrong generation-owner command");
        };
        assert_eq!(attempt, 1);
        reply
            .send(Ok(Instant::now()))
            .expect("acknowledge retired FS generation recovery");
    });

    run_fs_worker(
        &receiver,
        &handle,
        snapshot
            .take_initial_broker()
            .expect("transfer worker broker"),
        1,
        None,
    )
    .expect("reply write failure must retire the generation without killing the worker");
    owner.join().expect("retired-generation owner joins");

    assert!(
        current.lock().expect("current generation").is_none(),
        "failed terminal write left the corrupted generation installed"
    );
    assert!(
        reader_stop.load(Ordering::Acquire),
        "failed terminal write did not stop the retired generation reader"
    );
    assert!(
        matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "recoverable FS reply failure incorrectly fataled the app session"
    );
    let state = snapshot
        .fs
        .state
        .lock()
        .expect("FS state after failed reply");
    assert_eq!(state.in_flight, 0);
    assert!(state.pending_call.is_none());
    assert!(state.failed_write_attempt.is_none());
}

#[test]
#[cfg(target_os = "macos")]
fn successor_install_does_not_wait_for_retired_fs_recovery_ack() {
    use std::os::unix::net::UnixStream;

    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("FS recovery ordering root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("FS recovery ordering allowed root");
    let target = allowed.join("input.txt");
    fs::write(&target, b"recovery ordering").expect("FS recovery ordering input");
    let scope = allowed.display().to_string().replace('\\', "/");
    let manifest_text = format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"]}}}}}}"#);
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, &manifest_text).expect("write FS recovery ordering manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open FS recovery ordering manifest"),
        manifest_path,
        digest,
    )
    .expect("verify FS recovery ordering manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare FS recovery ordering broker");

    let (g1_server, g1_peer) = UnixStream::pair().expect("G1 recovery ordering pair");
    drop(g1_peer);
    let reader_stop = Arc::new(AtomicBool::new(false));
    let current = Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
        identity: PrimaryLinkIdentity::Fixture,
        attempt: 1,
        writer: g1_server,
        reader_stop: Arc::clone(&reader_stop),
        pending_quit: None,
    })));
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let (window_tx, window_rx) = mpsc::channel();
    let (commands, receiver) = mpsc::sync_channel(1);
    let handle = PrimaryRouterHandle {
        current: Arc::clone(&current),
        readers: Arc::new(Mutex::new(HashMap::new())),
        pending_echo: Arc::new(Mutex::new(None)),
        pending_echo_attempt: Arc::new(AtomicU32::new(0)),
        pending_echo_corr: Arc::new(AtomicU32::new(0)),
        next_host_corr: Arc::new(AtomicU32::new(1)),
        window_ready: Arc::new(AtomicBool::new(false)),
        last_window_closed: Arc::new(AtomicBool::new(false)),
        recovery_armed: Arc::new(AtomicBool::new(true)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        shutdown: SessionShutdownState::new(),
        fs: Some(snapshot.fs_weak()),
        fs_worker_commands: Some(commands.clone()),
        #[cfg(target_os = "macos")]
        auth: None,
        #[cfg(target_os = "macos")]
        auth_commands: None,
        guardian: GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        window_commands: window_tx,
        #[cfg(target_os = "macos")]
        quit_drain_hooks: Arc::default(),
    };
    let router = PrimaryRouter {
        handle: handle.clone(),
        fs_worker: None,
        #[cfg(target_os = "macos")]
        auth_worker: None,
    };

    let admitted = snapshot
        .fs
        .begin_call(PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(120),
        })
        .expect("admit G1 recovery-ordering FS call")
        .expect("G1 recovery-ordering admission remains open");
    let work = FsWorkItem {
        admitted,
        request: FsRequest::Read {
            path: target.display().to_string().replace('\\', "/"),
        },
        attempt: 1,
        correlation: CorrelationId(120),
        cancellation: Arc::new(AtomicBool::new(false)),
    };
    commands
        .send(FsWorkerCommand::Work(work))
        .expect("queue recovery-ordering FS work");

    let worker_handle = handle.clone();
    let worker_broker = snapshot
        .take_initial_broker()
        .expect("transfer worker broker");
    let worker =
        thread::spawn(move || run_fs_worker(&receiver, &worker_handle, worker_broker, 1, None));
    let command = guardian_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("failed terminal write must request retired-generation recovery");
    let GuardianOwnerCommand::FailRetiredGeneration(attempt, recovery_reply) = command else {
        panic!("failed terminal write used the wrong owner command");
    };
    assert_eq!(attempt, 1);
    assert_eq!(
        snapshot
            .fs
            .state
            .lock()
            .expect("failed-write recovery state")
            .failed_write_attempt,
        Some(1),
        "failed-write recovery marker was not published before owner notification"
    );
    assert!(
        current
            .lock()
            .expect("retired current generation")
            .is_none(),
        "failed terminal write did not retire G1 before owner notification"
    );

    let (g2_server, _g2_client) = UnixStream::pair().expect("G2 recovery ordering pair");
    let g2_handle = handle.clone();
    let (installed_tx, installed_rx) = mpsc::sync_channel(1);
    let install = thread::spawn(move || {
        let result = g2_handle.install_generation(2, g2_server);
        installed_tx.send(result).expect("report G2 installation");
    });

    let install_report = installed_rx.recv_timeout(Duration::from_secs(2));
    if install_report.is_err() {
        let _ = commands.send(FsWorkerCommand::Stop);
        let _ = recovery_reply.send(Ok(Instant::now()));
        let _ = worker.join();
        let _ = install.join();
        panic!("G2 installation waited for the retired-generation recovery acknowledgment");
    }
    install_report
        .expect("G2 installation report")
        .expect("G2 installs while owner recovery acknowledgment is pending");
    assert!(
        matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "recovery ordering sent an unexpected fatal window command"
    );

    commands
        .send(FsWorkerCommand::Stop)
        .expect("queue recovery-ordering worker stop");
    recovery_reply
        .send(Ok(Instant::now()))
        .expect("acknowledge retired-generation recovery after G2 installation");
    worker
        .join()
        .expect("recovery-ordering worker joins")
        .expect("recovery-ordering worker stays healthy");
    install.join().expect("G2 installer joins");
    router
        .shutdown()
        .expect("recovery-ordering router shutdown");
}

#[test]
#[cfg(target_os = "macos")]
fn fs_owner_receipt_poll_keeps_submission_deadline_and_original_error() {
    let (fixture, client) = guarded_test_router();
    let deadline = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("past real monotonic deadline");
    let (reply, received) = mpsc::sync_channel(1);
    reply
        .send(Ok(Instant::now()))
        .expect("late actual owner receipt");
    let pending = PendingFsWriteFailure {
        attempt: 1,
        session: Arc::clone(&fixture.snapshot.fs),
        primary: app_detail(
            "filesystem terminal reply",
            "original terminal write failure",
        ),
        reply: received,
        deadline,
    };
    let Err(error) = pending.poll() else {
        panic!("a late owner receipt renewed the submission deadline");
    };
    assert!(
        error
            .to_string()
            .contains("original terminal write failure"),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("did not acknowledge retired generation"),
        "{error}"
    );

    // A receipt observed by the owner within the same absolute budget stays
    // valid if native destruction delays the worker's next poll.
    let (reply, received) = mpsc::sync_channel(1);
    let completed = Instant::now();
    let deadline = completed;
    reply
        .send(Ok(completed))
        .expect("already completed owner receipt");
    let pending = PendingFsWriteFailure {
        attempt: 1,
        session: Arc::clone(&fixture.snapshot.fs),
        primary: app_detail(
            "filesystem terminal reply",
            "original terminal write failure",
        ),
        reply: received,
        deadline,
    };
    assert!(
        matches!(pending.poll(), Ok(None)),
        "on-time owner receipt became a false timeout"
    );
    fixture
        .router
        .shutdown()
        .expect("receipt deadline fixture cleanup");
    drop(client);
}

/// One guarded primary router over a socket pair, with the FS worker held
/// by a test gate after it takes its job (GH-528 T2 router tests).
#[cfg(target_os = "macos")]
struct GuardedTestRouter {
    router: PrimaryRouter,
    snapshot: GuardSnapshot,
    allowed: PathBuf,
    taken: Receiver<()>,
    release: Sender<()>,
    window: Receiver<AppWindowCommand>,
    guardian: Receiver<TestPrimaryOwnerCommand>,
    _temp: tempfile::TempDir,
}

/// A guarded router over a socket pair, and the pair's client end.
#[cfg(target_os = "macos")]
fn guarded_test_router() -> (GuardedTestRouter, std::os::unix::net::UnixStream) {
    use keld_ipc::link::AppLinkDeadlines as _;

    let (server, client) = std::os::unix::net::UnixStream::pair().expect("guarded pair");
    client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("guarded client deadlines");
    (guarded_router(server), client)
}

/// A guarded router over `server`: a pair end, or a Bun role's
/// authenticated app link.
#[cfg(target_os = "macos")]
fn guarded_router(server: std::os::unix::net::UnixStream) -> GuardedTestRouter {
    guarded_router_with_test_setup(server, None)
}

#[cfg(target_os = "macos")]
fn guarded_router_with_test_setup(
    server: std::os::unix::net::UnixStream,
    setup: Option<GuardedFsTestSetup>,
) -> GuardedTestRouter {
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("guarded router root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("guarded router allowed root");
    let scope = allowed.display().to_string().replace('\\', "/");
    let manifest_text =
        format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, &manifest_text).expect("write guarded router manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open guarded router manifest"),
        manifest_path,
        digest,
    )
    .expect("verify guarded router manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare guarded router broker");
    let (taken_tx, taken) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::channel();
    let mut gate = FsWorkerTestGate {
        taken: taken_tx,
        release: Mutex::new(release_rx),
        before_entry: None,
        initializer: Mutex::new(None),
    };
    if let Some(setup) = setup {
        setup(&mut gate);
    }
    let gate = Arc::new(gate);
    let (window_tx, window) = mpsc::channel();
    let (guardian_tx, guardian) = mpsc::channel();
    let router = PrimaryRouter::start_with_fs_test_gate(
        server,
        window_tx,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
        Some(&snapshot),
        Some(gate),
    )
    .expect("guarded test router");
    GuardedTestRouter {
        router,
        snapshot,
        allowed,
        taken,
        release,
        window,
        guardian,
        _temp: temp,
    }
}

#[cfg(target_os = "macos")]
mod accepted_primary_identity;

/// GH-528 T2 end-to-end cases: a Bun `WorkerLink` role against this
/// router (`app_session/tests/worker_link_e2e.rs`).
#[cfg(target_os = "macos")]
pub(in crate::app_session) mod worker_link_e2e;

#[cfg(target_os = "macos")]
fn write_fs_call(client: &mut std::os::unix::net::UnixStream, corr: u32, target: &Path) {
    write_frame(
        client,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(corr),
        &encode(&FsRequest::Write {
            path: target.display().to_string().replace('\\', "/"),
            bytes: b"t2".to_vec(),
        })
        .expect("encode FS write"),
    )
    .expect("send FS write");
}

#[cfg(target_os = "macos")]
fn write_quit_call(client: &mut std::os::unix::net::UnixStream, corr: u32) {
    write_frame(
        client,
        FrameKind::Call,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(corr),
        &encode(&LifecycleRequest::Quit).expect("encode Quit"),
    )
    .expect("send Quit");
}

/// Every frame the client reads before the link ends (EOF or reset).
#[cfg(target_os = "macos")]
fn read_frames_until_eof(
    client: &mut std::os::unix::net::UnixStream,
) -> Vec<(keld_ipc::FrameHeader, Vec<u8>)> {
    let mut frames = Vec::new();
    while let Ok(frame) = keld_ipc::link::read_frame(client) {
        frames.push(frame);
    }
    frames
}

/// GH-528 T2, spec gh527 criterion 6: retiring a generation answers its
/// pending calls on ERR-declaring channels with `KELD-IPC-023` before the
/// link closes. An admitted FS call held in its handler, and a Quit waiting
/// in its FS drain, are both pending; an echo call is answered at once and
/// never gets an ERR.
#[test]
#[cfg(target_os = "macos")]
fn retire_answers_pending_fs_and_quit_calls_with_023_before_close() {
    let (t, mut client) = guarded_test_router();
    assert_echo_call(&mut client, 10, "answered before retire");
    write_fs_call(&mut client, 11, &t.allowed.join("held.txt"));
    t.taken
        .recv_timeout(Duration::from_secs(5))
        .expect("worker took the FS job");
    let (drain_tx, drain_rx) = mpsc::sync_channel(1);
    t.snapshot.fs.observe_next_drain_wait(drain_tx);
    write_quit_call(&mut client, 12);
    drain_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("Quit waits in its FS drain");

    t.router.handle().retire_generation(1).expect("retire g1");
    let frames = read_frames_until_eof(&mut client);
    let answers: Vec<(FrameKind, keld_ipc::ChannelId, CorrelationId, String)> = frames
        .iter()
        .map(|(header, payload)| {
            let error: CallError = decode(payload).expect("retire answer is a CallError");
            (header.kind, header.channel, header.corr, error.code)
        })
        .collect();
    assert_eq!(
        answers,
        vec![
            (
                FrameKind::Err,
                FS_CHANNEL,
                CorrelationId(11),
                "KELD-IPC-023".to_owned()
            ),
            (
                FrameKind::Err,
                LIFECYCLE_CHANNEL,
                CorrelationId(12),
                "KELD-IPC-023".to_owned()
            ),
        ],
        "exactly the two pending calls are answered, then the link closes"
    );

    t.release.send(()).expect("release FS worker");
    t.router.shutdown().expect("router shutdown after retire");
    assert!(matches!(
        t.guardian.try_recv(),
        Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
    ));
}

/// GH-528 T2: a peer that stops reading cannot hold an accepted Quit open.
/// The client floods CALLs behind its Quit and never reads, so the host's
/// `KELD-IPC-024` answers fill both socket buffers. Each answer is one
/// best-effort write bounded by `HOST_ANSWER_BUDGET`, and the first that
/// times out ends the drain, so the Quit tail reaches its guardian
/// shutdown well before one writer deadline. The bound under test is
/// itself a time limit; the margin is one budget plus a poll (under 0.5 s)
/// against the 5 s `APP_LINK_IO_DEADLINE` that an unbounded write would
/// wait.
#[test]
#[cfg(target_os = "macos")]
fn quit_drain_answers_are_bounded_when_the_peer_stops_reading() {
    let (t, client) = guarded_test_router();
    let mut writer = client.try_clone().expect("flooding writer");
    let target = t.allowed.join("never.txt");
    let flood = std::thread::spawn(move || {
        write_quit_call(&mut writer, 30);
        for corr in 31..1_031 {
            let frame = write_frame(
                &mut writer,
                FrameKind::Call,
                0,
                FS_CHANNEL,
                CorrelationId(corr),
                &encode(&FsRequest::Write {
                    path: target.display().to_string().replace('\\', "/"),
                    bytes: b"t2".to_vec(),
                })
                .expect("encode flooding FS write"),
            );
            // Writes stop once the host closes the link.
            if frame.is_err() {
                break;
            }
        }
    });
    let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("Quit attribution")
    else {
        panic!("Quit skipped shutdown attribution");
    };
    prepare.send(Ok(())).expect("acknowledge attribution");
    let bound = APP_LINK_IO_DEADLINE
        .checked_sub(Duration::from_secs(1))
        .expect("writer deadline exceeds one second");
    let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
        .guardian
        .recv_timeout(bound)
        .expect("the Quit tail reached its shutdown before one writer deadline")
    else {
        panic!("unexpected guardian command after the Quit REPLY");
    };
    shutdown
        .send(Ok(()))
        .expect("acknowledge guardian shutdown");
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    drop(client);
    flood.join().expect("flooding writer joins");
    t.router.shutdown().expect("router shutdown after Quit");
}

/// GH-528 T2, spec gh527 criterion 7: after an accepted Quit's real REPLY
/// and its FS drain, a CALL already received on an ERR-declaring channel is
/// answered with `KELD-IPC-024` and never executed; an echo CALL gets no
/// frame (KEL-133 keeps echo REPLY-only). The host still closes the link.
#[test]
#[cfg(target_os = "macos")]
fn quit_drain_answers_received_calls_with_024_and_runs_none() {
    let (t, mut client) = guarded_test_router();
    let target = t.allowed.join("after-quit.txt");
    // All three frames are buffered before the host can finish the Quit:
    // its REPLY waits on the shutdown attribution acknowledged below.
    write_quit_call(&mut client, 20);
    write_fs_call(&mut client, 21, &target);
    let echo = keld_ipc::echo::EchoRequest {
        message: "after quit".to_owned(),
        count: 22,
    };
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        ECHO_CHANNEL,
        CorrelationId(22),
        &encode(&echo).expect("encode echo"),
    )
    .expect("send echo after Quit");

    let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("Quit attribution")
    else {
        panic!("Quit skipped shutdown attribution");
    };
    prepare.send(Ok(())).expect("acknowledge attribution");
    let (quit, quit_payload) = keld_ipc::link::read_frame(&mut client).expect("Quit REPLY");
    assert_eq!(
        (quit.kind, quit.channel, quit.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(20))
    );
    assert_eq!(
        decode::<LifecycleResponse>(&quit_payload).expect("Quit response"),
        LifecycleResponse::Quit
    );
    let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("guardian shutdown")
    else {
        panic!("unexpected guardian command after the Quit REPLY");
    };
    shutdown
        .send(Ok(()))
        .expect("acknowledge guardian shutdown");
    let after = read_frames_until_eof(&mut client);
    assert_eq!(after.len(), 1, "only the FS CALL is answered: {after:?}");
    let (header, payload) = &after[0];
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Err, FS_CHANNEL, CorrelationId(21))
    );
    assert_eq!(
        decode::<CallError>(payload).expect("024 CallError").code,
        "KELD-IPC-024"
    );
    assert!(!target.exists(), "a post-Quit FS CALL must not execute");
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    t.router.shutdown().expect("router shutdown after Quit");
}

/// GH-528 T2 (gate review of #636): the post-Quit drain ends on the peer's
/// EOF and never drops a CALL that has already arrived. Three FS CALLs are
/// buffered behind the Quit; the drain is then held before its first read
/// and given an idle deadline that has already passed, as a host stall
/// longer than its idle backstop would leave it, and only then does the
/// role end the link. Every buffered call still gets `KELD-IPC-024`, in
/// order, and none runs. *Negative control:* checking the idle deadline
/// before every read (as the earlier timer-window drain did) answers none
/// of them.
#[test]
#[cfg(target_os = "macos")]
fn quit_drain_answers_every_buffered_call_after_a_host_stall_then_eof() {
    let (t, mut client) = guarded_test_router();
    let (stalled_tx, stalled) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    t.router
        .handle()
        .stall_next_quit_drain(stalled_tx, resume_rx);
    let target = t.allowed.join("stalled.txt");
    write_quit_call(&mut client, 50);
    for corr in 51..=53 {
        write_fs_call(&mut client, corr, &target);
    }
    let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("Quit attribution")
    else {
        panic!("Quit skipped shutdown attribution");
    };
    prepare.send(Ok(())).expect("acknowledge attribution");
    let (quit, _) = keld_ipc::link::read_frame(&mut client).expect("Quit REPLY");
    assert_eq!(
        (quit.kind, quit.channel, quit.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(50))
    );
    stalled
        .recv_timeout(Duration::from_secs(5))
        .expect("the drain fixed its deadlines and stalled");
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("the role ends the link after the Quit REPLY");
    resume.send(()).expect("resume the stalled drain");

    // The drain ends on that EOF, and the host's link close reaches the
    // half-closed role before its tail asks the guardian to stop it.
    let answers: Vec<(FrameKind, keld_ipc::ChannelId, CorrelationId, String)> =
        read_frames_until_eof(&mut client)
            .iter()
            .map(|(header, payload)| {
                let error: CallError = decode(payload).expect("drain answer is a CallError");
                (header.kind, header.channel, header.corr, error.code)
            })
            .collect();
    let expected: Vec<_> = (51..=53)
        .map(|corr| {
            (
                FrameKind::Err,
                FS_CHANNEL,
                CorrelationId(corr),
                "KELD-IPC-024".to_owned(),
            )
        })
        .collect();
    assert_eq!(
        answers, expected,
        "every CALL written before the role ended the link gets 024"
    );
    assert!(!target.exists(), "a post-Quit FS CALL must not execute");
    let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("guardian shutdown after the drain")
    else {
        panic!("unexpected guardian command after the Quit drain");
    };
    shutdown
        .send(Ok(()))
        .expect("acknowledge guardian shutdown");
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    t.router.shutdown().expect("router shutdown after Quit");
}

/// GH-528 (#636 follow-up): a role that half-closes after its Quit REPLY
/// reads EOF from the host's link close itself, before the guardian is
/// asked to stop it. The Quit tail sends the guardian `Shutdown` only
/// after the link close (KEL-139 AC6), and the test does not acknowledge
/// it until after the probe, so neither a kill nor the host dropping its
/// link handles can have produced this EOF. The probe is a nonblocking
/// read, so the result does not depend on a timeout. *Negative control:*
/// `shutdown(Both)` in `shutdown_app_link` is `ENOTCONN` after the
/// half-close on macOS and sends no FIN, so the probe reads `WouldBlock`.
#[test]
#[cfg(target_os = "macos")]
fn quit_close_reaches_a_half_closed_role_before_the_guardian_stops_it() {
    use std::io::Read as _;

    let (t, mut client) = guarded_test_router();
    write_quit_call(&mut client, 80);
    let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("Quit attribution")
    else {
        panic!("Quit skipped shutdown attribution");
    };
    prepare.send(Ok(())).expect("acknowledge attribution");
    let (quit, _) = keld_ipc::link::read_frame(&mut client).expect("Quit REPLY");
    assert_eq!(
        (quit.kind, quit.channel, quit.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(80))
    );
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("the role ends the link after the Quit REPLY");

    let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("the Quit tail reached the guardian after the link close")
    else {
        panic!("unexpected guardian command after the Quit REPLY");
    };
    client.set_nonblocking(true).expect("nonblocking probe");
    let mut byte = [0_u8; 1];
    let probe = client.read(&mut byte);
    assert!(
        matches!(probe, Ok(0)),
        "the role reads the host's EOF before the guardian stops it: {probe:?}"
    );
    client.set_nonblocking(false).expect("blocking link");

    shutdown
        .send(Ok(()))
        .expect("acknowledge guardian shutdown");
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    t.router.shutdown().expect("router shutdown after Quit");
}

/// GH-528 T3 (#636 gate review): a role that fully closes right after its
/// Quit REPLY can leave a CALL it wrote behind the Quit in the host's
/// buffer. The drain still reads it, its `KELD-IPC-024` finds the role gone
/// (`EPIPE`), and the drain ends quietly as a lost link: the Quit tail goes
/// on to the guardian's shutdown and the UI Quit, with no link-failure
/// report and no `Fatal`. The drain is held before its first read until
/// the role has closed, so the answer's failure is certain. *Negative
/// control:* a failed answer that propagates as a session error sends the
/// UI `Fatal` instead of `Quit`.
#[test]
#[cfg(target_os = "macos")]
fn quit_drain_ends_quietly_when_its_answer_finds_the_role_gone() {
    let (t, mut client) = guarded_test_router();
    let (stalled_tx, stalled) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    let (end_tx, drain_end) = mpsc::sync_channel(1);
    let handle = t.router.handle();
    handle.stall_next_quit_drain(stalled_tx, resume_rx);
    handle.observe_next_quit_drain_end(end_tx);
    let target = t.allowed.join("behind.txt");
    write_quit_call(&mut client, 90);
    write_fs_call(&mut client, 91, &target);
    let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("Quit attribution")
    else {
        panic!("Quit skipped shutdown attribution");
    };
    prepare.send(Ok(())).expect("acknowledge attribution");
    let (quit, _) = keld_ipc::link::read_frame(&mut client).expect("Quit REPLY");
    assert_eq!(
        (quit.kind, quit.channel, quit.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(90))
    );
    stalled
        .recv_timeout(Duration::from_secs(5))
        .expect("the drain stalled before its first read");
    drop(client);
    resume.send(()).expect("resume the drain");

    assert_eq!(
        drain_end
            .recv_timeout(Duration::from_secs(5))
            .expect("the post-Quit drain ended"),
        QuitDrainEnd::AnswerLost
    );
    let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
        .guardian
        .recv_timeout(Duration::from_secs(5))
        .expect("the Quit tail reaches the guardian, not a link failure")
    else {
        panic!("a lost post-Quit answer must not report a link failure");
    };
    shutdown
        .send(Ok(()))
        .expect("acknowledge guardian shutdown");
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    assert!(!target.exists(), "a post-Quit FS CALL must not execute");
    t.router
        .shutdown()
        .expect("the accepted Quit stays a clean shutdown");
}

/// GH-528 T2 (gate review of #636, L2): a peer that stops reading cannot
/// hold a retirement, and its generation lock, for the writer's deadline.
/// The host-to-role direction is full before retirement and nothing reads
/// it, so the `KELD-IPC-023` for the held FS call cannot be written. It is
/// written under `HOST_ANSWER_BUDGET`, its failure marks the link lost,
/// and retirement still succeeds and closes the link: the role reads the
/// bytes queued before retirement, then the close, and no `ERR` byte.
/// *Negative control:* leaving the writer's deadline in place fails the
/// deadline assertion (and holds the lock for `APP_LINK_IO_DEADLINE`).
#[test]
#[cfg(target_os = "macos")]
fn retire_answers_are_bounded_when_the_peer_stops_reading() {
    use std::io::{ErrorKind, Read as _, Write as _};

    use keld_ipc::link::AppLinkDeadlines as _;

    let (server, mut client) = std::os::unix::net::UnixStream::pair().expect("pair");
    client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("client deadlines");
    server.set_nonblocking(true).expect("nonblocking fill");
    let mut queued = 0_usize;
    let chunk = [0_u8; 4096];
    for size in [chunk.len(), 1] {
        loop {
            match (&server).write(&chunk[..size]) {
                Ok(written) => queued += written,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => panic!("fill the host-to-role direction: {error}"),
            }
        }
    }
    server.set_nonblocking(false).expect("blocking link");
    let probe = server.try_clone().expect("deadline probe");
    let t = guarded_router(server);
    write_fs_call(&mut client, 70, &t.allowed.join("held.txt"));
    t.taken
        .recv_timeout(Duration::from_secs(5))
        .expect("worker took the FS job");

    t.router
        .handle()
        .retire_generation(1)
        .expect("retirement succeeds although its ERR cannot be written");
    assert_eq!(
        probe.app_link_write_deadline().expect("write deadline"),
        Some(HOST_ANSWER_BUDGET),
        "the 023 write ran under the answer budget, not the writer's deadline"
    );
    let mut received = Vec::new();
    client
        .read_to_end(&mut received)
        .expect("the role reads to the close");
    assert_eq!(
        received.len(),
        queued,
        "the role reads the bytes queued before retirement, then the close"
    );

    t.release.send(()).expect("release FS worker");
    t.router.shutdown().expect("router shutdown after retire");
}

/// GH-528 T2 (gate review of #636, L4): a real reply written first wins.
/// The FS worker writes its real REPLY and is held after it releases the
/// generation lock, with its lease still alive; a retirement then runs and
/// sends no `KELD-IPC-023` for the answered call. *Negative control:*
/// removing the terminal write's `retire_pending_call` (so the lease drop
/// alone clears the pending call) makes this retirement send a 023 after
/// the REPLY.
#[test]
#[cfg(target_os = "macos")]
fn retire_after_a_real_reply_sends_no_023() {
    let (t, mut client) = guarded_test_router();
    let target = t.allowed.join("replied.txt");
    let (written_tx, written) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    t.snapshot
        .fs
        .hold_next_terminal_write(written_tx, resume_rx);
    write_fs_call(&mut client, 60, &target);
    t.taken
        .recv_timeout(Duration::from_secs(5))
        .expect("worker took the FS job");
    t.release.send(()).expect("release FS worker");
    written
        .recv_timeout(Duration::from_secs(5))
        .expect("the FS REPLY was written");
    let (reply, _) = keld_ipc::link::read_frame(&mut client).expect("real FS REPLY");
    assert_eq!(
        (reply.kind, reply.channel, reply.corr),
        (FrameKind::Reply, FS_CHANNEL, CorrelationId(60))
    );

    t.router.handle().retire_generation(1).expect("retire g1");
    resume.send(()).expect("resume the FS worker");
    let after = read_frames_until_eof(&mut client);
    assert!(
        after.is_empty(),
        "an answered call gets no KELD-IPC-023: {after:?}"
    );
    assert!(target.exists(), "the answered FS write ran");
    t.router.shutdown().expect("router shutdown after retire");
}

#[test]
#[cfg(target_os = "macos")]
fn fs_worker_keeps_ping_responsive_and_quit_waits_for_fs_terminal_outcome() {
    use std::os::unix::net::UnixStream;

    use keld_ipc::link::{AppLinkDeadlines as _, read_frame, write_frame};
    use sha2::{Digest as _, Sha256};

    for (case, denied) in [("allowed", false), ("denied", true)] {
        let temp = tempfile::tempdir().expect("FS worker gate root");
        let allowed = temp.path().join("allowed");
        fs::create_dir(&allowed).expect("FS worker allowed root");
        let target = if denied {
            temp.path().join("denied-held.txt")
        } else {
            allowed.join("held.txt")
        };
        let scope = allowed.display().to_string().replace('\\', "/");
        let manifest_text =
            format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, &manifest_text).expect("write FS worker manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open FS worker manifest"),
            manifest_path,
            digest,
        )
        .expect("verify FS worker manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare FS worker broker");

        let (taken_tx, taken_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let gate = Arc::new(FsWorkerTestGate {
            taken: taken_tx,
            release: Mutex::new(release_rx),
            before_entry: None,
            initializer: Mutex::new(None),
        });
        let (server, mut client) = UnixStream::pair().expect("FS worker primary pair");
        client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("FS worker client deadlines");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let router = PrimaryRouter::start_with_fs_test_gate(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
            Some(&snapshot),
            Some(gate),
        )
        .expect("gated FS worker router");

        let committed = format!("{case} committed before quit").into_bytes();
        let base = if denied { 70 } else { 60 };
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(base),
            &encode(&FsRequest::Write {
                path: target.display().to_string().replace('\\', "/"),
                bytes: committed.clone(),
            })
            .expect("encode gated FS write"),
        )
        .expect("send gated FS write");
        taken_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("worker took FS job");

        write_frame(
            &mut client,
            FrameKind::Ping,
            0,
            keld_ipc::ChannelId::for_test(99),
            CorrelationId(base + 1),
            &[],
        )
        .expect("send Ping while FS worker is held");
        let (quit_drain_tx, quit_drain_rx) = mpsc::sync_channel(1);
        snapshot.fs.observe_next_drain_wait(quit_drain_tx);
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(base + 2),
            &encode(&LifecycleRequest::Quit).expect("encode held-worker Quit"),
        )
        .expect("send Quit while FS worker is held");

        let (ping, ping_payload) = read_frame(&mut client).expect("Ping while FS worker is held");
        assert_eq!(ping.kind, FrameKind::Ping, "{case}");
        assert_eq!(ping.channel, keld_ipc::ChannelId::for_test(99), "{case}");
        assert_eq!(ping.corr, CorrelationId(base + 1), "{case}");
        assert!(ping_payload.is_empty(), "{case}");
        assert!(
            matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "{case}: UI Quit occurred while admitted FS work was still held"
        );

        quit_drain_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|error| {
                panic!("{case}: Quit did not reach FS drain before worker release: {error}")
            });
        assert!(
            router.handle.shutdown.is_running(),
            "{case}: Quit claimed shutdown before the admitted FS terminal outcome"
        );
        assert!(
            matches!(guardian_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "{case}: shutdown attribution began before the admitted FS terminal outcome"
        );
        assert!(
            !target.exists(),
            "{case}: held native operation changed the target before worker release"
        );

        release_tx.send(()).expect("release FS worker");

        let prepare = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit preparation after FS terminal write");
        let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare_reply) = prepare else {
            panic!("{case}: FS worker Quit skipped shutdown attribution");
        };
        prepare_reply
            .send(Ok(()))
            .expect("acknowledge FS worker shutdown attribution");

        let (fs_terminal, fs_payload) =
            read_frame(&mut client).expect("FS terminal outcome before Quit reply");
        assert_eq!(fs_terminal.channel, FS_CHANNEL, "{case}");
        assert_eq!(fs_terminal.corr, CorrelationId(base), "{case}");
        if denied {
            assert_eq!(fs_terminal.kind, FrameKind::Err, "{case}");
            let error: CallError = decode(&fs_payload).expect("decode denied FS terminal outcome");
            assert_eq!(error.code, "KELD-GUARD002", "{case}");
            assert!(
                !target.exists(),
                "{case}: denied write produced a filesystem effect"
            );
        } else {
            assert_eq!(fs_terminal.kind, FrameKind::Reply, "{case}");
            assert!(matches!(
                decode::<FsResponse>(&fs_payload).expect("decode FS terminal outcome"),
                FsResponse::Write
            ));
            assert_eq!(
                fs::read(&target).expect("read committed FS bytes"),
                committed,
                "{case}: write effect and terminal FS outcome diverged"
            );
        }

        let (quit, quit_payload) = read_frame(&mut client).expect("Quit reply after FS terminal");
        assert_eq!(quit.kind, FrameKind::Reply, "{case}");
        assert_eq!(quit.channel, LIFECYCLE_CHANNEL, "{case}");
        assert_eq!(quit.corr, CorrelationId(base + 2), "{case}");
        assert_eq!(
            decode::<LifecycleResponse>(&quit_payload).expect("decode held-worker Quit reply"),
            LifecycleResponse::Quit,
            "{case}"
        );

        let shutdown = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("FS worker guardian shutdown after drain");
        let TestPrimaryOwnerCommand::Shutdown(shutdown_reply) = shutdown else {
            panic!("{case}: FS worker Quit sent unexpected guardian command");
        };
        shutdown_reply
            .send(Ok(()))
            .expect("complete FS worker guardian shutdown");
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("UI Quit after FS terminal outcome"),
            AppWindowCommand::Quit,
            "{case}"
        );
        router
            .shutdown()
            .expect("completed FS worker router shutdown");
    }
}

#[test]
#[cfg(target_os = "macos")]
fn retired_call_drop_cannot_clear_successor_single_flight() {
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("successor single-flight root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write successor manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open successor manifest"),
        manifest_path,
        digest,
    )
    .expect("verify successor manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare successor broker");

    let c1_pending = PendingFsCall {
        attempt: 1,
        correlation: CorrelationId(96),
    };
    let c1 = snapshot
        .fs
        .begin_call(c1_pending)
        .expect("admit C1")
        .expect("C1 admission remains open");
    c1.retire_pending_call().expect("retire C1 terminal marker");

    let c2_pending = PendingFsCall {
        attempt: 1,
        correlation: CorrelationId(97),
    };
    let c2 = snapshot
        .fs
        .begin_call(c2_pending)
        .expect("admit C2 after C1 terminal retirement")
        .expect("C2 admission remains open");

    drop(c1);
    let state = snapshot.fs.state.lock().expect("state after old C1 drop");
    assert!(
        state.pending_call == Some(c2_pending),
        "old C1 drop corrupted C2 correlation ownership"
    );
    assert!(
        state.call_outstanding,
        "old C1 drop cleared C2 single-flight occupancy"
    );
    drop(state);

    let c3 = snapshot.fs.begin_call(PendingFsCall {
        attempt: 1,
        correlation: CorrelationId(98),
    });
    assert!(
        c3.is_err(),
        "C3 was admitted after old C1 drop erased C2 occupancy"
    );

    c2.retire_pending_call().expect("retire C2");
    drop(c2);
}

#[test]
#[cfg(target_os = "macos")]
fn terminal_retirement_releases_call_occupancy_before_native_drain() {
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("terminal occupancy root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write terminal occupancy manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open terminal occupancy manifest"),
        manifest_path,
        digest,
    )
    .expect("verify terminal occupancy manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare terminal occupancy broker");
    let pending = PendingFsCall {
        attempt: 1,
        correlation: CorrelationId(99),
    };
    let admitted = snapshot
        .fs
        .begin_call(pending)
        .expect("admit terminal occupancy call")
        .expect("terminal occupancy admission remains open");
    assert!(snapshot.fs.has_outstanding_call_for(1));
    assert_eq!(
        snapshot
            .fs
            .state
            .lock()
            .expect("pre-terminal state")
            .in_flight,
        1
    );

    admitted
        .retire_pending_call()
        .expect("retire terminal correlation before reply");
    assert!(
        !snapshot.fs.has_outstanding_call_for(1),
        "terminal correlation retirement must release single-flight occupancy before the native lease drains"
    );
    assert_eq!(
        snapshot
            .fs
            .state
            .lock()
            .expect("post-terminal state")
            .in_flight,
        1,
        "drain accounting must remain held until the native lease is dropped"
    );

    drop(admitted);
    snapshot.fs.drain().expect("terminal occupancy drain");
}

#[cfg(target_os = "macos")]
fn successor_drain_test_router(
    snapshot: &GuardSnapshot,
    g1_server: std::os::unix::net::UnixStream,
) -> (PrimaryRouter, Receiver<AppWindowCommand>) {
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, _guardian_rx) = mpsc::channel();
    let handle = PrimaryRouterHandle {
        current: Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
            identity: PrimaryLinkIdentity::Fixture,
            attempt: 1,
            writer: g1_server,
            reader_stop: Arc::new(AtomicBool::new(false)),
            pending_quit: None,
        }))),
        readers: Arc::new(Mutex::new(HashMap::new())),
        pending_echo: Arc::new(Mutex::new(None)),
        pending_echo_attempt: Arc::new(AtomicU32::new(0)),
        pending_echo_corr: Arc::new(AtomicU32::new(0)),
        next_host_corr: Arc::new(AtomicU32::new(1)),
        window_ready: Arc::new(AtomicBool::new(true)),
        last_window_closed: Arc::new(AtomicBool::new(false)),
        recovery_armed: Arc::new(AtomicBool::new(true)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        shutdown: SessionShutdownState::new(),
        fs: Some(snapshot.fs_weak()),
        fs_worker_commands: None,
        #[cfg(target_os = "macos")]
        auth: None,
        #[cfg(target_os = "macos")]
        auth_commands: None,
        guardian: GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        window_commands: window_tx,
        #[cfg(target_os = "macos")]
        quit_drain_hooks: Arc::default(),
    };
    let router = PrimaryRouter::finish_start(
        handle,
        Some(
            snapshot
                .take_initial_broker()
                .expect("transfer successor drain broker"),
        ),
        1,
        None,
        #[cfg(target_os = "macos")]
        None,
    )
    .expect("successor drain router worker");
    (router, window_rx)
}

#[test]
#[cfg(target_os = "macos")]
fn successor_reader_waits_for_retired_fs_drain_before_ready_and_call() {
    use std::os::unix::net::UnixStream;

    use keld_ipc::link::{AppLinkDeadlines as _, read_frame, write_frame};
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("successor drain root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write successor drain manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open successor drain manifest"),
        manifest_path,
        digest,
    )
    .expect("verify successor drain manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare successor drain broker");

    let (g1_server, _g1_client) = UnixStream::pair().expect("G1 drain pair");
    let (mut router, window_rx) = successor_drain_test_router(&snapshot, g1_server);
    let old_call = snapshot
        .fs
        .begin_call(PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(110),
        })
        .expect("admit old generation FS call")
        .expect("old generation FS admission remains open");
    router
        .handle
        .retire_generation(1)
        .expect("retire G1 while native work is outstanding");

    let (drain_wait_tx, drain_wait_rx) = mpsc::sync_channel(1);
    snapshot.fs.observe_next_drain_wait(drain_wait_tx);
    let (g2_server, mut g2_client) = UnixStream::pair().expect("G2 drain pair");
    g2_client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("G2 deadlines");
    let install_handle = router.handle.clone();
    let (installed_tx, installed_rx) = mpsc::sync_channel(1);
    let install = thread::spawn(move || {
        let result = install_handle.install_generation(2, g2_server);
        installed_tx.send(result).expect("report G2 install");
    });

    drain_wait_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("G2 install must wait for the retired native call to drain");
    assert!(
        matches!(installed_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "G2 was published before old filesystem authority drained"
    );
    assert!(
        router
            .handle
            .current
            .lock()
            .expect("generation state")
            .is_none(),
        "G2 became current before retired native work drained"
    );
    assert!(
        matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "waiting for old FS work made the recovered session fatal"
    );

    drop(old_call);
    installed_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("G2 install completion")
        .expect("G2 installs after old FS drain");
    install.join().expect("G2 installer joins");

    let (ready, ready_payload) = read_frame(&mut g2_client).expect("G2 Ready replay");
    assert_eq!(ready.kind, FrameKind::Event);
    assert_eq!(ready.channel, LIFECYCLE_CHANNEL);
    assert_eq!(ready.corr, CorrelationId(0));
    assert_eq!(
        decode::<LifecycleEvent>(&ready_payload).expect("decode G2 Ready"),
        LifecycleEvent::Ready
    );

    write_frame(
        &mut g2_client,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(111),
        &encode(&FsRequest::Read {
            path: temp.path().join("denied.txt").display().to_string(),
        })
        .expect("encode G2 FS Call"),
    )
    .expect("send G2 FS Call");
    let (reply, payload) = read_frame(&mut g2_client).expect("G2 FS terminal response");
    assert_eq!(reply.kind, FrameKind::Err);
    assert_eq!(reply.channel, FS_CHANNEL);
    assert_eq!(reply.corr, CorrelationId(111));
    let error: CallError = decode(&payload).expect("decode G2 FS Err");
    assert_eq!(error.code, "KELD-GUARD001");
    assert!(
        matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "G2's first well-formed FS Call became a fatal recovery error"
    );

    router
        .stop_and_join()
        .expect("successor drain router shutdown");
}

#[test]
#[cfg(target_os = "macos")]
fn shutdown_during_retired_fs_drain_does_not_publish_successor() {
    use std::io::Read as _;
    use std::os::unix::net::UnixStream;

    use keld_ipc::link::AppLinkDeadlines as _;
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("shutdown during drain root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write drain shutdown manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open drain shutdown manifest"),
        manifest_path,
        digest,
    )
    .expect("verify drain shutdown manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare drain shutdown broker");

    let (g1_server, _g1_client) = UnixStream::pair().expect("G1 drain shutdown pair");
    let (mut router, _window_rx) = successor_drain_test_router(&snapshot, g1_server);
    let old_call = snapshot
        .fs
        .begin_call(PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(110),
        })
        .expect("admit old generation FS call")
        .expect("old generation FS admission remains open");
    router
        .handle
        .retire_generation(1)
        .expect("retire G1 with an outstanding native lease");

    let (drain_wait_tx, drain_wait_rx) = mpsc::sync_channel(1);
    snapshot.fs.observe_next_drain_wait(drain_wait_tx);
    let (g2_server, mut g2_client) = UnixStream::pair().expect("G2 drain shutdown pair");
    g2_client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("G2 deadlines");
    let install_handle = router.handle.clone();
    let (installed_tx, installed_rx) = mpsc::sync_channel(1);
    let install = thread::spawn(move || {
        let result = install_handle.install_generation(2, g2_server);
        installed_tx.send(result).expect("report G2 install");
    });

    // The installer has passed its first shutdown check. Retaining old_call
    // keeps it inside drain until shutdown is accepted, even on a spurious wake.
    drain_wait_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("G2 install must reach the held retired-FS drain");
    assert!(
        matches!(installed_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "G2 install completed while the old native lease was held"
    );
    assert!(router.handle.shutdown.claim_cli_lease_lost());
    assert!(router.handle.shutdown.reader_stop.load(Ordering::Acquire));
    drop(old_call);

    installed_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("G2 install completion after shutdown")
        .expect("shutdown refuses the late G2 without an install error");
    install.join().expect("G2 installer joins");

    // Observe completed installation before stop_and_join can erase an
    // incorrectly published generation or close its peer on the test's behalf.
    assert!(
        router
            .handle
            .current
            .lock()
            .expect("generation state after installer join")
            .is_none(),
        "accepted shutdown during retired-FS drain must prevent G2 publication"
    );
    assert!(
        !router
            .handle
            .readers
            .lock()
            .expect("readers after installer join")
            .contains_key(&2),
        "accepted shutdown during retired-FS drain must not register a G2 reader"
    );
    let mut byte = [0_u8; 1];
    assert_eq!(
        g2_client
            .read(&mut byte)
            .expect("refused G2 must reach EOF"),
        0,
        "refused G2 must close without Ready or another response"
    );

    router
        .stop_and_join()
        .expect("drain shutdown router cleanup");
}

#[test]
#[cfg(target_os = "macos")]
fn retired_generation_correlation_does_not_release_global_fs_single_flight() {
    use std::io::Cursor;

    use keld_ipc::link::write_frame;
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("global FS single-flight root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("global FS single-flight allowed root");
    let scope = allowed.display().to_string().replace('\\', "/");
    let manifest_text = format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"]}}}}}}"#);
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, &manifest_text).expect("write global FS manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open global FS manifest"),
        manifest_path,
        digest,
    )
    .expect("verify global FS manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare global FS broker");

    let g1 = snapshot
        .fs
        .begin_call(PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(100),
        })
        .expect("admit G1 FS call")
        .expect("G1 admission remains open");
    snapshot
        .fs
        .retire_pending_call_for_attempt(1)
        .expect("retire only G1 correlation metadata");
    assert_eq!(
        snapshot.fs.state.lock().expect("G1 state").in_flight,
        1,
        "generation retirement must not complete the admitted native operation"
    );

    let mut bytes = Vec::new();
    write_frame(
        &mut bytes,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(101),
        &[0xaa; 64],
    )
    .expect("encode G2 FS frame");
    let mut cursor = Cursor::new(bytes);
    let stop = AtomicBool::new(false);
    let error = read_primary_app_frame_interruptible_with_privileged_call(
        &mut cursor,
        &stop,
        || None,
        &keld_ipc::channel_table::FS,
        || snapshot.fs.has_outstanding_call_for(2),
    )
    .expect_err("G2 must be rejected while G1 native work is still outstanding");
    assert!(
        error.to_string().contains("KELD-IPC-005"),
        "wrong cross-generation single-flight classification: {error}"
    );
    assert_eq!(
        cursor.position(),
        u64::try_from(keld_ipc::HEADER_LEN).expect("header length fits u64"),
        "G2 payload was consumed before global single-flight rejection"
    );

    let second = snapshot.fs.begin_call(PendingFsCall {
        attempt: 2,
        correlation: CorrelationId(101),
    });
    assert!(
        second.is_err(),
        "G2 raced past the global single-flight backstop after G1 correlation retirement"
    );

    drop(g1);
    assert_eq!(
        snapshot.fs.state.lock().expect("post-G1 state").in_flight,
        0
    );
    snapshot
        .fs
        .begin_call(PendingFsCall {
            attempt: 3,
            correlation: CorrelationId(102),
        })
        .expect("G3 admission after G1 completion")
        .expect("G3 must admit after global occupancy clears");
}

#[test]
#[cfg(target_os = "macos")]
fn second_fs_call_is_rejected_before_its_payload_while_first_is_outstanding() {
    use std::io::Write as _;
    use std::os::unix::net::UnixStream;

    use keld_ipc::frame::FrameHeader;
    use keld_ipc::link::{AppLinkDeadlines as _, write_frame};
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("second FS Call root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("second FS Call allowed root");
    let first_target = allowed.join("held.txt");
    fs::write(&first_target, b"held worker bytes").expect("second FS Call input");
    let second_target = allowed.join("must-not-write.txt");
    let scope = allowed.display().to_string().replace('\\', "/");
    let manifest_text =
        format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, &manifest_text).expect("write second FS Call manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open second FS Call manifest"),
        manifest_path,
        digest,
    )
    .expect("verify second FS Call manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare second FS Call broker");

    let (taken_tx, taken_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let gate = Arc::new(FsWorkerTestGate {
        taken: taken_tx,
        release: Mutex::new(release_rx),
        before_entry: None,
        initializer: Mutex::new(None),
    });
    let (server, mut client) = UnixStream::pair().expect("second FS Call primary pair");
    client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("second FS Call client deadlines");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, _guardian_rx) = mpsc::channel();
    let router = PrimaryRouter::start_with_fs_test_gate(
        server,
        window_tx,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
        Some(&snapshot),
        Some(gate),
    )
    .expect("second FS Call router");

    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(80),
        &encode(&FsRequest::Read {
            path: first_target.display().to_string().replace('\\', "/"),
        })
        .expect("encode held first FS Call"),
    )
    .expect("send held first FS Call");
    taken_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("worker took first FS Call");
    assert_eq!(
        snapshot.fs.state.lock().expect("FS state").in_flight,
        1,
        "the held first FS Call is the only native admission"
    );

    let second_payload = encode(&FsRequest::Write {
        path: second_target.display().to_string().replace('\\', "/"),
        bytes: b"must not be read".to_vec(),
    })
    .expect("encode withheld second FS payload");
    let second_len = u32::try_from(second_payload.len()).expect("second FS payload length");
    client
        .write_all(
            &FrameHeader {
                kind: FrameKind::Call,
                flags: 0,
                channel: FS_CHANNEL,
                corr: CorrelationId(81),
                len: second_len,
            }
            .encode(),
        )
        .expect("send only second FS Call header");
    client.flush().expect("flush second FS Call header");

    let fatal = window_rx.recv_timeout(Duration::from_secs(1));
    assert_eq!(
        snapshot.fs.state.lock().expect("FS state").in_flight,
        1,
        "the withheld second payload reached native admission"
    );
    assert!(
        matches!(taken_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "the worker observed a second FS handler entry"
    );
    assert!(
        !second_target.exists(),
        "the header-only second FS Call produced a filesystem effect"
    );

    release_tx.send(()).expect("release first FS worker");
    let shutdown = router.shutdown();
    assert_eq!(
        fatal.expect("second FS Call must fail before its withheld payload is read"),
        AppWindowCommand::Fatal
    );
    let error = shutdown.expect_err("second outstanding FS Call must fail the session");
    assert!(
        error.to_string().contains("KELD-IPC-005"),
        "wrong second FS Call failure: {error}"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn stale_generation_precedes_malformed_fs_payload_classification() {
    use std::os::unix::net::UnixStream;

    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("stale FS request root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write stale FS manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open stale FS manifest"),
        manifest_path,
        digest,
    )
    .expect("verify stale FS manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare stale FS broker");
    let (server, _client) = UnixStream::pair().expect("stale FS router pair");
    let (window_tx, _window_rx) = mpsc::channel();
    let (guardian_tx, _guardian_rx) = mpsc::channel();
    let router = PrimaryRouter::start_with_fs(
        server,
        window_tx,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
        Some(&snapshot),
    )
    .expect("stale FS router");

    let error = must_err(
        prepare_fs_work(
            &router.handle(),
            2,
            CorrelationId(70),
            &[0xff],
            &Arc::new(AtomicBool::new(false)),
        ),
        "stale generation must fail before malformed payload decoding",
    );
    router.shutdown().expect("stale FS router shutdown");
    let rendered = error.to_string();
    assert!(
        rendered.contains("stale primary generation"),
        "wrong stale-generation classification: {rendered}"
    );
    assert!(
        !rendered.contains("KELD-IPC-003"),
        "codec classification overtook stale generation: {rendered}"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn authenticated_primary_link_rejects_foreign_hello_then_routes_fs() {
    use std::os::unix::net::UnixStream;

    use keld_ipc::link::{handshake_client, read_frame, write_frame};
    use keld_ipc::{
        BootstrapAdmission, BootstrapListener, BootstrapRejection, BootstrapRejectionObserver,
        SessionToken, parse_app_link,
    };
    use sha2::{Digest as _, Sha256};

    struct RejectionRecorder(Arc<Mutex<Vec<BootstrapRejection>>>);
    impl BootstrapRejectionObserver for RejectionRecorder {
        fn rejected(&self, rejection: BootstrapRejection) {
            self.0.lock().expect("rejection recorder").push(rejection);
        }
    }

    let temp = tempfile::tempdir().expect("authenticated FS temp root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("authenticated allowed root");
    let scope = allowed.display().to_string().replace('\\', "/");
    let manifest_text =
        format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, &manifest_text).expect("write authenticated FS manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open authenticated FS manifest"),
        manifest_path,
        digest,
    )
    .expect("verify authenticated FS manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare authenticated FS broker");

    let listener = Arc::new(BootstrapListener::bind().expect("bind production bootstrap"));
    let app_link = listener.app_link();
    let (endpoint, token) = parse_app_link(&app_link).expect("parse production app-link");

    let rejections = Arc::new(Mutex::new(Vec::new()));
    let listener_for_accept = Arc::clone(&listener);
    let rejection_for_accept = Arc::clone(&rejections);
    let accept = thread::spawn(move || {
        let observer = RejectionRecorder(rejection_for_accept);
        match listener_for_accept
            .accept_authenticated_until(Instant::now() + Duration::from_secs(5), &observer)
            .expect("bootstrap accept")
        {
            BootstrapAdmission::Authenticated(stream) => stream,
            BootstrapAdmission::Cancelled => panic!("bootstrap cancelled before valid peer"),
            BootstrapAdmission::DeadlineElapsed => {
                panic!("bootstrap deadline elapsed before valid peer")
            }
        }
    });

    let mut foreign_hex = token.to_hex().into_bytes();
    foreign_hex[0] = if foreign_hex[0] == b'0' { b'1' } else { b'0' };
    let foreign = SessionToken::from_hex(
        std::str::from_utf8(&foreign_hex).expect("foreign token remains ASCII"),
    )
    .expect("foreign token");
    let mut hostile = UnixStream::connect(endpoint).expect("foreign bootstrap connect");
    let foreign_error =
        handshake_client(&mut hostile, &foreign).expect_err("foreign HELLO must fail");
    assert!(
        matches!(foreign_error, IpcError::HelloAuth { .. } | IpcError::Io(_)),
        "foreign peer must observe auth rejection or host close: {foreign_error}"
    );
    drop(hostile);

    let denied_before_auth = allowed.join("foreign-must-not-write.txt");
    assert!(
        !denied_before_auth.exists(),
        "foreign HELLO reached filesystem before authentication"
    );

    let mut client = UnixStream::connect(endpoint).expect("valid bootstrap connect");
    handshake_client(&mut client, &token).expect("valid HELLO authenticates");
    let server = accept.join().expect("accept thread");
    let observed = rejections.lock().expect("rejection readback").clone();
    assert_eq!(observed, vec![BootstrapRejection::HelloAuth]);
    assert_eq!(observed[0].code(), "KELD-IPC-007");
    assert!(
        !listener.path().exists(),
        "one-use locator remained after authenticated consume"
    );
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("authenticated FS read deadline");

    let (window_tx, _window_rx) = mpsc::channel();
    let (guardian_tx, _guardian_rx) = mpsc::channel();
    let router = PrimaryRouter::start_with_fs(
        server,
        window_tx,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
        Some(&snapshot),
    )
    .expect("authenticated primary router");

    let target = allowed.join("authenticated-route.txt");
    let request = FsRequest::Write {
        path: target.display().to_string().replace('\\', "/"),
        bytes: b"authenticated-primary".to_vec(),
    };
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(51),
        &encode(&request).expect("encode authenticated write"),
    )
    .expect("send authenticated FS write");
    let (header, payload) = read_frame(&mut client).expect("authenticated FS reply");
    assert_eq!(header.kind, FrameKind::Reply);
    assert_eq!(header.channel, FS_CHANNEL);
    assert_eq!(header.corr, CorrelationId(51));
    assert!(matches!(
        decode::<FsResponse>(&payload).expect("decode authenticated FS response"),
        FsResponse::Write
    ));
    assert_eq!(
        fs::read(&target).expect("authenticated bytes"),
        b"authenticated-primary"
    );

    router.shutdown().expect("authenticated router shutdown");

    let late_target = allowed.join("post-quiesce-must-not-write.txt");
    let late = FsRequest::Write {
        path: late_target.display().to_string().replace('\\', "/"),
        bytes: b"too-late".to_vec(),
    };
    if write_frame(
        &mut client,
        FrameKind::Call,
        0,
        FS_CHANNEL,
        CorrelationId(52),
        &encode(&late).expect("encode post-quiesce write"),
    )
    .is_ok()
    {
        read_frame(&mut client).expect_err("retained old stream must be closed/rejected");
    }
    assert!(
        !late_target.exists(),
        "retained old stream entered FS handler after quiescing"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn fs_quiesce_refuses_new_admission_and_drains_existing_lease() {
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("FS quiesce root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write empty manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open empty manifest"),
        manifest_path,
        digest,
    )
    .expect("verify empty manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare empty FS broker");

    let admitted = snapshot
        .fs
        .admit()
        .expect("first FS admission")
        .expect("session initially accepts");
    snapshot.fs.quiesce().expect("publish FS quiescing");
    assert!(
        snapshot
            .fs
            .admit()
            .expect("post-quiesce admission check")
            .is_none(),
        "new FS handler entered after quiescing"
    );
    assert_eq!(
        snapshot.fs.state.lock().expect("FS state").in_flight,
        1,
        "pre-admitted lease disappeared before its terminal outcome"
    );
    drop(admitted);
    snapshot.fs.drain().expect("drain admitted FS work");
    assert_eq!(
        snapshot.fs.state.lock().expect("FS state").in_flight,
        0,
        "drain returned before the admitted lease terminated"
    );
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dequeued_fs_item_losing_quiesce_never_enters_handler() {
    let temp = tempfile::tempdir().expect("paused FS entry root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write paused FS manifest");
    // SHA-256 of the exact static fixture bytes `{}\n`. Keep the Linux
    // lifecycle regression independent of the macOS-only sha2 dependency.
    let digest = [
        0xca, 0x3d, 0x16, 0x3b, 0xab, 0x05, 0x53, 0x81, 0x82, 0x72, 0x26, 0x14, 0x05, 0x68, 0xf3,
        0xbe, 0xf7, 0xea, 0xac, 0x18, 0x7c, 0xeb, 0xd7, 0x68, 0x78, 0xe0, 0xb6, 0x3e, 0x9e, 0x44,
        0x23, 0x56,
    ];
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open paused FS manifest"),
        manifest_path,
        digest,
    )
    .expect("verify paused FS manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare paused FS broker");
    let admitted = snapshot
        .fs
        .admit()
        .expect("paused FS admission")
        .expect("session initially accepts");
    let invoked = Arc::new(AtomicBool::new(false));
    let invoked_in_worker = Arc::clone(&invoked);
    let (dequeued_tx, dequeued_rx) = mpsc::sync_channel(1);
    let (enter_tx, enter_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        dequeued_tx.send(()).expect("report dequeued FS item");
        enter_rx.recv().expect("release paused handler entry");
        admitted
            .handle_with(|| invoked_in_worker.store(true, Ordering::Release))
            .expect("attempt paused handler entry")
            .is_none()
    });

    dequeued_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("worker dequeued admitted FS item");
    snapshot.fs.quiesce().expect("publish FS quiescence");
    enter_tx.send(()).expect("release handler-entry attempt");
    assert!(worker.join().expect("paused FS worker joins"));
    assert!(
        !invoked.load(Ordering::Acquire),
        "dequeued item invoked its handler after quiescence publication"
    );
    snapshot.fs.drain().expect("drain rejected FS item");
}

#[test]
#[cfg(target_os = "macos")]
fn guard_snapshot_drops_only_after_ordered_cleanup() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join("index.html"), "<p>guarded</p>\n").expect("renderer");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");
    let ValidatedBootSelection {
        app,
        permissions_file,
        permissions_digest,
    } = validate_from_root(&root).expect("validated selection");
    let verified = load_verified_manifest(
        permissions_file,
        root.join(PERMISSIONS_FILE),
        permissions_digest,
    )
    .expect("verified manifest");
    drop(app);
    let dropped = Arc::new(AtomicBool::new(false));
    let mut snapshot = GuardSnapshot::prepare(verified).expect("prepare guarded FS broker");
    snapshot.drop_observer = Some(Arc::clone(&dropped));

    let digest = finish_guarded_session(Some(&snapshot), |live| {
        assert!(
            !dropped.load(Ordering::Acquire),
            "snapshot dropped before cleanup"
        );
        live.expect("guarded cleanup receives the snapshot")
            .verified
            .verified_sha256()
    });

    assert_eq!(digest, permissions_digest);
    assert!(
        !dropped.load(Ordering::Acquire),
        "borrowed cleanup helper destroyed the outer session owner"
    );
    drop(snapshot);
    assert!(
        dropped.load(Ordering::Acquire),
        "outer session owner did not destroy the snapshot after cleanup"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn boot_sidecar_symlink_is_rejected_before_target_resolution() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    let outside = temp.path().join("outside-boot.json");
    fs::write(&outside, valid_boot("src/main.ts", "index.html")).expect("outside boot");
    symlink(&outside, root.join("keld.boot.json")).expect("boot symlink");

    let error = must_err(validate_from_root(&root), "boot symlink must fail");
    assert!(error.to_string().contains("boot descriptor"), "{error}");
}

#[test]
#[cfg(target_os = "macos")]
fn descriptor_limit_does_not_narrow_renderer_size() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    let renderer = vec![b'x'; MAX_BOOT_BYTES + 1];
    fs::write(root.join("index.html"), &renderer).expect("large renderer");
    fs::write(root.join("keld.permissions.jsonc"), b"{}\n").expect("permissions");
    fs::write(
        root.join("keld.boot.json"),
        valid_boot("main.ts", "index.html"),
    )
    .expect("boot");

    let selection = validate_from_root(&root).expect("large renderer is valid");
    assert_eq!(selection.app.renderer_html, renderer);
}

#[test]
#[cfg(target_os = "macos")]
fn dev_lease_reader_is_nonblocking_cloexec_and_read_only() {
    let (reader, writer) = nix::unistd::pipe().expect("lease pipe");
    configure_dev_lease_fd(&reader).expect("configure lease reader");
    let descriptor = FdFlag::from_bits_truncate(
        fcntl(&reader, FcntlArg::F_GETFD).expect("lease descriptor flags"),
    );
    let status =
        OFlag::from_bits_truncate(fcntl(&reader, FcntlArg::F_GETFL).expect("lease status flags"));
    assert!(descriptor.contains(FdFlag::FD_CLOEXEC));
    assert!(status.contains(OFlag::O_NONBLOCK));
    assert!((status & OFlag::O_ACCMODE).is_empty());

    let error = configure_dev_lease_fd(&writer).expect_err("writer is not a lease reader");
    assert!(error.to_string().contains("read-only end"), "{error}");
}

#[test]
#[cfg(target_os = "macos")]
fn dev_lease_data_drain_yields_after_a_fixed_work_budget() {
    struct EndlessReader {
        reads: usize,
    }

    impl std::io::Read for EndlessReader {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            bytes.fill(b'x');
            Ok(bytes.len())
        }
    }

    let mut reader = EndlessReader { reads: 0 };
    assert!(!poll_dev_lease_reader(&mut reader).expect("bounded data drain"));
    assert_eq!(reader.reads, DEV_LEASE_DRAIN_READS);
}

#[test]
#[cfg(target_os = "macos")]
fn stage_directory_mode_is_mandatory() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o1700)).expect("wrong mode");
    let mode_error = must_err(validate_from_root(&root), "mode must be exact 0700");
    assert!(mode_error.to_string().contains("0o700"), "{mode_error}");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(clippy::too_many_lines)] // one test preserves the full Ready/calls/Quit ordering oracle
fn one_router_carries_ready_two_echo_calls_and_ordered_quit() {
    use std::io::Read as _;
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use keld_ipc::codec::{decode, encode};
    use keld_ipc::link::{AppLinkDeadlines, read_frame};

    let (server, mut client) = UnixStream::pair().expect("session pair");
    client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("client deadlines");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let (eof_tx, eof_rx) = mpsc::channel();
    let guardian_prepared = Arc::new(AtomicBool::new(false));
    let guardian_prepared_in_thread = Arc::clone(&guardian_prepared);
    let guardian_finished = Arc::new(AtomicBool::new(false));
    let guardian_finished_in_thread = Arc::clone(&guardian_finished);
    let guardian_thread = std::thread::spawn(move || {
        let prepare_reply = match guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian Quit preparation")
        {
            TestPrimaryOwnerCommand::PrepareAcceptedShutdown(reply) => reply,
            TestPrimaryOwnerCommand::Shutdown(_) => panic!("Quit reply lacked preparation"),
            TestPrimaryOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
            TestPrimaryOwnerCommand::FailGeneration(_, _)
            | TestPrimaryOwnerCommand::FailRetiredGeneration(_, _) => {
                panic!("unexpected link failure")
            }
            TestPrimaryOwnerCommand::ArmRecovery(_, _) => panic!("unexpected recovery arm"),
            TestPrimaryOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
        };
        guardian_prepared_in_thread.store(true, Ordering::Release);
        prepare_reply
            .send(Ok(()))
            .expect("guardian preparation reply");
        let shutdown_reply = match guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian shutdown request")
        {
            TestPrimaryOwnerCommand::Shutdown(reply) => reply,
            TestPrimaryOwnerCommand::PrepareAcceptedShutdown(_) => {
                panic!("duplicate preparation")
            }
            TestPrimaryOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
            TestPrimaryOwnerCommand::FailGeneration(_, _)
            | TestPrimaryOwnerCommand::FailRetiredGeneration(_, _) => {
                panic!("unexpected link failure")
            }
            TestPrimaryOwnerCommand::ArmRecovery(_, _) => panic!("unexpected recovery arm"),
            TestPrimaryOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
        };
        eof_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("link EOF before guardian reap");
        guardian_finished_in_thread.store(true, Ordering::Release);
        shutdown_reply
            .send(Ok(()))
            .expect("guardian shutdown reply");
    });
    let router = PrimaryRouter::start(
        server,
        window_tx,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
    )
    .expect("primary router");

    router.handle().signal_ready().expect("Ready");
    assert_lifecycle_event(&mut client, LifecycleEvent::Ready);

    for (correlation, message) in [(41_u32, "first"), (42_u32, "second")] {
        assert_echo_call(&mut client, correlation, message);
    }

    router
        .handle()
        .signal_last_window_closed()
        .expect("LastWindowClosed");
    assert_lifecycle_event(&mut client, LifecycleEvent::LastWindowClosed);

    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(43),
        &encode(&LifecycleRequest::Quit).expect("Quit request"),
    )
    .expect("write Quit Call");
    let (quit_header, quit_payload) = read_frame(&mut client).expect("Quit Reply");
    assert_eq!(quit_header.kind, FrameKind::Reply);
    assert_eq!(quit_header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(quit_header.corr, CorrelationId(43));
    assert_eq!(
        decode::<LifecycleResponse>(&quit_payload).expect("Quit response"),
        LifecycleResponse::Quit
    );
    assert!(
        guardian_prepared.load(Ordering::Acquire),
        "accepted shutdown attribution must precede the Quit reply"
    );
    let mut byte = [0_u8; 1];
    assert_eq!(client.read(&mut byte).expect("link EOF"), 0);
    eof_tx.send(()).expect("record EOF");
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit wake"),
        AppWindowCommand::Quit
    );
    assert!(
        guardian_finished.load(Ordering::Acquire),
        "UI Quit must follow guardian reap acknowledgement"
    );

    router.shutdown().expect("router shutdown");
    guardian_thread.join().expect("guardian thread joins");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn retired_generation_eof_cannot_fail_or_replace_the_successor() {
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    use keld_ipc::link::AppLinkDeadlines as _;

    let (g1_server, mut g1_client) = UnixStream::pair().expect("g1 session pair");
    g1_client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("g1 deadlines");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, _guardian_rx) = mpsc::channel();
    let router = PrimaryRouter::start(
        g1_server,
        window_tx,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
    )
    .expect("generation router");
    let handle = router.handle();
    handle.signal_ready().expect("g1 Ready");
    assert_lifecycle_event(&mut g1_client, LifecycleEvent::Ready);

    handle.retire_generation(1).expect("retire g1");
    handle
        .signal_last_window_closed()
        .expect("record last-window close during gap");
    let (g2_server, mut g2_client) = UnixStream::pair().expect("g2 session pair");
    g2_client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("g2 deadlines");
    handle.install_generation(2, g2_server).expect("install g2");
    assert_lifecycle_event(&mut g2_client, LifecycleEvent::Ready);
    assert_lifecycle_event(&mut g2_client, LifecycleEvent::LastWindowClosed);
    // A stale Quit returns before it reads; the peerless reader is unused.
    let (mut stale_reader, stale_peer) = UnixStream::pair().expect("stale Quit reader");
    drop(stale_peer);
    handle
        .lifecycle_quit(1, CorrelationId(72), &[0], &mut stale_reader)
        .expect("stale g1 Quit is ignored");
    assert!(
        handle.shutdown.is_running(),
        "stale g1 Quit claimed g2 shutdown"
    );
    drop(g1_client);
    assert!(
        window_rx.try_recv().is_err(),
        "retired g1 EOF woke the UI fatal path"
    );
    assert_echo_call(&mut g2_client, 73, "successor");
    drop(g2_client);
    router.shutdown().expect("generation router shutdown");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn recovered_generation_orders_window_events_once_across_installation() {
    use std::os::unix::net::UnixStream;
    if std::env::var_os(RECOVERED_CHILD_ENV).is_none() {
        assert_bounded_recovered_event_order_child();
        return;
    }

    // Gap, live delivery overlapping reader registration, and delivery after
    // installation are distinct schedules of the same real framed router.
    for close_at in ["gap", "registration", "installed"] {
        let (g1_server, mut g1_client) = UnixStream::pair().expect("g1 pair");
        g1_client
            .set_app_link_read_deadline(Some(Duration::from_secs(5)))
            .expect("g1 kill switch");
        let (window_tx, _window_rx) = mpsc::channel();
        let (command_tx, _command_rx) = mpsc::channel();
        let router = PrimaryRouter::start(
            g1_server,
            window_tx,
            PlatformPrimaryOwnerHandle { command_tx },
            SessionShutdownState::new(),
        )
        .expect("router");
        let handle = router.handle();
        handle.signal_ready().expect("initial Ready");
        assert_lifecycle_event(&mut g1_client, LifecycleEvent::Ready);
        handle.retire_generation(1).expect("retire g1");
        if close_at == "gap" {
            handle.signal_last_window_closed().expect("gap Close");
        }
        let (g2_server, mut g2_client) = UnixStream::pair().expect("g2 pair");
        g2_client
            .set_app_link_read_deadline(Some(Duration::from_secs(5)))
            .expect("test kill switch");
        let mut events = Vec::new();
        if close_at == "registration" {
            let registration = handle.readers.lock().expect("pause registration");
            let installer = handle.clone();
            let install = thread::spawn(move || installer.install_generation(2, g2_server));
            // A correlated reply proves the real reader is running while
            // installation cannot yet finish. No absence timeout or sleep.
            collect_events_before_echo(&mut g2_client, 90, &mut events);
            handle
                .signal_last_window_closed()
                .expect("overlapping Close");
            drop(registration);
            install
                .join()
                .expect("installer joins")
                .expect("install g2");
        } else {
            handle.install_generation(2, g2_server).expect("install g2");
            if close_at == "installed" {
                handle.signal_last_window_closed().expect("live Close");
            }
        }
        // Both producers have completed before this positive fence. Every
        // queued Event must precede this reply on the serialized writer.
        collect_events_before_echo(&mut g2_client, 91, &mut events);
        assert_eq!(
            events,
            [LifecycleEvent::Ready, LifecycleEvent::LastWindowClosed],
            "close during {close_at}"
        );
        let retire_deadline = Instant::now() + Duration::from_secs(3);
        let retired_reader_finished = loop {
            let finished = handle
                .readers
                .lock()
                .expect("reader registry")
                .get(&1)
                .expect("retired reader remains registered")
                .is_finished();
            if finished || Instant::now() >= retire_deadline {
                break finished;
            }
            std::thread::yield_now();
        };
        assert!(
            retired_reader_finished,
            "retired G1 reader remained live with its peer open after {close_at}"
        );
        router.shutdown().expect("router shutdown");
    }
    println!("{RECOVERED_CHILD_OBSERVED}");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn collect_events_before_echo(
    client: &mut std::os::unix::net::UnixStream,
    correlation: u32,
    events: &mut Vec<LifecycleEvent>,
) {
    use keld_ipc::echo::{EchoRequest, EchoResponse};
    use keld_ipc::link::read_frame;

    let request = EchoRequest {
        message: "fence".to_owned(),
        count: correlation,
    };
    write_frame(
        client,
        FrameKind::Call,
        0,
        ECHO_CHANNEL,
        CorrelationId(correlation),
        &encode(&request).expect("encode fence"),
    )
    .expect("write fence");
    loop {
        let (header, payload) = read_frame(client).expect("fence frame");
        if header.kind == FrameKind::Event {
            assert_eq!(header.channel, LIFECYCLE_CHANNEL);
            assert_eq!(header.corr, CorrelationId(0));
            events.push(decode(&payload).expect("lifecycle event"));
            assert!(events.len() <= 3, "unbounded lifecycle replay");
        } else {
            assert_eq!(header.kind, FrameKind::Reply);
            assert_eq!(header.channel, ECHO_CHANNEL);
            assert_eq!(header.corr, CorrelationId(correlation));
            assert_eq!(
                decode::<EchoResponse>(&payload).expect("fence payload"),
                EchoResponse {
                    message: request.message,
                    count: correlation
                }
            );
            break;
        }
    }
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn revoke_racing_accepted_quit_does_not_join_on_the_guardian_owner() {
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    let (server, mut client) = UnixStream::pair().expect("Quit race pair");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let (observed_tx, observed_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let guardian_thread = std::thread::spawn(move || {
        let prepare = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("accepted Quit preparation");
        let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(reply) = prepare else {
            panic!("Quit race skipped attribution");
        };
        observed_tx.send(()).expect("report blocked attribution");
        release_rx.recv().expect("release attribution");
        reply.send(Ok(())).expect("accepted Quit attribution reply");
        let shutdown = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("accepted Quit shutdown");
        let TestPrimaryOwnerCommand::Shutdown(reply) = shutdown else {
            panic!("Quit race skipped shutdown");
        };
        reply.send(Ok(())).expect("accepted Quit shutdown reply");
    });
    let router = PrimaryRouter::start(
        server,
        window_tx,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
    )
    .expect("Quit race router");
    let handle = router.handle();
    let quit_handle = handle.clone();
    // The router's own reader serves the link; this direct Quit drains a
    // peerless stream, so its post-REPLY drain ends at EOF at once.
    let (mut quit_reader, quit_peer) =
        std::os::unix::net::UnixStream::pair().expect("direct Quit reader");
    drop(quit_peer);
    let quit_thread = std::thread::spawn(move || {
        quit_handle.lifecycle_quit(1, CorrelationId(81), &[0], &mut quit_reader)
    });
    observed_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("Quit reached guardian owner");
    handle
        .retire_generation(1)
        .expect("terminal revoke must not join blocked reader");
    assert!(
        handle.is_current(1),
        "accepted Quit lost its current writer"
    );
    release_tx.send(()).expect("release Quit attribution");
    quit_thread
        .join()
        .expect("Quit thread joins")
        .expect("Quit race tail");
    let (header, _) = keld_ipc::link::read_frame(&mut client).expect("Quit race reply");
    assert_eq!(header.corr, CorrelationId(81));
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit race UI exit"),
        AppWindowCommand::Quit
    );
    router.shutdown().expect("Quit race router shutdown");
    guardian_thread.join().expect("Quit race guardian joins");
}

#[cfg(any(target_os = "linux", windows))]
use DirectPrimaryOwnerCommand as TestPrimaryOwnerCommand;
#[cfg(target_os = "macos")]
use GuardianOwnerCommand as TestPrimaryOwnerCommand;

#[cfg(target_os = "macos")]
fn primary_echo_test_router() -> (
    PrimaryRouter,
    PrimaryRouterHandle,
    std::os::unix::net::UnixStream,
    Receiver<TestPrimaryOwnerCommand>,
    Receiver<AppWindowCommand>,
) {
    let (server, client) = std::os::unix::net::UnixStream::pair().expect("Echo router pair");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("Echo client read deadline");
    let (command_tx, command_rx) = mpsc::channel();
    let (window_tx, window_rx) = mpsc::channel();
    let router = PrimaryRouter::start(
        server,
        window_tx,
        PlatformPrimaryOwnerHandle { command_tx },
        SessionShutdownState::new(),
    )
    .expect("Echo primary router");
    let handle = router.handle();
    (router, handle, client, command_rx, window_rx)
}

#[test]
#[cfg(target_os = "macos")]
fn renderer_dispatch_stop_does_not_depend_on_webview_sender_drop() {
    let (router, handle, _client, _owner_rx, _window_rx) = primary_echo_test_router();
    let (requests_tx, requests_rx) = mpsc::sync_channel(1);
    let (outcomes_tx, _outcomes_rx) = mpsc::channel();
    let dispatch =
        start_renderer_dispatch(requests_rx, outcomes_tx, handle).expect("renderer dispatch");
    let (done_tx, done_rx) = mpsc::channel();
    let joiner = std::thread::spawn(move || {
        let result = join_renderer_dispatch(dispatch);
        let _ = done_tx.send(result);
    });

    let observed = done_rx.recv_timeout(Duration::from_secs(2));
    drop(requests_tx);
    joiner.join().expect("renderer dispatch join probe");
    assert!(
        observed
            .expect("renderer dispatch ignored its explicit stop")
            .is_ok(),
        "renderer dispatch stop must not wait for WebKit to release its request sender"
    );
    router.shutdown().expect("renderer-stop router shutdown");
}

#[test]
#[cfg(target_os = "macos")]
fn host_echo_reply_shares_primary_reader_with_bun_call_and_ping() {
    use keld_ipc::echo::{EchoRequest, EchoResponse};
    use keld_ipc::link::{read_frame, write_frame};

    let (router, handle, mut client, _owner_rx, _window_rx) = primary_echo_test_router();
    let request = EchoRequest {
        message: "renderer".to_owned(),
        count: 42,
    };
    let request_bytes = encode(&request).expect("encode renderer Echo");
    let call = handle
        .begin_echo_call(&request_bytes)
        .expect("host-originated Echo call");

    let (outbound, outbound_payload) = read_frame(&mut client).expect("outbound renderer Call");
    assert_eq!(outbound.kind, FrameKind::Call);
    assert_eq!(outbound.channel, ECHO_CHANNEL);
    assert_eq!(outbound.corr, call.correlation);
    assert_eq!(outbound_payload, request_bytes);

    write_frame(
        &mut client,
        FrameKind::Ping,
        0,
        keld_ipc::ChannelId::for_test(99),
        CorrelationId(0),
        &[],
    )
    .expect("write interleaved Ping");
    let (pong, pong_payload) = read_frame(&mut client).expect("read interleaved Ping");
    assert_eq!(pong.kind, FrameKind::Ping);
    assert_eq!(pong.channel, keld_ipc::ChannelId::for_test(99));
    assert_eq!(pong.corr, CorrelationId(0));
    assert_eq!(pong_payload, [] as [u8; 0]);

    let bun_request = EchoRequest {
        message: "bun".to_owned(),
        count: 7,
    };
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        ECHO_CHANNEL,
        CorrelationId(91),
        &encode(&bun_request).expect("encode Bun Echo"),
    )
    .expect("write interleaved Bun Call");
    let (bun_reply, bun_payload) = read_frame(&mut client).expect("read Bun Echo Reply");
    assert_eq!(bun_reply.kind, FrameKind::Reply);
    assert_eq!(bun_reply.channel, ECHO_CHANNEL);
    assert_eq!(bun_reply.corr, CorrelationId(91));
    assert_eq!(
        decode::<EchoResponse>(&bun_payload).expect("decode Bun Echo Reply"),
        EchoResponse {
            message: bun_request.message,
            count: bun_request.count,
        }
    );

    let expected = encode(&EchoResponse {
        message: request.message,
        count: request.count,
    })
    .expect("encode renderer Echo Reply");
    write_frame(
        &mut client,
        FrameKind::Reply,
        0,
        ECHO_CHANNEL,
        call.correlation,
        &expected,
    )
    .expect("write renderer Echo Reply");
    match call
        .reply
        .recv_timeout(Duration::from_secs(2))
        .expect("renderer Echo terminal outcome")
        .expect("renderer Echo success")
    {
        PrimaryEchoReply::Reply(payload) => assert_eq!(payload, expected),
        PrimaryEchoReply::Err(error) => panic!("unexpected renderer Echo Err: {error}"),
    }

    router.shutdown().expect("Echo router shutdown");
}

#[test]
#[cfg(target_os = "macos")]
fn failed_primary_writes_retire_the_generation() {
    let (echo_handle, _echo_client, echo_commands) = initial_ready_router();
    let owner = std::thread::spawn(move || {
        let command = echo_commands
            .recv_timeout(Duration::from_secs(2))
            .expect("failed-write owner notification");
        match command {
            GuardianOwnerCommand::FailGeneration(1, reply) => {
                reply.send(Ok(())).expect("failed-write owner reply");
            }
            _ => panic!("failed primary write sent the wrong owner command"),
        }
    });
    {
        let mut current = echo_handle.current.lock().expect("generation lock");
        current
            .as_mut()
            .expect("active renderer generation")
            .writer
            .shutdown_app_link()
            .expect("close renderer writer before probe");
    }
    let error = echo_handle
        .begin_echo_call(&[0x14, 0x02])
        .expect_err("renderer Call write must fail");
    assert!(
        error.to_string().contains("renderer Echo call write"),
        "{error}"
    );
    assert!(
        echo_handle
            .current
            .lock()
            .expect("generation lock")
            .is_none(),
        "failed renderer write left a reusable primary generation"
    );
    owner.join().expect("failed-write owner joins");

    let (event_handle, _event_client, _event_commands) = initial_ready_router();
    {
        let mut current = event_handle.current.lock().expect("generation lock");
        current
            .as_mut()
            .expect("active lifecycle generation")
            .writer
            .shutdown_app_link()
            .expect("close lifecycle writer before probe");
    }
    let error = event_handle
        .signal_last_window_closed()
        .expect_err("lifecycle Event write must fail");
    assert!(error.to_string().contains("lifecycle event"), "{error}");
    assert!(
        event_handle
            .current
            .lock()
            .expect("generation lock")
            .is_none(),
        "failed lifecycle write left a reusable primary generation"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn second_host_echo_call_is_busy_until_the_first_reply_retires() {
    use keld_ipc::link::{read_frame, write_frame};

    let (router, handle, mut client, _owner_rx, _window_rx) = primary_echo_test_router();
    let first = handle.begin_echo_call(&[1, 2, 3]).expect("first Echo call");
    let (outbound, _) = read_frame(&mut client).expect("first Echo frame");
    assert_eq!(outbound.corr, first.correlation);

    let second = handle
        .begin_echo_call(&[4, 5])
        .expect_err("second Echo call must fail busy");
    assert!(
        second.to_string().contains("already pending"),
        "unexpected busy error: {second}"
    );

    write_frame(
        &mut client,
        FrameKind::Reply,
        0,
        ECHO_CHANNEL,
        first.correlation,
        &[9],
    )
    .expect("complete first Echo");
    match first
        .reply
        .recv_timeout(Duration::from_secs(2))
        .expect("first terminal outcome")
        .expect("first success")
    {
        PrimaryEchoReply::Reply(payload) => assert_eq!(payload, [9]),
        PrimaryEchoReply::Err(error) => panic!("unexpected first Err: {error}"),
    }

    let third = handle
        .begin_echo_call(&[6])
        .expect("third Echo after retirement");
    let (third_frame, _) = read_frame(&mut client).expect("third Echo frame");
    assert_eq!(third_frame.corr, third.correlation);
    write_frame(
        &mut client,
        FrameKind::Reply,
        0,
        ECHO_CHANNEL,
        third.correlation,
        &[8],
    )
    .expect("complete third Echo");
    assert!(third.reply.recv_timeout(Duration::from_secs(2)).is_ok());

    router.shutdown().expect("busy router shutdown");
}

#[test]
#[cfg(target_os = "macos")]
fn wrong_correlation_cannot_complete_host_echo_waiter() {
    use keld_ipc::link::{read_frame, write_frame};

    let (router, handle, mut client, _owner_rx, window_rx) = primary_echo_test_router();
    let call = handle.begin_echo_call(&[1]).expect("host Echo call");
    let (outbound, _) = read_frame(&mut client).expect("host Echo frame");
    assert_eq!(outbound.corr, call.correlation);
    let wrong = CorrelationId(call.correlation.0.wrapping_add(1).max(1));
    write_frame(&mut client, FrameKind::Reply, 0, ECHO_CHANNEL, wrong, &[2])
        .expect("wrong-corr reply bytes");

    let terminal = call
        .reply
        .recv_timeout(Duration::from_secs(2))
        .expect("wrong corr must terminalize the pending call")
        .expect_err("wrong corr cannot become a successful reply");
    assert!(
        terminal.to_string().contains("primary reader"),
        "unexpected waiter failure: {terminal}"
    );
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("protocol failure wakes UI"),
        AppWindowCommand::Fatal
    );
    assert!(
        router.shutdown().is_err(),
        "wrong correlation must remain a router/session failure"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn app_link_disconnect_terminalizes_pending_echo_and_fresh_session_still_works() {
    use keld_ipc::link::{read_frame, write_frame};

    let (router, handle, mut client, _owner_rx, window_rx) = primary_echo_test_router();
    let call = handle.begin_echo_call(&[1, 2]).expect("pending host Echo");
    let (outbound, _) = read_frame(&mut client).expect("pending Echo frame");
    assert_eq!(outbound.corr, call.correlation);
    drop(client);

    let terminal = call
        .reply
        .recv_timeout(Duration::from_secs(2))
        .expect("disconnect must settle the pending waiter")
        .expect_err("disconnect cannot become a successful reply");
    assert!(
        terminal.to_string().contains("primary reader"),
        "unexpected disconnect terminal outcome: {terminal}"
    );
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("disconnect wakes the fatal UI path"),
        AppWindowCommand::Fatal
    );
    assert!(
        router.shutdown().is_err(),
        "the failed reader remains a session failure"
    );

    let (fresh_router, fresh_handle, mut fresh_client, _owner_rx, _window_rx) =
        primary_echo_test_router();
    let fresh = fresh_handle
        .begin_echo_call(&[7])
        .expect("fresh non-recovery session Echo");
    let (fresh_frame, _) = read_frame(&mut fresh_client).expect("fresh Echo frame");
    assert_eq!(fresh_frame.corr, fresh.correlation);
    write_frame(
        &mut fresh_client,
        FrameKind::Reply,
        0,
        ECHO_CHANNEL,
        fresh.correlation,
        &[8],
    )
    .expect("fresh Echo Reply");
    match fresh
        .reply
        .recv_timeout(Duration::from_secs(2))
        .expect("fresh Echo terminal outcome")
        .expect("fresh Echo success")
    {
        PrimaryEchoReply::Reply(payload) => assert_eq!(payload, [8]),
        PrimaryEchoReply::Err(error) => panic!("unexpected fresh Echo Err: {error}"),
    }
    fresh_router.shutdown().expect("fresh router shutdown");
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn primary_test_stream_pair() -> (BootstrapStream, BootstrapStream) {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        std::os::unix::net::UnixStream::pair().expect("primary stream pair")
    }
    #[cfg(windows)]
    {
        let listener = Arc::new(keld_ipc::BootstrapListener::bind().expect("bind pipe"));
        let link = listener.app_link();
        let (endpoint, token) = keld_ipc::parse_app_link(&link).expect("parse pipe link");
        let endpoint = endpoint.to_owned();
        let acceptor = Arc::clone(&listener);
        let worker = thread::spawn(move || acceptor.accept_authenticated());
        let mut client = BootstrapStream::connect(&endpoint).expect("connect pipe client");
        client
            .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
            .expect("client deadlines");
        keld_ipc::link::handshake_client(&mut client, &token).expect("client HELLO");
        let server = worker
            .join()
            .expect("accept join")
            .expect("accept result")
            .expect("authenticated server stream");
        (server, client)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn initial_ready_router() -> (
    PrimaryRouterHandle,
    BootstrapStream,
    Receiver<TestPrimaryOwnerCommand>,
) {
    let (handle, client, command_rx, _window_rx) = initial_ready_router_with_window_loop();
    (handle, client, command_rx)
}

/// [`initial_ready_router`] whose UI loop's command receiver the test keeps, as the
/// live wake bridge holds it until the first terminal command.
fn initial_ready_router_with_window_loop() -> (
    PrimaryRouterHandle,
    BootstrapStream,
    Receiver<TestPrimaryOwnerCommand>,
    Receiver<AppWindowCommand>,
) {
    let (server, client) = primary_test_stream_pair();
    let (command_tx, command_rx) = mpsc::channel();
    let (window_tx, window_rx) = mpsc::channel();
    let handle = PrimaryRouterHandle {
        current: Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
            identity: PrimaryLinkIdentity::Fixture,
            attempt: 1,
            writer: server,
            reader_stop: Arc::new(AtomicBool::new(false)),
            pending_quit: None,
        }))),
        readers: Arc::new(Mutex::new(HashMap::new())),
        #[cfg(target_os = "macos")]
        pending_echo: Arc::new(Mutex::new(None)),
        #[cfg(target_os = "macos")]
        pending_echo_attempt: Arc::new(AtomicU32::new(0)),
        #[cfg(target_os = "macos")]
        pending_echo_corr: Arc::new(AtomicU32::new(0)),
        #[cfg(target_os = "macos")]
        next_host_corr: Arc::new(AtomicU32::new(1)),
        window_ready: Arc::new(AtomicBool::new(false)),
        last_window_closed: Arc::new(AtomicBool::new(false)),
        recovery_armed: Arc::new(AtomicBool::new(false)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        shutdown: SessionShutdownState::new(),
        fs: None,
        fs_worker_commands: None,
        #[cfg(target_os = "macos")]
        auth: None,
        #[cfg(target_os = "macos")]
        auth_commands: None,
        guardian: PlatformPrimaryOwnerHandle { command_tx },
        window_commands: window_tx,
        #[cfg(target_os = "macos")]
        quit_drain_hooks: Arc::default(),
    };
    (handle, client, command_rx, window_rx)
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn failed_successor_ready_replay_requests_its_retired_attempt_restart() {
    let (handle, _g1_client, _owner_rx) = initial_ready_router();
    handle.window_ready.store(true, Ordering::Release);
    handle.recovery_armed.store(true, Ordering::Release);
    handle.last_revoked_attempt.store(1, Ordering::Release);
    handle.retire_generation(1).expect("retire g1");
    let (g2_server, g2_client) = primary_test_stream_pair();
    drop(g2_client);
    let error = handle
        .install_generation(2, g2_server)
        .expect_err("closed g2 replay");
    assert!(
        error.error.to_string().contains("lifecycle event"),
        "{error:?}"
    );
    assert_eq!(error.retired_after_failed_ready, Some(2));
    assert!(handle.current.lock().expect("generation lock").is_none());
    let mut restarted = Vec::new();
    assert!(restart_failed_bound_generation(
        &handle,
        2,
        &error,
        |attempt| {
            assert!(matches!(
                handle.shutdown.transition.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
            restarted.push(attempt);
        }
    ));
    assert_eq!(
        restarted,
        [2],
        "the direct owner must request the failed bound attempt"
    );

    // The restart-admitted-first order is the other linearization of the
    // same transition used by teardown. Once teardown wins afterward, a
    // late successor is closed without publishing current or a reader.
    let mut router = PrimaryRouter {
        handle: handle.clone(),
        fs_worker: None,
        #[cfg(target_os = "macos")]
        auth_worker: None,
    };
    router
        .stop_and_join()
        .expect("teardown after admitted restart");
    assert!(handle.shutdown.reader_stop.load(Ordering::Acquire));
    let (late_server, _late_client) = primary_test_stream_pair();
    handle
        .install_generation(3, late_server)
        .expect("late bound after admitted restart closes cleanly");
    assert!(handle.current.lock().expect("generation lock").is_none());
    assert_eq!(handle.readers.lock().expect("reader lock").len(), 0);
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn failed_successor_ready_replay_cannot_restart_after_router_teardown() {
    let (handle, _g1_client, _owner_rx) = initial_ready_router();
    handle.window_ready.store(true, Ordering::Release);
    handle.recovery_armed.store(true, Ordering::Release);
    handle.last_revoked_attempt.store(1, Ordering::Release);
    handle.retire_generation(1).expect("retire g1");
    let (g2_server, g2_client) = primary_test_stream_pair();
    drop(g2_client);
    let failure = handle
        .install_generation(2, g2_server)
        .expect_err("closed g2 replay");
    assert_eq!(failure.retired_after_failed_ready, Some(2));
    let mut router = PrimaryRouter {
        handle: handle.clone(),
        fs_worker: None,
        #[cfg(target_os = "macos")]
        auth_worker: None,
    };
    router.stop_and_join().expect("router teardown");
    assert!(
        handle.shutdown.is_running(),
        "UI failure need not claim accepted shutdown"
    );
    assert!(handle.shutdown.reader_stop.load(Ordering::Acquire));
    let mut restarted = Vec::new();
    assert!(!restart_failed_bound_generation(
        &handle,
        2,
        &failure,
        |attempt| restarted.push(attempt)
    ));
    assert_eq!(restarted.len(), 0);
    assert!(handle.current.lock().expect("generation lock").is_none());
    let (late_server, _late_client) = primary_test_stream_pair();
    handle
        .install_generation(3, late_server)
        .expect("late bind is harmless after teardown");
    assert!(handle.current.lock().expect("generation lock").is_none());
    assert_eq!(handle.readers.lock().expect("reader lock").len(), 0);
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn failed_ready_restart_overlapping_router_teardown_has_only_serial_outcomes() {
    let (handle, _g1_client, _owner_rx) = initial_ready_router();
    handle.window_ready.store(true, Ordering::Release);
    handle.recovery_armed.store(true, Ordering::Release);
    handle.last_revoked_attempt.store(1, Ordering::Release);
    handle.retire_generation(1).expect("retire g1");
    let (g2_server, g2_client) = primary_test_stream_pair();
    drop(g2_client);
    let failure = handle
        .install_generation(2, g2_server)
        .expect_err("closed g2 replay");
    assert_eq!(failure.retired_after_failed_ready, Some(2));

    // The owner and UI teardown race for the same transition. The lock
    // makes this equivalent to one of two observable orders: either the
    // supervisor restart is admitted first, or reader_stop wins first.
    let start = std::sync::Arc::new(std::sync::Barrier::new(3));
    let owner_start = std::sync::Arc::clone(&start);
    let owner_handle = handle.clone();
    let (restart_tx, restart_rx) = mpsc::channel();
    let owner = thread::spawn(move || {
        owner_start.wait();
        let mut restarted = Vec::new();
        let admitted = restart_failed_bound_generation(&owner_handle, 2, &failure, |attempt| {
            assert!(
                !owner_handle.shutdown.reader_stop.load(Ordering::Acquire),
                "restart callback ran after router stop publication"
            );
            restart_tx.send(attempt).expect("record restart request");
            restarted.push(attempt);
        });
        (admitted, restarted)
    });
    let teardown_start = std::sync::Arc::clone(&start);
    let mut router = PrimaryRouter {
        handle: handle.clone(),
        fs_worker: None,
        #[cfg(target_os = "macos")]
        auth_worker: None,
    };
    let teardown = thread::spawn(move || {
        teardown_start.wait();
        router.stop_and_join().expect("overlapping router teardown");
    });
    start.wait();

    let (admitted, restarted) = owner.join().expect("owner decision joins");
    teardown.join().expect("teardown worker joins");
    let requests: Vec<u32> = restart_rx.try_iter().collect();
    assert_eq!(requests, restarted);
    assert!(requests.is_empty() || requests == [2], "{requests:?}");
    assert_eq!(admitted, !requests.is_empty());
    assert!(handle.shutdown.reader_stop.load(Ordering::Acquire));
    assert!(handle.current.lock().expect("generation lock").is_none());
    assert_eq!(handle.readers.lock().expect("reader lock").len(), 0);

    let (late_server, _late_client) = primary_test_stream_pair();
    handle
        .install_generation(3, late_server)
        .expect("late bound closes after teardown");
    assert!(handle.current.lock().expect("generation lock").is_none());
    assert_eq!(handle.readers.lock().expect("reader lock").len(), 0);
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn failed_bound_restart_rejects_unowned_or_terminal_transitions() {
    for case in [
        "mismatch",
        "stale",
        "pre-ready",
        "unarmed",
        "shutdown",
        "successor",
        "ordinary",
        "misordered",
    ] {
        let (handle, _client, _owner_rx) = initial_ready_router();
        handle.window_ready.store(true, Ordering::Release);
        handle.recovery_armed.store(true, Ordering::Release);
        handle.last_revoked_attempt.store(1, Ordering::Release);
        handle.retire_generation(1).expect("retire g1");
        let mut failure = PrimaryGenerationFailure {
            error: app_detail("lifecycle event", "forced failed replay"),
            retired_after_failed_ready: Some(2),
        };
        match case {
            "mismatch" => failure.retired_after_failed_ready = Some(1),
            "stale" => handle.last_revoked_attempt.store(2, Ordering::Release),
            "pre-ready" => handle.window_ready.store(false, Ordering::Release),
            "unarmed" => handle.recovery_armed.store(false, Ordering::Release),
            "shutdown" => assert!(handle.shutdown.claim_cli_lease_lost()),
            "successor" | "misordered" => {
                let (writer, _peer) = primary_test_stream_pair();
                *handle.current.lock().expect("generation lock") = Some(ActivePrimaryGeneration {
                    identity: PrimaryLinkIdentity::Fixture,
                    attempt: if case == "successor" { 3 } else { 1 },
                    writer,
                    reader_stop: Arc::new(AtomicBool::new(false)),
                    pending_quit: None,
                });
                if case != "successor" {
                    failure.retired_after_failed_ready = None;
                }
            }
            "ordinary" => failure.retired_after_failed_ready = None,
            _ => unreachable!(),
        }
        let mut restarted = Vec::new();
        assert!(
            !restart_failed_bound_generation(&handle, 2, &failure, |attempt| restarted
                .push(attempt)),
            "{case}"
        );
        assert!(restarted.is_empty(), "{case}: {restarted:?}");
    }
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn failed_bound_keeps_the_existing_current_attempt_restart_path() {
    let (handle, _client, _owner_rx) = initial_ready_router();
    handle.window_ready.store(true, Ordering::Release);
    handle.recovery_armed.store(true, Ordering::Release);
    let failure = PrimaryGenerationFailure::from(app_detail(
        "primary session reader",
        "forced post-publication reader spawn failure",
    ));
    let mut restarted = Vec::new();
    assert!(restart_failed_bound_generation(
        &handle,
        1,
        &failure,
        |attempt| restarted.push(attempt)
    ));
    assert_eq!(restarted, [1]);
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn failed_close_write_cannot_mint_ready_replay_recovery() {
    let (handle, client, _owner_rx) = initial_ready_router();
    drop(client);
    let error = handle
        .write_event_guarded(LifecycleEvent::LastWindowClosed)
        .expect_err("closed primary must fail Close");
    assert!(
        error.error.to_string().contains("lifecycle event"),
        "{error:?}"
    );
    assert!(handle.current.lock().expect("generation lock").is_none());
    assert_eq!(error.retired_after_failed_ready, None);
}

#[test]
#[cfg(any(target_os = "linux", windows))]
fn direct_recovery_arm_is_published_before_acknowledgement() {
    for armed in [false, true] {
        let (handle, _client, _owner_rx) = initial_ready_router();
        let result = armed.then_some(()).ok_or_else(|| String::from("denied"));
        let mut acknowledged = false;
        acknowledge_direct_recovery_arm(Some(&handle), result, |result| {
            assert_eq!(result.is_ok(), armed);
            assert_eq!(handle.recovery_armed.load(Ordering::Acquire), armed);
            acknowledged = true;
        });
        assert!(acknowledged);
    }
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn initial_ready_after_accepted_shutdown_does_not_arm_a_stopped_owner() {
    use std::io::Read as _;

    let (handle, mut client, command_rx) = initial_ready_router();
    assert!(handle.shutdown.claim_cli_lease_lost());
    drop(command_rx); // The accepted tail has already stopped its owner.
    handle
        .signal_ready()
        .expect("late navigation must not restart admission");
    assert!(!handle.window_ready.load(Ordering::Acquire));
    assert!(!handle.recovery_armed.load(Ordering::Acquire));
    client
        .set_nonblocking(true)
        .expect("observe absence without waiting");
    let error = client
        .read(&mut [0])
        .expect_err("late Ready must write no bytes");
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn initial_ready_releases_transition_before_owner_acknowledgment() {
    let (handle, mut client, command_rx) = initial_ready_router();
    let shutdown = handle.shutdown.clone();
    let ready = Arc::clone(&handle.window_ready);
    let owner = thread::spawn(move || {
        let TestPrimaryOwnerCommand::ArmRecovery(_, reply) =
            command_rx.recv().expect("arm request")
        else {
            panic!("first owner command must arm recovery");
        };
        // A real owner may need this lock while applying a generation
        // update before it can acknowledge the already-enqueued arm.
        let _transition = shutdown.transition_guard();
        assert!(ready.load(Ordering::Acquire));
        reply.send(Ok(())).expect("arm acknowledgment");
    });
    handle
        .signal_ready()
        .expect("owner callback must not deadlock");
    let (header, payload) = keld_ipc::link::read_frame(&mut client).expect("Ready bytes");
    assert_eq!(header.kind, FrameKind::Event);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(header.corr, CorrelationId(0));
    assert_eq!(payload, [0]); // Rust lifecycle Ready postcard discriminant.
    assert!(handle.recovery_armed.load(Ordering::Acquire));
    owner.join().expect("owner callback joins");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn initial_ready_preserves_unaccepted_owner_failures() {
    let (handle, _client, command_rx) = initial_ready_router();
    drop(command_rx);
    let error = handle
        .signal_ready()
        .expect_err("a missing live-session owner is a fault");
    assert!(error.to_string().contains("KELD-CORE-037"), "{error}");
    assert!(
        error.to_string().contains("primary recovery arm"),
        "{error}"
    );

    for reject in [true, false] {
        let (handle, _client, command_rx) = initial_ready_router();
        let owner = thread::spawn(move || {
            let TestPrimaryOwnerCommand::ArmRecovery(_, reply) =
                command_rx.recv().expect("arm request")
            else {
                panic!("first owner command must arm recovery");
            };
            if reject {
                reply
                    .send(Err(String::from("forced arm refusal")))
                    .expect("refusal reply");
            }
            // Dropping the reply without acknowledgment remains a real error.
        });
        let error = handle
            .signal_ready()
            .expect_err("arm fault must remain visible");
        assert!(error.to_string().contains("KELD-CORE-037"), "{error}");
        if reject {
            assert!(error.to_string().contains("forced arm refusal"), "{error}");
        }
        assert!(!handle.recovery_armed.load(Ordering::Acquire));
        owner.join().expect("owner probe joins");
    }
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn failed_initial_ready_write_denies_recovery_before_successor() {
    use std::collections::HashMap;
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    let (server, client) = UnixStream::pair().expect("failed Ready pair");
    drop(client);
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let (window_tx, _window_rx) = mpsc::channel();
    let handle = PrimaryRouterHandle {
        current: Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
            identity: PrimaryLinkIdentity::Fixture,
            attempt: 1,
            writer: server,
            reader_stop: Arc::new(AtomicBool::new(false)),
            pending_quit: None,
        }))),
        readers: Arc::new(Mutex::new(HashMap::new())),
        #[cfg(target_os = "macos")]
        pending_echo: Arc::new(Mutex::new(None)),
        #[cfg(target_os = "macos")]
        pending_echo_attempt: Arc::new(AtomicU32::new(0)),
        #[cfg(target_os = "macos")]
        pending_echo_corr: Arc::new(AtomicU32::new(0)),
        #[cfg(target_os = "macos")]
        next_host_corr: Arc::new(AtomicU32::new(1)),
        window_ready: Arc::new(AtomicBool::new(false)),
        last_window_closed: Arc::new(AtomicBool::new(false)),
        recovery_armed: Arc::new(AtomicBool::new(false)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        shutdown: SessionShutdownState::new(),
        fs: None,
        fs_worker_commands: None,
        #[cfg(target_os = "macos")]
        auth: None,
        #[cfg(target_os = "macos")]
        auth_commands: None,
        guardian: PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        window_commands: window_tx,
        #[cfg(target_os = "macos")]
        quit_drain_hooks: Arc::default(),
    };
    let error = handle
        .signal_ready()
        .expect_err("closed g1 must fail Ready");
    assert!(error.to_string().contains("lifecycle event"), "{error}");
    assert!(matches!(
        guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Ready failure recovery denial"),
        TestPrimaryOwnerCommand::DenyRecovery
    ));
    assert!(!handle.recovery_armed.load(Ordering::Acquire));
    assert!(!handle.window_ready.load(Ordering::Acquire));
}

#[test]
#[cfg(target_os = "macos")]
fn failed_link_recovery_signal_wakes_ui_fatal() {
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    let (server, client) = UnixStream::pair().expect("link failure pair");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let guardian_thread = std::thread::spawn(move || {
        let command = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("link failure command");
        let GuardianOwnerCommand::FailGeneration(1, reply) = command else {
            panic!("reader sent the wrong guardian command");
        };
        reply
            .send(Err(String::from("forced group-signal failure")))
            .expect("link failure reply");
    });
    let router = PrimaryRouter::start(
        server,
        window_tx,
        GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
    )
    .expect("link failure router");
    router.handle().signal_ready().expect("link failure Ready");
    drop(client);
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("link failure UI wake"),
        AppWindowCommand::Fatal
    );
    assert!(
        router.shutdown().is_err(),
        "link failure vanished at shutdown"
    );
    guardian_thread.join().expect("link failure guardian joins");
}

#[test]
#[cfg(target_os = "macos")]
fn retire_does_not_join_reader_waiting_for_link_failure_ack() {
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    let (server, client) = UnixStream::pair().expect("link retirement pair");
    let (window_tx, _window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let (blocked_tx, blocked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let guardian_thread = std::thread::spawn(move || {
        let command = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("link retirement command");
        let GuardianOwnerCommand::FailGeneration(1, reply) = command else {
            panic!("reader sent the wrong retirement command");
        };
        blocked_tx.send(()).expect("report blocked link failure");
        release_rx.recv().expect("release link failure");
        reply.send(Ok(())).expect("link failure reply");
    });
    let router = PrimaryRouter::start(
        server,
        window_tx,
        GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        SessionShutdownState::new(),
    )
    .expect("link retirement router");
    router
        .handle()
        .signal_ready()
        .expect("link retirement Ready");
    drop(client);
    blocked_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("reader waits for link failure acknowledgment");
    let retire_handle = router.handle();
    let (retired_tx, retired_rx) = mpsc::channel();
    let retire_thread = std::thread::spawn(move || {
        retired_tx
            .send(retire_handle.retire_generation(1))
            .expect("return retirement result");
    });
    retired_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("guardian-owner retirement must not join the blocked reader")
        .expect("retire blocked generation");
    release_tx
        .send(())
        .expect("release link failure acknowledgment");
    retire_thread.join().expect("retire thread joins");
    guardian_thread.join().expect("guardian thread joins");
    router.shutdown().expect("link retirement router shutdown");
}

#[cfg(target_os = "macos")]
fn begin_pending_renderer_echo(
    handle: &PrimaryRouterHandle,
    client: &mut std::os::unix::net::UnixStream,
) -> PrimaryEchoCall {
    let pending = handle
        .begin_echo_call(&[0x14, 0x02])
        .expect("lease-loss pending renderer Echo");
    let (outbound, _) = keld_ipc::link::read_frame(client).expect("lease-loss renderer Echo frame");
    assert_eq!(outbound.kind, FrameKind::Call);
    assert_eq!(outbound.channel, ECHO_CHANNEL);
    assert_eq!(outbound.corr, pending.correlation);
    pending
}

#[test]
#[cfg(target_os = "macos")]
fn cli_lease_loss_closes_link_before_reap_and_sends_no_quit_reply() {
    use std::io::Read as _;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    use keld_ipc::link::AppLinkDeadlines as _;

    let (server, mut client) = UnixStream::pair().expect("lease-loss session pair");
    client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("client deadlines");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let (eof_tx, eof_rx) = mpsc::channel();
    let (writer_tx, writer_rx) = mpsc::channel();
    let guardian_thread = std::thread::spawn(move || {
        let GuardianOwnerCommand::PrepareAcceptedShutdown(prepare_reply) = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian lease-loss attribution")
        else {
            panic!("unexpected guardian command before lease-loss attribution");
        };
        let writer: Arc<Mutex<Option<ActivePrimaryGeneration>>> = writer_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("router writer identity");
        assert!(
            writer.try_lock().is_ok(),
            "guardian RPC ran while the app-link writer was locked"
        );
        prepare_reply
            .send(Ok(()))
            .expect("guardian lease-loss attribution reply");
        let GuardianOwnerCommand::Shutdown(shutdown_reply) = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian lease-loss shutdown")
        else {
            panic!("unexpected guardian command during lease-loss shutdown");
        };
        eof_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("link EOF before guardian lease-loss reap");
        shutdown_reply
            .send(Ok(()))
            .expect("guardian lease-loss reply");
    });
    let shutdown = SessionShutdownState::new();
    let router = PrimaryRouter::start(
        server,
        window_tx,
        GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        shutdown.clone(),
    )
    .expect("primary router");
    writer_tx
        .send(Arc::clone(&router.handle.current))
        .expect("share router writer identity");

    let pending = begin_pending_renderer_echo(&router.handle(), &mut client);

    assert!(shutdown.claim_cli_lease_lost());
    let mut byte = [0_u8; 1];
    assert_eq!(
        client.read(&mut byte).expect("lease-loss link EOF"),
        0,
        "lease loss must not write a fabricated lifecycle reply"
    );
    eof_tx.send(()).expect("record lease-loss EOF");
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("lease-loss UI Quit wake"),
        AppWindowCommand::Quit
    );
    let terminal = pending
        .reply
        .recv_timeout(Duration::from_secs(2))
        .expect("lease loss must settle the pending renderer waiter")
        .expect_err("lease loss cannot produce an application reply");
    assert!(
        terminal.to_string().contains("CLI lease loss"),
        "unexpected lease-loss waiter result: {terminal}"
    );

    router.shutdown().expect("router shutdown");
    guardian_thread.join().expect("guardian thread joins");
}

#[test]
#[cfg(target_os = "macos")]
fn cli_lease_loss_during_generation_gap_runs_the_shutdown_tail() {
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    let (server, client) = UnixStream::pair().expect("gap session pair");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let shutdown = SessionShutdownState::new();
    let guardian_shutdown = shutdown.clone();
    let guardian_thread = std::thread::spawn(move || {
        let prepare = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("gap accepted-shutdown attribution");
        match prepare {
            GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => {
                assert_eq!(
                    guardian_shutdown.cause(),
                    SESSION_RUNNING,
                    "gap published CLI acceptance before guardian attribution"
                );
                assert_eq!(
                    guardian_shutdown.reserved_cause.load(Ordering::Acquire),
                    SESSION_CLI_LEASE_LOST,
                    "gap must reserve the CLI loss first"
                );
                assert!(
                    !guardian_shutdown.tail_started.load(Ordering::Acquire),
                    "gap started the tail before guardian attribution"
                );
                reply.send(Ok(())).expect("gap attribution reply");
            }
            _ => panic!("gap skipped accepted-shutdown attribution"),
        }
        let shutdown = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("gap guardian shutdown");
        match shutdown {
            GuardianOwnerCommand::Shutdown(reply) => {
                reply.send(Ok(())).expect("gap shutdown reply");
            }
            unexpected => {
                let detail = match unexpected {
                    GuardianOwnerCommand::AttachRouter(_, _) => "AttachRouter".to_owned(),
                    GuardianOwnerCommand::ArmRecovery(_, _) => "ArmRecovery".to_owned(),
                    GuardianOwnerCommand::DenyRecovery => "DenyRecovery".to_owned(),
                    GuardianOwnerCommand::FailGeneration(attempt, _) => {
                        format!("FailGeneration({attempt})")
                    }
                    GuardianOwnerCommand::FailRetiredGeneration(attempt, _) => {
                        format!("FailRetiredGeneration({attempt})")
                    }
                    GuardianOwnerCommand::PrepareAcceptedShutdown(_) => {
                        "PrepareAcceptedShutdown".to_owned()
                    }
                    GuardianOwnerCommand::Shutdown(_) => "Shutdown".to_owned(),
                };
                panic!("gap expected Shutdown, observed {detail}");
            }
        }
    });
    let router = PrimaryRouter::start(
        server,
        window_tx,
        GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        shutdown.clone(),
    )
    .expect("gap router");
    router.handle().signal_ready().expect("gap Ready");
    router.handle().retire_generation(1).expect("retire g1");
    drop(client);
    assert!(shutdown.reserve_cli_lease_lost());
    assert_eq!(
        shutdown.cause(),
        SESSION_RUNNING,
        "reservation is not acceptance"
    );
    assert!(!shutdown.tail_started.load(Ordering::Acquire));
    assert!(
        !router.handle().is_current(1),
        "fixture must remain a generation gap"
    );

    router
        .handle()
        .cli_lease_lost()
        .expect("generation-gap lease-loss tail");
    assert_eq!(
        shutdown.cause(),
        SESSION_CLI_LEASE_LOST,
        "acknowledged tail must publish the reserved cause"
    );
    assert!(shutdown.tail_started.load(Ordering::Acquire));
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("gap UI Quit"),
        AppWindowCommand::Quit
    );
    router.shutdown().expect("gap router shutdown");
    guardian_thread.join().expect("gap guardian thread joins");
}

#[test]
#[cfg(target_os = "macos")]
fn terminal_tail_failure_wakes_the_ui_fatal_path() {
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    let (server, _client) = UnixStream::pair().expect("tail-failure session pair");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let guardian_thread = std::thread::spawn(move || {
        let prepare_reply = match guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian tail-failure attribution")
        {
            GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => reply,
            GuardianOwnerCommand::Shutdown(_) => panic!("tail skipped attribution"),
            GuardianOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
            GuardianOwnerCommand::FailGeneration(_, _)
            | GuardianOwnerCommand::FailRetiredGeneration(_, _) => {
                panic!("unexpected link failure")
            }
            GuardianOwnerCommand::ArmRecovery(_, _) => panic!("unexpected recovery arm"),
            GuardianOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
        };
        prepare_reply
            .send(Err(String::from("forced attribution failure")))
            .expect("guardian tail-failure reply");
    });
    let shutdown = SessionShutdownState::new();
    let router = PrimaryRouter::start(
        server,
        window_tx,
        GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        shutdown.clone(),
    )
    .expect("primary router");

    assert!(shutdown.claim_cli_lease_lost());
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("terminal tail Fatal wake"),
        AppWindowCommand::Fatal
    );
    let error = router
        .shutdown()
        .expect_err("terminal attribution failure must remain visible");
    assert!(error.to_string().contains("forced attribution failure"));
    guardian_thread.join().expect("guardian thread joins");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn generation_reader_stop_preserves_successor_and_terminal_scope() {
    let shutdown = SessionShutdownState::new();
    let retired = shutdown.register_generation_reader();
    let successor = shutdown.register_generation_reader();
    retired.store(true, Ordering::Release);
    assert!(retired.load(Ordering::Acquire));
    assert!(!successor.load(Ordering::Acquire));
    assert!(!shutdown.reader_stop.load(Ordering::Acquire));

    shutdown.stop_reader();
    assert!(successor.load(Ordering::Acquire));
    assert!(
        shutdown
            .register_generation_reader()
            .load(Ordering::Acquire),
        "a reader registered after terminal shutdown must start stopped"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn session_shutdown_accepts_exactly_one_first_cause() {
    let shutdown = SessionShutdownState::new();
    assert!(shutdown.claim_cli_lease_lost());
    assert!(!shutdown.claim_lifecycle_quit());
    assert_eq!(shutdown.cause(), SESSION_CLI_LEASE_LOST);

    let lifecycle = SessionShutdownState::new();
    assert!(lifecycle.claim_lifecycle_quit());
    assert!(!lifecycle.claim_cli_lease_lost());
    assert_eq!(lifecycle.cause(), SESSION_LIFECYCLE_QUIT);

    let gated = SessionShutdownState::new();
    let transition = gated.transition_guard();
    let claimant = gated.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let claim = std::thread::spawn(move || {
        started_tx.send(()).expect("claim started");
        claimant.claim_cli_lease_lost()
    });
    started_rx.recv().expect("claim thread started");
    assert_eq!(
        gated.cause(),
        SESSION_RUNNING,
        "a terminal cause changed while a write transition was active"
    );
    drop(transition);
    assert!(claim.join().expect("claim thread joins"));
    assert!(!gated.claim_lifecycle_quit());
    assert_eq!(gated.cause(), SESSION_CLI_LEASE_LOST);
}

#[test]
#[cfg(target_os = "macos")]
fn terminal_cleanup_reports_independent_router_and_guardian_failures() {
    let error = collapse_app_results([
        Err(app_detail("router tail", "link close failed")),
        Err(app_detail("guardian tail", "group reap failed")),
    ])
    .expect_err("two terminal failures must be aggregated");
    let rendered = error.to_string();
    assert!(rendered.contains("link close failed"), "{rendered}");
    assert!(rendered.contains("group reap failed"), "{rendered}");
}

#[test]
#[cfg(target_os = "macos")]
fn link_shutdown_accepts_already_closed_but_propagates_other_io_errors() {
    finish_link_shutdown(
        Err(std::io::Error::from(std::io::ErrorKind::NotConnected)),
        "test link close",
    )
    .expect("already-closed link is idempotent success");
    let error = finish_link_shutdown(
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        "test link close",
    )
    .expect_err("unrelated link error must remain fatal");
    assert!(error.to_string().contains("permission denied"), "{error}");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn assert_echo_call(client: &mut std::os::unix::net::UnixStream, correlation: u32, message: &str) {
    use keld_ipc::codec::{decode, encode};
    use keld_ipc::echo::{EchoRequest, EchoResponse};
    use keld_ipc::link::read_frame;

    let request = EchoRequest {
        message: message.to_owned(),
        count: correlation,
    };
    write_frame(
        client,
        FrameKind::Call,
        0,
        ECHO_CHANNEL,
        CorrelationId(correlation),
        &encode(&request).expect("echo request"),
    )
    .expect("write echo Call");
    let (header, payload) = read_frame(client).expect("echo Reply");
    assert_eq!(header.kind, FrameKind::Reply);
    assert_eq!(header.channel, ECHO_CHANNEL);
    assert_eq!(header.corr, CorrelationId(correlation));
    assert_eq!(
        decode::<EchoResponse>(&payload).expect("echo response"),
        EchoResponse {
            message: message.to_owned(),
            count: correlation,
        }
    );
}

#[test]
#[cfg(windows)]
fn windows_shutdown_disconnect_is_idempotent_but_other_requests_fail() {
    let (shutdown_tx, shutdown_rx) = mpsc::sync_channel::<Result<(), String>>(1);
    drop(shutdown_tx);
    assert!(receive_direct_owner_reply(&shutdown_rx, "Windows primary shutdown", true).is_ok());

    let (request_tx, request_rx) = mpsc::sync_channel::<Result<(), String>>(1);
    drop(request_tx);
    assert!(
        receive_direct_owner_reply(&request_rx, "Windows primary recovery arm", false).is_err()
    );
}

#[test]
#[cfg(windows)]
fn windows_lease_tail_failure_wakes_the_window_fatally() {
    let (window_tx, window_rx) = mpsc::channel();
    let (error_tx, error_rx) = mpsc::channel();
    publish_windows_lease_result(
        Err(app_detail("Windows lease test", "forced tail failure")),
        &error_tx,
        &window_tx,
    );
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("lease failure Fatal wake"),
        AppWindowCommand::Fatal
    );
    assert!(
        error_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("lease failure diagnostic")
            .to_string()
            .contains("forced tail failure")
    );

    publish_windows_lease_result(Ok(()), &error_tx, &window_tx);
    assert!(matches!(
        window_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        error_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

#[test]
#[cfg(windows)]
#[allow(unsafe_code)] // test owns the live tempfile handle for every Win32 flag query/mutation
fn windows_lease_handle_inheritance_is_cleared_before_bun_spawn() {
    use std::os::windows::io::AsRawHandle as _;

    let file = tempfile::tempfile().expect("temporary handle");
    let handle = file.as_raw_handle().cast();
    // SAFETY: `handle` is borrowed from `file`, which remains live through
    // every call. The test changes and reads only HANDLE_FLAG_INHERIT.
    assert_ne!(
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
        0,
        "mark test handle inheritable: {}",
        io::Error::last_os_error()
    );
    let mut flags = 0;
    // SAFETY: `flags` is a live writable u32 and `handle` is live.
    assert_ne!(unsafe { GetHandleInformation(handle, &raw mut flags) }, 0);
    assert_ne!(flags & HANDLE_FLAG_INHERIT, 0);

    clear_windows_handle_inheritance(handle).expect("clear handle inheritance");
    // SAFETY: same live handle and output storage as above.
    assert_ne!(unsafe { GetHandleInformation(handle, &raw mut flags) }, 0);
    assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
}

#[test]
#[cfg(windows)]
#[allow(unsafe_code)] // test owns and closes both anonymous-pipe handles
fn windows_preflight_rejects_a_lease_pipe_with_no_writer() {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::Pipes::CreatePipe;

    let mut reader = std::ptr::null_mut();
    let mut writer = std::ptr::null_mut();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>())
            .expect("SECURITY_ATTRIBUTES size"),
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 0,
    };
    // SAFETY: both output pointers and the attributes value remain live;
    // successful handles are closed below exactly once.
    assert_ne!(
        unsafe { CreatePipe(&raw mut reader, &raw mut writer, &raw const attributes, 0) },
        0
    );
    // SAFETY: `writer` is the unique live handle returned by CreatePipe.
    assert_ne!(unsafe { CloseHandle(writer) }, 0);
    assert!(!windows_lease_pipe_is_live(reader.addr()).expect("peek closed lease"));
    // SAFETY: `reader` is the remaining unique live pipe handle.
    assert_ne!(unsafe { CloseHandle(reader) }, 0);
}

#[test]
#[cfg(windows)]
fn windows_shutdown_claim_blocks_late_ui_startup() {
    let shutdown = SessionShutdownState::new();
    assert!(shutdown.claim_cli_lease_lost());
    let created = AtomicBool::new(false);
    let result = run_direct_startup_if_session_running(&shutdown, || {
        created.store(true, Ordering::Release);
    });
    assert!(result.is_none());
    assert!(!created.load(Ordering::Acquire));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn assert_lifecycle_event(client: &mut std::os::unix::net::UnixStream, expected: LifecycleEvent) {
    let (header, payload) = keld_ipc::link::read_frame(client).expect("lifecycle Event");
    assert_eq!(header.kind, FrameKind::Event);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(
        keld_ipc::codec::decode::<LifecycleEvent>(&payload).expect("lifecycle payload"),
        expected
    );
}
