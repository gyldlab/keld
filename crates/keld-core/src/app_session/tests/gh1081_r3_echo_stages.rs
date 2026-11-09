// GH-1081 round 3 (research only; never-merge draft PR #1082). Host-side stage
// records for one renderer Echo CALL on the real primary router: a write that
// the kernel stops part-way (no production hook: the test fills the very same
// AF_UNIX socket through its own descriptor, and the router's existing
// APP_LINK_IO_DEADLINE ends the write), and retirement with a live peer.
//
// Oracles: the exact bytes the peer end receives up to EOF, the router's typed
// result, the owner command the router sends, and the reply channel's
// settlement count. Waits are blocking reads/receives on those observables;
// every timeout is a kill switch only.
#[cfg(target_os = "macos")]
mod gh1081_r3_echo_stages {
    use super::{
        AppWindowCommand, Duration, ECHO_CHANNEL, FrameKind, GuardianOwnerCommand, Instant,
        PlatformPrimaryOwnerHandle, PrimaryEchoReply, PrimaryRouter, PrimaryRouterHandle, Receiver,
        SessionShutdownState, encode, mpsc, primary_echo_test_router, write_frame,
    };
    use keld_ipc::HEADER_LEN;
    use keld_ipc::echo::EchoRequest;
    use keld_ipc::link::read_frame;
    use std::io::{Read as _, Write as _};
    use std::os::unix::net::UnixStream;

    const KILL_SWITCH: Duration = Duration::from_secs(60);
    const FILLER: u8 = 0xf7;

    /// The largest renderer payload (4,096 bytes) as a valid Echo request.
    fn max_payload() -> Vec<u8> {
        let payload = encode(&EchoRequest {
            message: "p".repeat(4_096 - 3),
            count: 9,
        })
        .expect("encode maximum Echo request");
        assert_eq!(payload.len(), 4_096);
        payload
    }

    struct FilledRouter {
        router: PrimaryRouter,
        handle: PrimaryRouterHandle,
        peer: UnixStream,
        filler: usize,
        owner: std::thread::JoinHandle<Vec<String>>,
        _window: Receiver<AppWindowCommand>,
    }

    /// A router whose host-to-peer socket buffer is full of test filler, then
    /// `free` bytes are drained so exactly that much space exists.
    fn filled_router(free: usize) -> FilledRouter {
        let (server, mut peer) = UnixStream::pair().expect("renderer Echo pair");
        peer.set_read_timeout(Some(KILL_SWITCH))
            .expect("peer kill switch");
        let mut fill = server
            .try_clone()
            .expect("test descriptor of the router socket");
        fill.set_nonblocking(true).expect("fill nonblocking");
        let mut filler = 0;
        loop {
            match fill.write(&[FILLER; 256]) {
                Ok(written) => filler += written,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("fill router socket: {error}"),
            }
        }
        // O_NONBLOCK is shared by every descriptor of this open socket.
        fill.set_nonblocking(false)
            .expect("restore blocking router socket");
        drop(fill);
        let mut drained = vec![0; free];
        peer.read_exact(&mut drained).expect("free exact space");
        assert!(drained.iter().all(|byte| *byte == FILLER));

        let (command_tx, command_rx) = mpsc::channel();
        let (window_tx, window) = mpsc::channel();
        let router = PrimaryRouter::start(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle { command_tx },
            SessionShutdownState::new(),
        )
        .expect("renderer Echo router");
        let handle = router.handle();
        // Answers the router's owner commands; records which ones it saw.
        let owner = std::thread::spawn(move || {
            let mut seen = Vec::new();
            while let Ok(command) = command_rx.recv_timeout(KILL_SWITCH) {
                match command {
                    GuardianOwnerCommand::FailGeneration(attempt, reply) => {
                        seen.push(format!("FailGeneration({attempt})"));
                        let _ = reply.send(Ok(()));
                    }
                    other => seen.push(format!("other:{}", command_name(&other))),
                }
            }
            seen
        });
        FilledRouter {
            router,
            handle,
            peer,
            filler,
            owner,
            _window: window,
        }
    }

