//! End-to-end echo over a real app-link transport (KEL-30).

#![allow(clippy::expect_used, clippy::needless_pass_by_value)] // extra test crate: expect is the assertion oracle; listen_and_serve owns PathBuf for the worker

use std::io::Write;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use keld_ipc::codec::encode;
use keld_ipc::frame::{ChannelId, CorrelationId, FrameHeader, FrameKind};
use keld_ipc::link::{handshake_client, handshake_server, read_frame, write_frame};
use keld_ipc::{
    ECHO_CHANNEL, EchoRequest, EchoResponse, HEADER_LEN, IpcError, MAGIC, PROTOCOL_VERSION,
    SessionToken, echo_call, echo_invoke, serve_echo_session,
};

#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;

#[cfg(windows)]
type Stream = std::net::TcpStream;

#[cfg(unix)]
fn connected_pair() -> (Stream, Stream) {
    std::os::unix::net::UnixStream::pair().expect("unix pair")
}

#[cfg(windows)]
fn connected_pair() -> (Stream, Stream) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let accept = thread::spawn(move || listener.accept().expect("accept").0);
    let client = std::net::TcpStream::connect(addr).expect("connect");
    let server = accept.join().expect("accept thread");
    (client, server)
}

const TEST_TOKEN_BYTES: [u8; 32] = [0xA5; 32];

fn test_token() -> SessionToken {
    SessionToken::from_bytes(TEST_TOKEN_BYTES)
}

fn spawn_echo_server(mut server: Stream) -> thread::JoinHandle<Result<(), IpcError>> {
    thread::spawn(move || serve_echo_session(&mut server, &test_token()))
}

#[cfg(unix)]
fn listen_and_serve(
    endpoint: std::path::PathBuf,
    ready: mpsc::Sender<()>,
) -> thread::JoinHandle<Result<(), IpcError>> {
    thread::spawn(move || {
        let _ = std::fs::remove_file(&endpoint);
        let listener = std::os::unix::net::UnixListener::bind(&endpoint)?;
        ready.send(()).expect("ready");
        let (mut stream, _) = listener.accept()?;
        serve_echo_session(&mut stream, &test_token())
    })
}

#[test]
fn echo_over_app_link_copies_request_fields() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    let req = EchoRequest {
        message: "hello kipc".to_owned(),
        count: 42,
    };
    let response = echo_call(&mut client, &req, &test_token()).expect("echo");
    drop(client);
    handle.join().expect("server thread").expect("serve");
    assert_eq!(response.message, "hello kipc");
    assert_eq!(response.count, 42);
    assert_ne!(
        response.message, "keld",
        "must not be a hardcoded demo string"
    );
}

#[test]
fn echo_empty_message_over_socket() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    let response = echo_call(
        &mut client,
        &EchoRequest {
            message: String::new(),
            count: 0,
        },
        &test_token(),
    )
    .expect("echo empty");
    drop(client);
    handle.join().expect("server thread").expect("serve");
    assert_eq!(response.message, "");
    assert_eq!(response.count, 0);
}

#[test]
fn echo_reply_is_reply_kind_not_call_echo() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    handshake_client(&mut client, &test_token()).expect("client hello");
    let req = EchoRequest {
        message: "corr-check".to_owned(),
        count: 7,
    };
    let payload = encode(&req).expect("encode");
    let corr = CorrelationId(99);
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        ECHO_CHANNEL,
        corr,
        &payload,
    )
    .expect("call");
    let (header, reply_bytes) = read_frame(&mut client).expect("reply");
    drop(client);
    handle.join().expect("server thread").expect("serve");
    assert_eq!(header.kind, FrameKind::Reply);
    assert_eq!(header.channel, ECHO_CHANNEL);
    assert_eq!(header.corr, corr);
    let res: EchoResponse = keld_ipc::codec::decode(&reply_bytes).expect("decode reply");
    assert_eq!(res.message, "corr-check");
    assert_eq!(res.count, 7);
}

