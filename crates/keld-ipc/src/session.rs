//! Blocking app-link session helpers (echo server + client for KEL-30).

use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::AtomicBool;

use crate::codec::{decode, encode};
use crate::echo::{ECHO_CHANNEL, EchoRequest, EchoResponse, handle_echo};
use crate::frame::{CorrelationId, FrameKind};
use crate::link::{
    AppLinkDeadlines, handshake_client, handshake_server, read_validated_frame,
    read_validated_frame_interruptible, write_frame,
};
use crate::receive::{ReceivePolicy, ValidatedFrameHeader};
use crate::token::SessionToken;
use crate::{APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, IpcError};

/// Serves one connected app-link peer until the stream closes.
///
/// `token` is required in the v2 `HELLO` (`handshake_server`) before any
/// `Call` is dispatched. The HELLO and all writes use
/// [`APP_LINK_IO_DEADLINE`]. After authentication, idle reader polls are
/// retried so a persistent peer can wait quietly between calls; a partial
/// frame still has the five-second stall limit.
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, handler, auth, or deadline failures.
pub fn serve_echo_session<S: Read + Write + AppLinkDeadlines>(
    stream: &mut S,
    token: &SessionToken,
) -> Result<(), IpcError> {
    let never_stopped = AtomicBool::new(false);
    serve_echo_session_until_stopped(stream, token, &never_stopped)
}

/// Serves one connected echo peer until it closes or `stop` is observed.
///
/// The HELLO handshake uses [`APP_LINK_IO_DEADLINE`]. After it succeeds, the
/// reader uses [`APP_LINK_READER_POLL`] and
/// [`read_frame_interruptible`] so a quiet persistent session is not a
/// timeout. The writer keeps its five-second deadline. A partial frame that
/// stalls still returns [`IpcError::Timeout`].
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, handler, auth, or frame-stall
/// failures.
pub fn serve_echo_session_until_stopped<S: Read + Write + AppLinkDeadlines>(
    stream: &mut S,
    token: &SessionToken,
    stop: &AtomicBool,
) -> Result<(), IpcError> {
    stream.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
    handshake_server(stream, token)?;
    serve_echo_requests_until_stopped(stream, stop)
}

/// Serves echo requests on an already authenticated app link until EOF.
///
/// [`serve_echo_session`] owns the authentication step. Host-owned bootstrap
/// listeners use this function after they have already verified `HELLO`, so
/// the wire handshake remains single-sourced in [`handshake_server`].
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, handler, or deadline failures.
pub fn serve_echo_requests<S: Read + Write>(stream: &mut S) -> Result<(), IpcError> {
    let policy = ReceivePolicy::echo_receiver();
    loop {
        let (header, payload) = match read_validated_frame(stream, &policy) {
            Ok(frame) => frame,
            Err(IpcError::Io(error)) if is_peer_eof(&error) => break,
            Err(e) => return Err(e),
        };
        serve_echo_frame(stream, header, &payload)?;
    }
    Ok(())
}

/// Serves authenticated echo requests until EOF or `stop`.
///
/// This is the persistent-session variant for a host that owns cancellation.
/// It changes the reader deadline only; the caller's configured writer
/// deadline is retained.
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, handler, or a started-frame stall.
pub fn serve_echo_requests_until_stopped<S: Read + Write + AppLinkDeadlines>(
    stream: &mut S,
    stop: &AtomicBool,
) -> Result<(), IpcError> {
    stream.set_app_link_read_deadline(Some(APP_LINK_READER_POLL))?;
    let policy = ReceivePolicy::echo_receiver();
    loop {
        let (header, payload) = match read_validated_frame_interruptible(stream, &policy, stop) {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(IpcError::Io(error)) if is_peer_eof(&error) => break,
            Err(error) => return Err(error),
        };
        serve_echo_frame(stream, header, &payload)?;
    }
    Ok(())
}

fn is_peer_eof(error: &std::io::Error) -> bool {
    if error.kind() == ErrorKind::UnexpectedEof {
        return true;
    }
    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(109 | 232 | 233))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Dispatches one *admitted* echo-session frame. Semantic admission already
/// happened in the shared validator (kel133 AC1–AC2): only the policy's
/// declared kinds can reach this, so the dispatch is total over them and the
/// old ad-hoc kind/channel matching is deleted rather than duplicated.
fn serve_echo_frame<S: Write>(
    stream: &mut S,
    header: ValidatedFrameHeader,
    payload: &[u8],
) -> Result<(), IpcError> {
    match header.kind() {
        FrameKind::Call => {
            let reply = handle_echo(payload)?;
            write_frame(
                stream,
                FrameKind::Reply,
                0,
                ECHO_CHANNEL,
                header.corr(),
                &reply,
            )?;
        }
        FrameKind::Ping => {
            write_frame(
                stream,
                FrameKind::Ping,
                0,
                header.channel(),
                header.corr(),
                &[],
            )?;
        }
        other => {
            debug_assert!(false, "validator admitted an undeclared kind: {other:?}");
            return Err(IpcError::Protocol {
                detail: "unexpected frame kind in echo session",
            });
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
    // kel133 AC5: the shared waiter policy admits only the awaited REPLY —
    // matching kind, flags 0, declared channel, exact correlation.
    let (_reply, payload) = read_validated_frame(stream, &ReceivePolicy::echo_reply_waiter(corr))?;
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
