//! kipc — Keld's IPC plane.
//!
//! This crate owns the wire protocol between the Keld host, the app process,
//! and webviews. Normative spec: `docs/architecture/02-ipc.md`.
//!
//! Design constraints (do not violate without a spec change):
//! - Hot paths are allocation-free state machines; no async runtime here.
//! - Frames are little-endian, fixed 16-byte header, versioned at handshake.
//! - Channel names never travel per-call; they resolve to `ChannelId` handles.

use std::io::ErrorKind;
use std::time::Duration;

pub mod call_error;
pub mod codec;
pub mod echo;
pub mod frame;
pub mod link;
pub mod session;
pub mod token;

pub use call_error::CallError;
pub use echo::{ECHO_CHANNEL, EchoRequest, EchoResponse};
pub use frame::{ChannelId, CorrelationId, FrameHeader, FrameKind, HeaderError};
pub use link::AppLinkDeadlines;
pub use session::{echo_call, serve_echo_session};
pub use token::{SESSION_TOKEN_LEN, SessionToken, format_app_link, parse_app_link};

/// Deadline for one blocking app-link read or write (arch/02 §7).
///
/// v0 sets `SO_RCVTIMEO`/`SO_SNDTIMEO` on the connected stream — not an async
/// timer and not the eventual readiness-driven reader. Expiry is
/// [`IpcError::Timeout`] (`KELD-IPC-006`).
pub const APP_LINK_IO_DEADLINE: Duration = Duration::from_secs(5);

/// Protocol magic: `b"KI"` little-endian.
pub const MAGIC: u16 = u16::from_le_bytes(*b"KI");

/// Current protocol version negotiated in `HELLO`.
///
/// v2 requires a 32-byte session token as the `HELLO` payload (KEL-60).
pub const PROTOCOL_VERSION: u8 = 2;

/// Fixed size of an encoded frame header in bytes.
pub const HEADER_LEN: usize = 16;

/// Maximum control-plane frame payload length in bytes (16 MiB).
///
/// The wire `len` field is a `u32` (~4 GiB), but control-plane frames carry
/// postcard-encoded messages and small `RAW` fallbacks only. Large transfers
/// belong on the bulk plane (shm rings / `keld://`; see `docs/architecture/02-ipc.md`
/// §3). Readers must enforce this cap **before** allocating the payload buffer so
/// a forged header cannot force a multi-GiB allocation.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

/// Errors on the kipc control plane.
#[derive(Debug)]
pub enum IpcError {
    /// OS I/O failure.
    Io(std::io::Error),
    /// Header decode failure.
    Header(HeaderError),
    /// Postcard codec failure.
    Codec(postcard::Error),
    /// Payload length exceeds [`MAX_FRAME_LEN`] (or `u32` on write).
    PayloadTooLarge,
    /// Unexpected frame or state.
    Protocol {
        /// Short explanation for logs/tests.
        detail: &'static str,
    },
    /// `HELLO` session token missing, malformed, or mismatched.
    HelloAuth {
        /// Short explanation for logs/tests. Must not contain the token.
        detail: &'static str,
    },
    /// A blocking read or write exceeded [`APP_LINK_IO_DEADLINE`].
    Timeout,
}

