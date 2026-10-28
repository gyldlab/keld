//! GH-1095 (`docs/specs/gh1095-request-error-codes.md` §4.2): what the real
//! primary router does with one renderer Echo CALL at each stage S5–S10b.
//!
//! Oracles are host-observable facts: the bytes the Bun end of the socket
//! receives up to EOF, the refusal `begin_echo_call` returns, whether the
//! generation and the pending slot survive, and how the reply channel
//! settles. Socket-pressure tests fill the send buffer and use the real
//! `APP_LINK_IO_DEADLINE`. Exact-byte tests use a `Write` wrapper that forwards
//! the accepted prefix to the real socket, then injects a failure at the next
//! write. Both exercise the same router cleanup. Waits are blocking reads
//! and receives on those observables; every timeout is a kill switch only.

use super::super::{EchoCallRefusal, RendererEchoTerminal, dispatched_echo_terminal};
use super::{
    AppWindowCommand, CallError, CorrelationId, Duration, ECHO_CHANNEL, FrameKind,
    GuardianOwnerCommand, PlatformPrimaryOwnerHandle, PrimaryEchoCall, PrimaryEchoReply,
    PrimaryRouter, PrimaryRouterHandle, Receiver, SessionShutdownState, app_detail, encode, mpsc,
    primary_echo_test_router, write_frame,
};
use keld_ipc::echo::EchoRequest;
use keld_ipc::{FrameHeader, HEADER_LEN};
use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;

const KILL_SWITCH: Duration = Duration::from_secs(60);
const FILLER: u8 = 0xf7;

/// A valid Echo request whose bytes name the call, so a frame on the wire
/// can be attributed to exactly one logical call.
fn request(label: &str) -> Vec<u8> {
    encode(&EchoRequest {
        message: label.to_owned(),
        count: 7,
    })
    .expect("encode Echo request")
}

/// The largest renderer payload (4,096 bytes) as a valid Echo request.
fn max_request() -> Vec<u8> {
    let payload = encode(&EchoRequest {
        message: "p".repeat(4_096 - 3),
        count: 9,
    })
    .expect("encode maximum Echo request");
    assert_eq!(payload.len(), 4_096);
    payload
}

/// Everything the Bun end receives until the host closes its writer. The
/// caller set the peer's read deadline (the kill switch) at creation: macOS
/// refuses socket options once the host has shut the socket down.
fn bytes_until_eof(peer: &mut UnixStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    peer.read_to_end(&mut bytes)
        .expect("host closes its writer before the kill switch");
    bytes
}

/// Complete Echo CALL payloads in `bytes`, in wire order. A trailing partial
/// frame fails the caller: these streams must hold only whole frames.
fn echo_call_payloads(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut calls = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        assert!(rest.len() >= HEADER_LEN, "partial frame header on the wire");
        let header = FrameHeader::decode(rest[..HEADER_LEN].try_into().expect("header bytes"))
            .expect("valid frame header");
        let end = HEADER_LEN + header.len as usize;
        assert!(rest.len() >= end, "partial frame payload on the wire");
        if header.kind == FrameKind::Call && header.channel == ECHO_CHANNEL {
            calls.push(rest[HEADER_LEN..end].to_vec());
        }
        rest = &rest[end..];
    }
    calls
}

/// The renderer-visible code of one terminal; a reply has none.
fn renderer_code(terminal: &RendererEchoTerminal) -> Option<&str> {
    match terminal {
        RendererEchoTerminal::Reply(_) => None,
        RendererEchoTerminal::Error { code, .. } => Some(code),
    }
}

fn assert_slot_empty(handle: &PrimaryRouterHandle) {
    assert!(
        handle.pending_echo.lock().expect("pending lock").is_none(),
        "a refused call left the session-wide renderer slot armed"
    );
}

/// Reads the next frame the Bun end receives and answers it with `reply`.
fn answer_next_call(peer: &mut UnixStream, call: &PrimaryEchoCall, expected: &[u8]) {
    let (header, payload) = keld_ipc::link::read_frame(peer).expect("successor CALL frame");
    assert_eq!(header.kind, FrameKind::Call);
    assert_eq!(header.channel, ECHO_CHANNEL);
    assert_eq!(header.corr, call.correlation);
    assert_eq!(payload, expected);
    write_frame(
        peer,
        FrameKind::Reply,
        0,
        ECHO_CHANNEL,
        call.correlation,
        &[0x5a],
    )
    .expect("successor Reply");
    match call
        .reply
        .recv_timeout(KILL_SWITCH)
        .expect("successor reply settles")
        .expect("successor reply is a Reply")
    {
        PrimaryEchoReply::Reply(bytes) => assert_eq!(bytes, [0x5a]),
        PrimaryEchoReply::Err(error) => panic!("unexpected successor Err: {error}"),
    }
}

