//! Postcard payload of a `FrameKind::Err` reply to a `Call`.

use serde::{Deserialize, Serialize};

/// Application-level failure for one `Call` (guard deny, later handler errors).
///
/// This is not [`crate::IpcError`]: the session stays up and the peer gets a
/// correlated `Err` frame. v0 privileged deny fills `code` with `KELD-GUARD*`
/// and `message` with the full `DenyReason` display text (code + fix). Frame
/// layout and [`crate::PROTOCOL_VERSION`] are unchanged — `Err` was already
/// kind `3`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallError {
    /// Stable `KELD-*` code (e.g. `KELD-GUARD001`).
    pub code: String,
    /// Display text including the fix sentence.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode, encode};

    #[test]
    fn call_error_postcard_bytes_are_pinned() {
        // Wire fact: postcard struct-as-tuple, varint string lens, UTF-8.
        // A field reorder or codec swap must fail this test.
        let err = CallError {
            code: "KELD-GUARD001".to_owned(),
            message: "x".to_owned(),
        };
        let bytes = encode(&err).expect("encode");
        assert_eq!(
            bytes,
            [
                0x0d, b'K', b'E', b'L', b'D', b'-', b'G', b'U', b'A', b'R', b'D', b'0', b'0', b'1',
                0x01, b'x'
            ]
        );
        let decoded: CallError = decode(&bytes).expect("decode");
        assert_eq!(decoded, err);
    }

    #[test]
    fn empty_slice_is_not_a_valid_call_error() {
        let err = decode::<CallError>(&[]).expect_err("empty payload must not decode");
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    }
}
