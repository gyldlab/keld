//! `FrameKind::Err` payload — one structured shape for every privileged channel.
//!
//! Spec: `docs/architecture/02-ipc.md` §2 ("ERR payload (v0)"). A privileged
//! broker that fails one `Call` replies with a correlated `Err` frame whose
//! payload is a postcard [`CallError`], written through
//! [`write_call_error`]. A broker MUST NOT invent a per-channel `Err`
//! encoding: the pre-KEL-102 shape was `encode(&err.to_string())`, one bare
//! postcard `String` per broker, which forced every peer to string-parse the
//! `KELD-*` code back out and had already drifted (a test writer shipped a
//! payload with no code in it at all).
//!
//! This is deliberately not [`crate::IpcError`]. `IpcError` is a transport or
//! session fault and tears the session down; a `CallError` leaves the session
//! up and answers exactly one `Call` on the same channel and correlation id.

use std::io::Write;

use keld_guard::DenyReason;
use serde::{Deserialize, Serialize};

use crate::IpcError;
use crate::codec::encode;
use crate::frame::{ChannelId, CorrelationId, FrameKind};
use crate::link::write_frame;

/// Application-level failure of one privileged `Call`.
///
/// `code` is the registered `KELD-*` code from
/// `docs/engineering/keld-error-codes.md`, owned by the crate that produced
/// the failure — `keld_guard::DenyReason::code()` for a policy denial,
/// the broker's own code (e.g. `KELD-NATIVE-001`) for a post-allow OS
/// failure. Peers match on `code`; they MUST NOT parse `message`.
///
/// `message` is that error's full `Display` text, which by
/// `docs/architecture/07-agent-experience.md` §2 already begins with `code`
/// and ends with the imperative fix sentence. Splitting the fix into its own
/// field would require `DenyReason` to expose a code-free body, so v0 keeps
/// the two fields and carries the fix inside `message`.
///
/// Wire note: postcard encodes a struct as its concatenated fields and
/// [`crate::codec::decode`] rejects trailing bytes, so adding a field changes
/// the payload schema. Per `docs/onboarding/04-wire-formats-and-contracts.md`
/// §14 that is a **public-API review gate**, not a
/// [`crate::PROTOCOL_VERSION`] bump — the version gate covers frame layout,
/// `FrameKind`, flags and the handshake (crate `AGENTS.md`). Practical v0
/// consequence: `PROTOCOL_VERSION` is checked at HELLO, before any payload
/// moves, so a mismatched pair completes the handshake and only discovers a
/// payload-shape difference on the first failed `Call`. Acceptable pre-1.0 —
/// nothing is published and the host and `@keld/electron` ship together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallError {
    /// Stable registered `KELD-*` code (e.g. `KELD-GUARD001`).
    pub code: String,
    /// Full `Display` text of the failure, including the fix sentence.
    pub message: String,
}

impl core::fmt::Display for CallError {
    /// Renders the code exactly once.
    ///
    /// A well-formed `CallError` carries a `message` that already begins with
    /// `code`, and is printed verbatim. `message` arrives from a peer and is
    /// not trusted to be well-formed, so one that does not lead with `code`
    /// is prefixed rather than printed alone — the code is never lost.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.message.starts_with(self.code.as_str()) {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.code, self.message)
        }
    }
}

impl core::error::Error for CallError {}

impl From<&DenyReason> for CallError {
    /// The single owner of the guard-denial → wire mapping.
    ///
    /// It has no per-variant knowledge: `keld-guard` owns the variant → code
    /// table (`DenyReason::code()`) and the actionable text (`Display`), both
    /// of which are copied through unchanged. A new `DenyReason` variant
    /// needs no edit here, and no broker may re-derive this mapping.
    fn from(reason: &DenyReason) -> Self {
        Self {
            code: reason.code().to_owned(),
            message: reason.to_string(),
        }
    }
}

