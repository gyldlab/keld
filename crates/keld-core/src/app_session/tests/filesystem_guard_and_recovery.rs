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
        dialog_nonce_high_water: 0,
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
        windows: None,
        recovery_armed: Arc::new(AtomicBool::new(true)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        pre_ready_link_failed: Arc::new(AtomicBool::new(false)),
        shutdown,
        fs: Some(snapshot.fs_weak()),
        fs_worker_commands: None,
        #[cfg(target_os = "macos")]
        auth: None,
        #[cfg(target_os = "macos")]
        auth_commands: None,
        #[cfg(target_os = "macos")]
        dialog: None,
        #[cfg(target_os = "macos")]
        menu: None,
        #[cfg(target_os = "macos")]
        compat: None,
        guardian: GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        window_commands: window_tx,
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

// The recovery assertion runs while the native worker can be waiting for an
// owner receipt. Unwinding before a manual join must still release that wait,
// finish a pending successor install, stop the worker and join both threads.
#[cfg(target_os = "macos")]
struct FsRecoveryOrderingCleanup {
    commands: mpsc::SyncSender<FsWorkerCommand>,
    guardian: mpsc::Receiver<GuardianOwnerCommand>,
    recovery_reply: Option<mpsc::SyncSender<Result<Instant, String>>>,
    acknowledged: bool,
    installer: Option<thread::JoinHandle<()>>,
    router: Option<PrimaryRouter>,
    report: Option<mpsc::SyncSender<Result<(), String>>>,
    finished: bool,
    worker: Option<thread::JoinHandle<Result<(), HostAppError>>>,
}

#[cfg(target_os = "macos")]
impl FsRecoveryOrderingCleanup {
    fn receive_recovery(&mut self) -> u32 {
        let command = self
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("failed terminal write must request retired-generation recovery");
        let GuardianOwnerCommand::FailRetiredGeneration(attempt, reply) = command else {
            panic!("failed terminal write used the wrong owner command");
        };
        self.recovery_reply = Some(reply);
        attempt
    }

    fn acknowledge_recovery(&mut self) -> Result<(), String> {
        let reply = self
            .recovery_reply
            .take()
            .ok_or_else(|| String::from("missing retired-generation recovery reply"))?;
        self.acknowledged = true;
        reply
            .send(Ok(Instant::now()))
            .map_err(|_| String::from("retired-generation recovery receiver closed"))
    }

    fn finish(&mut self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let mut errors = Vec::new();
        if !self.acknowledged {
            if self.recovery_reply.is_none() {
                match self.guardian.recv_timeout(Duration::from_secs(5)) {
                    Ok(GuardianOwnerCommand::FailRetiredGeneration(attempt, reply)) => {
                        if attempt != 1 {
                            errors.push(format!("unexpected retired generation {attempt}"));
                        }
                        self.recovery_reply = Some(reply);
                    }
                    Ok(_) => errors.push(String::from("unexpected guardian recovery command")),
                    Err(error) => {
                        errors.push(format!("guardian recovery receipt missing: {error}"));
                    }
                }
            }
            if self.recovery_reply.is_some()
                && let Err(error) = self.acknowledge_recovery()
            {
                errors.push(error);
            }
        }
        // An installer may be waiting for the worker's Retire/Prepare receipt.
        // Let it finish before enqueueing Stop, or Stop could overtake Prepare.
        if let Some(installer) = self.installer.take()
            && installer.join().is_err()
        {
            errors.push(String::from("successor installer panicked"));
        }
        if self.worker.is_some() {
            if self.commands.send(FsWorkerCommand::Stop).is_err() {
                errors.push(String::from("FS worker stopped before Stop delivery"));
            }
            if let Some(worker) = self.worker.take() {
                match worker.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => errors.push(format!("FS worker failed: {error}")),
                    Err(_) => errors.push(String::from("FS worker panicked")),
                }
            }
        }
        if let Some(router) = self.router.take()
            && let Err(error) = router.shutdown()
        {
            errors.push(format!("router shutdown failed: {error}"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for FsRecoveryOrderingCleanup {
    fn drop(&mut self) {
        // A cleanup panic must not turn the original test assertion into a
        // double-panic abort. Both thread joins precede router shutdown.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.finish()))
            .unwrap_or_else(|_| Err(String::from("recovery-ordering cleanup panicked")));
        if let Some(report) = self.report.take() {
            let _ = report.send(result);
        } else if let Err(error) = result {
            // A cleanup error cannot replace the assertion that caused unwind.
            eprintln!("recovery-ordering fixture cleanup: {error}");
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
enum FsRecoveryEarlyFailure {
    AfterWorker,
    AfterInstaller,
}

#[test]
#[cfg(target_os = "macos")]
fn successor_install_does_not_wait_for_retired_fs_recovery_ack() {
    exercise_fs_recovery_ordering(None, None);
}

#[test]
#[cfg(target_os = "macos")]
fn recovery_ordering_cleanup_joins_after_forced_early_panic() {
    for stage in [
        FsRecoveryEarlyFailure::AfterWorker,
        FsRecoveryEarlyFailure::AfterInstaller,
    ] {
        let (report, observed) = mpsc::sync_channel(1);
        let unwind =
            std::panic::catch_unwind(|| exercise_fs_recovery_ordering(Some(stage), Some(report)));
        let failure = unwind.expect_err("early failure must cause a real assertion unwind");
        let message = failure
            .downcast_ref::<&'static str>()
            .copied()
            .or_else(|| failure.downcast_ref::<String>().map(String::as_str));
        let expected = match stage {
            FsRecoveryEarlyFailure::AfterWorker => "forced failure after worker spawn",
            FsRecoveryEarlyFailure::AfterInstaller => "forced failure after installer spawn",
        };
        assert_eq!(message, Some(expected), "unexpected panic at {stage:?}");
        observed
            .recv_timeout(Duration::from_secs(5))
            .expect("cleanup did not join both threads after unwind")
            .unwrap_or_else(|error| panic!("{stage:?} left recovery threads live: {error}"));
    }
}

#[cfg(target_os = "macos")]
fn exercise_fs_recovery_ordering(
    early_failure: Option<FsRecoveryEarlyFailure>,
    report: Option<mpsc::SyncSender<Result<(), String>>>,
) {
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
        dialog_nonce_high_water: 0,
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
        windows: None,
        recovery_armed: Arc::new(AtomicBool::new(true)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        pre_ready_link_failed: Arc::new(AtomicBool::new(false)),
        shutdown: SessionShutdownState::new(),
        fs: Some(snapshot.fs_weak()),
        fs_worker_commands: Some(commands.clone()),
        #[cfg(target_os = "macos")]
        auth: None,
        #[cfg(target_os = "macos")]
        auth_commands: None,
        #[cfg(target_os = "macos")]
        dialog: None,
        #[cfg(target_os = "macos")]
        menu: None,
        #[cfg(target_os = "macos")]
        compat: None,
        guardian: GuardianOwnerHandle {
            command_tx: guardian_tx,
        },
        window_commands: window_tx,
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
    // The worker spawn is the last initializer: no unowned, fallible step
    // remains between a successful spawn and installation of the Drop guard.
    let mut cleanup = FsRecoveryOrderingCleanup {
        commands,
        guardian: guardian_rx,
        recovery_reply: None,
        acknowledged: false,
        installer: None,
        router: Some(router),
        report,
        finished: false,
        worker: Some(thread::spawn(move || {
            run_fs_worker(&receiver, &worker_handle, worker_broker, 1, None)
        })),
    };
    if matches!(early_failure, Some(FsRecoveryEarlyFailure::AfterWorker)) {
        panic!("forced failure after worker spawn");
    }
    let attempt = cleanup.receive_recovery();
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
    cleanup.installer = Some(thread::spawn(move || {
        let result = g2_handle.install_generation(2, g2_server);
        // The assertion thread may unwind before receiving this result.
        let _ = installed_tx.send(result);
    }));
    if matches!(early_failure, Some(FsRecoveryEarlyFailure::AfterInstaller)) {
        panic!("forced failure after installer spawn");
    }

    let install_report = installed_rx.recv_timeout(Duration::from_secs(2));
    assert!(
        install_report.is_ok(),
        "G2 installation waited for the retired-generation recovery acknowledgment"
    );
    install_report
        .expect("G2 installation report")
        .expect("G2 installs while owner recovery acknowledgment is pending");
    assert!(
        matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "recovery ordering sent an unexpected fatal window command"
    );

    cleanup
        .acknowledge_recovery()
        .expect("acknowledge retired-generation recovery after G2 installation");
    cleanup
        .finish()
        .expect("recovery-ordering threads join and router shuts down");
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
