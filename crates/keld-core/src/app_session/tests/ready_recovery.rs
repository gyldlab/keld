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
pub(in crate::app_session) fn initial_ready_router_with_window_loop() -> (
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
        windows: None,
        recovery_armed: Arc::new(AtomicBool::new(false)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        pre_ready_link_failed: Arc::new(AtomicBool::new(false)),
        shutdown: SessionShutdownState::new(),
        fs: None,
        fs_worker_commands: None,
        #[cfg(target_os = "macos")]
        auth: None,
        #[cfg(target_os = "macos")]
        auth_commands: None,
        guardian: PlatformPrimaryOwnerHandle { command_tx },
        window_commands: window_tx,
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

/// The `finish_tail` decision on every OS (ported from #1018 into the
/// decomposed layout): an accepted lifecycle Quit has already removed the
/// primary generation, so its retired reader cannot send a current-generation
/// `Fatal`. When the primary owner (Guardian on macOS, `DirectPrimaryOwner` on
/// Linux and Windows) then fails its shutdown, the tail alone must wake the UI
/// loop, exactly once, and still return the owner's error. CLI lease loss and
/// the profile handover have their own terminal paths, so they must not get a
/// second UI event from the tail.
#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn failed_owner_shutdown_after_accepted_quit_wakes_ui_on_every_os() {
    for (cause, wakes_ui) in [
        (SESSION_LIFECYCLE_QUIT, true),
        (SESSION_CLI_LEASE_LOST, false),
        (SESSION_HANDOVER, false),
    ] {
        let (handle, _client, owner_rx, window_rx) = initial_ready_router_with_window_loop();
        assert!(handle.shutdown.claim(cause), "claim session cause {cause}");
        let owner = thread::spawn(move || {
            match owner_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("primary owner terminal shutdown")
            {
                TestPrimaryOwnerCommand::Shutdown(reply) => reply
                    .send(Err(String::from("owned primary shutdown denied")))
                    .expect("report exact owner failure"),
                _ => panic!("finish_tail sends only the terminal shutdown"),
            }
        });
        let error = handle
            .finish_tail("lifecycle Quit")
            .expect_err("the owner's shutdown failure is returned");
        owner.join().expect("test primary owner joins");
        assert!(
            error.to_string().contains("owned primary shutdown denied"),
            "cause {cause}: {error}"
        );
        if wakes_ui {
            assert_eq!(
                window_rx.try_recv(),
                Ok(AppWindowCommand::Fatal),
                "a failed owner shutdown after an accepted Quit must wake the UI"
            );
        }
        assert!(
            window_rx.try_recv().is_err(),
            "cause {cause}: at most one terminal UI event, and none for other causes"
        );
    }
}

/// GH-1182: before Ready, a link EOF must not fail the session from the reader.
/// The process owner decides the generation's outcome, so the pre-Ready gate in
/// `apply_generation_update` reports the one typed startup failure whichever of
/// the EOF or the process exit is observed first.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn pre_ready_link_eof_defers_to_the_owner_generation_outcome() {
    let (handle, _g1_client, owner_rx) = initial_ready_router();
    assert!(!handle.window_ready.load(Ordering::Acquire));
    let (mut reader, peer) = primary_test_stream_pair();
    drop(peer);
    let caller = super::PrimaryCallerContext {
        attempt: 1,
        identity: PrimaryLinkIdentity::Fixture,
        reader_stop: Arc::new(AtomicBool::new(false)),
    };
    let reader_handle = handle.clone();
    let reader_thread =
        thread::spawn(move || super::read_primary_frames(&mut reader, &reader_handle, &caller));
    let TestPrimaryOwnerCommand::FailGeneration(attempt, reply) = owner_rx
        .recv_timeout(APP_LINK_IO_DEADLINE)
        .expect("pre-Ready EOF asks the owner to fail its generation")
    else {
        panic!("pre-Ready EOF sent the wrong owner command");
    };
    assert_eq!(attempt, 1);
    reply.send(Ok(())).expect("owner acknowledges the failure");
    reader_thread
        .join()
        .expect("pre-Ready reader joins")
        .expect("pre-Ready EOF leaves the outcome to the owner");
}

