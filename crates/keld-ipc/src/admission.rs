//! Platform-neutral admission rejection taxonomy for host bootstrap listeners.
//!
//! A bootstrap listener admits exactly one authenticated peer. When the caller
//! supplies a [`BootstrapRejectionObserver`], every peer that fails before
//! authentication is recorded, redacted, so the host can see *why* admission is
//! not completing without ever learning the token, the endpoint, or raw parser
//! detail. Callers that pass no observer still classify each failure and then
//! discard the record; the guarantee is about classification, not storage.
//!
//! This module is deliberately transport- and platform-neutral. The Unix
//! listener in [`crate::bootstrap`] and the Windows named-pipe listener
//! specified in `docs/specs/kel101-windows-named-pipe-dacl.md` record the same
//! classes against the same codes; a second copy of this mapping per transport
//! is the drift that AGENTS.md "one rule, one owner" forbids.
//!
//! The mapping is normative in that spec's §4 admission table and acceptance
//! criterion 4: EOF or non-timeout I/O is `KELD-IPC-001`, a started partial
//! frame that reaches the app-link I/O deadline is `KELD-IPC-006`, a malformed
//! header is `KELD-IPC-002`, an oversized envelope is `KELD-IPC-004`, and a
//! well-formed non-`HELLO` or wrong-shape frame — including nonzero flags or a
//! payload length that is not exactly 32 bytes (kel133 criterion 4) — is
//! `KELD-IPC-005`. None of them is `HelloAuth`; that code is reserved for an
//! exactly shaped foreign token alone.

use crate::IpcError;

/// Redacted reason recorded by the host for an untrusted bootstrap rejection.
///
/// Carries no payload: a rejection record must never be able to disclose the
/// session token, the endpoint, or peer-supplied bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapRejection {
    /// EOF or a non-timeout I/O failure before authentication completed.
    Io,
    /// A malformed frame header: bad magic, an unsupported protocol version, or
    /// an unknown frame-kind byte ([`crate::frame::HeaderError`] in full).
    Header,
    /// A declared payload larger than `MAX_FRAME_LEN`.
    PayloadTooLarge,
    /// A well-formed frame that is not the expected `HELLO`, or one whose
    /// reserved fields are not zero.
    ///
    /// Also carries [`IpcError::Codec`], which this handshake cannot produce —
    /// see [`Self::classify`] for why it is classified rather than dropped.
    Protocol,
    /// A started frame that did not complete before the app-link I/O deadline.
    ///
    /// This is the per-handshake deadline, which is recoverable: the listener
    /// disconnects that peer and keeps accepting. The generation-wide deadline
    /// is terminal and is not a rejection — it ends admission entirely.
    Timeout,
    /// The peer sent an exactly shaped `HELLO` whose 32-byte token is
    /// foreign. Wrong lengths are [`Self::Protocol`] shape failures (kel133
    /// criterion 4), never authentication.
    HelloAuth,
}

impl BootstrapRejection {
    /// Stable error code for host-only logs and tests.
    ///
    /// Never includes the endpoint or token.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Io => "KELD-IPC-001",
            Self::Header => "KELD-IPC-002",
            Self::PayloadTooLarge => "KELD-IPC-004",
            Self::Protocol => "KELD-IPC-005",
            Self::Timeout => "KELD-IPC-006",
            Self::HelloAuth => "KELD-IPC-007",
        }
    }

    /// Classifies a pre-authentication handshake failure into its recorded class.
    ///
    /// Total by construction. Before this existed the accept loop matched only
    /// `HelloAuth` and discarded every other pre-authentication error, so a peer
    /// that failed on a bad header, an oversized envelope, or a partial frame
    /// was indistinguishable from no peer at all.
    ///
    /// [`IpcError::Codec`] cannot be produced by the bootstrap handshake, which
    /// reads a raw 32-byte `HELLO` payload and never decodes a schema type. It
    /// is mapped to [`Self::Protocol`] rather than dropped so that adding a
    /// decoding step later cannot silently reintroduce a swallowed class; the
    /// spec's admission table has no `KELD-IPC-003` row precisely because the
    /// case is unreachable here.
    #[must_use]
    pub const fn classify(error: &IpcError) -> Self {
        match error {
            IpcError::Io(_) => Self::Io,
            IpcError::Header(_) => Self::Header,
            IpcError::PayloadTooLarge => Self::PayloadTooLarge,
            IpcError::Protocol { .. } | IpcError::Codec(_) => Self::Protocol,
            IpcError::Timeout => Self::Timeout,
            IpcError::HelloAuth { .. } => Self::HelloAuth,
        }
    }
}

