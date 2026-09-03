//! Framed read/write on a byte stream (app-link control plane v0).

#![forbid(unsafe_code)]

#[cfg(test)]
use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::frame::{ChannelId, CorrelationId, FrameHeader, FrameKind};
use crate::receive::{
    AbsoluteDeadline, ReceivePolicy, ValidatedFrameHeader, validate_received_header,
};
use crate::token::SessionToken;
use crate::{APP_LINK_IO_DEADLINE, HEADER_LEN, IpcError, MAX_FRAME_LEN};

#[cfg(test)]
thread_local! {
    static TEST_READ_ENTRY_WITNESS: RefCell<Option<std::sync::mpsc::Sender<Instant>>> =
        const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_test_read_entry_witness(witness: Option<std::sync::mpsc::Sender<Instant>>) {
    TEST_READ_ENTRY_WITNESS.with(|slot| *slot.borrow_mut() = witness);
}

#[cfg(test)]
fn report_test_read_entry() {
    TEST_READ_ENTRY_WITNESS.with(|slot| {
        if let Some(witness) = slot.borrow().as_ref() {
            let _ = witness.send(Instant::now());
        }
    });
}

/// Connected app-link streams that can bound a blocking read or write.
///
/// Spec: `docs/architecture/02-ipc.md` §7. v0 uses socket timeouts, not an
/// async runtime. On Unix, `SO_RCVTIMEO` / `SO_SNDTIMEO` are socket-wide and
/// shared across `try_clone` fds, so a combined setter cannot clear the
/// reader deadline without also clearing the writer. On Windows those
/// options are per-handle, and `write_timeout()` may also return `None`
/// even when `SO_SNDTIMEO` is set — a getter on a clone is not an oracle.
pub trait AppLinkDeadlines {
    /// Sets read and write timeouts. `None` restores blocking-forever on both.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the OS rejects the timeout. This is NOT rare:
    /// macOS returns `EINVAL` on an accepted socket whose peer has already
    /// closed, while Linux accepts the same call. Callers on an accepted
    /// connection must treat a failure here as a fact about that peer, never
    /// as a listener fault -- see `bootstrap::BootstrapListener::accept_loop`.
    fn set_app_link_deadlines(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_app_link_read_deadline(timeout)?;
        self.set_app_link_write_deadline(timeout)
    }

    /// Sets only `SO_RCVTIMEO`. `None` restores blocking-forever reads.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the OS rejects the timeout.
    fn set_app_link_read_deadline(&self, timeout: Option<Duration>) -> io::Result<()>;

    /// Sets only `SO_SNDTIMEO`. `None` restores blocking-forever writes.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the OS rejects the timeout.
    fn set_app_link_write_deadline(&self, timeout: Option<Duration>) -> io::Result<()>;

    /// Returns the current read timeout (`SO_RCVTIMEO`).
    ///
    /// `None` means blocking-forever, or that this platform does not
    /// round-trip the option through the getter (std: some platforms do
    /// not provide access to the current timeout).
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the OS rejects the query.
    fn app_link_read_deadline(&self) -> io::Result<Option<Duration>>;

    /// Returns the current write timeout (`SO_SNDTIMEO`).
    ///
    /// See [`Self::app_link_read_deadline`]: a `None` getter is not proof
    /// the send deadline was cleared.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the OS rejects the query.
    fn app_link_write_deadline(&self) -> io::Result<Option<Duration>>;

    /// Shuts down both directions of the connected stream.
    ///
    /// Unblocks a **peer** `read_frame` / `write_frame` (FIN / I/O error).
    /// Closing one cloned fd is not enough while another clone still holds
    /// the connection open.
    ///
    /// On Unix, shutdown of a cloned fd also unblocks a local `read` on
    /// another clone of the same socket. On Windows, `TcpStream::shutdown`
    /// does **not** wake a blocking `read` already in progress on another
    /// thread ([rust-lang/rust#121594](https://github.com/rust-lang/rust/issues/121594))
    /// — clone-shutdown is not peer-FIN. Local teardown must use
    /// [`read_frame_interruptible`] with a bounded `SO_RCVTIMEO`.
    ///
    /// # Errors
    ///
    /// Returns [`io::Error`] if the OS rejects the shutdown.
    fn shutdown_app_link(&self) -> io::Result<()>;
}

#[cfg(unix)]
impl AppLinkDeadlines for std::os::unix::net::UnixStream {
    fn set_app_link_read_deadline(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(timeout)
    }

    fn set_app_link_write_deadline(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_write_timeout(timeout)
    }

    fn app_link_read_deadline(&self) -> io::Result<Option<Duration>> {
        self.read_timeout()
    }

    fn app_link_write_deadline(&self) -> io::Result<Option<Duration>> {
        self.write_timeout()
    }

    fn shutdown_app_link(&self) -> io::Result<()> {
        self.shutdown(Shutdown::Both)
    }
}

impl AppLinkDeadlines for std::net::TcpStream {
    fn set_app_link_read_deadline(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_read_timeout(timeout)
    }

    fn set_app_link_write_deadline(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.set_write_timeout(timeout)
    }

    fn app_link_read_deadline(&self) -> io::Result<Option<Duration>> {
        self.read_timeout()
    }

    fn app_link_write_deadline(&self) -> io::Result<Option<Duration>> {
        self.write_timeout()
    }

    fn shutdown_app_link(&self) -> io::Result<()> {
        self.shutdown(Shutdown::Both)
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

/// Reads one kipc frame admissible for `policy` (spec kel133 §3 criteria 1–5).
///
/// Stage order is the contract: header syntax (`KELD-IPC-002`), envelope cap
/// (`KELD-IPC-004`), then the shared semantic validator (`KELD-IPC-005`) —
/// all **before** the payload buffer is allocated or read. On a semantic
/// rejection the payload bytes remain unconsumed; the failure action for
/// every `005` row is closing the link, so nothing may read them afterwards.
///
/// After any error the stream is unusable: close the link; do not retry.
///
/// # Errors
///
/// Returns [`IpcError`] on I/O failure, bad header, oversized payload,
/// semantic rejection, or deadline expiry mapped by the stream's timeout.
pub fn read_validated_frame<S: Read>(
    stream: &mut S,
    policy: &ReceivePolicy,
) -> Result<(ValidatedFrameHeader, Vec<u8>), IpcError> {
    // One stall clock across header and payload: the first byte starts it and
    // partial reads cannot renew it (spec kel133 §3 criterion 7). `read_exact`
    // alone would let a byte-drip peer renew the per-receive `SO_RCVTIMEO`
    // forever — the ordinary-reader gap architecture 02 §7 recorded.
    let mut stall_deadline = None;
    let mut header_bytes = [0u8; HEADER_LEN];
    read_exact_stalled(stream, &mut header_bytes, &mut stall_deadline)?;
    let header = FrameHeader::decode(&header_bytes)?;
    let len = usize::try_from(header.len).map_err(|_| IpcError::PayloadTooLarge)?;
    ensure_payload_len(len)?;
    let validated = validate_received_header(policy, header)?;
    let mut payload = vec![0u8; len];
    if !payload.is_empty() {
        read_exact_stalled(stream, &mut payload, &mut stall_deadline)?;
    }
    Ok((validated, payload))
}

#[cfg(test)]
fn read_validated_frame_with_stall<S: Read>(
    stream: &mut S,
    policy: &ReceivePolicy,
    stall_limit: Duration,
) -> Result<(ValidatedFrameHeader, Vec<u8>), IpcError> {
    let mut stall_deadline = None;
    let mut header_bytes = [0u8; HEADER_LEN];
    read_exact_stalled_with(stream, &mut header_bytes, &mut stall_deadline, stall_limit)?;
    let header = FrameHeader::decode(&header_bytes)?;
    let len = usize::try_from(header.len).map_err(|_| IpcError::PayloadTooLarge)?;
    ensure_payload_len(len)?;
    let validated = validate_received_header(policy, header)?;
    let mut payload = vec![0u8; len];
    if !payload.is_empty() {
        read_exact_stalled_with(stream, &mut payload, &mut stall_deadline, stall_limit)?;
    }
    Ok((validated, payload))
}

/// Fills `buf`, sharing one started-frame stall clock across calls.
///
/// Each blocking wait is individually bounded by the stream's own read
/// deadline (`SO_RCVTIMEO`); this adds the frame-wide bound: once the first
/// byte arrives, later partial reads that land past
/// `first_byte + APP_LINK_IO_DEADLINE` are `KELD-IPC-006` instead of renewing
/// the per-receive clock. Exact-instant expiry needs the pollable
/// interruptible reader (tolerance one `APP_LINK_READER_POLL`). This generic
/// reader cannot shrink the socket timeout and consults the stall clock only
/// between partial reads, so its exact bound is
/// `first_byte + APP_LINK_IO_DEADLINE + one SO_RCVTIMEO` — 10 s with the
/// default 5 s + 5 s — and with no socket timeout the stall clock is only
/// observed when a byte arrives.
fn read_exact_stalled<S: Read>(
    stream: &mut S,
    buf: &mut [u8],
    stall_deadline: &mut Option<Instant>,
) -> Result<(), IpcError> {
    read_exact_stalled_with(stream, buf, stall_deadline, APP_LINK_IO_DEADLINE)
}

fn read_exact_stalled_with<S: Read>(
    stream: &mut S,
    buf: &mut [u8],
    stall_deadline: &mut Option<Instant>,
    stall_limit: Duration,
) -> Result<(), IpcError> {
    let mut filled = 0;
    while filled < buf.len() {
        if stall_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(IpcError::Timeout);
        }
        match stream.read(&mut buf[filled..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "failed to fill whole buffer",
                )
                .into());
            }
            Ok(n) => {
                if stall_deadline.is_none() {
                    match Instant::now().checked_add(stall_limit) {
                        Some(deadline) => *stall_deadline = Some(deadline),
                        None => return Err(IpcError::Timeout),
                    }
                }
                filled += n;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Reads one kipc frame, retrying idle `SO_RCVTIMEO` until `stop` is set.
///
/// Unlike [`read_frame`], an idle timeout (`WouldBlock` / `TimedOut` with
/// the read deadline and **no bytes of this frame yet**) is **not**
/// [`IpcError::Timeout`]: the wait continues so a quiet persistent reader
/// is not `KELD-IPC-006`.
///
/// After the first byte of a frame, short poll timeouts still retry so a
/// `SO_RCVTIMEO` interval cannot desynchronize the stream, but the rest of
/// that frame (header remainder and payload) must complete within
/// [`APP_LINK_IO_DEADLINE`]. A peer that writes a partial header — or a
/// header then stalls on payload — and stays silent is `KELD-IPC-006`.
/// The stall clock is wall time from the first byte; per-`recv`
/// `SO_RCVTIMEO` resets every syscall and is not an overall frame deadline.
///
/// Returns `Ok(None)` when `stop` is observed. The stream is then only
/// safe to close — partial header/payload bytes may already have been
/// consumed. EOF is still [`IpcError::Io`] with
/// [`io::ErrorKind::UnexpectedEof`].
///
/// The caller MUST set a bounded [`AppLinkDeadlines::set_app_link_read_deadline`]
/// on a **blocking** stream. Non-blocking sockets are unsupported:
/// `WouldBlock` is treated as poll expiry (`SO_RCVTIMEO` on a blocking
/// socket), so a non-blocking fd idle-spins until `stop` and, after the
/// first byte, busy-waits until the stall deadline. Without a recv
/// timeout, `stop` is only observed before the first blocking `read`
/// (or after peer-FIN). Win32 `TcpStream::shutdown` on a cloned handle
/// does not wake that `read`
/// ([rust-lang/rust#121594](https://github.com/rust-lang/rust/issues/121594)).
///
/// After [`IpcError::Timeout`] or [`IpcError::Io`], the stream is
/// unusable: close the link; do not retry on the same stream.
///
/// # Errors
///
/// Returns [`IpcError`] on I/O failure, bad header, oversized payload, or
/// [`IpcError::Timeout`] (`KELD-IPC-006`) when a started frame does not
/// finish within [`APP_LINK_IO_DEADLINE`].
pub fn read_frame_interruptible<S: Read>(
    stream: &mut S,
    stop: &AtomicBool,
) -> Result<Option<(FrameHeader, Vec<u8>)>, IpcError> {
    read_frame_interruptible_with_limits(stream, stop, None, APP_LINK_IO_DEADLINE, None)
}

#[cfg(test)]
fn read_frame_interruptible_with_stall<S: Read>(
    stream: &mut S,
    stop: &AtomicBool,
    stall_limit: Duration,
) -> Result<Option<(FrameHeader, Vec<u8>)>, IpcError> {
    read_frame_interruptible_with_limits(stream, stop, None, stall_limit, None)
}

/// Interruptible [`read_validated_frame`]: idle polls observe `stop`, a
/// started frame keeps the stall clock, and the shared validator rejects a
/// semantically invalid header before the payload is allocated or read.
///
/// # Errors
///
/// As [`read_validated_frame`], plus [`IpcError::Timeout`] when a started
/// frame stalls past [`APP_LINK_IO_DEADLINE`].
pub fn read_validated_frame_interruptible<S: Read>(
    stream: &mut S,
    policy: &ReceivePolicy,
    stop: &AtomicBool,
) -> Result<Option<(ValidatedFrameHeader, Vec<u8>)>, IpcError> {
    mint_validated(
        policy,
        read_frame_interruptible_with_limits(
            stream,
            stop,
            None,
            APP_LINK_IO_DEADLINE,
            Some(policy),
        )?,
    )
}

/// [`read_validated_frame_interruptible`] additionally capped by an absolute
/// admission/session deadline that byte trickle cannot renew (spec kel133 §4
/// deadline model).
pub(crate) fn read_validated_frame_interruptible_until<S: Read>(
    stream: &mut S,
    policy: &ReceivePolicy,
    stop: &AtomicBool,
    deadline: AbsoluteDeadline,
) -> Result<Option<(ValidatedFrameHeader, Vec<u8>)>, IpcError> {
    mint_validated(
        policy,
        read_frame_interruptible_with_limits(
            stream,
            stop,
            Some(deadline.instant()),
            APP_LINK_IO_DEADLINE,
            Some(policy),
        )?,
    )
}

/// Re-mints the typed header for a frame the policy-aware reader already
/// validated pre-payload. The second validation is a handful of fixed-value
/// compares and cannot fail; it exists so [`ValidatedFrameHeader`]'s only
/// constructor stays [`validate_received_header`].
fn mint_validated(
    policy: &ReceivePolicy,
    frame: Option<(FrameHeader, Vec<u8>)>,
) -> Result<Option<(ValidatedFrameHeader, Vec<u8>)>, IpcError> {
    match frame {
        None => Ok(None),
        Some((header, payload)) => Ok(Some((validate_received_header(policy, header)?, payload))),
    }
}

fn read_frame_interruptible_with_limits<S: Read>(
    stream: &mut S,
    stop: &AtomicBool,
    deadline: Option<Instant>,
    stall_limit: Duration,
    policy: Option<&ReceivePolicy>,
) -> Result<Option<(FrameHeader, Vec<u8>)>, IpcError> {
    let mut stall_deadline = None;
    let mut header_bytes = [0u8; HEADER_LEN];
    if !read_exact_interruptible(
        stream,
        &mut header_bytes,
        stop,
        deadline,
        &mut stall_deadline,
        stall_limit,
    )? {
        return Ok(None);
    }
    let header = FrameHeader::decode(&header_bytes)?;
    let len = usize::try_from(header.len).map_err(|_| IpcError::PayloadTooLarge)?;
    ensure_payload_len(len)?;
    if let Some(policy) = policy {
        // Semantic admission decision before payload allocation (kel133 AC1).
        validate_received_header(policy, header)?;
    }
    let mut payload = vec![0u8; len];
    if !payload.is_empty()
        && !read_exact_interruptible(
            stream,
            &mut payload,
            stop,
            deadline,
            &mut stall_deadline,
            stall_limit,
        )?
    {
        return Ok(None);
    }
    Ok(Some((header, payload)))
}

/// Fills `buf` from `stream`, retrying idle timeouts until `stop`.
///
/// Returns `Ok(true)` when `buf` is full, `Ok(false)` when `stop` is set.
///
/// `stall_deadline` is shared across the header and payload of one frame.
/// The first read that yields bytes starts it (`now + stall_limit`). Later
/// `TimedOut` / `WouldBlock` after that instant are [`IpcError::Timeout`].
fn read_exact_interruptible<S: Read>(
    stream: &mut S,
    buf: &mut [u8],
    stop: &AtomicBool,
    deadline: Option<Instant>,
    stall_deadline: &mut Option<Instant>,
    stall_limit: Duration,
) -> Result<bool, IpcError> {
    let mut filled = 0;
    while filled < buf.len() {
        if stop.load(Ordering::Acquire) {
            return Ok(false);
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(IpcError::Timeout);
        }
        if stall_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(IpcError::Timeout);
        }
        #[cfg(test)]
        report_test_read_entry();
        match stream.read(&mut buf[filled..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "failed to fill whole buffer",
                )
                .into());
            }
            Ok(n) => {
                if stall_deadline.is_none() {
                    match Instant::now().checked_add(stall_limit) {
                        Some(deadline) => *stall_deadline = Some(deadline),
                        None => return Err(IpcError::Timeout),
                    }
                }
                filled += n;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                if stall_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    return Err(IpcError::Timeout);
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
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

fn read_and_verify_hello<S: Read>(
    stream: &mut S,
    token: &SessionToken,
    policy: &ReceivePolicy,
) -> Result<(), IpcError> {
    let (_validated, payload) = read_validated_frame(stream, policy)?;
    verify_hello_token(&payload, token)
}

/// Compares an exactly shaped `HELLO` payload against the host token.
///
/// Shape (kind, flags, channel, correlation, exact 32-byte length) is the
/// shared validator's job and fails `KELD-IPC-005` *before* this runs
/// (spec kel133 §3 criterion 4); this function owns only the constant-time
/// token comparison, so `KELD-IPC-007` is reserved for an exactly shaped
/// foreign token and never discloses the token or link string.
fn verify_hello_token(payload: &[u8], token: &SessionToken) -> Result<(), IpcError> {
    let peer = SessionToken::try_from_slice(payload)?;
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
/// Returns [`IpcError::Protocol`] if the peer sends a wrong frame or `HELLO`
/// shape, [`IpcError::HelloAuth`] for an exactly shaped foreign token, or
/// [`IpcError::Timeout`] if the peer stays silent past the stream deadline.
pub fn handshake_client<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
) -> Result<(), IpcError> {
    write_hello(stream, token)?;
    read_and_verify_hello(stream, token, &ReceivePolicy::client_await_hello())
}

/// Server `HELLO`: read and verify the client's `HELLO`, then write `token`.
///
/// Must not write the session token until the peer proves possession.
/// A `HELLO` whose shape is wrong — non-zero flags/channel/correlation or a
/// payload that is not exactly 32 bytes — is `KELD-IPC-005` from the shared
/// validator; `KELD-IPC-007` is reserved for an exactly shaped foreign token
/// (spec kel133 §3 criterion 4). Either way an unauthorized connector never
/// learns the secret from the host's first frame.
///
/// # Errors
///
/// Returns [`IpcError::Protocol`] on a wrong frame or `HELLO` shape,
/// [`IpcError::HelloAuth`] on an exactly shaped foreign token, or
/// [`IpcError::Timeout`] if the peer stays silent past the stream deadline.
pub fn handshake_server<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
) -> Result<(), IpcError> {
    read_and_verify_hello(stream, token, &ReceivePolicy::server_pre_auth_hello())?;
    write_hello(stream, token)
}

pub(crate) fn handshake_server_interruptible_until<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
    stop: &AtomicBool,
    deadline: AbsoluteDeadline,
) -> Result<bool, IpcError> {
    let policy = ReceivePolicy::server_pre_auth_hello();
    let Some((_validated, payload)) =
        read_validated_frame_interruptible_until(stream, &policy, stop, deadline)?
    else {
        return Ok(false);
    };
    if stop.load(Ordering::Acquire) {
        return Ok(false);
    }
    if deadline.expired() {
        return Err(IpcError::Timeout);
    }
    verify_hello_token(&payload, token)?;
    write_hello(stream, token)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, ErrorKind, Write as _};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
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
        assert!(
            msg.contains("bulk/resource adapter"),
            "missing fix hint in: {msg}"
        );
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
        let err =
            handshake_client(&mut client, &test_token()).expect_err("31-byte HELLO must fail");
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

    #[test]
    fn read_and_write_deadlines_are_independent() {
        // Getter round-trip is not the Windows oracle: `write_timeout()` on
        // Win32 may return `None` even when `SO_SNDTIMEO` is set (std:
        // some platforms do not provide access to the current timeout).
        // Prove the send deadline still fires after a reader-only clear.
        let (client, mut server) = connected_pair();
        server
            .set_app_link_deadlines(Some(Duration::from_millis(200)))
            .expect("both");
        server
            .set_app_link_read_deadline(None)
            .expect("clear recv only");
        if let Some(reported) = server.app_link_write_deadline().expect("query snd") {
            assert_eq!(
                reported,
                Duration::from_millis(200),
                "when the getter round-trips, it must still show the send deadline"
            );
        }
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let payload = vec![0u8; 64 * 1024];
            let result = loop {
                match write_frame(
                    &mut server,
                    FrameKind::Ping,
                    0,
                    ChannelId(0),
                    CorrelationId(1),
                    &payload,
                ) {
                    Ok(()) => {}
                    Err(err) => break err,
                }
            };
            let _ = done_tx.send(result);
        });
        let result = done_rx.recv_timeout(Duration::from_secs(2)).expect(
            "clearing SO_RCVTIMEO must leave SO_SNDTIMEO in effect; \
             hang means the send deadline was cleared",
        );
        drop(client);
        assert!(
            matches!(result, IpcError::Timeout),
            "reader-only clear must not disable the send deadline, got {result}"
        );
        assert!(result.to_string().contains("KELD-IPC-006"), "{result}");
    }

    #[test]
    fn write_frame_non_reading_peer_is_ipc_006() {
        let (client, mut server) = connected_pair();
        server
            .set_app_link_write_deadline(Some(Duration::from_millis(200)))
            .expect("send deadline");
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let payload = vec![0u8; 64 * 1024];
            let result = loop {
                match write_frame(
                    &mut server,
                    FrameKind::Ping,
                    0,
                    ChannelId(0),
                    CorrelationId(1),
                    &payload,
                ) {
                    Ok(()) => {}
                    Err(err) => break err,
                }
            };
            let _ = done_tx.send(result);
        });
        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("non-reading peer must hit SO_SNDTIMEO, not hang");
        drop(client);
        assert!(
            matches!(result, IpcError::Timeout),
            "full send buffer must be KELD-IPC-006, got {result}"
        );
        assert!(result.to_string().contains("KELD-IPC-006"), "{result}");
    }

    #[test]
    fn shutdown_app_link_unblocks_blocked_read() {
        let (mut reader, writer) = connected_pair();
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = read_frame(&mut reader);
            let _ = done_tx.send(result);
        });
        writer
            .shutdown_app_link()
            .expect("shutdown both directions");
        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("shutdown must unblock read_frame; timeout means the read leaked");
        let err = result.expect_err("shutdown is not a frame");
        assert!(
            matches!(err, IpcError::Io(ref e) if matches!(
                e.kind(),
                ErrorKind::UnexpectedEof
                    | ErrorKind::ConnectionReset
                    | ErrorKind::BrokenPipe
                    | ErrorKind::ConnectionAborted
            )),
            "shutdown must surface as I/O close, got {err}"
        );
        assert!(err.to_string().contains("KELD-IPC-001"), "{err}");
    }

    #[test]
    fn interruptible_read_stops_on_flag_without_peer_close() {
        // Win32 clone-shutdown does not wake a local blocking read
        // (rust-lang/rust#121594). The stop flag + bounded SO_RCVTIMEO is
        // the local wakeup; this must not depend on the peer sending FIN.
        let (mut reader, _writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(50)))
            .expect("poll");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_reader = Arc::clone(&stop);
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = started_tx.send(());
            let result = read_frame_interruptible(&mut reader, stop_for_reader.as_ref());
            let _ = done_tx.send(result);
        });
        started_rx.recv().expect("reader entered");
        stop.store(true, Ordering::Release);
        let result = done_rx.recv_timeout(Duration::from_secs(2)).expect(
            "stop flag must unblock the reader within the poll interval; \
             timeout means the read leaked (clone-shutdown is not this test)",
        );
        assert!(
            matches!(result, Ok(None)),
            "stop must return Ok(None), not a frame or KELD-IPC-006: {result:?}"
        );
    }

    #[test]
    fn interruptible_read_retries_timeout_mid_header() {
        // Poll interval is not the frame deadline: a second chunk inside
        // APP_LINK_IO_DEADLINE must still assemble. A stall past that
        // deadline is interruptible_read_stalled_mid_header_is_ipc_006.
        let (mut reader, mut writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(50)))
            .expect("poll");
        let stop = AtomicBool::new(false);
        let header = header_with_len(0);
        writer.write_all(&header[..8]).expect("partial header");
        writer.flush().expect("flush");

        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = read_frame_interruptible(&mut reader, &stop);
            let _ = done_tx.send(result);
        });

        let early = done_rx.recv_timeout(Duration::from_millis(400));
        assert!(
            early.is_err(),
            "poll timeout mid-header must retry within APP_LINK_IO_DEADLINE, \
             not surface KELD-IPC-006: {early:?}"
        );

        writer.write_all(&header[8..]).expect("header rest");
        writer.flush().expect("flush rest");
        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("completed header after poll retry");
        let (got, payload) = result.expect("I/O").expect("stop was not set");
        assert_eq!(got.kind, FrameKind::Ping);
        assert!(payload.is_empty());
    }

    #[test]
    fn interruptible_read_delivers_a_frame_when_peer_writes() {
        let (mut reader, mut writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(50)))
            .expect("poll");
        let stop = AtomicBool::new(false);
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = read_frame_interruptible(&mut reader, &stop);
            let _ = done_tx.send(result);
        });
        write_frame(
            &mut writer,
            FrameKind::Ping,
            0,
            ChannelId(7),
            CorrelationId(9),
            b"hi",
        )
        .expect("frame");
        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("frame must arrive");
        let (header, payload) = result.expect("I/O").expect("not stopped");
        assert_eq!(header.kind, FrameKind::Ping);
        assert_eq!(header.channel, ChannelId(7));
        assert_eq!(payload, b"hi");
    }

    #[test]
    fn interruptible_read_idle_does_not_hit_stall_deadline() {
        // Stall clock starts after the first byte, not at read entry.
        let (mut reader, _writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(50)))
            .expect("poll");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_reader = Arc::clone(&stop);
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = read_frame_interruptible_with_stall(
                &mut reader,
                stop_for_reader.as_ref(),
                Duration::from_millis(200),
            );
            let _ = done_tx.send(result);
        });
        let early = done_rx.recv_timeout(Duration::from_millis(400));
        assert!(
            early.is_err(),
            "idle wait must keep retrying; stall deadline is not an idle timeout: {early:?}"
        );
        stop.store(true, Ordering::Release);
        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("stop flag must unblock the idle reader");
        assert!(
            matches!(result, Ok(None)),
            "stop must return Ok(None), got {result:?}"
        );
    }

    #[test]
    fn interruptible_read_stalled_mid_header_is_ipc_006() {
        // Live peer writes a partial header then stays silent. Poll
        // retries must not wait forever: the overall stall deadline after
        // the first byte is KELD-IPC-006. recv_timeout is the hang kill
        // switch, not the assertion.
        let (mut reader, mut writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(50)))
            .expect("poll");
        let stop = AtomicBool::new(false);
        writer
            .write_all(&header_with_len(0)[..8])
            .expect("partial header");
        writer.flush().expect("flush");

        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result =
                read_frame_interruptible_with_stall(&mut reader, &stop, Duration::from_millis(200));
            let _ = done_tx.send(result);
        });

        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("stalled mid-header must hit the overall stall deadline, not hang");
        let err = result.expect_err("partial header + live silent peer is not a frame");
        assert!(
            matches!(err, IpcError::Timeout),
            "deadline must be KELD-IPC-006, got {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-006"), "{msg}");
        assert!(
            !msg.contains("KELD-IPC-001"),
            "must not classify a live stalled peer as EOF: {msg}"
        );
        drop(writer);
    }

    #[test]
    fn interruptible_read_stalled_mid_payload_is_ipc_006() {
        // Complete header, no payload bytes: the stall clock started on
        // the header and must still expire (not reset to an idle wait).
        let (mut reader, mut writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(50)))
            .expect("poll");
        let stop = AtomicBool::new(false);
        writer
            .write_all(&header_with_len(8))
            .expect("complete header claiming 8 payload bytes");
        writer.flush().expect("flush");

        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result =
                read_frame_interruptible_with_stall(&mut reader, &stop, Duration::from_millis(200));
            let _ = done_tx.send(result);
        });

        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("stalled mid-payload must hit the overall stall deadline, not hang");
        let err = result.expect_err("header without payload + live peer is not a frame");
        assert!(
            matches!(err, IpcError::Timeout),
            "deadline must be KELD-IPC-006, got {err}"
        );
        assert!(err.to_string().contains("KELD-IPC-006"), "{err}");
        drop(writer);
    }
}