/// PR #1300 review (P1): a pre-Ready EOF the reader has observed must win over
/// a later `Ready`. The owner learns of the EOF asynchronously, so without an
/// ordered failure transition a `NavigationReady` arriving before the owner's
/// `Revoked` would publish `Ready` and arm recovery, letting the failed
/// generation's restart provision a successor. The owner here answers every
/// request, so a premature arm cannot hang this test; it only records them.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn pre_ready_link_eof_refuses_a_later_ready_and_its_recovery_arm() {
    let (handle, mut g1_client, owner_rx) = initial_ready_router();
    let owner = thread::spawn(move || {
        let mut requests = Vec::new();
        // Ends once every router handle (and its command sender) is dropped.
        while let Ok(command) = owner_rx.recv() {
            match command {
                TestPrimaryOwnerCommand::FailGeneration(attempt, reply) => {
                    requests.push(format!("FailGeneration({attempt})"));
                    reply.send(Ok(())).expect("acknowledge the link failure");
                }
                TestPrimaryOwnerCommand::ArmRecovery(_, reply) => {
                    requests.push(String::from("ArmRecovery"));
                    reply.send(Ok(())).expect("acknowledge the recovery arm");
                }
                TestPrimaryOwnerCommand::DenyRecovery => {
                    requests.push(String::from("DenyRecovery"));
                }
                _ => requests.push(String::from("unexpected owner command")),
            }
        }
        requests
    });
    let (mut reader, peer) = primary_test_stream_pair();
    drop(peer);
    let caller = super::PrimaryCallerContext {
        attempt: 1,
        identity: PrimaryLinkIdentity::Fixture,
        reader_stop: Arc::new(AtomicBool::new(false)),
    };
    let reader_handle = handle.clone();
    thread::spawn(move || super::read_primary_frames(&mut reader, &reader_handle, &caller))
        .join()
        .expect("pre-Ready reader joins")
        .expect("pre-Ready EOF leaves the outcome to the owner");

    handle
        .signal_ready()
        .expect("a refused Ready is not itself a session fault");
    assert!(
        !handle.window_ready.load(Ordering::Acquire),
        "Ready was published after the pre-Ready EOF"
    );
    assert!(
        !handle.recovery_armed.load(Ordering::Acquire),
        "recovery was armed after the pre-Ready EOF"
    );
    // The first lifecycle frame the application receives is this probe, so
    // no Ready was written before it.
    handle
        .signal_last_window_closed()
        .expect("write the ordering probe");
    let (header, payload) =
        keld_ipc::link::read_frame(&mut g1_client).expect("first lifecycle frame");
    assert_eq!(header.kind, FrameKind::Event);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(
        decode::<LifecycleEvent>(&payload).expect("lifecycle payload"),
        LifecycleEvent::LastWindowClosed,
        "Ready reached the application after the pre-Ready EOF"
    );
    drop(handle);
    assert_eq!(
        owner.join().expect("test owner joins"),
        ["FailGeneration(1)"],
        "the pre-Ready EOF must reach the owner and nothing may arm recovery"
    );
}

/// GH-1182: the first reader starts before the router attaches, so a link
/// failure in that window must still reach the Supervisor; only a request for a
/// generation the attached router no longer holds is stale.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn direct_link_failure_restarts_before_router_attach_and_rejects_stale_attempts() {
    assert!(super::direct_link_failure_restarts(None, 1));
    let (handle, _g1_client, _owner_rx) = initial_ready_router();
    assert!(super::direct_link_failure_restarts(Some(&handle), 1));
    assert!(!super::direct_link_failure_restarts(Some(&handle), 2));
}

/// The real role supervisor queues a pre-Ready revocation while the direct
/// owner has no router yet. Its typed startup error must not depend on whether
/// `AttachRouter` was processed before the revocation channel.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn revoked_before_router_attachment_preserves_pre_ready_error() {
    #[cfg(target_os = "linux")]
    let config =
        keld_runtime::primary::PrimaryRoleConfig::primary("/bin/sh").args(["-c", "exit 19"]);
    #[cfg(windows)]
    let config =
        keld_runtime::primary::PrimaryRoleConfig::primary("cmd.exe").args(["/C", "exit 19"]);
    let (supervisor, recovery) =
        keld_runtime::primary::PrimaryRoleSupervisor::start_with_bound_generations_gated(
            config.admission_timeout(Duration::from_secs(2)),
        )
        .expect("start a real, initially unready supervisor");
    let deadline = Instant::now() + Duration::from_secs(5);
    let error = loop {
        match super::drain_direct_primary_role_events(&supervisor, None) {
            Ok(()) => {
                assert!(
                    Instant::now() < deadline,
                    "real child exit did not deliver a pre-attach revocation"
                );
                thread::yield_now();
            }
            Err(error) => break error,
        }
    };
    let _ = recovery.deny();
    supervisor.shutdown();
    let _ = supervisor.wait_for_outcome();
    assert!(
        error
            .to_string()
            .contains("Bun terminated before the initial window became ready"),
        "pre-attach revocation lost its typed startup cause: {error}"
    );
}

