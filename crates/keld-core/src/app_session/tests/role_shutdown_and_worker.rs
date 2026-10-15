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
