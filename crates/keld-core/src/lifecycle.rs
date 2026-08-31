//! Host-owned app-lifecycle session (KEL-72).
//!
//! The host owns windows and session lifetime. This module speaks the
//! generic [`keld_ipc::LIFECYCLE_CHANNEL`] contract: `Event` `Ready` /
//! `LastWindowClosed`, `Call` `Quit`. Electron names stay in
//! `@keld/electron` (`crates/keld-compat/AGENTS.md`: no Electron-isms here).

use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use keld_ipc::codec::{decode, encode};
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::link::{AppLinkDeadlines, handshake_server, read_frame_interruptible, write_frame};
use keld_ipc::{
    APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, IpcError, LIFECYCLE_CHANNEL, LifecycleEvent,
    LifecycleRequest, LifecycleResponse, SessionToken,
};

/// Host side of one app-link lifecycle session.
///
/// After [`Self::handshake`] or [`Self::from_authenticated`], the caller sends
/// `Ready` / `LastWindowClosed` explicitly — they are never implied by
/// handshake. A 5-second I/O deadline
/// applies to `HELLO` and to subsequent writes. The persistent reader then
/// polls with a short `SO_RCVTIMEO` until `Quit`, EOF, or Drop so a quiet
/// `whenReady` wait is not `KELD-IPC-006` and so Drop can join on Windows.
/// Drop sets a stop flag, shuts down both directions of the app-link (peer
/// FIN), and joins the reader.
pub struct LifecycleSession<W: AppLinkDeadlines> {
    writer: Arc<Mutex<W>>,
    windows: u32,
    ready: bool,
    quit_rx: Receiver<Result<(), IpcError>>,
    reader: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl<W: AppLinkDeadlines> core::fmt::Debug for LifecycleSession<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LifecycleSession")
            .field("windows", &self.windows)
            .field("ready", &self.ready)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send + AppLinkDeadlines + 'static> LifecycleSession<W> {
    /// Completes v2 `HELLO` on `reader`, then spawns a reader thread for
    /// `Quit` `Call`s. `writer` must be a cloned handle of the same socket
    /// (`UnixStream::try_clone` / `TcpStream::try_clone`).
    ///
    /// # Errors
    ///
    /// Returns [`IpcError`] on handshake, deadline, or I/O failure.
    pub fn handshake<R>(mut reader: R, writer: W, token: &SessionToken) -> Result<Self, IpcError>
    where
        R: Read + Write + AppLinkDeadlines + Send + 'static,
    {
        reader.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
        writer.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
        handshake_server(&mut reader, token)?;
        Self::start_authenticated(reader, writer)
    }

    /// Starts lifecycle handling on a stream whose host-owned bootstrap has
    /// already verified v2 `HELLO`.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError`] if session deadlines cannot be configured.
    pub fn from_authenticated<R>(reader: R, writer: W) -> Result<Self, IpcError>
    where
        R: Read + Write + AppLinkDeadlines + Send + 'static,
    {
        writer.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
        Self::start_authenticated(reader, writer)
    }

    fn start_authenticated<R>(reader: R, writer: W) -> Result<Self, IpcError>
    where
        R: Read + Write + AppLinkDeadlines + Send + 'static,
    {
        // Persistent session: a quiet child waiting on Ready is expected.
        // `read_frame` cannot retry after Timeout, so the handshake recv
        // deadline must not remain on the reader. Replacing it with a short
        // poll lets Drop join on Windows (clone-shutdown does not wake a
        // local `read`; rust-lang/rust#121594). `SO_SNDTIMEO` stays the
        // handshake send deadline — spec exception is reader-only.
        reader.set_app_link_read_deadline(Some(APP_LINK_READER_POLL))?;

        let writer = Arc::new(Mutex::new(writer));
        let writer_for_reader = Arc::clone(&writer);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_reader = Arc::clone(&stop);
        let (quit_tx, quit_rx) = mpsc::channel();
        let reader_handle = thread::spawn(move || {
            let outcome = read_until_quit(reader, &writer_for_reader, stop_for_reader.as_ref());
            let _ = quit_tx.send(outcome);
        });

        Ok(Self {
            writer,
            windows: 0,
            ready: false,
            quit_rx,
            reader: Some(reader_handle),
            stop,
        })
    }

