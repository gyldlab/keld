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
fn accepted_quit_with_failed_owner_shutdown_still_ends_the_ui_loop_as_fatal() {
    // GH-824 hosted macOS "host did not exit": the accepted Quit tail cleared
    // the current generation, then the primary owner's shutdown failed (native
    // Guardian CLEAR group signal: EPERM). The tail returned that error before
    // any terminal UI command, the reader no longer owned a current attempt,
    // and the owner thread had already exited, so nothing ever woke the event
    // loop. The oracle is the UI command channel itself: the failed tail MUST
    // deliver Fatal (never a successful Quit) without the window's help.
    use std::io::Read as _;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    use keld_ipc::codec::{decode, encode};
    use keld_ipc::link::{AppLinkDeadlines, read_frame};

    const OWNER_FAILURE: &str = "injected primary owner shutdown failure";
    let (server, mut client) = UnixStream::pair().expect("session pair");
    client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("client deadlines");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, guardian_rx) = mpsc::channel();
    let guardian_thread = std::thread::spawn(move || {
        let Ok(TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare_reply)) =
            guardian_rx.recv_timeout(Duration::from_secs(5))
        else {
            panic!("accepted Quit must first prepare owner shutdown");
        };
        prepare_reply.send(Ok(())).expect("owner preparation reply");
        let Ok(TestPrimaryOwnerCommand::Shutdown(shutdown_reply)) =
            guardian_rx.recv_timeout(Duration::from_secs(5))
        else {
            panic!("accepted Quit tail must request owner shutdown");
        };
        shutdown_reply
            .send(Err(OWNER_FAILURE.to_owned()))
            .expect("owner shutdown failure reply");
        // The production owner thread exits after a failed shutdown.
        drop(guardian_rx);
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

    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(91),
        &encode(&LifecycleRequest::Quit).expect("Quit request"),
    )
    .expect("write Quit Call");
    let (quit_header, quit_payload) = read_frame(&mut client).expect("Quit Reply");
    assert_eq!(quit_header.kind, FrameKind::Reply);
    assert_eq!(quit_header.corr, CorrelationId(91));
    assert_eq!(
        decode::<LifecycleResponse>(&quit_payload).expect("Quit response"),
        LifecycleResponse::Quit
    );
    let mut byte = [0_u8; 1];
    assert_eq!(client.read(&mut byte).expect("link EOF"), 0);
    guardian_thread.join().expect("owner fixture joins");

    // Kill switch only: the receipt is the condition awaited.
    assert_eq!(
        window_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("failed Quit tail must still wake the UI loop"),
        AppWindowCommand::Fatal,
        "a failed owner shutdown must not report a successful Quit"
    );
    assert!(
        window_rx.try_recv().is_err(),
        "the failed tail must deliver exactly one terminal UI command"
    );
    let shutdown = router
        .shutdown()
        .expect_err("router must retain the failed Quit tail");
    assert!(
        shutdown.to_string().contains(OWNER_FAILURE),
        "unexpected router failure: {shutdown}"
    );
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