    fn command_name(command: &GuardianOwnerCommand) -> &'static str {
        match command {
            GuardianOwnerCommand::FailGeneration(..) => "FailGeneration",
            _ => "non-failure",
        }
    }

    /// Everything the peer receives after the drained space, up to EOF.
    fn read_to_eof(peer: &mut UnixStream) -> Vec<u8> {
        let mut bytes = Vec::new();
        peer.read_to_end(&mut bytes)
            .expect("peer reads to EOF before the kill switch");
        bytes
    }

    struct PartialRecord {
        free: usize,
        prefix: usize,
        outcome: String,
        retired: bool,
        next_call: String,
        owner: Vec<String>,
        elapsed_ms: u128,
    }

    /// One arm: begin the CALL with `free` bytes of space and record what the
    /// router returned and exactly which CALL bytes ever reached the peer.
    fn partial_arm(free: usize) -> PartialRecord {
        let payload = max_payload();
        let mut fixture = filled_router(free);
        let started = Instant::now();
        let result = fixture.handle.begin_echo_call(&payload);
        let elapsed_ms = started.elapsed().as_millis();
        let outcome = match &result {
            Ok(call) => format!("Ok(corr={})", call.correlation.0),
            Err(error) => format!("Err({error})"),
        };
        assert!(result.is_err(), "free={free}: {outcome}");
        let retired = fixture
            .handle
            .current
            .lock()
            .expect("generation lock")
            .is_none();
        let next_call = match fixture.handle.begin_echo_call(&payload) {
            Ok(call) => format!("Ok(corr={})", call.correlation.0),
            Err(error) => format!("Err({error})"),
        };
        let received = read_to_eof(&mut fixture.peer);
        let filler_left = fixture.filler - free;
        assert!(received.len() >= filler_left, "free={free}: lost filler");
        assert!(received[..filler_left].iter().all(|byte| *byte == FILLER));
        let call_bytes = &received[filler_left..];
        // The CALL bytes are an exact strict prefix of one Echo CALL frame.
        assert!(call_bytes.len() < HEADER_LEN + payload.len(), "free={free}");
        if call_bytes.len() >= HEADER_LEN {
            let header = keld_ipc::FrameHeader::decode(
                call_bytes[..HEADER_LEN].try_into().expect("header bytes"),
            )
            .expect("valid CALL header");
            assert_eq!(header.kind, FrameKind::Call);
            assert_eq!(header.channel, ECHO_CHANNEL);
            assert_eq!(header.len as usize, payload.len());
            assert_eq!(
                &call_bytes[HEADER_LEN..],
                &payload[..call_bytes.len() - HEADER_LEN]
            );
        }
        let FilledRouter {
            router,
            handle,
            owner,
            ..
        } = fixture;
        let shutdown = router.shutdown();
        // The owner thread ends when the last command sender (router, handle) is gone.
        drop(handle);
        let owner = owner.join().expect("owner thread");
        let record = PartialRecord {
            free,
            prefix: call_bytes.len(),
            outcome,
            retired,
            next_call,
            owner,
            elapsed_ms,
        };
        println!(
            "GH1081_R3_CORE partial free={} frame_len={} call_bytes_received={} outcome={:?} generation_retired={} next_call={:?} owner={:?} router_shutdown_ok={} elapsed_ms={}",
            record.free,
            HEADER_LEN + payload.len(),
            record.prefix,
            record.outcome,
            record.retired,
            record.next_call,
            record.owner,
            shutdown.is_ok(),
            record.elapsed_ms,
        );
        record
    }

    fn assert_not_sent(record: &PartialRecord) {
        assert!(
            record.outcome.contains("renderer Echo call write"),
            "{}",
            record.outcome
        );
        assert!(record.retired, "failed write left the generation current");
        assert!(
            record.next_call.contains("no admitted primary generation"),
            "{}",
            record.next_call
        );
        assert!(
            !record.owner.is_empty(),
            "the router never reported the failed link"
        );
        assert!(
            record
                .owner
                .iter()
                .all(|command| command == "FailGeneration(1)"),
            "{:?}",
            record.owner
        );
    }

    /// k = 0: no CALL byte enters the socket before the write deadline.
    #[test]
    fn gh1081_r3_partial_write_zero_bytes() {
        let record = partial_arm(0);
        assert_eq!(record.prefix, 0);
        assert_not_sent(&record);
    }

    /// k = `HEADER_LEN`: the whole header, no payload byte.
    #[test]
    fn gh1081_r3_partial_write_header_only() {
        let record = partial_arm(HEADER_LEN);
        assert_eq!(record.prefix, HEADER_LEN);
        assert_not_sent(&record);
    }

    /// k inside the payload.
    #[test]
    fn gh1081_r3_partial_write_mid_payload() {
        let record = partial_arm(3_000);
        assert!(
            record.prefix > HEADER_LEN && record.prefix < HEADER_LEN + 4_096,
            "{}",
            record.prefix
        );
        assert_not_sent(&record);
    }

    /// k = frame length - 1: one byte short.
    #[test]
    fn gh1081_r3_partial_write_one_byte_short() {
        let record = partial_arm(HEADER_LEN + 4_096 - 1);
        assert_eq!(record.prefix, HEADER_LEN + 4_096 - 1);
        assert_not_sent(&record);
    }

    /// Negative control: the same filled socket with room for the whole
    /// frame. The write completes, the CALL is pending, and the peer receives
    /// the complete frame, so the partial arms' oracle fails here.
    #[test]
    fn gh1081_r3_partial_write_nc_full_space_dispatches() {
        let payload = max_payload();
        let free = HEADER_LEN + payload.len();
        let mut fixture = filled_router(free);
        let call = fixture
            .handle
            .begin_echo_call(&payload)
            .expect("NC: the write completes");
        let mut filler = vec![0; fixture.filler - free];
        fixture
            .peer
            .read_exact(&mut filler)
            .expect("remaining filler");
        let (header, received) = read_frame(&mut fixture.peer).expect("complete CALL frame");
        assert_eq!(header.kind, FrameKind::Call);
        assert_eq!(header.corr, call.correlation);
        assert_eq!(received, payload);
        let retired = fixture
            .handle
            .current
            .lock()
            .expect("generation lock")
            .is_none();
        assert!(!retired, "NC: a completed write retires nothing");
        write_frame(
            &mut fixture.peer,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            call.correlation,
            &[7],
        )
        .expect("NC reply");
        let reply = call
            .reply
            .recv_timeout(KILL_SWITCH)
            .expect("NC terminal")
            .expect("NC success");
        println!(
            "GH1081_R3_CORE partial-nc free={free} frame_len={} call_bytes_received={} outcome=Ok(corr={}) generation_retired={retired} reply={reply:?}",
            HEADER_LEN + payload.len(),
            HEADER_LEN + received.len(),
            call.correlation.0
        );
        assert!(matches!(reply, PrimaryEchoReply::Reply(bytes) if bytes == [7]));
        let FilledRouter {
            router,
            handle,
            peer,
            owner,
            ..
        } = fixture;
        router.shutdown().expect("NC router shutdown");
        drop((handle, peer));
        assert_eq!(owner.join().expect("owner thread"), Vec::<String>::new());
    }

    /// After the CALL reached a live peer, the host retires the generation
    /// (the path a Bun crash, FS failure or revoke takes). The waiter settles
    /// exactly once, the late REPLY is never delivered, and the host never
    /// writes the CALL again.
    #[test]
    fn gh1081_r3_retired_after_dispatch_settles_once_and_never_replays() {
        let (router, handle, mut client, _owner_rx, _window_rx) = primary_echo_test_router();
        let call = handle.begin_echo_call(&[1, 2]).expect("dispatched Echo");
        let (outbound, _) = read_frame(&mut client).expect("the CALL reached the peer");
        assert_eq!(outbound.corr, call.correlation);
        handle.retire_generation(1).expect("retire after dispatch");
        let first = call.reply.recv_timeout(KILL_SWITCH).expect("one terminal");
        let late = write_frame(
            &mut client,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            call.correlation,
            &[9],
        );
        let second = call.reply.recv_timeout(KILL_SWITCH);
        let mut after = Vec::new();
        client
            .set_read_timeout(Some(KILL_SWITCH))
            .expect("peer kill switch");
        let tail = client.read_to_end(&mut after);
        println!(
            "GH1081_R3_CORE retired first={first:?} late_reply_write={late:?} second={second:?} host_bytes_after_retire={} tail={tail:?}",
            after.len()
        );
        let terminal = first.expect_err("retirement is not a success");
        assert!(
            terminal
                .to_string()
                .contains("retired with the primary generation"),
            "{terminal}"
        );
        assert!(
            matches!(second, Err(mpsc::RecvTimeoutError::Disconnected)),
            "{second:?}"
        );
        assert!(
            after.is_empty(),
            "the host wrote after retirement: {after:?}"
        );
        let _ = router.shutdown();
    }

    /// Negative control for the test above: no retirement, the same late
    /// REPLY bytes settle the call successfully, so the treatment's oracle
    /// ("retired" terminal, nothing delivered) fails here.
    #[test]
    fn gh1081_r3_retired_nc_live_generation_delivers_the_reply() {
        let (router, handle, mut client, _owner_rx, _window_rx) = primary_echo_test_router();
        let call = handle.begin_echo_call(&[1, 2]).expect("dispatched Echo");
        let (outbound, _) = read_frame(&mut client).expect("the CALL reached the peer");
        write_frame(
            &mut client,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            outbound.corr,
            &[9],
        )
        .expect("NC reply");
        let first = call.reply.recv_timeout(KILL_SWITCH).expect("one terminal");
        let second = call.reply.recv_timeout(KILL_SWITCH);
        println!("GH1081_R3_CORE retired-nc first={first:?} second={second:?}");
        assert!(matches!(first, Ok(PrimaryEchoReply::Reply(ref bytes)) if bytes == &[9]));
        assert!(
            matches!(second, Err(mpsc::RecvTimeoutError::Disconnected)),
            "{second:?}"
        );
        router.shutdown().expect("NC router shutdown");
    }
}