#[cfg(test)]
mod validated_read_tests {
    use super::*;
    use crate::receive::ReceivePolicy;

    fn frame_bytes(
        kind: FrameKind,
        flags: u16,
        channel: u16,
        corr: u32,
        payload: &[u8],
    ) -> Vec<u8> {
        let header = FrameHeader {
            kind,
            flags,
            channel: ChannelId(channel),
            corr: CorrelationId(corr),
            len: u32::try_from(payload.len()).expect("test payload fits"),
        };
        let mut bytes = header.encode().to_vec();
        bytes.extend_from_slice(payload);
        bytes
    }

    /// Spec kel133 §3 criterion 1: the semantic validator rejects before the
    /// payload buffer is allocated or read. The cursor position is the
    /// falsifiable oracle — validating after the payload read would consume
    /// the payload bytes and move the cursor past `HEADER_LEN`.
    #[test]
    fn semantic_rejection_happens_before_the_payload_is_read() {
        let policy = ReceivePolicy::echo_receiver();
        let hostile = frame_bytes(FrameKind::Call, 0, 1, 0, &[0xAA; 64]); // corr 0
        let mut cursor = std::io::Cursor::new(hostile);
        let err =
            read_validated_frame(&mut cursor, &policy).expect_err("CALL corr 0 must not admit");
        assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
        assert_eq!(
            cursor.position(),
            HEADER_LEN as u64,
            "payload bytes must remain unread on semantic rejection"
        );
    }

