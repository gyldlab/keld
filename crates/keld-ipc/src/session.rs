//! Blocking app-link session helpers (echo server + client for KEL-30).

use std::io::{ErrorKind, Read, Write};

use crate::codec::{decode, encode};
use crate::echo::{ECHO_CHANNEL, EchoRequest, EchoResponse, handle_echo};
use crate::frame::{CorrelationId, FrameKind};
use crate::link::{AppLinkDeadlines, handshake, read_frame, write_frame};
use crate::{APP_LINK_IO_DEADLINE, IpcError};

/// Serves one connected app-link peer until the stream closes.
///
/// Applies [`APP_LINK_IO_DEADLINE`] so a silent peer cannot block the host.
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, handler, or deadline failures.
pub fn serve_echo_session<S: Read + Write + AppLinkDeadlines>(
    stream: &mut S,
) -> Result<(), IpcError> {
    stream.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
    handshake(stream)?;
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

/// Sends one echo `Call` and returns the decoded response.
///
/// Applies [`APP_LINK_IO_DEADLINE`] before the handshake.
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, codec, or deadline failures.
pub fn echo_call<S: Read + Write + AppLinkDeadlines>(
    stream: &mut S,
    request: &EchoRequest,
) -> Result<EchoResponse, IpcError> {
    stream.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
    handshake(stream)?;
    let payload = encode(request)?;
    let corr = CorrelationId(1);
    write_frame(stream, FrameKind::Call, 0, ECHO_CHANNEL, corr, &payload)?;
    let (header, payload) = read_frame(stream)?;
    if header.kind != FrameKind::Reply || header.corr != corr || header.channel != ECHO_CHANNEL {
        return Err(IpcError::Protocol {
            detail: "expected REPLY for echo CALL",
        });
    }
    decode(&payload)
}
