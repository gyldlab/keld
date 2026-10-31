/// One guarded primary router over a socket pair, with the FS worker held
/// by a test gate after it takes its job (GH-528 T2 router tests; T5 runs
/// them on every OS with a router).
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

/// A guarded router over a connected link pair (a Unix socket pair, or an
/// authenticated named-pipe pair on Windows), and the pair's client end.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn guarded_test_router() -> (GuardedTestRouter, BootstrapStream) {
    use keld_ipc::link::AppLinkDeadlines as _;

    let (server, client) = primary_test_stream_pair();
    client
        .set_app_link_deadlines(Some(Duration::from_secs(5)))
        .expect("guarded client deadlines");
    (guarded_router(server), client)
}

/// A guarded router over `server`: a pair end, or a Bun role's
/// authenticated app link.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn guarded_router(server: BootstrapStream) -> GuardedTestRouter {
    guarded_router_with_test_setup(server, None)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn guarded_router_with_test_setup(
    server: BootstrapStream,
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
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub(in crate::app_session) mod worker_link_e2e;

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn write_fs_call(client: &mut BootstrapStream, corr: u32, target: &Path) {
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn write_quit_call(client: &mut BootstrapStream, corr: u32) {
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
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn read_frames_until_eof(client: &mut BootstrapStream) -> Vec<(keld_ipc::FrameHeader, Vec<u8>)> {
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
///
/// This client has no read pending until `retire_generation` has returned,
/// because the test thread is its only reader. On Windows the answers
/// therefore survive only if the host's close keeps unread pipe data, which
/// `DisconnectNamedPipe` does not (#1073 W2, gh527 §10).
#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
    let expected = vec![
        (
            FrameKind::Err,
            FS_CHANNEL,
            CorrelationId(11),
            "KELD-IPC-023".to_owned(),
        ),
        (
            FrameKind::Err,
            LIFECYCLE_CHANNEL,
            CorrelationId(12),
            "KELD-IPC-023".to_owned(),
        ),
    ];
    assert_eq!(
        answers, expected,
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
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
///
/// The client reads nothing until the guardian's `Shutdown`, which the
/// Quit tail sends only after it has closed the link. So what it reads
/// depends only on what the close keeps. The drain ends at its idle
/// backstop (`QuitDrainEnd::IdleBackstop`), because that client never closes.
///
/// Every OS expects the same frames: Windows runs the same post-Quit drain
/// since the W1 fix (#1073, gh527 §10), and its close keeps the unread
/// frames (#1073 W2).
#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn quit_drain_answers_received_calls_with_024_and_runs_none() {
    let (t, mut client) = guarded_test_router();
    let (end_tx, drain_end) = mpsc::sync_channel(1);
    t.router.handle().observe_next_quit_drain_end(end_tx);
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
    // The client neither writes nor closes until the guardian's Shutdown, so
    // the drain can only end at its idle backstop, not at the hard deadline.
    assert_eq!(
        drain_end
            .recv_timeout(Duration::from_secs(5))
            .expect("drain end"),
        QuitDrainEnd::IdleBackstop
    );
    let frames: Vec<(FrameKind, keld_ipc::ChannelId, CorrelationId, String)> =
        read_frames_until_eof(&mut client)
            .iter()
            .map(|(header, payload)| {
                let body = if header.kind == FrameKind::Reply {
                    assert_eq!(
                        decode::<LifecycleResponse>(payload).expect("Quit response"),
                        LifecycleResponse::Quit
                    );
                    String::from("Quit")
                } else {
                    decode::<CallError>(payload).expect("024 CallError").code
                };
                (header.kind, header.channel, header.corr, body)
            })
            .collect();
    let quit_reply = (
        FrameKind::Reply,
        LIFECYCLE_CHANNEL,
        CorrelationId(20),
        String::from("Quit"),
    );
    let expected = vec![
        quit_reply,
        (
            FrameKind::Err,
            FS_CHANNEL,
            CorrelationId(21),
            String::from("KELD-IPC-024"),
        ),
    ];
    assert_eq!(
        frames, expected,
        "the Quit REPLY, then only the FS CALL is answered"
    );
    assert!(!target.exists(), "a post-Quit FS CALL must not execute");
    assert_eq!(
        t.window
            .recv_timeout(Duration::from_secs(5))
            .expect("UI Quit"),
        AppWindowCommand::Quit
    );
    assert_router_shutdown_after_quit_with_trailing_call(t.router);
}

/// Joins a router whose role wrote a CALL after its accepted Quit. The
/// post-Quit drain answers that CALL, so the router shuts down cleanly on
/// every OS (Windows since the W1 fix, #1073, gh527 §10).
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn assert_router_shutdown_after_quit_with_trailing_call(router: PrimaryRouter) {
    router.shutdown().expect("router shutdown after Quit");
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
/// Unix only by OS fact (#528 T5): the role half-closes the link, which a
/// named pipe cannot do, because closing a pipe handle ends both directions.
#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
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
