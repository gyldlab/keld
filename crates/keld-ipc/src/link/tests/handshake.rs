use super::{
    AppLinkDeadlines, ChannelId, CorrelationId, Duration, FrameKind, IpcError, TEST_TOKEN_BYTES,
    connected_pair, handshake_client, handshake_server, read_frame, test_token, write_frame,
};
use std::thread;

#[test]
fn handshake_rejects_non_hello_kind() {
    let (mut client, mut server) = connected_pair();
    write_frame(
        &mut server,
        FrameKind::Ping,
        0,
        ChannelId(0),
        CorrelationId(0),
        &[],
    )
    .expect("peer writes ping instead of hello");
    let err =
        handshake_client(&mut client, &test_token()).expect_err("ping must not satisfy HELLO");
    assert!(
        matches!(err, IpcError::Protocol { detail } if detail.contains("not declared")),
        "got {err}"
    );
    assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
}

#[test]
fn handshake_rejects_empty_hello_payload() {
    let (mut client, mut server) = connected_pair();
    write_frame(
        &mut server,
        FrameKind::Hello,
        0,
        ChannelId(0),
        CorrelationId(0),
        &[],
    )
    .expect("peer writes empty HELLO");
    let err = handshake_client(&mut client, &test_token()).expect_err("empty HELLO must fail");
    // kel133 AC4: a wrong-length HELLO is a shape failure (005), never
    // authentication (007) — collapsing shape into 007 must fail here.
    assert!(
        matches!(err, IpcError::Protocol { detail } if detail.contains("exact shape")),
        "got {err}"
    );
    let msg = err.to_string();
    assert!(msg.contains("KELD-IPC-005"), "{msg}");
    assert!(!msg.contains("KELD-IPC-007"), "{msg}");
    assert!(
        !msg.contains("a5"),
        "must not leak the expected token: {msg}"
    );
}

#[test]
fn handshake_rejects_wrong_length_hello_payload() {
    let (mut client, mut server) = connected_pair();
    write_frame(
        &mut server,
        FrameKind::Hello,
        0,
        ChannelId(0),
        CorrelationId(0),
        &[0xA5; 31],
    )
    .expect("peer writes 31-byte HELLO");
    let err = handshake_client(&mut client, &test_token()).expect_err("31-byte HELLO must fail");
    // kel133 AC4: 31 bytes is a shape failure, not an auth failure.
    assert!(matches!(err, IpcError::Protocol { .. }), "got {err}");
    assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
}

#[test]
fn handshake_rejects_mismatched_token() {
    let (mut client, mut server) = connected_pair();
    let mut foreign = TEST_TOKEN_BYTES;
    foreign[0] ^= 1;
    write_frame(
        &mut server,
        FrameKind::Hello,
        0,
        ChannelId(0),
        CorrelationId(0),
        &foreign,
    )
    .expect("peer writes foreign token");
    let err = handshake_client(&mut client, &test_token()).expect_err("foreign token must fail");
    assert!(
        matches!(err, IpcError::HelloAuth { detail } if detail.contains("mismatch")),
        "got {err}"
    );
    let msg = err.to_string();
    assert!(msg.contains("KELD-IPC-007"), "{msg}");
    assert!(!msg.contains("a5"), "must not leak the token: {msg}");
}

#[test]
fn handshake_accepts_matching_token() {
    let (mut client, mut server) = connected_pair();
    let server_token = test_token();
    let client_token = test_token();
    let handle = thread::spawn(move || handshake_server(&mut server, &server_token));
    handshake_client(&mut client, &client_token).expect("client hello");
    handle.join().expect("server thread").expect("server hello");
}

/// Negative control for KEL-60: a connector that never sends HELLO must
/// not learn the session token from the host.
#[test]
fn handshake_as_server_does_not_disclose_token_to_silent_peer() {
    let (mut attacker, mut server) = connected_pair();
    server
        .set_app_link_deadlines(Some(Duration::from_millis(200)))
        .expect("server deadline");
    attacker
        .set_app_link_deadlines(Some(Duration::from_millis(200)))
        .expect("attacker deadline");
    let server_token = test_token();
    let handle = thread::spawn(move || handshake_server(&mut server, &server_token));
    let leaked = match read_frame(&mut attacker) {
        Ok((_, payload)) => payload == TEST_TOKEN_BYTES,
        Err(_) => false,
    };
    drop(attacker);
    let _ = handle.join();
    assert!(
        !leaked,
        "server must not write the session token before the peer proves possession"
    );
}

#[test]
fn handshake_server_does_not_disclose_token_on_empty_hello() {
    let (mut attacker, mut server) = connected_pair();
    write_frame(
        &mut attacker,
        FrameKind::Hello,
        0,
        ChannelId(0),
        CorrelationId(0),
        &[],
    )
    .expect("attacker writes empty HELLO");
    let err = handshake_server(&mut server, &test_token()).expect_err("empty HELLO");
    assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
    drop(server);
    let leaked = match read_frame(&mut attacker) {
        Ok((_, payload)) => payload == TEST_TOKEN_BYTES,
        Err(_) => false,
    };
    assert!(
        !leaked,
        "empty HELLO must not be answered with the session token"
    );
}

#[test]
fn handshake_rejects_hello_on_nonzero_channel() {
    let (mut client, mut server) = connected_pair();
    write_frame(
        &mut server,
        FrameKind::Hello,
        0,
        ChannelId(1),
        CorrelationId(0),
        &[],
    )
    .expect("peer writes HELLO on channel 1");
    let err = handshake_client(&mut client, &test_token()).expect_err("reserved channel must be 0");
    assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
}
