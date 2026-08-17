//! Blocking app-link session helpers (echo server + client for KEL-30).

use std::io::{ErrorKind, Read, Write};

use crate::codec::{decode, encode};
use crate::echo::{ECHO_CHANNEL, EchoRequest, EchoResponse, handle_echo};
use crate::frame::{CorrelationId, FrameKind};
use crate::link::{AppLinkDeadlines, handshake_client, handshake_server, read_frame, write_frame};
use crate::token::SessionToken;
use crate::{APP_LINK_IO_DEADLINE, IpcError};

/// Serves one connected app-link peer until the stream closes.
///
/// Applies [`APP_LINK_IO_DEADLINE`] so a silent peer cannot block the host.
/// `token` is required in the v2 `HELLO` (`handshake_server`) before any `Call`
/// is dispatched.
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, handler, auth, or deadline failures.
pub fn serve_echo_session<S: Read + Write + AppLinkDeadlines>(
    stream: &mut S,
    token: &SessionToken,
) -> Result<(), IpcError> {
    stream.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
    handshake_server(stream, token)?;
    loop {
        let (header, payload) = match read_frame(stream) {
            Ok(frame) => frame,
            Err(IpcError::Io(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };
        match header.kind {
            FrameKind::Call if header.channel == ECHO_CHANNEL => {
                let reply = handle_echo(&payload)?;
                write_frame(
                    stream,
                    FrameKind::Reply,
                    0,
                    ECHO_CHANNEL,
                    header.corr,
                    &reply,
                )?;
            }
            FrameKind::Ping => {
                write_frame(stream, FrameKind::Ping, 0, header.channel, header.corr, &[])?;
            }
            _ => {
                return Err(IpcError::Protocol {
                    detail: "unexpected frame kind in echo session",
                });
            }
        }
    }
    Ok(())
}

/// Sends one echo `Call` on an already-handshaken app-link stream.
///
/// `corr` must not be `0` (reserved for `HELLO`). This is the session-loop
/// primitive: one `HELLO` per connection, then N `CALL`/`REPLY` pairs until
/// EOF. One-shot connect+handshake+call is [`echo_call`].
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, or codec failure, or when `corr`
/// is `0`.
pub fn echo_invoke<S: Read + Write>(
    stream: &mut S,
    request: &EchoRequest,
    corr: CorrelationId,
) -> Result<EchoResponse, IpcError> {
    if corr == CorrelationId(0) {
        return Err(IpcError::Protocol {
            detail: "echo CALL correlation id 0 is reserved for HELLO",
        });
    }
    let payload = encode(request)?;
    write_frame(stream, FrameKind::Call, 0, ECHO_CHANNEL, corr, &payload)?;
    let (header, payload) = read_frame(stream)?;
    if header.kind != FrameKind::Reply || header.corr != corr || header.channel != ECHO_CHANNEL {
        return Err(IpcError::Protocol {
            detail: "expected REPLY for echo CALL",
        });
    }
    decode(&payload)
}

/// Sends one echo `Call` and returns the decoded response.
///
/// Applies [`APP_LINK_IO_DEADLINE`] before `handshake_client`. `token` must
/// match the server's session token. A second [`echo_call`] on the same
/// stream sends a second `HELLO`: the server fails with [`IpcError::Protocol`]
/// (`KELD-IPC-005`) and the client observes [`IpcError::Io`] (`KELD-IPC-001`)
/// because the peer closed. Further calls use [`echo_invoke`].
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, codec, auth, or deadline failures.
pub fn echo_call<S: Read + Write + AppLinkDeadlines>(
    stream: &mut S,
    request: &EchoRequest,
    token: &SessionToken,
) -> Result<EchoResponse, IpcError> {
    stream.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
    handshake_client(stream, token)?;
    echo_invoke(stream, request, CorrelationId(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_invoke_rejects_corr_zero_before_io() {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        let err = echo_invoke(
            &mut cursor,
            &EchoRequest {
                message: "must-not-write".to_owned(),
                count: 1,
            },
            CorrelationId(0),
        )
        .expect_err("corr 0 must fail closed");
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-005"), "{msg}");
        assert!(msg.contains("reserved for HELLO"), "{msg}");
        assert!(
            cursor.get_ref().is_empty(),
            "must not write a CALL frame for corr 0: {:?}",
            cursor.get_ref()
        );
    }
}
