//! Postcard payload encoding (hot-path codec per spec §2).

#![forbid(unsafe_code)]

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

/// Count the canonical postcard encoding without allocating output storage.
///
/// Callers that own a bounded writer position reserve it before this pass, check
/// the returned length against that position's limit, then use [`encode_into`]
/// with exactly that much storage. Counting is not successful delivery and does
/// not replace the final bounded encoding check if the value can change.
///
/// # Errors
///
/// Returns [`IpcError::Codec`] when the same postcard serializer rejects `value`.
pub fn encoded_len<T: serde::Serialize + ?Sized>(value: &T) -> Result<usize, IpcError> {
    postcard::serialize_with_flavor(value, postcard::ser_flavors::Size::default())
        .map_err(IpcError::Codec)
}

/// Encodes into caller-owned storage and returns only its used prefix.
///
/// Uses the same postcard schema as [`encode`], without allocating an output
/// buffer. An error may leave a partial encoding in `buffer`; callers must not
/// transmit that storage unless this function succeeds.
///
/// # Errors
///
/// Returns [`IpcError::Codec`] if serialization fails or `buffer` is too small.
pub fn encode_into<'buffer, T: serde::Serialize + ?Sized>(
    value: &T,
    buffer: &'buffer mut [u8],
) -> Result<&'buffer [u8], IpcError> {
    let used = postcard::to_slice(value, buffer).map_err(IpcError::Codec)?;
    Ok(used)
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
    fn caller_storage_preserves_bytes_address_and_unwritten_tail() {
        let request = EchoRequest {
            message: "kipc".to_owned(),
            count: 3,
        };
        let mut storage = [0xa5; 32];
        let address = storage.as_ptr();
        let used = encode_into(&request, &mut storage).expect("encode in caller storage");
        assert_eq!(used, &[0x04, b'k', b'i', b'p', b'c', 0x03]);
        assert_eq!(used.as_ptr(), address);
        assert_eq!(used, encode(&request).expect("same owning codec"));
        assert_eq!(&storage[6..], &[0xa5; 26]);
    }

    #[test]
    fn caller_storage_exhaustion_is_typed_and_storage_can_be_reused() {
        let mut storage = [0; 4];
        assert!(matches!(
            encode_into(&u32::MAX, &mut storage),
            Err(IpcError::Codec(postcard::Error::SerializeBufferFull))
        ));
        assert_eq!(
            encode_into(&0_u32, &mut storage).expect("reuse after error"),
            &[0]
        );
        assert_eq!(
            encode_into("ok", &mut storage).expect("unsized input"),
            &[2, b'o', b'k']
        );
        assert!(matches!(
            encode_into(&1_u8, &mut []),
            Err(IpcError::Codec(postcard::Error::SerializeBufferFull))
        ));
    }

    #[test]
    fn counted_size_matches_canonical_bytes_at_varint_boundaries() {
        for length in [0, 1, 127, 128, 16_383, 16_384] {
            let value = EchoRequest {
                message: "x".repeat(length),
                count: u32::MAX,
            };
            let count = encoded_len(&value).expect("count canonical postcard bytes");
            let expected = encode(&value).expect("existing canonical encoding");
            assert_eq!(count, expected.len());
            let mut exact = vec![0; count].into_boxed_slice();
            assert_eq!(
                encode_into(&value, &mut exact).expect("exact bounded encoding"),
                expected
            );
            assert!(
                encode_into(&value, &mut exact[..count - 1]).is_err(),
                "one byte below the counted boundary refuses"
            );
        }
        assert_eq!(encoded_len(&()).expect("empty canonical value"), 0);
        assert_eq!(
            encoded_len("unsized").expect("borrowed value"),
            encode(&"unsized").expect("same codec").len()
        );
    }

    #[test]
    fn counted_size_preserves_serializer_refusal() {
        struct Refused;
        impl serde::Serialize for Refused {
            fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("fixture serialization refusal"))
            }
        }
        assert!(encoded_len(&Refused).is_err());
        assert!(encode(&Refused).is_err());
        assert!(encode_into(&Refused, &mut [0; 8]).is_err());
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