    /// Criterion order: the envelope cap (`KELD-IPC-004`) fires before the
    /// semantic validator even when the header is also semantically invalid.
    #[test]
    fn envelope_cap_precedes_semantic_validation() {
        let policy = ReceivePolicy::echo_receiver();
        let mut header = FrameHeader {
            kind: FrameKind::Call,
            flags: crate::frame::FLAG_RAW, // also semantically invalid
            channel: ChannelId(9),         // also wrong channel
            corr: CorrelationId(0),        // also reserved correlation
            len: u32::try_from(MAX_FRAME_LEN).expect("cap fits u32") + 1,
        }
        .encode()
        .to_vec();
        header.extend_from_slice(&[0u8; 4]);
        let mut cursor = std::io::Cursor::new(header);
        let err = read_validated_frame(&mut cursor, &policy)
            .expect_err("oversized envelope must not admit");
        assert!(err.to_string().contains("KELD-IPC-004"), "{err}");
    }

    /// The valid structured CALL still admits and returns its exact payload.
    #[test]
    fn valid_call_admits_with_payload() {
        let policy = ReceivePolicy::echo_receiver();
        let payload = crate::codec::encode(&crate::EchoRequest {
            message: "kipc".to_owned(),
            count: 2,
        })
        .expect("encode");
        let bytes = frame_bytes(FrameKind::Call, 0, 1, 7, &payload);
        let mut cursor = std::io::Cursor::new(bytes);
        let (validated, got) =
            read_validated_frame(&mut cursor, &policy).expect("valid CALL admits");
        assert_eq!(validated.kind(), FrameKind::Call);
        assert_eq!(validated.corr(), CorrelationId(7));
        assert_eq!(got, payload);
    }