/// S8: with no admitted generation the host writes nothing to any stream,
/// and the refused call never reaches the successor, which serves three
/// later calls (the "census 0 forever" re-check). The successor's three
/// CALL frames are this oracle's own control: the same observer counts a
/// call the host does write.
#[test]
fn s8_call_without_an_admitted_generation_reaches_no_stream() {
    let (router, handle, mut g1_peer, _owner, _window) = primary_echo_test_router();
    handle.retire_generation(1).expect("revoke G1");
    assert!(handle.current.lock().expect("generation lock").is_none());

    let refused = request("s8-refused");
    let error = handle
        .begin_echo_call(&refused)
        .expect_err("no admitted generation");
    assert!(matches!(error, EchoCallRefusal::NoGeneration), "{error:?}");
    assert_eq!(
        renderer_code(&error.renderer_terminal()),
        Some("KELD-WV-012"),
        "no write was attempted, so the call is definitely not dispatched"
    );
    assert_slot_empty(&handle);
    assert_eq!(
        bytes_until_eof(&mut g1_peer),
        Vec::<u8>::new(),
        "the revoked generation received bytes after its revocation"
    );

    let (g2_server, mut g2_peer) = UnixStream::pair().expect("successor pair");
    g2_peer
        .set_read_timeout(Some(KILL_SWITCH))
        .expect("successor kill switch");
    handle
        .install_generation(2, g2_server)
        .expect("admit the successor");
    let mut served = Vec::new();
    for label in ["g2-a", "g2-b", "g2-c"] {
        let payload = request(label);
        let call = handle.begin_echo_call(&payload).expect("successor call");
        answer_next_call(&mut g2_peer, &call, &payload);
        served.push(payload);
    }
    router.shutdown().expect("S8 router shutdown");
    let tail = echo_call_payloads(&bytes_until_eof(&mut g2_peer));
    assert!(tail.is_empty(), "the successor received a replayed CALL");
    assert!(
        !served.contains(&refused),
        "the refused call's bytes reached the successor"
    );
}

/// S5, S6, S7: each refusal returns before the host attempts a write. The
/// first call of the S7 pair is on the wire (the control that this peer
/// sees a written CALL); the refused second call never is.
#[test]
fn s5_s6_s7_refusals_return_before_any_write() {
    // S7: the session-wide slot is busy.
    let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
    let first = request("s7-first");
    let call = handle.begin_echo_call(&first).expect("first call");
    let busy = handle
        .begin_echo_call(&request("s7-refused"))
        .expect_err("slot busy");
    assert!(matches!(busy, EchoCallRefusal::Busy), "{busy:?}");
    assert_eq!(
        busy.renderer_terminal(),
        RendererEchoTerminal::error("KELD-WV-011", "another application call is already pending")
    );
    answer_next_call(&mut peer, &call, &first);
    router.shutdown().expect("S7 router shutdown");
    assert!(
        echo_call_payloads(&bytes_until_eof(&mut peer)).is_empty(),
        "the busy refusal reached the stream"
    );

    // S5: the session is quiescing.
    let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
    assert!(handle.shutdown.claim(super::SESSION_LIFECYCLE_QUIT));
    let quitting = handle
        .begin_echo_call(&request("s5-refused"))
        .expect_err("quiescing");
    assert!(
        matches!(quitting, EchoCallRefusal::Quitting),
        "{quitting:?}"
    );
    assert_eq!(
        quitting.renderer_terminal(),
        RendererEchoTerminal::error("KELD-WV-011", "application is quitting")
    );
    assert_slot_empty(&handle);
    drop(router.shutdown());
    assert!(
        echo_call_payloads(&bytes_until_eof(&mut peer)).is_empty(),
        "the quiescing refusal reached the stream"
    );

    // S6: either host call-state lock is poisoned.
    for poisoned in ["generation", "pending"] {
        let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
        let victim = handle.clone();
        let _ = std::thread::spawn(move || {
            let _generation;
            let _pending;
            if poisoned == "generation" {
                _generation = victim.current.lock().expect("generation lock");
            } else {
                _pending = victim.pending_echo.lock().expect("pending lock");
            }
            panic!("poison the {poisoned} lock");
        })
        .join();
        let unavailable = handle
            .begin_echo_call(&request("s6-refused"))
            .expect_err("poisoned call state");
        assert!(
            matches!(unavailable, EchoCallRefusal::StateUnavailable),
            "{unavailable:?}"
        );
        assert_eq!(
            unavailable.renderer_terminal(),
            RendererEchoTerminal::error("KELD-WV-011", "host call state is unavailable")
        );
        drop(router.shutdown());
        assert!(
            echo_call_payloads(&bytes_until_eof(&mut peer)).is_empty(),
            "the {poisoned}-lock refusal reached the stream"
        );
    }
}