impl core::fmt::Display for IpcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(e) => write!(
                f,
                "KELD-IPC-001: I/O error — {e}. \
                 Check the app-link path is live and the peer has not closed the socket."
            ),
            Self::Header(e) => write!(
                f,
                "KELD-IPC-002: bad frame header — {e}. \
                 Peers must speak kipc v{PROTOCOL_VERSION} with magic 'KI'."
            ),
            Self::Codec(e) => write!(
                f,
                "KELD-IPC-003: codec error — {e}. \
                 Payload must be postcard for the channel's schema type."
            ),
            Self::PayloadTooLarge => write!(
                f,
                "KELD-IPC-004: frame payload exceeds MAX_FRAME_LEN ({MAX_FRAME_LEN} bytes). \
                 Shrink the payload or move large transfers to the bulk plane (shm / keld://)."
            ),
            Self::Protocol { detail } => write!(
                f,
                "KELD-IPC-005: protocol error — {detail}. \
                 Check frame kind and channel match the session contract."
            ),
            Self::HelloAuth { detail } => write!(
                f,
                "KELD-IPC-007: HELLO session token rejected — {detail}. \
                 Mint the token with the host (`keld dev`) into KELD_APP_LINK \
                 as `<endpoint>#<64 hex chars>` and send those 32 bytes as the \
                 HELLO payload; empty or foreign tokens are rejected."
            ),
            Self::Timeout => write!(
                f,
                "KELD-IPC-006: app-link I/O deadline exceeded. \
                 Check the peer is still running and sending kipc frames; \
                 a silent or wedged process will not be waited on forever."
            ),
        }
    }
}

impl core::error::Error for IpcError {}

impl From<std::io::Error> for IpcError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            // Darwin/Linux `SO_RCVTIMEO`: EAGAIN → WouldBlock. Windows: TimedOut.
            // v0 sockets are blocking; WouldBlock after a deadline is not a
            // readiness poll (that reader is not this slice).
            ErrorKind::TimedOut | ErrorKind::WouldBlock => Self::Timeout,
            _ => Self::Io(err),
        }
    }
}

impl From<HeaderError> for IpcError {
    fn from(err: HeaderError) -> Self {
        Self::Header(err)
    }
}

impl From<postcard::Error> for IpcError {
    fn from(err: postcard::Error) -> Self {
        Self::Codec(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::decode;

    fn assert_code_and_fix(err: &IpcError, code: &str, fix: &str) {
        let msg = err.to_string();
        assert!(msg.contains(code), "missing {code} in: {msg}");
        assert!(msg.contains(fix), "missing fix hint `{fix}` in: {msg}");
    }

    #[test]
    fn display_messages_carry_error_codes_and_fix_guidance() {
        assert_code_and_fix(
            &IpcError::Io(std::io::Error::other("boom")),
            "KELD-IPC-001",
            "app-link path",
        );
        assert_code_and_fix(
            &IpcError::from(HeaderError::BadVersion(99)),
            "KELD-IPC-002",
            "kipc v2",
        );
        let codec_err = decode::<EchoRequest>(&[0xff]).expect_err("invalid postcard");
        assert_code_and_fix(&codec_err, "KELD-IPC-003", "postcard");
        assert_code_and_fix(&IpcError::PayloadTooLarge, "KELD-IPC-004", "bulk plane");
        assert_code_and_fix(
            &IpcError::Protocol {
                detail: "expected HELLO from peer",
            },
            "KELD-IPC-005",
            "session contract",
        );
        assert_code_and_fix(
            &IpcError::HelloAuth {
                detail: "HELLO session token mismatch",
            },
            "KELD-IPC-007",
            "KELD_APP_LINK",
        );
        assert_code_and_fix(&IpcError::Timeout, "KELD-IPC-006", "deadline");
        assert_code_and_fix(
            &IpcError::from(std::io::Error::from(ErrorKind::TimedOut)),
            "KELD-IPC-006",
            "silent or wedged",
        );
        assert_code_and_fix(
            &IpcError::from(std::io::Error::from(ErrorKind::WouldBlock)),
            "KELD-IPC-006",
            "silent or wedged",
        );
        assert!(
            !matches!(
                IpcError::from(std::io::Error::from(ErrorKind::UnexpectedEof)),
                IpcError::Timeout
            ),
            "EOF is a close, not a deadline"
        );
    }

    #[test]
    fn magic_is_ki_little_endian() {
        assert_eq!(MAGIC.to_le_bytes(), *b"KI");
        assert_eq!(PROTOCOL_VERSION, 2);
    }
}
