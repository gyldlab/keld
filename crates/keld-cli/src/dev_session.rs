//! `keld dev`'s real session server: echo (KEL-30 demo) + app lifecycle
//! (KEL-72) on one kipc connection.
//!
//! `app.whenReady()`/`app.quit()` only mean something real over a
//! persistent, already-`HELLO`'d session — [`AppRequest::WhenReady`] blocks
//! on a real signal from the host's window creation (not
//! `Promise.resolve()`), and [`AppRequest::Quit`] ends this session for
//! real. Window-close forwarding (`window-all-closed`) is deliberately not
//! wired here yet — it needs the same kind of bridge in the other
//! direction (host window → Bun) and is a separate follow-up.

use std::io::{ErrorKind, Read, Write};
use std::sync::mpsc;

use keld_ipc::app::{APP_CHANNEL, AppRequest, AppResponse};
use keld_ipc::codec::{decode, encode};
use keld_ipc::echo::{ECHO_CHANNEL, handle_echo};
use keld_ipc::frame::FrameKind;
use keld_ipc::link::{handshake_server, read_frame, write_frame};
use keld_ipc::{IpcError, SessionToken};

/// Serves one `keld dev` app-process session on `stream`, after the v2
/// `HELLO` handshake: echo `Call`s (KEL-30 demo) and app-lifecycle `Call`s
/// (KEL-72) on the same connection.
///
/// `window_ready` fires exactly once, from the host's main thread, when the
/// dev window has been created. The first `AppRequest::WhenReady` blocks on
/// it; later ones return immediately (readiness, once reached, stays
/// reached — matches Electron's own repeat-call semantics for
/// `app.whenReady()`). `AppRequest::Quit` ends the session — it does not
/// close the host window (that direction is the deferred follow-up).
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, auth, or deadline failures.
pub fn serve_dev_session<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
    window_ready: mpsc::Receiver<()>,
) -> Result<(), IpcError> {
    handshake_server(stream, token)?;
    let mut window_ready = Some(window_ready);
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
            FrameKind::Call if header.channel == APP_CHANNEL => {
                let req: AppRequest = decode(&payload)?;
                match req {
                    AppRequest::WhenReady => {
                        if let Some(rx) = window_ready.take() {
                            // Blocks the session thread only — the host's
                            // main thread keeps running its own event loop
                            // independently. A dropped Sender (window
                            // creation failed) unblocks this with an Err,
                            // which we treat the same as "ready enough to
                            // reply" — the caller finds out the window is
                            // gone the next time it tries to use it.
                            let _ = rx.recv();
                        }
                        let reply = encode(&AppResponse::Ready)?;
                        write_frame(
                            stream,
                            FrameKind::Reply,
                            0,
                            APP_CHANNEL,
                            header.corr,
                            &reply,
                        )?;
                    }
                    AppRequest::Quit => {
                        let reply = encode(&AppResponse::Quitting)?;
                        write_frame(
                            stream,
                            FrameKind::Reply,
                            0,
                            APP_CHANNEL,
                            header.corr,
                            &reply,
                        )?;
                        return Ok(());
                    }
                }
            }
            FrameKind::Ping => {
                write_frame(stream, FrameKind::Ping, 0, header.channel, header.corr, &[])?;
            }
            _ => {
                return Err(IpcError::Protocol {
                    detail: "unexpected frame kind in dev session",
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keld_ipc::frame::{ChannelId, CorrelationId};
    use keld_ipc::link::handshake_client;
    use std::thread;

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

    const TEST_TOKEN_BYTES: [u8; 32] = [0x9c; 32];

    fn test_token() -> SessionToken {
        SessionToken::from_bytes(TEST_TOKEN_BYTES)
    }

    fn call_app(stream: &mut Stream, req: AppRequest, corr: u32) -> AppResponse {
        let payload = encode(&req).expect("encode");
        write_frame(
            stream,
            FrameKind::Call,
            0,
            APP_CHANNEL,
            CorrelationId(corr),
            &payload,
        )
        .expect("write call");
        let (header, reply) = read_frame(stream).expect("read reply");
        assert_eq!(header.kind, FrameKind::Reply, "expected Reply frame");
        assert_eq!(header.channel, APP_CHANNEL);
        assert_eq!(header.corr, CorrelationId(corr));
        decode(&reply).expect("decode AppResponse")
    }

    #[test]
    fn when_ready_blocks_until_the_host_signals_then_returns_ready() {
        let (mut client, mut server) = connected_pair();
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || serve_dev_session(&mut server, &test_token(), ready_rx));

        handshake_client(&mut client, &test_token()).expect("hello");

        // Send WhenReady on a second thread so this test thread can prove
        // the reply does not arrive before the host signals readiness.
        let mut probe = client.try_clone().expect("clone");
        let (got_reply_tx, got_reply_rx) = mpsc::channel();
        let call_handle = thread::spawn(move || {
            let resp = call_app(&mut probe, AppRequest::WhenReady, 1);
            let _ = got_reply_tx.send(());
            resp
        });

        // No reply should have landed yet — nothing has signaled readiness.
        assert!(
            got_reply_rx
                .recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "WhenReady replied before the host signaled readiness"
        );

        ready_tx.send(()).expect("signal ready");
        got_reply_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("WhenReady must resolve once the host signals readiness");
        let resp = call_handle.join().expect("call thread");
        assert_eq!(resp, AppResponse::Ready);

        // A second WhenReady must not block again.
        let resp2 = call_app(&mut client, AppRequest::WhenReady, 2);
        assert_eq!(resp2, AppResponse::Ready);

        let quit = call_app(&mut client, AppRequest::Quit, 3);
        assert_eq!(quit, AppResponse::Quitting);
        drop(client);
        handle.join().expect("server thread").expect("serve");
    }

    #[test]
    fn quit_ends_the_session_without_waiting_for_ready() {
        let (mut client, mut server) = connected_pair();
        let (_ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || serve_dev_session(&mut server, &test_token(), ready_rx));
        handshake_client(&mut client, &test_token()).expect("hello");

        let quit = call_app(&mut client, AppRequest::Quit, 1);
        assert_eq!(quit, AppResponse::Quitting);
        drop(client);
        handle
            .join()
            .expect("server thread")
            .expect("Quit must end the session cleanly, not error");
    }

    #[test]
    fn echo_and_app_calls_share_one_connection() {
        let (mut client, mut server) = connected_pair();
        let (ready_tx, ready_rx) = mpsc::channel();
        ready_tx.send(()).expect("pre-signal ready");
        let handle = thread::spawn(move || serve_dev_session(&mut server, &test_token(), ready_rx));
        handshake_client(&mut client, &test_token()).expect("hello");

        let echo_req = keld_ipc::EchoRequest {
            message: "shared-session".to_owned(),
            count: 5,
        };
        let payload = encode(&echo_req).expect("encode echo");
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            ECHO_CHANNEL,
            CorrelationId(1),
            &payload,
        )
        .expect("write echo call");
        let (header, reply) = read_frame(&mut client).expect("echo reply");
        assert_eq!(header.kind, FrameKind::Reply);
        assert_eq!(header.channel, ECHO_CHANNEL);
        let echo_resp: keld_ipc::EchoResponse = decode(&reply).expect("decode echo");
        assert_eq!(echo_resp.message, "shared-session");
        assert_eq!(echo_resp.count, 5);

        let ready = call_app(&mut client, AppRequest::WhenReady, 2);
        assert_eq!(ready, AppResponse::Ready);

        let quit = call_app(&mut client, AppRequest::Quit, 3);
        assert_eq!(quit, AppResponse::Quitting);
        drop(client);
        handle.join().expect("server thread").expect("serve");
    }

    #[test]
    fn unknown_channel_still_fails_closed() {
        let (mut client, mut server) = connected_pair();
        let (_ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || serve_dev_session(&mut server, &test_token(), ready_rx));
        handshake_client(&mut client, &test_token()).expect("hello");
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            ChannelId(99),
            CorrelationId(1),
            &[],
        )
        .expect("write bogus call");
        drop(client);
        let err = handle
            .join()
            .expect("server thread")
            .expect_err("unknown channel must fail the session");
        assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
    }
}