/// The session caller must receive the terminal owner's typed error after a
/// pre-attach Revoked, not a replacement attachment-channel disconnect error.
/// Run the real supervisor/owner and wait for the owner thread to end before
/// sending `AttachRouter`, making the cross-channel order deterministic.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn caller_sees_pre_ready_owner_cause_when_attachment_loses_the_race() {
    #[cfg(target_os = "linux")]
    let config =
        keld_runtime::primary::PrimaryRoleConfig::primary("/bin/sh").args(["-c", "exit 19"]);
    #[cfg(windows)]
    let config =
        keld_runtime::primary::PrimaryRoleConfig::primary("cmd.exe").args(["/C", "exit 19"]);
    let (supervisor, recovery) =
        keld_runtime::primary::PrimaryRoleSupervisor::start_with_bound_generations_gated(
            config.admission_timeout(Duration::from_secs(2)),
        )
        .expect("start real supervisor");
    let (window_tx, _window_rx) = mpsc::channel();
    let mut owner = super::DirectPrimaryOwner::start(
        supervisor,
        recovery,
        window_tx,
        SessionShutdownState::new(),
    )
    .expect("start direct primary owner");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !owner
        .handle
        .as_ref()
        .is_some_and(std::thread::JoinHandle::is_finished)
    {
        assert!(
            Instant::now() < deadline,
            "pre-Ready owner did not terminate after child exit"
        );
        thread::yield_now();
    }
    let (router, _client, _test_owner_rx) = initial_ready_router();
    let error = owner
        .attach_router(router)
        .expect_err("failed owner cannot acknowledge router attachment");
    assert!(
        error
            .to_string()
            .contains("Bun terminated before the initial window became ready"),
        "caller lost the owner's typed pre-Ready cause: {error}"
    );
    assert!(
        owner.handle.is_none(),
        "terminal owner result must be joined once"
    );
}

/// Runs a real, initially unready supervisor whose only child exits with
/// `code` before any HELLO, under a direct owner sharing `shutdown`, and
/// returns the owner's terminal error once its thread has ended.
#[cfg(any(target_os = "linux", windows))]
fn pre_ready_owner_terminal_error(shutdown: SessionShutdownState, code: u8) -> HostAppError {
    #[cfg(target_os = "linux")]
    let config = keld_runtime::primary::PrimaryRoleConfig::primary("/bin/sh")
        .args(["-c".to_owned(), format!("exit {code}")]);
    #[cfg(windows)]
    let config = keld_runtime::primary::PrimaryRoleConfig::primary("cmd.exe")
        .args(["/C".to_owned(), format!("exit {code}")]);
    let (supervisor, recovery) =
        keld_runtime::primary::PrimaryRoleSupervisor::start_with_bound_generations_gated(
            config.admission_timeout(Duration::from_secs(2)),
        )
        .expect("start real supervisor");
    let (window_tx, _window_rx) = mpsc::channel();
    let mut owner = super::DirectPrimaryOwner::start(supervisor, recovery, window_tx, shutdown)
        .expect("start direct primary owner");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !owner
        .handle
        .as_ref()
        .is_some_and(std::thread::JoinHandle::is_finished)
    {
        assert!(
            Instant::now() < deadline,
            "owner did not terminate after its child exited"
        );
        thread::yield_now();
    }
    let (router, _client, _test_owner_rx) = initial_ready_router();
    owner
        .attach_router(router)
        .expect_err("a terminated owner returns its terminal error")
}

/// PR #1300 review (P2-1): the owner's typed cause outranks the window loop's
/// error only when the owner reached it before teardown began. Teardown stops
/// the readers and closes the app link first, so a Bun that then exits makes
/// the owner see a pre-Ready `Revoked` (or a self-termination) that is that
/// teardown's consequence; it must not displace the window's own failure,
/// such as a navigation deadline. Exit 19 takes the gated crash path, so the
/// owner sees `Revoked`; exit 0 may reach it as `Revoked` or as `Stopped`.
#[test]
#[cfg(any(target_os = "linux", windows))]
fn owner_failure_outranks_the_window_only_when_it_preceded_teardown() {
    for code in [19, 0] {
        let first = pre_ready_owner_terminal_error(SessionShutdownState::new(), code);
        assert!(
            super::owner_cause_outranks_window(&first),
            "exit {code}: an owner failure before teardown lost its precedence: {first}"
        );
        let teardown = SessionShutdownState::new();
        teardown.stop_reader();
        let after = pre_ready_owner_terminal_error(teardown, code);
        assert!(
            !super::owner_cause_outranks_window(&after),
            "exit {code}: an owner failure seen after teardown began outranked the window: {after}"
        );
    }
}