struct FilledRouter {
    router: PrimaryRouter,
    handle: PrimaryRouterHandle,
    peer: UnixStream,
    filler: usize,
    owner: std::thread::JoinHandle<Vec<u32>>,
    _window: Receiver<AppWindowCommand>,
}

/// A router whose host-to-Bun socket buffer the test fills with filler
/// through its own descriptor of the same socket, then drains `free` bytes
/// so that exactly that much space exists when the CALL is written.
///
/// The fill uses 2,048-byte writes, then halving sizes down to one byte, so
/// the byte limit binds rather than the kernel's per-buffer bookkeeping and
/// the remaining space is zero before the drain.
fn filled_router(free: usize) -> FilledRouter {
    let (server, mut peer) = UnixStream::pair().expect("renderer Echo pair");
    peer.set_read_timeout(Some(KILL_SWITCH))
        .expect("peer kill switch");
    let mut fill = server.try_clone().expect("test descriptor of the socket");
    fill.set_nonblocking(true).expect("fill nonblocking");
    let mut filler = 0;
    let mut size = 2_048;
    while size > 0 {
        match fill.write(&vec![FILLER; size]) {
            Ok(written) => filler += written,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => size /= 2,
            Err(error) => panic!("fill the socket: {error}"),
        }
    }
    // O_NONBLOCK belongs to the open socket, shared by every descriptor.
    fill.set_nonblocking(false)
        .expect("restore the blocking socket");
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
    let owner = std::thread::spawn(move || {
        let mut failed = Vec::new();
        while let Ok(command) = command_rx.recv_timeout(KILL_SWITCH) {
            if let GuardianOwnerCommand::FailGeneration(attempt, reply) = command {
                failed.push(attempt);
                let _ = reply.send(Ok(()));
            }
        }
        failed
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

/// What one S9 arm observed.
struct PartialWrite {
    /// Exact CALL bytes the Bun end received before EOF.
    call_bytes: usize,
    /// Whether the generation was retired.
    retired: bool,
    /// Owner `FailGeneration` notifications.
    failed: Vec<u32>,
    /// What a second call returned after the failure.
    next_call: Result<(), EchoCallRefusal>,
}

/// One S9 arm: write the maximum CALL with `free` bytes of space.
fn partial_write(free: usize) -> PartialWrite {
    let payload = max_request();
    let mut fixture = filled_router(free);
    let error = fixture
        .handle
        .begin_echo_call(&payload)
        .expect_err("the stopped write fails");
    assert!(
        matches!(&error, EchoCallRefusal::WriteFailed(source)
            if source.to_string().contains("renderer Echo call write")),
        "free={free}: {error:?}"
    );
    assert_eq!(
        renderer_code(&error.renderer_terminal()),
        Some("KELD-WV-013"),
        "free={free}: a failed write is outcome unknown, never not-dispatched"
    );
    assert_slot_empty(&fixture.handle);
    let retired = fixture
        .handle
        .current
        .lock()
        .expect("generation lock")
        .is_none();
    let next_call = fixture.handle.begin_echo_call(&payload).map(|_| ());
    let received = bytes_until_eof(&mut fixture.peer);
    let filler_left = fixture.filler - free;
    assert!(received.len() >= filler_left, "free={free}: lost filler");
    assert!(received[..filler_left].iter().all(|byte| *byte == FILLER));
    let call = &received[filler_left..];
    // The CALL bytes are a strict prefix of one Echo CALL frame: nothing was
    // written after the failure, and no second frame followed.
    assert!(call.len() < HEADER_LEN + payload.len(), "free={free}");
    if call.len() >= HEADER_LEN {
        let header = FrameHeader::decode(call[..HEADER_LEN].try_into().expect("header bytes"))
            .expect("valid CALL header");
        assert_eq!(header.kind, FrameKind::Call);
        assert_eq!(header.channel, ECHO_CHANNEL);
        assert_eq!(header.len as usize, payload.len());
        assert_eq!(&call[HEADER_LEN..], &payload[..call.len() - HEADER_LEN]);
    }
    let FilledRouter {
        router,
        handle,
        owner,
        ..
    } = fixture;
    drop(router.shutdown());
    drop(handle);
    PartialWrite {
        call_bytes: call.len(),
        retired,
        failed: owner.join().expect("owner thread"),
        next_call,
    }
}

/// One S9 arm: `free` bytes of space, and the exact count of CALL bytes the
/// kernel then accepts. The write fails, the slot clears, the generation
/// retires once, and no byte follows the failure.
fn assert_failed_write_retires(free: usize, accepted: usize) {
    let observed = partial_write(free);
    println!(
        "GH1095_S9 free={free} call_bytes={} retired={} failed={:?} next_call={:?}",
        observed.call_bytes, observed.retired, observed.failed, observed.next_call
    );
    assert_eq!(
        observed.call_bytes, accepted,
        "free={free}: CALL bytes accepted"
    );
    assert!(
        observed.retired,
        "free={free}: generation survived the failure"
    );
    assert_eq!(observed.failed, [1], "free={free}: one owner failure");
    assert!(
        matches!(observed.next_call, Err(EchoCallRefusal::NoGeneration)),
        "free={free}: {:?}",
        observed.next_call
    );
}

/// S9, k = 0: the kernel accepts no CALL byte.
#[test]
fn s9_write_failing_after_0_bytes_retires_the_generation() {
    assert_failed_write_retires(0, 0);
}

/// S9 with 1 and `HEADER_LEN − 1` bytes free. Measured on macOS 26.5.1: the
/// kernel accepts none of the 16-byte header, because a stream write below
/// the socket's default send low-water mark waits for more space (XNU
/// `sosend`). Lowering that mark needs `setsockopt(SO_SNDLOWAT)`, which no
/// existing dependency exposes, so k = 1 and k = `HEADER_LEN − 1` are not
/// reachable here; these arms prove only that S9 still holds when the space
/// is smaller than the header.
#[test]
fn s9_write_with_less_space_than_a_header_retires_the_generation() {
    assert_failed_write_retires(1, 0);
    assert_failed_write_retires(HEADER_LEN - 1, 0);
}

/// S9, k = `HEADER_LEN`: the whole header, no payload byte.
#[test]
fn s9_write_failing_after_the_header_retires_the_generation() {
    assert_failed_write_retires(HEADER_LEN, HEADER_LEN);
}

/// S9, k = `HEADER_LEN + len − 1`: all but the last payload byte.
#[test]
fn s9_write_failing_one_byte_short_retires_the_generation() {
    let frame = HEADER_LEN + max_request().len();
    assert_failed_write_retires(frame - 1, frame - 1);
}

/// S9's control: with space for the whole frame the write reaches the
/// dispatch point, so the same fixture produces a call, not a failure.
#[test]
fn s9_control_full_space_reaches_the_dispatch_point() {
    let payload = max_request();
    let frame = HEADER_LEN + payload.len();
    let mut fixture = filled_router(frame);
    let call = fixture
        .handle
        .begin_echo_call(&payload)
        .expect("the write completes");
    assert!(
        fixture
            .handle
            .current
            .lock()
            .expect("generation lock")
            .is_some(),
        "a completed write retired the generation"
    );
    let filler_left = fixture.filler - frame;
    let mut skipped = vec![0; filler_left];
    fixture
        .peer
        .read_exact(&mut skipped)
        .expect("remaining filler");
    let (header, bytes) = keld_ipc::link::read_frame(&mut fixture.peer).expect("whole CALL");
    assert_eq!(header.corr, call.correlation);
    assert_eq!(bytes, payload);
    let FilledRouter {
        router,
        handle,
        owner,
        ..
    } = fixture;
    drop(router.shutdown());
    drop(handle);
    assert_eq!(owner.join().expect("owner thread"), [] as [u32; 0]);
}

/// Exercise the router's actual `begin_echo_call` cleanup with deterministic
/// `Write` failures while forwarding every accepted byte to its real socket.
/// Unlike the socket-space test, this also reaches k=1 and k=15 on macOS.
struct FaultingSocketWriter<'a> {
    socket: &'a mut UnixStream,
    accept: usize,
    written: usize,
    fail_flush: bool,
    write_errors: usize,
    flushes: usize,
}

impl std::io::Write for FaultingSocketWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.written == self.accept {
            self.write_errors += 1;
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "injected real-socket write failure",
            ));
        }
        let accepted = self
            .socket
            .write(&buf[..buf.len().min(self.accept - self.written)])?;
        self.written += accepted;
        Ok(accepted)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flushes += 1;
        if self.fail_flush {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "injected real-socket flush failure",
            ))
        } else {
            self.socket.flush()
        }
    }
}

