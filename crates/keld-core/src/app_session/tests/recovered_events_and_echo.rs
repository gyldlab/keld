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
