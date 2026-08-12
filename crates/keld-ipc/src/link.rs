//! Framed read/write on a byte stream (app-link control plane v0).

use std::io::{Read, Write};

use crate::frame::{FrameHeader, FrameKind};
use crate::{HEADER_LEN, IpcError, MAX_FRAME_LEN};

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
/// # Errors
///
/// Returns [`IpcError`] on I/O failure, bad header, or oversized payload length.
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
/// Returns [`IpcError::Io`] on write failure or [`IpcError::PayloadTooLarge`] if
/// `payload.len()` exceeds [`MAX_FRAME_LEN`] (or does not fit in `u32`).
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

/// Performs the v0 `HELLO` exchange (empty payloads).
///
/// # Errors
///
/// Returns [`IpcError::Protocol`] if the peer does not send `Hello`.
pub fn handshake<S: Read + Write>(stream: &mut S) -> Result<(), IpcError> {
    write_frame(
        stream,
        FrameKind::Hello,
        0,
        crate::frame::ChannelId(0),
        crate::frame::CorrelationId(0),
        &[],
    )?;
    let (header, _) = read_frame(stream)?;
    if header.kind != FrameKind::Hello {
        return Err(IpcError::Protocol {
            detail: "expected HELLO from peer",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, ErrorKind, Write as _};

    use super::*;
    use crate::frame::{ChannelId, CorrelationId, HeaderError};

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
        let err = handshake(&mut client).expect_err("ping must not satisfy HELLO");
        assert!(
            matches!(err, IpcError::Protocol { detail } if detail.contains("HELLO")),
            "got {err}"
        );
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
}