fn assert_s9_generation_unusable(
    handle: &PrimaryRouterHandle,
    stop: &std::sync::atomic::AtomicBool,
    accepted: usize,
) {
    assert_slot_empty(handle);
    assert_eq!(
        handle
            .pending_echo_attempt
            .load(std::sync::atomic::Ordering::Acquire),
        0,
        "k={accepted}: pending attempt atomic survived"
    );
    assert_eq!(
        handle
            .pending_echo_corr
            .load(std::sync::atomic::Ordering::Acquire),
        0,
        "k={accepted}: pending correlation atomic survived"
    );
    assert!(
        stop.load(std::sync::atomic::Ordering::Acquire),
        "k={accepted}: G1 reader not stopped"
    );
    assert!(
        handle.current.lock().expect("generation lock").is_none(),
        "k={accepted}: poisoned G1 is still admitted"
    );
    let refused = handle
        .begin_echo_call(&request("never-retry-g1"))
        .expect_err("poisoned G1 cannot accept another call");
    assert!(
        matches!(refused, EchoCallRefusal::NoGeneration),
        "{refused:?}"
    );
}

fn assert_s9_g1_socket_prefix(
    peer: &mut UnixStream,
    payload: &[u8],
    correlation: CorrelationId,
    accepted: usize,
) {
    let mut expected = FrameHeader {
        kind: FrameKind::Call,
        flags: 0,
        channel: ECHO_CHANNEL,
        corr: correlation,
        len: u32::try_from(payload.len()).expect("small payload"),
    }
    .encode()
    .to_vec();
    expected.extend_from_slice(payload);
    assert_eq!(expected.len(), 4_112);
    assert_eq!(
        bytes_until_eof(peer),
        expected[..accepted],
        "k={accepted}: G1 must observe only the exact written prefix and EOF"
    );
}

