//! Echo command — first typed kipc vertical slice (KEL-30).

use serde::{Deserialize, Serialize};

use crate::IpcError;
use crate::codec::{decode, encode};
use crate::frame::ChannelId;

/// Channel handle for the echo command (resolved at handshake in later versions).
pub const ECHO_CHANNEL: ChannelId = ChannelId(1);

/// Echo request payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoRequest {
    /// Message to echo back.
    pub message: String,
    /// Repeat count metadata (demonstrates structured fields).
    pub count: u32,
}

/// Echo response payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoResponse {
    /// Echoed message.
    pub message: String,
    /// Echoed count.
    pub count: u32,
}

/// Handles one echo call payload and returns the reply bytes.
///
/// # Errors
///
/// Returns [`IpcError::Codec`] if the request cannot be decoded or the response encoded.
pub fn handle_echo(payload: &[u8]) -> Result<Vec<u8>, IpcError> {
    let req: EchoRequest = decode(payload)?;
    let res = EchoResponse {
        message: req.message,
        count: req.count,
    };
    encode(&res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_roundtrip_copies_fields_not_input_bytes() {
        let req = EchoRequest {
            message: "kipc".to_owned(),
            count: 3,
        };
        let mut bytes = encode(&req).expect("encode");
        bytes.push(0x00); // trailing garbage a no-op handler would return unchanged
        let err = handle_echo(&bytes)
            .expect_err("decode must reject leftover postcard bytes (stricter than from_bytes)");
        assert!(
            err.to_string().contains("KELD-IPC-003"),
            "expected codec error, got {err}"
        );

        let clean = encode(&req).expect("encode");
        let out = handle_echo(&clean).expect("handle");
        let res: EchoResponse = decode(&out).expect("decode");
        assert_eq!(res.message, "kipc");
        assert_eq!(res.count, 3);
        assert_eq!(out, encode(&res).expect("re-encode"));
        assert_ne!(out, bytes, "trailing-byte input must not be echoed as-is");
    }

    #[test]
    fn empty_message_and_zero_count_roundtrip() {
        let req = EchoRequest {
            message: String::new(),
            count: 0,
        };
        let out = handle_echo(&encode(&req).expect("encode")).expect("handle");
        let res: EchoResponse = decode(&out).expect("decode");
        assert_eq!(res.message, "");
        assert_eq!(res.count, 0);
    }

    #[test]
    fn unicode_and_max_count_roundtrip() {
        let req = EchoRequest {
            message: "héllo 🦀".to_owned(),
            count: u32::MAX,
        };
        let out = handle_echo(&encode(&req).expect("encode")).expect("handle");
        let res: EchoResponse = decode(&out).expect("decode");
        assert_eq!(res.message, "héllo 🦀");
        assert_eq!(res.count, u32::MAX);
    }

    #[test]
    fn empty_payload_is_codec_error_not_ok() {
        // A no-op `handle_echo` that returns the input would yield Ok(vec![]).
        let err = handle_echo(&[]).expect_err("empty CALL payload is not a valid EchoRequest");
        assert!(matches!(err, IpcError::Codec(_)));
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    }

    #[test]
    fn invalid_postcard_is_codec_error() {
        let err = handle_echo(&[0xff, 0xff]).expect_err("garbage must not echo");
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    }
}
