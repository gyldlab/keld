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
