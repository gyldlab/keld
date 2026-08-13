//! Framed read/write on a byte stream (app-link control plane v0).

use std::io::{self, Read, Write};
use std::time::Duration;

use crate::frame::{ChannelId, CorrelationId, FrameHeader, FrameKind};
use crate::token::SessionToken;
use crate::{HEADER_LEN, IpcError, MAX_FRAME_LEN};

/// Connected app-link streams that can bound a blocking read or write.
///
/// Spec: `docs/architecture/02-ipc.md` §7. v0 uses socket timeouts, not an
/// async runtime.
pub trait AppLinkDeadlines {
    /// Sets read and write timeouts. `None` restores blocking-forever.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the OS rejects the timeout (should not on
    /// connected `UnixStream` / `TcpStream`).
    fn set_app_link_deadlines(&self, timeout: Option<Duration>) -> io::Result<()>;
}

#[cfg(unix)]
impl AppLinkDeadlines for std::os::unix::net::UnixStream {
    fn set_app_link_deadlines(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(timeout)?;
        self.set_write_timeout(timeout)
    }
}

impl AppLinkDeadlines for std::net::TcpStream {
    fn set_app_link_deadlines(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(timeout)?;
        self.set_write_timeout(timeout)
    }
}

fn ensure_payload_len(len: usize) -> Result<(), IpcError> {
    if len > MAX_FRAME_LEN {
        return Err(IpcError::PayloadTooLarge);
    }
    Ok(())
}

/// Reads one kipc frame (header + payload) from `stream`.
///
/// Rejects `header.len` above [`MAX_FRAME_LEN`] before allocating the payload
/// buffer (forged `u32` lengths must not become multi-GiB `Vec`s).
///
/// After [`IpcError::Timeout`] or [`IpcError::Io`], the stream is unusable: a
/// partial header or payload may already have been consumed. Close the link;
/// do not retry on the same stream.
///
/// # Errors
///
/// Returns [`IpcError`] on I/O failure, bad header, oversized payload, or
/// deadline (`KELD-IPC-006`) when the stream has an I/O timeout set.
pub fn read_frame<S: Read>(stream: &mut S) -> Result<(FrameHeader, Vec<u8>), IpcError> {
    let mut header_bytes = [0u8; HEADER_LEN];
    stream.read_exact(&mut header_bytes)?;
    let header = FrameHeader::decode(&header_bytes)?;
    let len = usize::try_from(header.len).map_err(|_| IpcError::PayloadTooLarge)?;
    ensure_payload_len(len)?;
    let mut payload = vec![0u8; len];
    if !payload.is_empty() {
        stream.read_exact(&mut payload)?;
    }
    Ok((header, payload))
}

/// Writes one kipc frame to `stream`.
///
/// # Errors
///
/// Returns [`IpcError::Io`] or [`IpcError::Timeout`] on write failure, or
/// [`IpcError::PayloadTooLarge`] if `payload.len()` exceeds [`MAX_FRAME_LEN`]
/// (or does not fit in `u32`).
pub fn write_frame<S: Write>(
    stream: &mut S,
    kind: FrameKind,
    flags: u16,
    channel: crate::frame::ChannelId,
    corr: crate::frame::CorrelationId,
    payload: &[u8],
) -> Result<(), IpcError> {
    ensure_payload_len(payload.len())?;
    let len = u32::try_from(payload.len()).map_err(|_| IpcError::PayloadTooLarge)?;
    let header = FrameHeader {
        kind,
        flags,
        channel,
        corr,
        len,
    };
    stream.write_all(&header.encode())?;
    if !payload.is_empty() {
        stream.write_all(payload)?;
    }
    stream.flush()?;
    Ok(())
}

fn write_hello<S: Write>(stream: &mut S, token: &SessionToken) -> Result<(), IpcError> {
    write_frame(
        stream,
        FrameKind::Hello,
        0,
        ChannelId(0),
        CorrelationId(0),
        token.as_bytes(),
    )
}