/// S9: exact `Write` boundary on the real router, then an independent G2
/// exchange. The bytes, pending state, owner commands and future call are
/// separate observations; a reported WV-013 alone does not prove cleanup.
fn assert_s9_exact_injected_failure(accepted: usize, fail_flush: bool) {
    let (router, handle, mut g1_peer, owner_commands, _window) = primary_echo_test_router();
    let owner = std::thread::spawn(move || {
        let mut failures = Vec::new();
        while let Ok(command) = owner_commands.recv_timeout(KILL_SWITCH) {
            if let GuardianOwnerCommand::FailGeneration(attempt, reply) = command {
                failures.push(attempt);
                reply.send(Ok(())).expect("owner accepts G1 failure");
            }
        }
        failures
    });
    let stop = handle
        .current
        .lock()
        .expect("generation lock")
        .as_ref()
        .expect("G1 exists")
        .reader_stop
        .clone();
    let payload = max_request();
    let first_corr = CorrelationId(
        handle
            .next_host_corr
            .load(std::sync::atomic::Ordering::Acquire),
    );
    let mut observed = (0, 0, 0);
    let error = handle
        .begin_echo_call_with(&payload, |stream, correlation, bytes| {
            let mut writer = FaultingSocketWriter {
                socket: stream,
                accept: accepted,
                written: 0,
                fail_flush,
                write_errors: 0,
                flushes: 0,
            };
            let result = write_frame(
                &mut writer,
                FrameKind::Call,
                0,
                ECHO_CHANNEL,
                correlation,
                bytes,
            );
            observed = (writer.written, writer.write_errors, writer.flushes);
            result
        })
        .expect_err("injected I/O fault reaches the real router");
    assert!(
        matches!(&error, EchoCallRefusal::WriteFailed(source)
            if source.to_string().contains("renderer Echo call write")),
        "accepted={accepted}, flush={fail_flush}: {error:?}"
    );
    assert_eq!(
        renderer_code(&error.renderer_terminal()),
        Some("KELD-WV-013"),
        "k={accepted}: any attempted failed CALL is outcome unknown"
    );
    assert_eq!(
        observed,
        (accepted, usize::from(!fail_flush), usize::from(fail_flush)),
        "k={accepted}: injected socket write and flush census"
    );
    assert_s9_generation_unusable(&handle, &stop, accepted);
    assert_s9_g1_socket_prefix(&mut g1_peer, &payload, first_corr, accepted);

    let (g2_server, mut g2_peer) = UnixStream::pair().expect("successor socketpair");
    g2_peer
        .set_read_timeout(Some(KILL_SWITCH))
        .expect("G2 kill switch");
    handle
        .install_generation(2, g2_server)
        .expect("admit healthy successor");
    let healthy = request("fresh-g2-only");
    let call = handle.begin_echo_call(&healthy).expect("healthy G2 call");
    answer_next_call(&mut g2_peer, &call, &healthy);
    router.shutdown().expect("S9 injected router shutdown");
    assert!(
        bytes_until_eof(&mut g2_peer).is_empty(),
        "k={accepted}: replayed G1 CALL to G2"
    );
    drop(handle);
    assert_eq!(
        owner.join().expect("owner thread"),
        [1],
        "k={accepted}: retire G1 once"
    );
}