    /// Syntax stays first: a bad header byte is `KELD-IPC-002`, not 005.
    #[test]
    fn header_syntax_precedes_semantic_validation() {
        let policy = ReceivePolicy::echo_receiver();
        let mut bytes = frame_bytes(FrameKind::Call, 0, 1, 7, &[]);
        bytes[0] = b'X'; // bad magic
        let mut cursor = std::io::Cursor::new(bytes);
        let err = read_validated_frame(&mut cursor, &policy).expect_err("bad magic must not admit");
        assert!(err.to_string().contains("KELD-IPC-002"), "{err}");
    }
}

#[cfg(test)]
mod deadline_contract_tests {
    //! Spec kel133 §3 criteria 7–8 over real sockets: a started frame expires
    //! at the earlier of the enclosing session/call deadline and the stall
    //! limit (both orderings), an idle session terminates at its separately
    //! declared deadline with a bounded poll cadence, and shutdown stays
    //! promptly cancellable. Timeout assertions are lower bounds plus generous
    //! upper kill-switch bounds — never sleeps for synchronization.

    use std::sync::atomic::AtomicU32;
    use std::sync::mpsc;
    use std::thread;

    use super::*;
    use crate::receive::ReceivePolicy;

    #[cfg(unix)]
    fn connected_pair() -> (
        std::os::unix::net::UnixStream,
        std::os::unix::net::UnixStream,
    ) {
        std::os::unix::net::UnixStream::pair().expect("unix pair")
    }