#[test]
fn handshake_protocol_version_mismatch_is_ipc_002() {
    let (mut client, mut server) = connected_pair();
    let mut hello = FrameHeader {
        kind: FrameKind::Hello,
        flags: 0,
        channel: ChannelId(0),
        corr: CorrelationId(0),
        len: 0,
    }
    .encode();
    hello[2] = 99;
    server.write_all(&hello).expect("write bad version hello");
    server.flush().expect("flush");
    let err = handshake_client(&mut client, &test_token()).expect_err("v99 must fail");
    assert!(
        matches!(err, IpcError::Header(keld_ipc::HeaderError::BadVersion(99))),
        "got {err}"
    );
    let msg = err.to_string();
    assert!(msg.contains("KELD-IPC-002"), "{msg}");
    assert!(msg.contains("99"), "{msg}");
    assert_eq!(hello[0..2], MAGIC.to_le_bytes());
    assert_ne!(hello[2], PROTOCOL_VERSION);
}

#[test]
fn call_on_unknown_channel_is_ipc_005() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    handshake_client(&mut client, &test_token()).expect("hello");
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        ChannelId(99),
        CorrelationId(1),
        &[],
    )
    .expect("wrong channel");
    drop(client);
    let err = handle
        .join()
        .expect("server thread")
        .expect_err("unknown channel must fail the session");
    assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
}

#[test]
fn invalid_postcard_call_is_ipc_003() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    handshake_client(&mut client, &test_token()).expect("hello");
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        ECHO_CHANNEL,
        CorrelationId(1),
        &[0xff, 0xff],
    )
    .expect("garbage call");
    drop(client);
    let err = handle
        .join()
        .expect("server thread")
        .expect_err("garbage payload must fail handle_echo");
    assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
}

#[test]
fn echo_call_rejects_reply_on_wrong_channel() {
    let (mut client, mut server) = connected_pair();
    let handle = thread::spawn(move || {
        handshake_server(&mut server, &test_token())?;
        let (header, payload) = read_frame(&mut server)?;
        write_frame(
            &mut server,
            FrameKind::Reply,
            0,
            ChannelId(99),
            header.corr,
            &payload,
        )?;
        Ok::<(), IpcError>(())
    });
    let err = echo_call(
        &mut client,
        &EchoRequest {
            message: "cross-channel".to_owned(),
            count: 1,
        },
        &test_token(),
    )
    .expect_err("REPLY on another channel must not decode as echo");
    drop(client);
    let _ = handle.join();
    assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
    assert!(
        !err.to_string().contains("cross-channel"),
        "must not surface the payload as a successful echo: {err}"
    );
}

