//! Postcard payload encoding (hot-path codec per spec §2).

use postcard::{take_from_bytes, to_allocvec};

use crate::IpcError;

/// Decodes a structured payload from postcard bytes.
///
/// Rejects leftover bytes after `T` so a truncated-or-padded buffer cannot
/// silently decode (postcard `from_bytes` ignores trailing data).
///
/// # Errors
///
/// Returns [`IpcError::Codec`] if the bytes are not valid postcard for `T`,
/// or if bytes remain after `T`.
pub fn decode<T: serde::de::DeserializeOwned>(payload: &[u8]) -> Result<T, IpcError> {
    let (value, rest) = take_from_bytes(payload).map_err(IpcError::Codec)?;
    if rest.is_empty() {
        Ok(value)
    } else {
        Err(IpcError::Codec(postcard::Error::DeserializeBadEncoding))
    }
}

/// Encodes a value to postcard bytes.
///
/// # Errors
///
/// Returns [`IpcError::Codec`] if encoding fails.
pub fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, IpcError> {
    to_allocvec(value).map_err(IpcError::Codec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::echo::EchoRequest;

    #[test]
    fn echo_request_postcard_bytes_are_pinned() {
        // Wire fact: postcard struct-as-tuple, varint string len, UTF-8, varint u32.
        // A field reorder or codec swap (JSON, etc.) must fail this test.
        let req = EchoRequest {
            message: "kipc".to_owned(),
            count: 3,
        };
        let bytes = encode(&req).expect("encode");
        assert_eq!(bytes, [0x04, b'k', b'i', b'p', b'c', 0x03]);
        let decoded: EchoRequest = decode(&bytes).expect("decode");
        assert_eq!(decoded, req);
    }

    #[test]
    fn empty_slice_is_not_a_valid_echo_request() {
        let err = decode::<EchoRequest>(&[]).expect_err("empty payload must not decode");
        let msg = err.to_string();
        assert!(msg.contains("KELD-IPC-003"), "{msg}");
        assert!(matches!(err, IpcError::Codec(_)));
    }

    #[test]
    fn trailing_bytes_are_codec_error() {
        let req = EchoRequest {
            message: "kipc".to_owned(),
            count: 3,
        };
        let mut bytes = encode(&req).expect("encode");
        bytes.push(0x00);
        let err = decode::<EchoRequest>(&bytes).expect_err("trailing bytes must not decode");
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
        assert!(matches!(err, IpcError::Codec(_)));
    }

    #[test]
    fn truncated_string_payload_is_codec_error() {
        // Claims 10-byte string, provides 1 byte.
        let err = decode::<EchoRequest>(&[0x0A, b'x']).expect_err("truncated postcard");
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    }
}
