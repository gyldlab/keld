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
        windows: None,
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
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
