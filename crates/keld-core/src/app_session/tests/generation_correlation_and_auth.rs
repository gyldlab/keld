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