fn read_and_verify_hello<S: Read>(stream: &mut S, token: &SessionToken) -> Result<(), IpcError> {
    let (header, payload) = read_frame(stream)?;
    if header.kind != FrameKind::Hello {
        return Err(IpcError::Protocol {
            detail: "expected HELLO from peer",
        });
    }
    if header.channel != ChannelId(0) || header.corr != CorrelationId(0) {
        return Err(IpcError::Protocol {
            detail: "HELLO must have reserved channel/corr 0",
        });
    }
    let peer = SessionToken::try_from_slice(&payload)?;
    if peer != *token {
        return Err(IpcError::HelloAuth {
            detail: "HELLO session token mismatch",
        });
    }
    Ok(())
}

/// Client `HELLO`: write `token`, then read and verify the server's `HELLO`.
///
/// The child already possesses `token` from `KELD_APP_LINK`. Writing first
/// does not leak a secret the peer lacked.
///
/// # Errors
///
/// Returns [`IpcError::Protocol`] if the peer does not send `Hello`,
/// [`IpcError::HelloAuth`] if the token is missing or wrong, or
/// [`IpcError::Timeout`] if the peer stays silent past the stream deadline.
pub fn handshake_client<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
) -> Result<(), IpcError> {
    write_hello(stream, token)?;
    read_and_verify_hello(stream, token)
}