/// Writes `error` as the correlated `Err` reply to one privileged `Call`.
///
/// This is the sanctioned `Err` writer for every privileged channel; brokers
/// call it instead of hand-rolling [`encode`] + [`write_frame`], so the
/// payload shape is not a per-broker choice. `channel` and `corr` are the
/// ones from the `Call` being answered — an `Err` on a different correlation
/// id would strand the caller's in-flight request.
///
/// The session stays open: returning here is answering the call, not failing
/// the link.
///
/// # Errors
///
/// Returns [`IpcError`] if the payload cannot be encoded or the frame cannot
/// be written.
pub fn write_call_error<S: Write>(
    stream: &mut S,
    channel: ChannelId,
    corr: CorrelationId,
    error: &CallError,
) -> Result<(), IpcError> {
    let payload = encode(error)?;
    write_frame(stream, FrameKind::Err, 0, channel, corr, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::decode;
    use keld_guard::{Decision, Principal, evaluate, parse_manifest};

    /// Wire fact: postcard struct-as-tuple, varint string lengths, UTF-8, in
    /// declaration order. `KELD-GUARD001` is 13 bytes (`0x0d`), `x` is 1
    /// (`0x01`). A field reorder, a rename to a self-describing codec, or a
    /// third field must fail this.
    const PINNED: [u8; 16] = [
        0x0d, b'K', b'E', b'L', b'D', b'-', b'G', b'U', b'A', b'R', b'D', b'0', b'0', b'1', 0x01,
        b'x',
    ];

    fn pinned_value() -> CallError {
        CallError {
            code: "KELD-GUARD001".to_owned(),
            message: "x".to_owned(),
        }
    }

    #[test]
    fn call_error_postcard_bytes_are_pinned() {
        assert_eq!(encode(&pinned_value()).expect("encode"), PINNED);
    }

    #[test]
    fn pinned_bytes_decode_to_the_pinned_value() {
        // Decoded from the literal vector, not from this codec's own output,
        // so an encoder/decoder pair that drifted together still fails.
        let decoded: CallError = decode(&PINNED).expect("decode pinned bytes");
        assert_eq!(decoded, pinned_value());
    }

    #[test]
    fn empty_slice_is_not_a_valid_call_error() {
        let err = decode::<CallError>(&[]).expect_err("empty payload must not decode");
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    }

    #[test]
    fn trailing_bytes_after_a_call_error_are_rejected() {
        let mut bytes = PINNED.to_vec();
        bytes.push(0x00);
        let err = decode::<CallError>(&bytes).expect_err("trailing bytes must not decode");
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    }

    /// Negative control for the KEL-102 migration: the pre-KEL-102 payload was
    /// one bare postcard `String`. Decoding it as a `CallError` must fail, or
    /// a broker left on the old encoding would go unnoticed.
    ///
    /// Deterministic for every input, not just this one: a bare `String` is
    /// `varint(n) + n bytes`, so `code` consumes the whole remainder and the
    /// decoder then needs a second varint that is not there.
    #[test]
    fn the_pre_kel102_bare_string_payload_is_not_a_valid_call_error() {
        let legacy = encode(
            &"KELD-GUARD001: capability `fs.write` is not granted. \
              Append \"/tmp/x\" to `/app/fs/write` in keld.permissions.jsonc."
                .to_owned(),
        )
        .expect("encode legacy shape");
        let err = decode::<CallError>(&legacy).expect_err("old bare String must not decode");
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    }

    /// The other direction of the same migration: a stale peer decoding the
    /// new payload as a bare `String` must fail loudly rather than return a
    /// plausible string, so a mixed rollout cannot silently mis-report.
    #[test]
    fn a_call_error_does_not_decode_as_a_bare_string() {
        let err = decode::<String>(&PINNED).expect_err("CallError must not decode as String");
        assert!(err.to_string().contains("KELD-IPC-003"), "{err}");
    }

    #[test]
    fn deny_reason_mapping_copies_the_guard_code_and_fix_text() {
        let manifest = parse_manifest("{}").expect("empty manifest");
        let reason = match evaluate(&manifest, Principal::AppProcess, "fs.read", "$APPDATA/x") {
            Decision::Deny(reason) => reason,
            Decision::Allow => unreachable!("empty manifest must deny fs.read"),
        };
        let err = CallError::from(&reason);
        assert_eq!(err.code, "KELD-GUARD001");
        assert_eq!(err.message, reason.to_string());
        assert!(
            err.message.contains("keld.permissions.jsonc"),
            "the fix sentence must survive onto the wire: {}",
            err.message
        );
    }

    /// The contract `Display` relies on and peers depend on: every
    /// `DenyReason`'s text leads with its own code. If `keld-guard`'s
    /// `Display` ever drops the prefix, this fails here rather than silently
    /// producing a wire payload whose `message` no longer names the code.
    #[test]
    fn every_deny_reason_message_starts_with_its_code() {
        let reasons = [
            DenyReason::NotGranted {
                capability: "fs.read".to_owned(),
                json_pointer: "/app/fs/read".to_owned(),
                requested: "/tmp/x".to_owned(),
            },
            DenyReason::OutOfScope {
                capability: "fs.write".to_owned(),
                scope: "/tmp/**".to_owned(),
                json_pointer: "/app/fs/write".to_owned(),
                requested: "/etc/passwd".to_owned(),
            },
            DenyReason::ChannelForbidden {
                channel: "fs".to_owned(),
            },
            DenyReason::NotAppProcess {
                principal: Principal::Webview {
                    id: 1,
                    generation: 1,
                },
            },
            DenyReason::MediaPrincipalRequired {
                capability: "web.camera".to_owned(),
                presented: None,
            },
        ];
        for reason in &reasons {
            let err = CallError::from(reason);
            assert_eq!(err.code, reason.code(), "{reason:?}");
            assert!(
                err.message.starts_with(&err.code),
                "message must lead with the code: {}",
                err.message
            );
            // Display must not print the code twice for a well-formed value.
            assert_eq!(err.to_string(), err.message, "{reason:?}");
        }
    }

    /// `write_call_error`'s whole documented contract is the header it writes:
    /// kind `Err`, flags 0, and the channel + correlation id of the `Call` being
    /// answered. Asserted as literal bytes — writing a different channel or corr
    /// would strand the caller's in-flight request, and no other test reads them.
    #[test]
    fn write_call_error_header_carries_kind_flags_channel_and_corr() {
        let mut out: Vec<u8> = Vec::new();
        write_call_error(
            &mut out,
            ChannelId(2),
            CorrelationId(0x0102_0304),
            &pinned_value(),
        )
        .expect("write");
        let payload_len = u32::try_from(PINNED.len()).expect("payload fits u32");
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"KI"); // magic
        expected.push(crate::PROTOCOL_VERSION); // ver
        expected.push(FrameKind::Err as u8); // kind == 3
        expected.extend_from_slice(&0u16.to_le_bytes()); // flags
        expected.extend_from_slice(&2u16.to_le_bytes()); // channel
        expected.extend_from_slice(&0x0102_0304u32.to_le_bytes()); // corr
        expected.extend_from_slice(&payload_len.to_le_bytes()); // len
        expected.extend_from_slice(&PINNED);
        assert_eq!(out, expected);
    }

    /// A new `DenyReason` variant must be added to `reasons` above: this match has
    /// no `_` arm, so the crate stops compiling until it is. Without this lock the
    /// hand-written array silently under-covers the enum it claims to check.
    #[allow(dead_code, reason = "compile-time exhaustiveness lock, never called")]
    fn deny_reason_variants_are_exhaustive(reason: &DenyReason) {
        match reason {
            DenyReason::NotGranted { .. }
            | DenyReason::OutOfScope { .. }
            | DenyReason::ChannelForbidden { .. }
            | DenyReason::NotAppProcess { .. }
            | DenyReason::MediaPrincipalRequired { .. } => (),
        }
    }

    #[test]
    fn display_prefixes_the_code_when_a_peer_message_omits_it() {
        // Untrusted peer payload: the code must still reach the developer.
        // `KELD-GUARD001` is already emitted by this file's mapping tests, so using it
        // here mints nothing new in a directory the error registry scans.
        let err = CallError {
            code: "KELD-GUARD001".to_owned(),
            message: "capability is not granted".to_owned(),
        };
        assert_eq!(err.to_string(), "KELD-GUARD001: capability is not granted");
    }
}