    /// Sends [`LifecycleEvent::Ready`]. Idempotent: a second call does not
    /// write another frame.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError`] if the event cannot be written.
    pub fn signal_ready(&mut self) -> Result<(), IpcError> {
        if self.ready {
            return Ok(());
        }
        self.write_event(LifecycleEvent::Ready)?;
        self.ready = true;
        Ok(())
    }

    /// Records that the host now owns one more window.
    pub fn window_opened(&mut self) {
        self.windows = self.windows.saturating_add(1);
    }

    /// Records that one host-owned window closed. When the count reaches
    /// zero, sends [`LifecycleEvent::LastWindowClosed`].
    ///
    /// # Errors
    ///
    /// Returns [`IpcError`] if the event cannot be written.
    pub fn window_closed(&mut self) -> Result<(), IpcError> {
        if self.windows == 0 {
            return Ok(());
        }
        self.windows -= 1;
        if self.windows == 0 {
            self.write_event(LifecycleEvent::LastWindowClosed)?;
        }
        Ok(())
    }

    /// Blocks until the peer sends `Quit` (or the session ends on EOF).
    ///
    /// # Errors
    ///
    /// Returns the reader thread's [`IpcError`], or a protocol error if the
    /// thread vanished without sending.
    pub fn wait_for_quit(&mut self) -> Result<(), IpcError> {
        match self.quit_rx.recv() {
            Ok(result) => {
                if let Some(handle) = self.reader.take() {
                    let _ = handle.join();
                }
                result
            }
            Err(_) => Err(IpcError::Protocol {
                detail: "lifecycle reader thread ended without a quit outcome",
            }),
        }
    }

    fn write_event(&self, event: LifecycleEvent) -> Result<(), IpcError> {
        let payload = encode(&event)?;
        let mut guard = self.writer.lock().map_err(|_| IpcError::Protocol {
            detail: "lifecycle writer lock poisoned",
        })?;
        write_frame(
            &mut *guard,
            FrameKind::Event,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            &payload,
        )
    }
}

impl<W: AppLinkDeadlines> Drop for LifecycleSession<W> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        {
            let guard = match self.writer.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let _ = guard.shutdown_app_link();
        }
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
    }
}