    /// The windows-latest acceptance row (spec kel133 §7 row 8) proves the
    /// shipped named-pipe reader clock, not loopback TCP. The reader is the
    /// server end, as in production.
    #[cfg(windows)]
    fn connected_pair() -> (
        crate::bootstrap::WindowsNamedPipeBootstrapStream,
        crate::bootstrap::WindowsNamedPipeBootstrapStream,
    ) {
        crate::bootstrap::connected_named_pipe_pair().expect("named-pipe pair")
    }

    fn hello_header_prefix() -> [u8; 4] {
        let bytes = FrameHeader {
            kind: FrameKind::Hello,
            flags: 0,
            channel: ChannelId(0),
            corr: CorrelationId(0),
            len: 32,
        }
        .encode();
        [bytes[0], bytes[1], bytes[2], bytes[3]]
    }

    /// AC7 ordering A: the enclosing session/call deadline is earlier than
    /// `first_byte + APP_LINK_IO_DEADLINE`; the frame expires at the session
    /// deadline with exact `KELD-IPC-006`. Ignoring the enclosing deadline
    /// would run to the 5 s stall limit and fail the upper bound.
    #[test]
    fn started_frame_expires_at_the_earlier_enclosing_deadline() {
        let (mut reader, mut writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(20)))
            .expect("read poll");
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let peer = thread::spawn(move || {
            writer
                .write_all(&hello_header_prefix())
                .expect("write partial header");
            writer.flush().expect("flush");
            let _ = release_rx.recv(); // hold the socket open; released on exit
        });