#[cfg(unix)]
#[test]
fn echo_over_uds_path() {
    let endpoint = std::env::temp_dir().join(format!(
        "keld-echo-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let (ready_tx, ready_rx) = mpsc::channel();
    let handle = listen_and_serve(endpoint.clone(), ready_tx);
    ready_rx.recv().expect("bound");
    let mut stream = std::os::unix::net::UnixStream::connect(&endpoint).expect("connect");
    let response = echo_call(
        &mut stream,
        &EchoRequest {
            message: "uds".to_owned(),
            count: 1,
        },
        &test_token(),
    )
    .expect("echo");
    drop(stream);
    handle.join().expect("server thread").expect("serve");
    let _ = std::fs::remove_file(&endpoint);
    assert_eq!(response.message, "uds");
    assert_eq!(response.count, 1);
}

#[test]
fn empty_hello_is_rejected_before_echo_dispatch() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    write_frame(
        &mut client,
        FrameKind::Hello,
        0,
        ChannelId(0),
        CorrelationId(0),
        &[],
    )
    .expect("client writes empty HELLO");
    let err = handle
        .join()
        .expect("server thread")
        .expect_err("empty HELLO must not reach echo");
    let msg = err.to_string();
    // kel133 AC4: empty HELLO is a 005 shape failure; 007 stays reserved for
    // an exactly shaped foreign token. Disclosure rules are unchanged.
    assert!(msg.contains("KELD-IPC-005"), "{msg}");
    assert!(
        !msg.contains("a5"),
        "must not leak the session token: {msg}"
    );
    let leaked = match read_frame(&mut client) {
        Ok((_, payload)) => payload == TEST_TOKEN_BYTES,
        Err(_) => false,
    };
    assert!(
        !leaked,
        "serve_echo_session must not write the token after an empty HELLO"
    );
}

#[test]
fn hello_frame_bytes_are_pinned() {
    let bytes = FrameHeader {
        kind: FrameKind::Hello,
        flags: 0,
        channel: ChannelId(0),
        corr: CorrelationId(0),
        len: 0,
    }
    .encode();
    assert_eq!(bytes.len(), HEADER_LEN);
    assert_eq!(
        bytes,
        [
            b'K',
            b'I',
            PROTOCOL_VERSION,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
        ]
    );
}

#[test]
fn hello_token_payload_len_is_pinned() {
    assert_eq!(PROTOCOL_VERSION, 2);
    let bytes = FrameHeader {
        kind: FrameKind::Hello,
        flags: 0,
        channel: ChannelId(0),
        corr: CorrelationId(0),
        len: 32,
    }
    .encode();
    assert_eq!(
        bytes,
        [b'K', b'I', 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0]
    );
    assert_eq!(test_token().as_bytes().len(), 32);
}

#[test]
fn echo_call_header_bytes_are_pinned() {
    let payload = encode(&EchoRequest {
        message: "kipc".to_owned(),
        count: 3,
    })
    .expect("encode");
    assert_eq!(payload, [0x04, b'k', b'i', b'p', b'c', 0x03]);
    let header = FrameHeader {
        kind: FrameKind::Call,
        flags: 0,
        channel: ECHO_CHANNEL,
        corr: CorrelationId(1),
        len: u32::try_from(payload.len()).expect("len"),
    }
    .encode();
    assert_eq!(
        header,
        [b'K', b'I', 2, 1, 0, 0, 1, 0, 1, 0, 0, 0, 6, 0, 0, 0]
    );
}

#[test]
fn echo_invoke_two_calls_on_one_handshake() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    handshake_client(&mut client, &test_token()).expect("hello once");
    let first = echo_invoke(
        &mut client,
        &EchoRequest {
            message: "kel30-first".to_owned(),
            count: 2,
        },
        CorrelationId(1),
    )
    .expect("first invoke");
    let second = echo_invoke(
        &mut client,
        &EchoRequest {
            message: "kel30-second".to_owned(),
            count: 9,
        },
        CorrelationId(2),
    )
    .expect("second invoke");
    drop(client);
    handle.join().expect("server thread").expect("serve");
    assert_eq!(first.message, "kel30-first");
    assert_eq!(first.count, 2);
    assert_eq!(second.message, "kel30-second");
    assert_eq!(second.count, 9);
    assert_ne!(
        first.message, second.message,
        "two invokes must not share a hardcoded reply"
    );
}

#[test]
fn persistent_echo_session_retries_idle_reader_polls() {
    let (mut client, server) = connected_pair();
    let (terminal_tx, terminal_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut stream = server;
        let _ = terminal_tx.send(serve_echo_session(&mut stream, &test_token()));
    });

    let first = echo_call(
        &mut client,
        &EchoRequest {
            message: "before-idle".to_owned(),
            count: 1,
        },
        &test_token(),
    )
    .expect("first echo");
    assert_eq!(first.message, "before-idle");

    let early = terminal_rx.recv_timeout(Duration::from_millis(750));
    assert!(
        early.is_err(),
        "idle reader polls must not end a persistent session before the next CALL: {early:?}"
    );

    let second = echo_invoke(
        &mut client,
        &EchoRequest {
            message: "after-idle".to_owned(),
            count: 2,
        },
        CorrelationId(2),
    )
    .expect("second CALL after idle reader polls");
    assert_eq!(second.message, "after-idle");
    drop(client);
    terminal_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("server must finish after peer EOF")
        .expect("persistent session must end cleanly at peer EOF");
}

