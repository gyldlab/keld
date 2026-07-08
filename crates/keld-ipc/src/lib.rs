//! kipc — Keld's IPC plane.
//!
//! This crate owns the wire protocol between the Keld host, the app process,
//! and webviews. Normative spec: `docs/architecture/02-ipc.md`.
//!
//! Design constraints (do not violate without a spec change):
//! - Hot paths are allocation-free state machines; no async runtime here.
//! - Frames are little-endian, fixed 16-byte header, versioned at handshake.
//! - Channel names never travel per-call; they resolve to `ChannelId` handles.

pub mod frame;

pub use frame::{ChannelId, CorrelationId, FrameHeader, FrameKind, HeaderError};

/// Protocol magic: `b"KI"` little-endian.
pub const MAGIC: u16 = u16::from_le_bytes(*b"KI");

/// Current protocol version negotiated in `HELLO`.
pub const PROTOCOL_VERSION: u8 = 1;

/// Fixed size of an encoded frame header in bytes.
pub const HEADER_LEN: usize = 16;