#[test]
fn s9_real_router_injects_exact_byte_faults_and_recovers_on_successor() {
    for accepted in [0, 1, HEADER_LEN - 1, HEADER_LEN, 4_111] {
        assert_s9_exact_injected_failure(accepted, false);
    }
    assert_s9_exact_injected_failure(4_112, true);
}

#[test]
fn s9_exact_injected_full_write_control_dispatches_normally() {
    let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
    let payload = max_request();
    let mut observed = (0, 0, 0);
    let call = handle
        .begin_echo_call_with(&payload, |stream, correlation, bytes| {
            let mut writer = FaultingSocketWriter {
                socket: stream,
                accept: 4_112,
                written: 0,
                fail_flush: false,
                write_errors: 0,
                flushes: 0,
            };
            let result = write_frame(
                &mut writer,
                FrameKind::Call,
                0,
                ECHO_CHANNEL,
                correlation,
                bytes,
            );
            observed = (writer.written, writer.write_errors, writer.flushes);
            result
        })
        .expect("full CALL and flush complete");
    assert_eq!(observed, (4_112, 0, 1));
    assert!(handle.current.lock().expect("generation lock").is_some());
    answer_next_call(&mut peer, &call, &payload);
    router.shutdown().expect("full-write router shutdown");
    assert_eq!(bytes_until_eof(&mut peer), [] as [u8; 0]);
}

/// S10: after the CALL was written, every retirement settles the pending
/// call exactly once with an error, never with a reply. Each arm reads the
/// CALL frame first, so the dispatch point is proven reached.
#[test]
fn s10_retirement_after_the_write_settles_once_with_an_error() {
    // Link EOF: Bun's end closes.
    let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
    let call = written_call(&handle, &mut peer, "s10-eof");
    drop(peer);
    assert_settled_once_with_error(&call, "link EOF");
    drop(router.shutdown());

    // Revocation: the owner retires the generation while the call is pending.
    let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
    let call = written_call(&handle, &mut peer, "s10-revoke");
    handle.retire_generation(1).expect("revoke G1");
    assert_settled_once_with_error(&call, "revocation");
    router.shutdown().expect("revocation router shutdown");

    // Control: without a retirement the same call settles with Bun's reply,
    // so the oracle tells a reply from a retirement.
    let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
    let payload = request("s10-control");
    let call = handle.begin_echo_call(&payload).expect("dispatched call");
    answer_next_call(&mut peer, &call, &payload);
    router.shutdown().expect("control router shutdown");

    // Reader error: a reply with the wrong correlation is a protocol failure.
    let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
    let call = written_call(&handle, &mut peer, "s10-reader");
    let wrong = CorrelationId(call.correlation.0.wrapping_add(1).max(1));
    write_frame(&mut peer, FrameKind::Reply, 0, ECHO_CHANNEL, wrong, &[1])
        .expect("wrong-correlation reply");
    assert_settled_once_with_error(&call, "reader error");
    drop(router.shutdown());
}

