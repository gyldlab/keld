//! Frame header encoding/decoding.
//!
//! Layout (16 bytes, little-endian):
//!
//! ```text
//! magic:u16 | ver:u8 | kind:u8 | flags:u16 | channel:u16 | corr:u32 | len:u32
//! ```

use crate::{HEADER_LEN, MAGIC, PROTOCOL_VERSION};

/// Identifies a channel; resolved from schema names during handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelId(pub u16);

/// Correlates a `Reply`/`Err` with its originating `Call`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CorrelationId(pub u32);

/// Frame kinds carried in the header's `kind` byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    /// Handshake: version + channel table exchange.
    Hello = 0,
    /// Request expecting exactly one `Reply` or `Err`.
    Call = 1,
    /// Successful response to a `Call`.
    Reply = 2,
    /// Failed response to a `Call`.
    Err = 3,
    /// Fire-and-forget notification.
    Event = 4,
    /// Opens a stream on a channel.
    StreamOpen = 5,
    /// One stream chunk (payload may reference the bulk lane).
    StreamChunk = 6,
    /// Graceful end of stream.
    StreamClose = 7,
    /// Cancels an in-flight `Call` or stream.
    Cancel = 8,
    /// Grants flow-control credit on a channel.
    Grant = 9,
    /// Liveness probe.
    Ping = 10,
}

impl FrameKind {
    /// Decodes a kind byte.
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Hello,
            1 => Self::Call,
            2 => Self::Reply,
            3 => Self::Err,
            4 => Self::Event,
            5 => Self::StreamOpen,
            6 => Self::StreamChunk,
            7 => Self::StreamClose,
            8 => Self::Cancel,
            9 => Self::Grant,
            10 => Self::Ping,
            _ => return None,
        })
    }
}

/// Header flag: payload is raw bytes (bulk-lane reference or inline), not codec-encoded.
pub const FLAG_RAW: u16 = 1 << 0;

/// A decoded frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// Frame kind.
    pub kind: FrameKind,
    /// Bit flags (`FLAG_*`).
    pub flags: u16,
    /// Target channel.
    pub channel: ChannelId,
    /// Request/response correlation id (0 for uncorrelated kinds).
    pub corr: CorrelationId,
    /// Payload length in bytes following this header.
    pub len: u32,
}

/// Errors produced when decoding a frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderError {
    /// First two bytes were not the kipc magic.
    BadMagic(u16),
    /// Unsupported protocol version.
    BadVersion(u8),
    /// Unknown frame kind byte.
    BadKind(u8),
}

impl core::fmt::Display for HeaderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadMagic(m) => write!(f, "bad kipc magic: {m:#06x}"),
            Self::BadVersion(v) => write!(f, "unsupported kipc version: {v}"),
            Self::BadKind(k) => write!(f, "unknown kipc frame kind: {k}"),
        }
    }
}

impl core::error::Error for HeaderError {}

impl FrameHeader {
    /// Encodes the header into its 16-byte wire form.
    #[must_use]
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0..2].copy_from_slice(&MAGIC.to_le_bytes());
        out[2] = PROTOCOL_VERSION;
        out[3] = self.kind as u8;
        out[4..6].copy_from_slice(&self.flags.to_le_bytes());
        out[6..8].copy_from_slice(&self.channel.0.to_le_bytes());
        out[8..12].copy_from_slice(&self.corr.0.to_le_bytes());
        out[12..16].copy_from_slice(&self.len.to_le_bytes());
        out
    }

    /// Decodes a header from its 16-byte wire form.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderError`] if the magic, version, or kind byte is invalid.
    pub fn decode(bytes: &[u8; HEADER_LEN]) -> Result<Self, HeaderError> {
        let magic = u16::from_le_bytes([bytes[0], bytes[1]]);
        if magic != MAGIC {
            return Err(HeaderError::BadMagic(magic));
        }
        let version = bytes[2];
        if version != PROTOCOL_VERSION {
            return Err(HeaderError::BadVersion(version));
        }
        let kind = FrameKind::from_u8(bytes[3]).ok_or(HeaderError::BadKind(bytes[3]))?;
        Ok(Self {
            kind,
            flags: u16::from_le_bytes([bytes[4], bytes[5]]),
            channel: ChannelId(u16::from_le_bytes([bytes[6], bytes[7]])),
            corr: CorrelationId(u32::from_le_bytes([
                bytes[8], bytes[9], bytes[10], bytes[11],
            ])),
            len: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_16_bytes() {
        // The wire size is a protocol constant, independent of struct layout.
        assert_eq!(HEADER_LEN, 16);
    }

    #[test]
    fn roundtrip_all_kinds() {
        for kind_byte in 0..=10u8 {
            let kind = FrameKind::from_u8(kind_byte).expect("kind byte in valid range");
            let header = FrameHeader {
                kind,
                flags: FLAG_RAW,
                channel: ChannelId(42),
                corr: CorrelationId(0xDEAD_BEEF),
                len: 1024,
            };
            let decoded = FrameHeader::decode(&header.encode()).expect("roundtrip decode");
            assert_eq!(decoded, header);
        }
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = FrameHeader {
            kind: FrameKind::Ping,
            flags: 0,
            channel: ChannelId(0),
            corr: CorrelationId(0),
            len: 0,
        }
        .encode();
        bytes[0] = b'X';
        assert!(matches!(
            FrameHeader::decode(&bytes),
            Err(HeaderError::BadMagic(_))
        ));
    }

    #[test]
    fn rejects_unknown_kind() {
        let mut bytes = FrameHeader {
            kind: FrameKind::Ping,
            flags: 0,
            channel: ChannelId(0),
            corr: CorrelationId(0),
            len: 0,
        }
        .encode();
        bytes[3] = 200;
        assert_eq!(FrameHeader::decode(&bytes), Err(HeaderError::BadKind(200)));
    }
}