#[test]
fn second_echo_call_on_same_stream_client_001_server_005() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    let first = echo_call(
        &mut client,
        &EchoRequest {
            message: "once".to_owned(),
            count: 1,
        },
        &test_token(),
    )
    .expect("first one-shot");
    assert_eq!(first.message, "once");
    let err = echo_call(
        &mut client,
        &EchoRequest {
            message: "twice".to_owned(),
            count: 2,
        },
        &test_token(),
    )
    .expect_err("second HELLO on a live session must fail");
    drop(client);
    let server_err = handle
        .join()
        .expect("server thread")
        .expect_err("server must fail closed on a second HELLO");
    let client_msg = err.to_string();
    assert!(
        client_msg.contains("KELD-IPC-001"),
        "client observes peer close after the server rejects the second HELLO: {client_msg}"
    );
    assert!(
        !client_msg.contains("twice"),
        "must not surface the second payload as a successful echo: {client_msg}"
    );
    let server_msg = server_err.to_string();
    assert!(
        server_msg.contains("KELD-IPC-005"),
        "server must reject a second HELLO as protocol, not serve it: {server_msg}"
    );
}

/// kel133 AC2 over a live authenticated session: each reserved combination on
/// the echo channel closes the link with host-visible `KELD-IPC-005`, the
/// peer receives no handler reply, and no echo side effect occurs. The
/// absence of any reply bytes before EOF is the zero-handler-effect oracle —
/// a server that dispatched the hostile CALL would have written a REPLY frame.
#[test]
fn hostile_authenticated_frames_close_with_005_and_zero_reply_bytes() {
    let hostile_headers: [(&str, FrameKind, u16, u16, u32); 4] = [
        ("corr zero", FrameKind::Call, 0, 1, 0),
        ("flag raw", FrameKind::Call, keld_ipc::frame::FLAG_RAW, 1, 7),
        ("unknown flag", FrameKind::Call, 1 << 3, 1, 7),
        ("wrong channel", FrameKind::Call, 0, 2, 7),
    ];
    for (case, kind, flags, channel, corr) in hostile_headers {
        let (mut client, server) = connected_pair();
        let handle = spawn_echo_server(server);
        handshake_client(&mut client, &test_token()).expect("authenticate");

        let payload = encode(&EchoRequest {
            message: "hostile".to_owned(),
            count: 1,
        })
        .expect("encode");
        write_frame(
            &mut client,
            kind,
            flags,
            ChannelId(channel),
            CorrelationId(corr),
            &payload,
        )
        .expect("client writes hostile frame");

        let err = handle
            .join()
            .expect("server thread")
            .expect_err("hostile frame must tear the session down");
        assert!(
            err.to_string().contains("KELD-IPC-005"),
            "{case}: expected 005, got {err}"
        );

        // Zero handler effect: the peer observes no REPLY frame, only close.
        let leaked_reply = match read_frame(&mut client) {
            Ok((header, _)) => header.kind == FrameKind::Reply,
            Err(_) => false,
        };
        assert!(!leaked_reply, "{case}: hostile CALL must never be answered");
    }
}

/// kel133 AC2 codec half over a live session: valid header semantics but
/// malformed/trailing payload bytes stay `KELD-IPC-003` with the same
/// zero-effect close.
#[test]
fn trailing_payload_bytes_close_with_003_and_zero_reply_bytes() {
    let (mut client, server) = connected_pair();
    let handle = spawn_echo_server(server);
    handshake_client(&mut client, &test_token()).expect("authenticate");

    let mut payload = encode(&EchoRequest {
        message: "kipc".to_owned(),
        count: 3,
    })
    .expect("encode");
    payload.push(0x00); // trailing byte: postcard must reject leftovers
    write_frame(
        &mut client,
        FrameKind::Call,
        0,
        ChannelId(1),
        CorrelationId(9),
        &payload,
    )
    .expect("client writes trailing-byte call");

    let err = handle
        .join()
        .expect("server thread")
        .expect_err("trailing bytes must tear the session down");
    assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    let leaked_reply = match read_frame(&mut client) {
        Ok((header, _)) => header.kind == FrameKind::Reply,
        Err(_) => false,
    };
    assert!(!leaked_reply, "malformed CALL must never be answered");
}