fn written_call(
    handle: &PrimaryRouterHandle,
    peer: &mut UnixStream,
    label: &str,
) -> PrimaryEchoCall {
    let payload = request(label);
    let call = handle.begin_echo_call(&payload).expect("dispatched call");
    let (header, bytes) = keld_ipc::link::read_frame(peer).expect("the CALL frame");
    assert_eq!(header.kind, FrameKind::Call);
    assert_eq!(header.corr, call.correlation);
    assert_eq!(bytes, payload);
    call
}

fn assert_settled_once_with_error(call: &PrimaryEchoCall, arm: &str) {
    let terminal = call
        .reply
        .recv_timeout(KILL_SWITCH)
        .unwrap_or_else(|error| panic!("{arm}: the pending call never settled: {error}"));
    assert!(
        terminal.is_err(),
        "{arm}: a retirement became a reply: {terminal:?}"
    );
    assert_eq!(
        renderer_code(&dispatched_echo_terminal(Some(terminal))),
        Some("KELD-WV-013"),
        "{arm}: a retirement after the write is outcome unknown"
    );
    assert!(
        matches!(
            call.reply.recv_timeout(KILL_SWITCH),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ),
        "{arm}: the call settled more than once"
    );
}

/// S10b: a Bun ERR reaches the reply channel with Bun's own code, whatever
/// it claims, including a host-only renderer code. The dispatch mapping then
/// rewrites every `KELD-WV-*` claim to `KELD-WV-013` (criterion 11).
#[test]
fn s10b_bun_err_codes_reach_the_reply_channel_verbatim() {
    for code in [
        "KELD-WV-011",
        "KELD-WV-012",
        "KELD-WV-013",
        "KELD-API-001",
        "KELD-API-002",
        "KELD-IPC-023",
    ] {
        let (router, handle, mut peer, _owner, _window) = primary_echo_test_router();
        let call = written_call(&handle, &mut peer, "s10b");
        let claimed = CallError {
            code: code.to_owned(),
            message: format!("{code}: claimed by the peer"),
        };
        keld_ipc::write_call_error(&mut peer, ECHO_CHANNEL, call.correlation, &claimed)
            .expect("Bun ERR");
        match call
            .reply
            .recv_timeout(KILL_SWITCH)
            .expect("ERR settles the call")
            .expect("an ERR is an answer, not a retirement")
        {
            PrimaryEchoReply::Err(error) => {
                assert_eq!(error, claimed);
                let rendered = dispatched_echo_terminal(Some(Ok(PrimaryEchoReply::Err(error))));
                let expected = if code.starts_with("KELD-WV-") {
                    "KELD-WV-013"
                } else {
                    code
                };
                assert_eq!(
                    renderer_code(&rendered),
                    Some(expected),
                    "Bun claimed {code}"
                );
            }
            PrimaryEchoReply::Reply(bytes) => panic!("{code}: ERR became a Reply {bytes:?}"),
        }
        router.shutdown().expect("S10b router shutdown");
    }
}

/// The renderer terminal each refusal stage maps to (§4.2 S5–S9). The code
/// comes from the variant alone; the write error's text never reaches it.
#[test]
fn refusal_stages_map_to_their_renderer_terminal() {
    for (refusal, code, detail) in [
        (
            EchoCallRefusal::Quitting,
            "KELD-WV-011",
            "application is quitting",
        ),
        (
            EchoCallRefusal::StateUnavailable,
            "KELD-WV-011",
            "host call state is unavailable",
        ),
        (
            EchoCallRefusal::Busy,
            "KELD-WV-011",
            "another application call is already pending",
        ),
        (
            EchoCallRefusal::NoGeneration,
            "KELD-WV-012",
            "no application generation is admitted; the call was not sent",
        ),
        (
            EchoCallRefusal::WriteFailed(app_detail("renderer Echo call write", "KELD-WV-012")),
            "KELD-WV-013",
            "application call write failed; the application may have received it",
        ),
    ] {
        assert_eq!(
            refusal.renderer_terminal(),
            RendererEchoTerminal::error(code, detail),
            "{refusal:?}"
        );
    }
}