/// Server `HELLO`: read and verify the client's `HELLO`, then write `token`.
///
/// Must not write the session token until the peer proves possession
/// (`KELD-IPC-007` on empty, truncated, or mismatched payloads). Otherwise a
/// Windows loopback connector learns the secret from the host's first frame.
///
/// # Errors
///
/// Returns [`IpcError::Protocol`] if the peer does not send `Hello`,
/// [`IpcError::HelloAuth`] if the token is missing or wrong, or
/// [`IpcError::Timeout`] if the peer stays silent past the stream deadline.
pub fn handshake_server<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
) -> Result<(), IpcError> {
    read_and_verify_hello(stream, token)?;
    write_hello(stream, token)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, ErrorKind, Write as _};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::frame::HeaderError;
    use crate::token::SessionToken;

    const TEST_TOKEN_BYTES: [u8; 32] = [0xA5; 32];

    fn test_token() -> SessionToken {
        SessionToken::from_bytes(TEST_TOKEN_BYTES)
    }

    #[cfg(unix)]
    fn connected_pair() -> (
        std::os::unix::net::UnixStream,
        std::os::unix::net::UnixStream,
    ) {
        std::os::unix::net::UnixStream::pair().expect("unix pair")
    }

    #[cfg(windows)]
    fn connected_pair() -> (std::net::TcpStream, std::net::TcpStream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let accept = std::thread::spawn(move || listener.accept().expect("accept").0);
        let client = std::net::TcpStream::connect(addr).expect("connect");
        let server = accept.join().expect("accept thread");
        (client, server)
    }

    fn header_with_len(len: u32) -> [u8; HEADER_LEN] {
        FrameHeader {
            kind: FrameKind::Ping,
            flags: 0,
            channel: ChannelId(0),
            corr: CorrelationId(0),
            len,
        }
        .encode()
    }

    #[test]
    fn ping_empty_payload_roundtrips_bytes() {
        let mut cursor = Cursor::new(Vec::new());
        write_frame(
            &mut cursor,
            FrameKind::Ping,
            0,
            ChannelId(7),
            CorrelationId(9),
            &[],
        )
        .expect("write empty ping");
        cursor.set_position(0);
        let (header, payload) = read_frame(&mut cursor).expect("read");
        assert_eq!(header.kind, FrameKind::Ping);
        assert_eq!(header.channel, ChannelId(7));
        assert_eq!(header.corr, CorrelationId(9));
        assert_eq!(header.len, 0);
        assert!(payload.is_empty());
    }

    #[test]
    fn write_then_read_preserves_payload_bytes() {
        let payload = b"not-a-noop";
        let mut cursor = Cursor::new(Vec::new());
        write_frame(
            &mut cursor,
            FrameKind::Call,
            0,
            ChannelId(1),
            CorrelationId(1),
            payload,
        )
        .expect("write");
        cursor.set_position(0);
        let (header, got) = read_frame(&mut cursor).expect("read");
        assert_eq!(header.kind, FrameKind::Call);
        assert_eq!(got, payload);
    }

    #[test]
    fn read_frame_rejects_oversized_len_before_alloc() {
        // Classic DoS: claim ~4 GiB. Must fail on the length check without
        // attempting `vec![0u8; len]` (which would OOM / thrash).
        let bytes = header_with_len(u32::MAX);
        let mut cursor = Cursor::new(bytes.as_slice());
        let err = read_frame(&mut cursor).expect_err("oversized len must be rejected");
        assert!(
            matches!(err, IpcError::PayloadTooLarge),
            "expected PayloadTooLarge, got {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-004"), "missing error code in: {msg}");
        assert!(msg.contains("bulk plane"), "missing fix hint in: {msg}");
    }

    #[test]
    fn read_frame_rejects_just_over_max_frame_len() {
        let over = u32::try_from(MAX_FRAME_LEN + 1).expect("MAX_FRAME_LEN+1 fits u32");
        let bytes = header_with_len(over);
        let mut cursor = Cursor::new(bytes.as_slice());
        let err = read_frame(&mut cursor).expect_err("MAX_FRAME_LEN+1");
        assert!(matches!(err, IpcError::PayloadTooLarge));
        assert!(err.to_string().contains("KELD-IPC-004"), "{err}");
    }

    #[test]
    fn write_frame_rejects_over_max_without_emitting_header() {
        let mut cursor = Cursor::new(Vec::new());
        let oversized = vec![0u8; MAX_FRAME_LEN + 1];
        let err = write_frame(
            &mut cursor,
            FrameKind::Call,
            0,
            ChannelId(1),
            CorrelationId(1),
            &oversized,
        )
        .expect_err("write must refuse oversized payload");
        assert!(matches!(err, IpcError::PayloadTooLarge));
        assert!(
            cursor.get_ref().is_empty(),
            "must not write a header before the length check"
        );
    }

    #[test]
    fn ensure_payload_len_rejects_over_max() {
        assert!(ensure_payload_len(MAX_FRAME_LEN).is_ok());
        assert!(matches!(
            ensure_payload_len(MAX_FRAME_LEN + 1),
            Err(IpcError::PayloadTooLarge)
        ));
    }

    #[test]
    fn max_frame_len_is_16_mib() {
        assert_eq!(MAX_FRAME_LEN, 16 * 1024 * 1024);
        assert!(u32::try_from(MAX_FRAME_LEN).is_ok());
    }

    #[test]
    fn read_frame_rejects_protocol_version_mismatch() {
        let mut bytes = header_with_len(0);
        bytes[2] = 99;
        let mut cursor = Cursor::new(bytes.as_slice());
        let err = read_frame(&mut cursor).expect_err("version 99 must not decode");
        assert!(
            matches!(err, IpcError::Header(HeaderError::BadVersion(99))),
            "got {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-002"), "{msg}");
        assert!(msg.contains("99"), "{msg}");
    }

    #[test]
    fn read_frame_truncated_payload_is_ipc_001() {
        let mut bytes = header_with_len(8).to_vec();
        bytes.extend_from_slice(&[1, 2, 3]); // 3 of 8 claimed bytes
        let mut cursor = Cursor::new(bytes);
        let err = read_frame(&mut cursor).expect_err("truncated payload");
        assert!(matches!(err, IpcError::Io(ref e) if e.kind() == ErrorKind::UnexpectedEof));
        assert!(err.to_string().contains("KELD-IPC-001"), "{err}");
    }

    #[test]
    fn read_frame_truncated_header_is_ipc_001() {
        let bytes = &header_with_len(0)[..8];
        let mut cursor = Cursor::new(bytes);
        let err = read_frame(&mut cursor).expect_err("truncated header");
        assert!(matches!(err, IpcError::Io(ref e) if e.kind() == ErrorKind::UnexpectedEof));
        assert!(err.to_string().contains("KELD-IPC-001"), "{err}");
    }

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
            matches!(err, IpcError::Protocol { detail } if detail.contains("HELLO")),
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
        assert!(
            matches!(err, IpcError::HelloAuth { detail } if detail.contains("32-byte")),
            "got {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-007"), "{msg}");
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
        let err =
            handshake_client(&mut client, &test_token()).expect_err("31-byte HELLO must fail");
        assert!(matches!(err, IpcError::HelloAuth { .. }), "got {err}");
        assert!(err.to_string().contains("KELD-IPC-007"), "{err}");
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
        let err =
            handshake_client(&mut client, &test_token()).expect_err("foreign token must fail");
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
        assert!(err.to_string().contains("KELD-IPC-007"), "{err}");
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
        let err =
            handshake_client(&mut client, &test_token()).expect_err("reserved channel must be 0");
        assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
    }

    #[test]
    fn shutdown_mid_header_is_ipc_001() {
        let (mut reader, mut writer) = connected_pair();
        writer
            .write_all(&header_with_len(0)[..8])
            .expect("partial header");
        writer.flush().expect("flush");
        drop(writer);
        let err = read_frame(&mut reader).expect_err("peer closed mid-header");
        assert!(matches!(err, IpcError::Io(ref e) if e.kind() == ErrorKind::UnexpectedEof));
        assert!(err.to_string().contains("KELD-IPC-001"), "{err}");
    }

    #[test]
    fn read_frame_silent_peer_is_ipc_006() {
        let (mut reader, _writer) = connected_pair();
        reader
            .set_app_link_deadlines(Some(Duration::from_millis(200)))
            .expect("deadline");
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = read_frame(&mut reader);
            let _ = done_tx.send(result);
        });
        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("silent peer must hit the I/O deadline, not hang");
        let err = result.expect_err("no bytes from a live peer is not a frame");
        assert!(
            matches!(err, IpcError::Timeout),
            "deadline must be KELD-IPC-006, got {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-006"), "{msg}");
        assert!(msg.contains("deadline"), "{msg}");
        assert!(msg.contains("silent or wedged"), "{msg}");
        assert!(
            !msg.contains("KELD-IPC-001"),
            "must not classify a live silent peer as a generic I/O close: {msg}"
        );
    }

    #[test]
    fn read_frame_timeout_mid_header_leaves_stream_unusable() {
        // Partial header + deadline must error; leftover bytes are consumed.
        // A later complete frame on the same stream must not be returned as
        // Ok — docs on `read_frame`: Timeout makes the link unusable.
        let (mut reader, mut writer) = connected_pair();
        reader
            .set_app_link_deadlines(Some(Duration::from_millis(200)))
            .expect("deadline");

        let header = header_with_len(0);
        writer.write_all(&header[..8]).expect("partial header");
        writer.flush().expect("flush");

        let (first_tx, first_rx) = mpsc::channel();
        let (go_tx, go_rx) = mpsc::channel();
        let (second_tx, second_rx) = mpsc::channel();
        thread::spawn(move || {
            let first = read_frame(&mut reader);
            let _ = first_tx.send(first);
            if go_rx.recv().is_err() {
                return;
            }
            let second = read_frame(&mut reader);
            let _ = second_tx.send(second);
        });

        let first = first_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("partial header must hit the I/O deadline, not hang");
        let err = first.expect_err("partial header + deadline is not a frame");
        assert!(
            matches!(err, IpcError::Timeout),
            "deadline must be KELD-IPC-006, got {err}"
        );
        assert!(err.to_string().contains("KELD-IPC-006"), "{err}");

        writer.write_all(&header[8..]).expect("header rest");
        write_frame(
            &mut writer,
            FrameKind::Ping,
            0,
            ChannelId(7),
            CorrelationId(9),
            b"later",
        )
        .expect("later valid frame");
        go_tx.send(()).expect("reader still waiting to retry");

        let second = second_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("retry after Timeout must not hang");
        second.expect_err(
            "stream is unusable after Timeout; must not return a spliced or later frame",
        );
    }

    #[test]
    fn handshake_silent_peer_is_ipc_006() {
        let (mut client, _server) = connected_pair();
        client
            .set_app_link_deadlines(Some(Duration::from_millis(200)))
            .expect("deadline");
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = handshake_client(&mut client, &test_token());
            let _ = done_tx.send(result);
        });
        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("handshake with a silent peer must hit the deadline, not hang");
        let err = result.expect_err("peer never sent HELLO");
        assert!(matches!(err, IpcError::Timeout), "got {err}");
        assert!(err.to_string().contains("KELD-IPC-006"), "{err}");
    }
}
