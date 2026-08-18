//! Host-owned app-lifecycle session (KEL-72).
//!
//! The host owns windows and session lifetime. This module speaks the
//! generic [`keld_ipc::LIFECYCLE_CHANNEL`] contract: `Event` `Ready` /
//! `LastWindowClosed`, `Call` `Quit`. Electron names stay in
//! `@keld/electron` (`crates/keld-compat/AGENTS.md`: no Electron-isms here).

use std::io::{ErrorKind, Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use keld_ipc::codec::{decode, encode};
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::link::{AppLinkDeadlines, handshake_server, read_frame, write_frame};
use keld_ipc::{
    APP_LINK_IO_DEADLINE, IpcError, LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleRequest,
    LifecycleResponse, SessionToken,
};

/// Host side of one app-link lifecycle session.
///
/// After [`Self::handshake`], the caller sends `Ready` / `LastWindowClosed`
/// explicitly — they are never implied by handshake. A 5-second I/O deadline
/// applies to `HELLO` only; the persistent reader then blocks until `Quit`
/// or EOF so a quiet `whenReady` wait is not `KELD-IPC-006`.
pub struct LifecycleSession<W> {
    writer: Arc<Mutex<W>>,
    windows: u32,
    ready: bool,
    quit_rx: Receiver<Result<(), IpcError>>,
    reader: Option<JoinHandle<()>>,
}

impl<W> core::fmt::Debug for LifecycleSession<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LifecycleSession")
            .field("windows", &self.windows)
            .field("ready", &self.ready)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send + 'static> LifecycleSession<W> {
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
        W: AppLinkDeadlines,
    {
        reader.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
        writer.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
        handshake_server(&mut reader, token)?;
        // Persistent session: a quiet child waiting on Ready is expected.
        // `read_frame` cannot retry after Timeout (partial consume), so the
        // handshake deadline must not remain on the reader.
        reader.set_app_link_deadlines(None)?;
        writer.set_app_link_deadlines(None)?;

        let writer = Arc::new(Mutex::new(writer));
        let writer_for_reader = Arc::clone(&writer);
        let (quit_tx, quit_rx) = mpsc::channel();
        let reader_handle = thread::spawn(move || {
            let outcome = read_until_quit(reader, &writer_for_reader);
            let _ = quit_tx.send(outcome);
        });

        Ok(Self {
            writer,
            windows: 0,
            ready: false,
            quit_rx,
            reader: Some(reader_handle),
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

impl<W> Drop for LifecycleSession<W> {
    fn drop(&mut self) {
        // Do not join the reader here: it may still be blocked on `read_frame`.
        // Closing the peer (test drop / child exit) unblocks it with EOF.
        let _ = self.reader.take();
    }
}

fn read_until_quit<R, W>(mut reader: R, writer: &Mutex<W>) -> Result<(), IpcError>
where
    R: Read,
    W: Write,
{
    loop {
        let (header, payload) = match read_frame(&mut reader) {
            Ok(frame) => frame,
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
    use super::*;
    use keld_ipc::codec::decode;
    use keld_ipc::link::{handshake_client, read_frame, write_frame};
    use keld_ipc::{IpcError, LifecycleEvent, LifecycleRequest};

    #[allow(clippy::expect_used)]
    fn test_token() -> SessionToken {
        SessionToken::from_bytes([0x72; 32])
    }

    #[cfg(unix)]
    type Stream = std::os::unix::net::UnixStream;
    #[cfg(windows)]
    type Stream = std::net::TcpStream;

    #[cfg(unix)]
    #[allow(clippy::expect_used)]
    fn connected_pair() -> (Stream, Stream) {
        std::os::unix::net::UnixStream::pair().expect("unix pair")
    }

    #[cfg(windows)]
    #[allow(clippy::expect_used)]
    fn connected_pair() -> (Stream, Stream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let accept = std::thread::spawn(move || listener.accept().expect("accept").0);
        let client = std::net::TcpStream::connect(addr).expect("connect");
        let server = accept.join().expect("accept thread");
        (client, server)
    }

    #[allow(clippy::expect_used)]
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
}
