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
        windows: None,
        recovery_armed: Arc::new(AtomicBool::new(true)),
        last_revoked_attempt: Arc::new(AtomicU32::new(0)),
        pre_ready_link_failed: Arc::new(AtomicU32::new(0)),
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
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