fn read_until_quit<R, W>(
    mut reader: R,
    writer: &Mutex<W>,
    stop: &AtomicBool,
) -> Result<(), IpcError>
where
    R: Read,
    W: Write,
{
    loop {
        let (header, payload) = match read_frame_interruptible(&mut reader, stop) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(()),
            Err(IpcError::Io(e)) if e.kind() == ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        match header.kind {
            FrameKind::Call if header.channel == LIFECYCLE_CHANNEL => {
                let req: LifecycleRequest = decode(&payload)?;
                match req {
                    LifecycleRequest::Quit => {
                        let bytes = encode(&LifecycleResponse::Quit)?;
                        let mut guard = writer.lock().map_err(|_| IpcError::Protocol {
                            detail: "lifecycle writer lock poisoned",
                        })?;
                        write_frame(
                            &mut *guard,
                            FrameKind::Reply,
                            0,
                            LIFECYCLE_CHANNEL,
                            header.corr,
                            &bytes,
                        )?;
                        return Ok(());
                    }
                }
            }
            FrameKind::Call => {
                return Err(IpcError::Protocol {
                    detail: "lifecycle Call on a non-lifecycle channel",
                });
            }
            FrameKind::Ping => {
                let mut guard = writer.lock().map_err(|_| IpcError::Protocol {
                    detail: "lifecycle writer lock poisoned",
                })?;
                write_frame(
                    &mut *guard,
                    FrameKind::Ping,
                    0,
                    header.channel,
                    header.corr,
                    &[],
                )?;
            }
            _ => {
                return Err(IpcError::Protocol {
                    detail: "unexpected frame kind in lifecycle session",
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use super::*;
    use keld_ipc::codec::decode;
    use keld_ipc::link::{AppLinkDeadlines, handshake_client, read_frame, write_frame};
    use keld_ipc::{ECHO_CHANNEL, IpcError, LifecycleEvent, LifecycleRequest};

    fn test_token() -> SessionToken {
        SessionToken::from_bytes([0x72; 32])
    }

    #[cfg(unix)]
    type Stream = std::os::unix::net::UnixStream;
    #[cfg(windows)]
    type Stream = std::net::TcpStream;

    #[cfg(unix)]
    #[allow(clippy::expect_used)] // test helper: pair() failing is a fixture bug
    fn connected_pair() -> (Stream, Stream) {
        std::os::unix::net::UnixStream::pair().expect("unix pair")
    }

    #[cfg(windows)]
    #[allow(clippy::expect_used)] // test helper: loopback bind/accept is the fixture
    fn connected_pair() -> (Stream, Stream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let accept = std::thread::spawn(move || listener.accept().expect("accept").0);
        let client = std::net::TcpStream::connect(addr).expect("connect");
        let server = accept.join().expect("accept thread");
        (client, server)
    }

    #[allow(clippy::expect_used)] // test helper: handshake failure is a fixture bug
    fn begin_session(server: Stream) -> LifecycleSession<Stream> {
        let writer = server.try_clone().expect("clone");
        LifecycleSession::handshake(server, writer, &test_token()).expect("handshake")
    }

    #[test]
    fn ready_event_is_not_sent_until_signal_ready() {
        let (mut client, server) = connected_pair();
        let host_thread = std::thread::spawn(move || begin_session(server));
        handshake_client(&mut client, &test_token()).expect("client hello");
        let mut host = host_thread.join().expect("host thread");

        client
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .expect("short timeout as kill switch, not a wait");
        let idle = read_frame(&mut client);
        assert!(
            matches!(idle, Err(IpcError::Timeout)),
            "no Event must appear before signal_ready (negative control: sending Ready inside handshake would make this succeed): {idle:?}"
        );
        client.set_read_timeout(None).expect("clear");

        host.signal_ready().expect("ready");
        let (header, payload) = read_frame(&mut client).expect("ready event");
        assert_eq!(header.kind, FrameKind::Event);
        assert_eq!(header.channel, LIFECYCLE_CHANNEL);
        assert_eq!(header.corr, CorrelationId(0));
        assert_eq!(
            decode::<LifecycleEvent>(&payload).expect("event"),
            LifecycleEvent::Ready
        );
    }

    #[test]
    fn last_window_closed_event_fires_only_when_the_host_count_hits_zero() {
        let (mut client, server) = connected_pair();
        let host_thread = std::thread::spawn(move || begin_session(server));
        handshake_client(&mut client, &test_token()).expect("client hello");
        let mut host = host_thread.join().expect("host thread");
        host.window_opened();
        host.window_opened();
        host.window_closed().expect("first close");

        client
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .expect("kill switch");
        let idle = read_frame(&mut client);
        assert!(
            matches!(idle, Err(IpcError::Timeout)),
            "one remaining window must not emit LastWindowClosed: {idle:?}"
        );
        client.set_read_timeout(None).expect("clear");

        host.window_closed().expect("last close");
        let (header, payload) = read_frame(&mut client).expect("last-window event");
        assert_eq!(header.kind, FrameKind::Event);
        assert_eq!(
            decode::<LifecycleEvent>(&payload).expect("event"),
            LifecycleEvent::LastWindowClosed
        );
    }

    #[test]
    fn quit_call_replies_and_ends_the_session() {
        let (mut client, server) = connected_pair();
        let host_thread = std::thread::spawn(move || begin_session(server));
        handshake_client(&mut client, &test_token()).expect("client hello");
        let mut host = host_thread.join().expect("host thread");

        let payload = encode(&LifecycleRequest::Quit).expect("enc");
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(1),
            &payload,
        )
        .expect("quit call");
        let (header, reply) = read_frame(&mut client).expect("quit reply");
        assert_eq!(header.kind, FrameKind::Reply);
        assert_eq!(header.corr, CorrelationId(1));
        assert_eq!(
            decode::<LifecycleResponse>(&reply).expect("resp"),
            LifecycleResponse::Quit
        );

        host.wait_for_quit()
            .expect("session must end after Quit reply");
    }

    #[test]
    fn lifecycle_call_on_non_lifecycle_channel_is_protocol_error() {
        let (mut client, server) = connected_pair();
        let host_thread = std::thread::spawn(move || begin_session(server));
        handshake_client(&mut client, &test_token()).expect("client hello");
        let mut host = host_thread.join().expect("host thread");

        let payload = encode(&LifecycleRequest::Quit).expect("enc");
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            ECHO_CHANNEL,
            CorrelationId(1),
            &payload,
        )
        .expect("call on echo channel");

        let err = host
            .wait_for_quit()
            .expect_err("a foreign-channel Call must not count as Quit");
        assert!(
            matches!(
                err,
                IpcError::Protocol { detail }
                    if detail.contains("lifecycle Call on a non-lifecycle channel")
            ),
            "Call on a non-lifecycle channel must not be 'unexpected frame kind': {err}"
        );
        assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
    }

    #[test]
    fn hello_reader_poll_keeps_writer_deadline() {
        let (mut client, server) = connected_pair();
        let host_thread = std::thread::spawn(move || begin_session(server));
        handshake_client(&mut client, &test_token()).expect("client hello");
        let host = host_thread.join().expect("host thread");

        let reported = host
            .writer
            .lock()
            .expect("lock")
            .app_link_write_deadline()
            .expect("query the writer handle, not a clone");
        if let Some(deadline) = reported {
            assert_eq!(
                deadline, APP_LINK_IO_DEADLINE,
                "HELLO cleanup must not clear SO_SNDTIMEO (spec exception is reader-only)"
            );
            drop(host);
            return;
        }

        // Windows `write_timeout()` may return None even when SO_SNDTIMEO is
        // set (std: some platforms do not provide access to the current
        // timeout). Observe the option: a non-reading peer must still hit
        // the send deadline. Hang = the deadline was actually cleared.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let payload = vec![0u8; 64 * 1024];
            let err = loop {
                let mut guard = host.writer.lock().expect("lock");
                match write_frame(
                    &mut *guard,
                    FrameKind::Ping,
                    0,
                    LIFECYCLE_CHANNEL,
                    CorrelationId(0),
                    &payload,
                ) {
                    Ok(()) => {}
                    Err(err) => break err,
                }
            };
            let _ = done_tx.send(err);
        });
        let err = done_rx
            .recv_timeout(APP_LINK_IO_DEADLINE + Duration::from_secs(2))
            .expect(
                "writer SO_SNDTIMEO must still fire after HELLO reader-poll setup; \
                 hang means the send deadline was cleared",
            );
        assert!(
            matches!(err, IpcError::Timeout),
            "HELLO cleanup is reader-only, got {err}"
        );
        assert!(err.to_string().contains("KELD-IPC-006"), "{err}");
    }

    #[test]
    fn stop_flag_joins_blocked_reader_without_clone_shutdown() {
        // rust-lang/rust#121594: TcpStream::shutdown on a cloned Win32
        // handle does not wake a blocking read on another thread. Drop
        // still calls shutdown (peer FIN), but join must not depend on it.
        let (mut client, server) = connected_pair();
        let host_thread = std::thread::spawn(move || begin_session(server));
        handshake_client(&mut client, &test_token()).expect("client hello");
        let mut host = host_thread.join().expect("host thread");

        host.stop.store(true, Ordering::Release);
        let handle = host.reader.take().expect("reader thread");
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = handle.join();
            let _ = done_tx.send(());
        });
        done_rx.recv_timeout(Duration::from_secs(2)).expect(
            "lifecycle reader must honor the stop flag on a bounded SO_RCVTIMEO poll; \
             timeout means join still depends on clone-shutdown",
        );
        drop(host);
        drop(client);
    }

    #[test]
    fn drop_with_idle_peer_shuts_down_and_joins_reader() {
        let (mut client, server) = connected_pair();
        let host_thread = std::thread::spawn(move || begin_session(server));
        handshake_client(&mut client, &test_token()).expect("client hello");
        let host = host_thread.join().expect("host thread");

        // Kill switch for the post-Drop read: Darwin rejects set_read_timeout
        // on a socket the peer already shut down (EINVAL).
        client
            .set_read_timeout(Some(Duration::from_millis(200)))
            .expect("kill switch");

        // Peer stays connected and silent (no Quit, no close). Win32
        // clone-shutdown does not wake the local reader; stop + poll must.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(host);
            let _ = done_tx.send(());
        });
        done_rx.recv_timeout(Duration::from_secs(2)).expect(
            "Drop must set the stop flag, shutdown (peer FIN), and join the reader; \
             timeout means the local read leaked on an idle peer",
        );

        let eof = read_frame(&mut client);
        assert!(
            matches!(
                eof,
                Err(IpcError::Io(ref e)) if matches!(
                    e.kind(),
                    ErrorKind::UnexpectedEof
                        | ErrorKind::ConnectionReset
                        | ErrorKind::BrokenPipe
                        | ErrorKind::ConnectionAborted
                )
            ),
            "idle peer must observe shutdown, not a live socket (Timeout) or hang: {eof:?}"
        );
    }
}