/// Host-only observer for redacted bootstrap rejection records.
pub trait BootstrapRejectionObserver {
    /// Records one rejected peer without token, endpoint, or raw parser detail.
    fn rejected(&self, rejection: BootstrapRejection);
}

#[cfg(test)]
mod tests {
    use super::BootstrapRejection;
    use crate::IpcError;
    use crate::frame::HeaderError;

    /// The normative mapping from `docs/specs/kel101-windows-named-pipe-dacl.md`
    /// §4 and acceptance criterion 4. If a code here changes, a shipped host
    /// record changes with it, so this table is the contract.
    #[test]
    fn every_pre_auth_failure_maps_to_its_specified_code() {
        let cases: [(IpcError, &str); 7] = [
            (
                IpcError::Io(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
                "KELD-IPC-001",
            ),
            (IpcError::Header(HeaderError::BadMagic(0)), "KELD-IPC-002"),
            (IpcError::Header(HeaderError::BadKind(0xFF)), "KELD-IPC-002"),
            (IpcError::PayloadTooLarge, "KELD-IPC-004"),
            (
                IpcError::Protocol {
                    detail: "expected HELLO",
                },
                "KELD-IPC-005",
            ),
            (IpcError::Timeout, "KELD-IPC-006"),
            (
                IpcError::HelloAuth {
                    detail: "foreign token",
                },
                "KELD-IPC-007",
            ),
        ];

        for (error, expected) in &cases {
            let rejection = BootstrapRejection::classify(error);
            assert_eq!(
                rejection.code(),
                *expected,
                "{error} must be recorded as {expected}"
            );
        }
    }

    /// The spec's §7 test table names "collapse-to-`HelloAuth`" as a mutation
    /// that must fail. This is that negative control: if classification ever
    /// folds the other classes into token failure, this fails.
    #[test]
    fn non_token_failures_are_never_recorded_as_hello_auth() {
        let not_token_failures = [
            IpcError::Io(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
            IpcError::Header(HeaderError::BadMagic(0)),
            IpcError::PayloadTooLarge,
            IpcError::Protocol {
                detail: "not hello",
            },
            IpcError::Timeout,
        ];

        for error in &not_token_failures {
            let rejection = BootstrapRejection::classify(error);
            assert_ne!(
                rejection,
                BootstrapRejection::HelloAuth,
                "{error} must not be recorded as a token failure"
            );
            assert_ne!(rejection.code(), "KELD-IPC-007");
        }
    }

    /// Distinctness matters: an observer that cannot tell a bad header from an
    /// oversized envelope gives the host no more signal than the swallow did.
    #[test]
    fn each_class_has_a_distinct_code() {
        let all = [
            BootstrapRejection::Io,
            BootstrapRejection::Header,
            BootstrapRejection::PayloadTooLarge,
            BootstrapRejection::Protocol,
            BootstrapRejection::Timeout,
            BootstrapRejection::HelloAuth,
        ];
        let mut codes: Vec<&str> = all.iter().map(|r| r.code()).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(before, codes.len(), "rejection codes must be distinct");
    }

    /// `KELD-IPC-003` is a payload-decode failure. The bootstrap handshake never
    /// decodes a payload, so the spec's admission table has no row for it. It
    /// must still classify rather than fall through to a silent drop.
    #[test]
    fn codec_error_classifies_as_protocol_rather_than_being_dropped() {
        let error = IpcError::Codec(postcard::Error::DeserializeBadEncoding);
        assert_eq!(
            BootstrapRejection::classify(&error),
            BootstrapRejection::Protocol
        );
    }
}