/// The renderer terminal for a dispatched call (§4.2 S10, S10b, S15): a
/// reply passes through, every retirement is outcome unknown, a Bun
/// `KELD-WV-*` claim is rewritten, and any other Bun code keeps its value.
#[test]
fn dispatched_calls_map_to_their_renderer_terminal() {
    assert_eq!(
        dispatched_echo_terminal(Some(Ok(PrimaryEchoReply::Reply(vec![1, 2])))),
        RendererEchoTerminal::Reply(vec![1, 2])
    );
    let ended = RendererEchoTerminal::error(
        "KELD-WV-013",
        "application link ended before the renderer reply",
    );
    assert_eq!(
        dispatched_echo_terminal(Some(Err(app_detail("renderer Echo call", "retired")))),
        ended,
        "a retirement"
    );
    assert_eq!(
        dispatched_echo_terminal(None),
        ended,
        "a dropped reply channel"
    );
    // Codes no Keld component emits are built at run time, so the error
    // registry scanner does not read these hostile inputs as emitted codes.
    let unassigned = format!("{}{}", "KELD-WV-", 999);
    let look_alike = format!("{}{}", "KELD-WVX-", "012");
    for claimed in ["KELD-WV-011", "KELD-WV-012", "KELD-WV-013", &unassigned] {
        assert_eq!(
            dispatched_echo_terminal(Some(Ok(PrimaryEchoReply::Err(CallError {
                code: claimed.to_owned(),
                message: format!("{claimed}: claimed by the peer"),
            })))),
            RendererEchoTerminal::error(
                "KELD-WV-013",
                "application answered with a host-only renderer code"
            ),
            "Bun claimed {claimed}"
        );
    }
    // The rewrite is scoped to the `KELD-WV-` prefix: other codes, including a
    // look-alike, keep Bun's code and message.
    for code in ["KELD-API-001", "KELD-API-002", "KELD-IPC-023", &look_alike] {
        let message = format!("{code}: claimed by the peer");
        assert_eq!(
            dispatched_echo_terminal(Some(Ok(PrimaryEchoReply::Err(CallError {
                code: code.to_owned(),
                message: message.clone(),
            })))),
            RendererEchoTerminal::Error {
                code: code.to_owned(),
                detail: message,
            },
            "a Bun ERR {code}"
        );
    }
}

/// Criterion 1 at the mapping: across every stage the host can reach, only
/// the no-generation refusal yields `KELD-WV-012`. Every other input,
/// including a Bun claim of that code, yields something else.
#[test]
fn only_the_no_generation_refusal_is_not_dispatched() {
    let refusals = [
        EchoCallRefusal::Quitting,
        EchoCallRefusal::StateUnavailable,
        EchoCallRefusal::Busy,
        EchoCallRefusal::NoGeneration,
        EchoCallRefusal::WriteFailed(app_detail("renderer Echo call write", "test")),
    ];
    let not_dispatched: Vec<String> = refusals
        .iter()
        .filter(|refusal| renderer_code(&refusal.renderer_terminal()) == Some("KELD-WV-012"))
        .map(|refusal| format!("{refusal:?}"))
        .collect();
    assert_eq!(not_dispatched, ["NoGeneration"]);
    let dispatched = [
        Some(Ok(PrimaryEchoReply::Reply(Vec::new()))),
        Some(Err(app_detail("renderer Echo call", "retired"))),
        None,
        Some(Ok(PrimaryEchoReply::Err(CallError {
            code: "KELD-WV-012".to_owned(),
            message: "KELD-WV-012: claimed by the peer".to_owned(),
        }))),
    ];
    for reply in dispatched {
        assert_ne!(
            renderer_code(&dispatched_echo_terminal(reply)),
            Some("KELD-WV-012")
        );
    }
}

/// Spec r2 §4.9 (one prefix rule, one owner): the host forwards a Bun ERR's
/// message as the detail unchanged, whatever its shape; only the page facade
/// applies the prefix rule. `code` comes only from the code field.
#[test]
fn forwarded_bun_details_are_unchanged() {
    for (code, message) in [
        ("KELD-API-002", "KELD-API-002: no handler"),
        ("KELD-API-001", "application failed"),
        ("KELD-API-001", "KELD-API-001 failed"),
        ("KELD-API-002", "KELD-API-001: x"),
        // A code that is only a prefix of the host-only family, with a
        // message that claims a host-only code: not rewritten, not prefixed.
        ("KELD-WV", "KELD-WV-012: not sent"),
    ] {
        assert_eq!(
            dispatched_echo_terminal(Some(Ok(PrimaryEchoReply::Err(CallError {
                code: code.to_owned(),
                message: message.to_owned(),
            })))),
            RendererEchoTerminal::error(code, message),
            "Bun ERR {code} {message:?}"
        );
    }
}