        let stop = AtomicBool::new(false);
        let policy = ReceivePolicy::server_pre_auth_hello();
        let session_deadline = Duration::from_millis(200);
        let started = Instant::now();
        let err = read_validated_frame_interruptible_until(
            &mut reader,
            &policy,
            &stop,
            AbsoluteDeadline::at(started + session_deadline),
        )
        .expect_err("a stalled frame must expire at the session deadline");
        let elapsed = started.elapsed();

        assert!(err.to_string().starts_with("KELD-IPC-006"), "{err}");
        assert!(
            elapsed >= session_deadline,
            "expired before the absolute session deadline: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "the earlier session deadline must govern, not the 5 s stall limit: {elapsed:?}"
        );
        drop(release_tx);
        peer.join().expect("peer thread");
    }

    /// AC7 ordering B: the stall limit is earlier than the enclosing
    /// deadline; the frame expires at `first_byte + stall` with exact
    /// `KELD-IPC-006`. Resetting the stall clock per read would push expiry
    /// toward the 10 s enclosing deadline and fail the upper bound.
    #[test]
    fn started_frame_expires_at_the_earlier_stall_limit() {
        let (mut reader, mut writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(20)))
            .expect("read poll");
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let peer = thread::spawn(move || {
            writer
                .write_all(&hello_header_prefix())
                .expect("write partial header");
            writer.flush().expect("flush");
            let _ = release_rx.recv();
        });

        let stop = AtomicBool::new(false);
        let policy = ReceivePolicy::server_pre_auth_hello();
        let stall = Duration::from_millis(150);
        let started = Instant::now();
        let err = read_frame_interruptible_with_limits(
            &mut reader,
            &stop,
            Some(started + Duration::from_secs(10)),
            stall,
            Some(&policy),
        )
        .expect_err("a stalled frame must expire at the stall limit");
        let elapsed = started.elapsed();

        assert!(err.to_string().starts_with("KELD-IPC-006"), "{err}");
        assert!(
            elapsed >= stall,
            "expired before the started-frame stall limit: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "the earlier stall limit must govern, not the 10 s enclosing deadline: {elapsed:?}"
        );
        drop(release_tx);
        peer.join().expect("peer thread");
    }

    struct CountingReader<S> {
        inner: S,
        reads: std::sync::Arc<AtomicU32>,
    }

    impl<S: Read> Read for CountingReader<S> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            self.inner.read(buf)
        }
    }

    /// AC8: an idle session with no frame byte crosses multiple bounded
    /// reader polls without starting a frame clock, then terminates at its
    /// separately declared session deadline with exact `KELD-IPC-006`. The
    /// read-attempt counter is the cadence observable: a tight `WouldBlock`
    /// retry loop produces orders of magnitude more attempts and fails the
    /// upper bound; a blocking read that never rearms produces too few and
    /// fails the lower bound (or hangs into the kill switch).
    #[test]
    fn idle_session_terminates_at_its_declared_deadline_with_bounded_cadence() {
        let (reader, _writer_keepalive) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(50)))
            .expect("read poll");
        let reads = std::sync::Arc::new(AtomicU32::new(0));
        let mut counting = CountingReader {
            inner: reader,
            reads: std::sync::Arc::clone(&reads),
        };

        let stop = AtomicBool::new(false);
        let policy = ReceivePolicy::echo_receiver();
        let session_deadline = Duration::from_millis(400);
        let started = Instant::now();
        let err = read_validated_frame_interruptible_until(
            &mut counting,
            &policy,
            &stop,
            AbsoluteDeadline::at(started + session_deadline),
        )
        .expect_err("an idle session must terminate at its declared deadline");
        let elapsed = started.elapsed();
        let attempts = reads.load(Ordering::Relaxed);

        assert!(err.to_string().starts_with("KELD-IPC-006"), "{err}");
        assert!(
            elapsed >= session_deadline,
            "terminated before the declared session deadline: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "idle termination must not wait for the stall limit: {elapsed:?}"
        );
        // ~8 polls at 50 ms over 400 ms; generous slack for scheduler jitter,
        // hard bound against busy-spin (a tight retry loop yields thousands).
        assert!(
            (2..=40).contains(&attempts),
            "poll cadence out of bounds: {attempts} read attempts in {elapsed:?}"
        );
    }

    /// The ordinary-reader byte-trickle gap (architecture 02 §7) is closed:
    /// a peer dripping one byte per 20 ms — always inside the per-receive
    /// deadline, which `read_exact` alone would renew forever — hits the
    /// frame-wide stall clock on the plain validated read path.
    #[test]
    fn plain_validated_read_closes_the_byte_trickle_gap() {
        // The per-receive deadline is deliberately LONGER than the stall limit.
        // `IpcError::from` folds `WouldBlock`/`TimedOut` into `Timeout`
        // (lib.rs), so a bare `SO_RCVTIMEO` expiry and a frame-wide stall
        // expiry are the same `KELD-IPC-006` at this level. With a receive
        // deadline shorter than the stall limit, a single missed drip window
        // makes the socket time out first and the test would credit the stall
        // clock for a per-receive expiry (observed on a loaded macOS runner:
        // 006 at 108 ms against a 150 ms stall limit). Ordering them this way
        // makes the stall clock the only thing that can fire, so the assertion
        // below tests the property instead of the scheduler.
        const RECEIVE_DEADLINE: Duration = Duration::from_millis(400);
        let (mut reader, mut writer) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(RECEIVE_DEADLINE))
            .expect("read deadline");
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        // Observable start condition, not a sleep: the drip thread signals
        // after its first byte is written, so the reader never begins with an
        // idle poll that could expire before any byte exists (the plain path
        // treats a pre-first-byte idle expiry as the per-receive deadline).
        let (first_tx, first_rx) = mpsc::channel::<()>();
        let drip = thread::spawn(move || {
            let mut bytes = FrameHeader {
                kind: FrameKind::Hello,
                flags: 0,
                channel: ChannelId(0),
                corr: CorrelationId(0),
                len: 32,
            }
            .encode()
            .to_vec();
            // A complete, valid HELLO (header + 32 token bytes): without the
            // frame-wide stall clock the reader would eventually ADMIT it
            // (~960 ms at 20 ms per byte), so the oracle is admit-vs-006, not
            // elapsed time alone.
            bytes.extend_from_slice(&[0xA5; 32]);
            for (index, byte) in bytes.into_iter().enumerate() {
                if !matches!(stop_rx.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                    return; // stopped, or the test side is gone
                }
                if writer
                    .write_all(&[byte])
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    return;
                }
                if index == 0 {
                    let _ = first_tx.send(());
                }
                thread::sleep(Duration::from_millis(20));
            }
            let _ = stop_rx.recv();
        });
        first_rx.recv().expect("first dripped byte is in flight");

        let policy = ReceivePolicy::server_pre_auth_hello();
        let stall = Duration::from_millis(150);
        let started = Instant::now();
        let err = read_validated_frame_with_stall(&mut reader, &policy, stall)
            .expect_err("a dripping frame must expire at the stall clock");
        let elapsed = started.elapsed();

        assert!(err.to_string().starts_with("KELD-IPC-006"), "{err}");
        assert!(
            elapsed >= stall,
            "expired before the frame-wide stall clock: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "drip renewed the plain read path: {elapsed:?}"
        );
        // Plain-path bound: the stall clock is only consulted between partial
        // reads, so expiry lands within one receive deadline of it.
        assert!(
            elapsed < stall + RECEIVE_DEADLINE + Duration::from_millis(400),
            "plain-path bound is stall + one receive deadline: {elapsed:?}"
        );
        drop(stop_tx);
        drip.join().expect("drip thread");
    }

    /// AC8 shutdown half: an idle wait observes `stop` promptly — it returns
    /// `Ok(None)` long before its 10 s session deadline instead of consuming
    /// a poll budget.
    #[test]
    fn idle_session_shutdown_is_promptly_cancellable() {
        let (reader, _writer_keepalive) = connected_pair();
        reader
            .set_app_link_read_deadline(Some(Duration::from_millis(50)))
            .expect("read poll");
        let mut reader = reader;

        let stop = AtomicBool::new(true); // shutdown already requested
        let policy = ReceivePolicy::echo_receiver();
        let started = Instant::now();
        let outcome = read_validated_frame_interruptible_until(
            &mut reader,
            &policy,
            &stop,
            AbsoluteDeadline::at(started + Duration::from_secs(10)),
        )
        .expect("shutdown is not an error");
        let elapsed = started.elapsed();

        assert!(outcome.is_none(), "stop must yield Ok(None)");
        assert!(
            elapsed < Duration::from_secs(1),
            "shutdown must be observed promptly: {elapsed:?}"
        );
    }
}
