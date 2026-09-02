//! Session-aware receiver semantics: one shared validator and absolute clocks.
//!
//! Spec: `docs/specs/kel133-kipc-receiver-semantics.md` §4. This module is the
//! single owner of v0 frame/session *semantic* validation — the rules that say
//! which decoded header is admissible for the receiver state the host selected.
//! Syntax stays in [`crate::frame::FrameHeader::decode`] (`KELD-IPC-002`), the
//! envelope cap stays in the readers (`KELD-IPC-004`, checked before payload
//! allocation), payload codecs stay per channel (`KELD-IPC-003`), and token
//! authentication stays in the HELLO owner (`KELD-IPC-007`). Everything this
//! module rejects is `KELD-IPC-005` with zero handler effect.
//!
//! [`ReceivePolicy`] is host-selected trusted state: no frame chooses its
//! policy, session class, principal, or payload codec. [`ValidatedFrameHeader`]
//! is the only header type a privileged dispatch adapter accepts; it cannot be
//! constructed outside this module.
//!
//! The validator is a synchronous, allocation-free decision over fixed-size
//! values. It does not decode channel payloads, call the guard, write replies,
//! close sockets, or own handler lifecycle. The receiver owns those transitions
//! and records the exact outcome (spec §4 failure/continuation table).

#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

use crate::IpcError;
use crate::frame::{ChannelId, CorrelationId, FrameHeader, FrameKind};

/// Which peer this receiver expects frames from.
///
/// v0 policies use direction for documentation and telemetry; both directions
/// share one semantic table. It is part of the policy so a future asymmetric
/// rule has a home without a second policy type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Host/server side receiving from the app/client peer.
    FromClient,
    /// App/client side receiving from the host/server peer.
    FromServer,
}

/// Authentication phase of the session this receiver serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    /// Before `HELLO` token verification: only `HELLO` shapes are admissible.
    PreAuth,
    /// After `HELLO`: the session's declared structured frames are admissible.
    Authenticated,
}

/// Payload rule the selected policy imposes on an admissible frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadMode {
    /// Declared length must be exactly this many bytes (e.g. the 32-byte
    /// `HELLO` token). A mismatch is `KELD-IPC-005` *shape*, never
    /// `KELD-IPC-007` authentication (spec §3 criterion 4).
    ExactLen(u32),
    /// Declared length must be zero.
    Empty,
    /// Any length within `MAX_FRAME_LEN`; the channel codec decides validity
    /// (`KELD-IPC-003`) after admission.
    Codec,
}

/// Correlation rule the selected policy imposes on an admissible frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedCorrelation {
    /// Correlation must be zero (`HELLO`, `EVENT`).
    Zero,
    /// Correlation must be nonzero (structured `CALL`).
    NonZero,
    /// Correlation must match exactly one outstanding id (reply/err waiters).
    Exactly(CorrelationId),
}

/// Set of frame kinds a policy admits, as a bitmask over `FrameKind` values.
///
/// Kinds outside the set are invalid for the session even though their kind
/// byte is syntactically known: `FrameKind::from_u8` recognising a byte never
/// admits it (spec §4 v0 semantic table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowedKinds(u16);

impl AllowedKinds {
    /// Empty set.
    pub const NONE: Self = Self(0);

    /// Set containing exactly `kind`.
    #[must_use]
    pub const fn only(kind: FrameKind) -> Self {
        Self(1 << (kind as u8))
    }

    /// Union with `kind`.
    #[must_use]
    pub const fn with(self, kind: FrameKind) -> Self {
        Self(self.0 | (1 << (kind as u8)))
    }

    /// Whether `kind` is in the set.
    #[must_use]
    pub const fn contains(self, kind: FrameKind) -> bool {
        self.0 & (1 << (kind as u8)) != 0
    }
}

/// Static semantic contract selected by the host for one receiver state.
///
/// Construct with the named v0 constructors; ad-hoc policies in consumers are
/// the drift this module exists to delete. The `allow_ping` capability admits
/// the live v0 liveness probe (`PING` with flags `0` and an empty payload,
/// channel/correlation echoed by the receiver); it exists because `PING` is a
/// positive pre-KEL-133 vector on the echo, lifecycle, and primary sessions
/// and criterion 11 forbids changing accepted valid bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive] // consumers select a named constructor; ad-hoc policies are the drift this owner deletes
pub struct ReceivePolicy {
    /// Which peer frames are expected from.
    pub direction: Direction,
    /// Authentication phase of the receiver.
    pub phase: SessionPhase,
    /// Declared channel for the structured kinds in `kinds`.
    pub channel: ChannelId,
    /// Payload rule for the structured kinds in `kinds`.
    pub payload: PayloadMode,
    /// Correlation rule for the structured kinds in `kinds`.
    pub expected_corr: ExpectedCorrelation,
    /// Structured frame kinds this policy admits.
    pub kinds: AllowedKinds,
    /// Whether the v0 `PING` liveness probe is admissible on this session.
    pub allow_ping: bool,
    /// Second declared channel for the one live multiplexed session (the
    /// primary app link carries spec-table rows 3 and 5 — echo and lifecycle
    /// `CALL`s — on a single stream). `None` everywhere else; a third channel
    /// is a new spec row, not a longer list.
    pub also_channel: Option<ChannelId>,
}

/// [`crate::token::SESSION_TOKEN_LEN`] as the wire `u32` declared length.
// A const assert guards the narrowing so the owner constant growing past
// `u32` fails compilation here instead of truncating silently.
#[allow(clippy::cast_possible_truncation)] // guarded by the const assert below
const SESSION_TOKEN_WIRE_LEN: u32 = {
    assert!(crate::token::SESSION_TOKEN_LEN <= u32::MAX as usize);
    crate::token::SESSION_TOKEN_LEN as u32
};

/// Flag bits the selected policy permits. All structured v0 rows use zero:
/// `FLAG_RAW` and every undefined bit are invalid (spec §4 semantic table).
const STRUCTURED_FLAGS_MASK: u16 = 0;

impl ReceivePolicy {
    /// Server admission state: expect the client's `HELLO` (spec table row 1).
    #[must_use]
    pub const fn server_pre_auth_hello() -> Self {
        Self::hello(Direction::FromClient)
    }

    /// Client state awaiting the server's `HELLO` echo (spec table row 2).
    #[must_use]
    pub const fn client_await_hello() -> Self {
        Self::hello(Direction::FromServer)
    }

    const fn hello(direction: Direction) -> Self {
        Self {
            direction,
            phase: SessionPhase::PreAuth,
            channel: ChannelId(0),
            payload: PayloadMode::ExactLen(SESSION_TOKEN_WIRE_LEN),
            expected_corr: ExpectedCorrelation::Zero,
            kinds: AllowedKinds::only(FrameKind::Hello),
            allow_ping: false,
            also_channel: None,
        }
    }

    /// Host echo receiver: authenticated `CALL`s on the echo channel
    /// (spec table row 3), plus the live `PING` probe.
    #[must_use]
    pub const fn echo_receiver() -> Self {
        Self {
            direction: Direction::FromClient,
            phase: SessionPhase::Authenticated,
            channel: crate::echo::ECHO_CHANNEL,
            payload: PayloadMode::Codec,
            expected_corr: ExpectedCorrelation::NonZero,
            kinds: AllowedKinds::only(FrameKind::Call),
            allow_ping: true,
            also_channel: None,
        }
    }

    /// Echo caller waiter: the correlated `REPLY` for one outstanding call
    /// (spec table row 4, "REPLY or declared ERR"). The echo session declares
    /// no `ERR` payload codec, so `ERR` is not in its declared set and stays
    /// `KELD-IPC-005` — exactly the live pre-KEL-133 client behavior.
    #[must_use]
    pub const fn echo_reply_waiter(corr: CorrelationId) -> Self {
        Self {
            direction: Direction::FromServer,
            phase: SessionPhase::Authenticated,
            channel: crate::echo::ECHO_CHANNEL,
            payload: PayloadMode::Codec,
            expected_corr: ExpectedCorrelation::Exactly(corr),
            kinds: AllowedKinds::only(FrameKind::Reply),
            allow_ping: false,
            also_channel: None,
        }
    }

    /// Host lifecycle receiver: authenticated `CALL`s on the lifecycle
    /// channel (spec table row 5), plus the live `PING` probe.
    #[must_use]
    pub const fn lifecycle_receiver() -> Self {
        Self {
            direction: Direction::FromClient,
            phase: SessionPhase::Authenticated,
            channel: crate::lifecycle::LIFECYCLE_CHANNEL,
            payload: PayloadMode::Codec,
            expected_corr: ExpectedCorrelation::NonZero,
            kinds: AllowedKinds::only(FrameKind::Call),
            allow_ping: true,
            also_channel: None,
        }
    }

    /// The live primary app-link receiver: the one v0 session that
    /// multiplexes spec-table rows 3 and 5 — authenticated echo and
    /// lifecycle `CALL`s — plus the `PING` probe on a single stream.
    /// Both channels share the structured rules; payload codecs stay
    /// per-channel in the consumer.
    #[must_use]
    pub const fn primary_app_receiver() -> Self {
        Self {
            direction: Direction::FromClient,
            phase: SessionPhase::Authenticated,
            channel: crate::echo::ECHO_CHANNEL,
            payload: PayloadMode::Codec,
            expected_corr: ExpectedCorrelation::NonZero,
            kinds: AllowedKinds::only(FrameKind::Call),
            allow_ping: true,
            also_channel: Some(crate::lifecycle::LIFECYCLE_CHANNEL),
        }
    }

    /// App-side lifecycle event receiver: uncorrelated `EVENT`s on the
    /// lifecycle channel (spec table row 6).
    #[must_use]
    pub const fn lifecycle_event_receiver() -> Self {
        Self {
            direction: Direction::FromServer,
            phase: SessionPhase::Authenticated,
            channel: crate::lifecycle::LIFECYCLE_CHANNEL,
            payload: PayloadMode::Codec,
            expected_corr: ExpectedCorrelation::Zero,
            kinds: AllowedKinds::only(FrameKind::Event),
            allow_ping: true,
            also_channel: None,
        }
    }

    /// App-side lifecycle reply waiter (spec table row 7): `REPLY` or the
    /// declared [`crate::CallError`]-carrying `ERR`.
    #[must_use]
    pub const fn lifecycle_reply_waiter(corr: CorrelationId) -> Self {
        Self::reply_waiter(crate::lifecycle::LIFECYCLE_CHANNEL, corr)
    }

    /// Future privileged receiver: authenticated `CALL`s on a host-declared
    /// channel (spec table row 8). The KEL-102/T3 consumer selects the
    /// channel; this policy cannot mint authority — the guard still runs
    /// after payload decode.
    #[must_use]
    pub const fn privileged_call_receiver(channel: ChannelId) -> Self {
        Self {
            direction: Direction::FromClient,
            phase: SessionPhase::Authenticated,
            channel,
            payload: PayloadMode::Codec,
            expected_corr: ExpectedCorrelation::NonZero,
            kinds: AllowedKinds::only(FrameKind::Call),
            allow_ping: false,
            also_channel: None,
        }
    }

    const fn reply_waiter(channel: ChannelId, corr: CorrelationId) -> Self {
        Self {
            direction: Direction::FromServer,
            phase: SessionPhase::Authenticated,
            channel,
            payload: PayloadMode::Codec,
            expected_corr: ExpectedCorrelation::Exactly(corr),
            kinds: AllowedKinds::only(FrameKind::Reply).with(FrameKind::Err),
            allow_ping: false,
            also_channel: None,
        }
    }
}

/// Header whose reserved fields are valid for the selected policy.
///
/// The only construction path is [`validate_received_header`]; consumers get
/// read-only accessors. Holding one proves the semantic admission decision was
/// made by this module, not re-derived (or skipped) at a call-site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedFrameHeader(FrameHeader);

impl ValidatedFrameHeader {
    /// Frame kind.
    #[must_use]
    pub const fn kind(&self) -> FrameKind {
        self.0.kind
    }

    /// Flag bits (always within the policy's allowed mask; zero for v0).
    #[must_use]
    pub const fn flags(&self) -> u16 {
        self.0.flags
    }

    /// Target channel.
    #[must_use]
    pub const fn channel(&self) -> ChannelId {
        self.0.channel
    }

    /// Correlation id.
    #[must_use]
    pub const fn corr(&self) -> CorrelationId {
        self.0.corr
    }

    /// Declared payload length in bytes.
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.0.len
    }

    /// Whether the declared payload is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.len == 0
    }
}

/// Validates a syntactically decoded header against the selected policy.
///
/// Runs after [`crate::frame::FrameHeader::decode`] (`KELD-IPC-002`) and the
/// `MAX_FRAME_LEN` envelope cap (`KELD-IPC-004`), and before payload
/// allocation, payload decode, token comparison, or handler dispatch
/// (spec §3 criteria 1–5). Synchronous and allocation-free.
///
/// # Errors
///
/// Returns [`IpcError::Protocol`] (`KELD-IPC-005`) naming the first rule the
/// header violates, in the fixed order kind → flags → channel → correlation →
/// declared length. The order is part of the contract so corpus rows have one
/// expected code and detail.
pub fn validate_received_header(
    policy: &ReceivePolicy,
    header: FrameHeader,
) -> Result<ValidatedFrameHeader, IpcError> {
    if policy.allow_ping && header.kind == FrameKind::Ping {
        // Live v0 liveness probe: flags 0, empty payload; channel and
        // correlation are echoed by the receiver rather than constrained.
        if header.flags & !STRUCTURED_FLAGS_MASK != 0 {
            return Err(IpcError::Protocol {
                detail: "PING flags must be 0",
            });
        }
        if header.len != 0 {
            return Err(IpcError::Protocol {
                detail: "PING payload must be empty",
            });
        }
        return Ok(ValidatedFrameHeader(header));
    }

    if !policy.kinds.contains(header.kind) {
        return Err(IpcError::Protocol {
            detail: "frame kind is not declared by the session policy",
        });
    }

    if header.flags & crate::frame::FLAG_RAW != 0 {
        return Err(IpcError::Protocol {
            detail: "FLAG_RAW is invalid for a structured session",
        });
    }
    if header.flags & !STRUCTURED_FLAGS_MASK != 0 {
        return Err(IpcError::Protocol {
            detail: "unknown flag bits are reserved",
        });
    }

    let channel_declared = header.channel == policy.channel
        || policy
            .also_channel
            .is_some_and(|also| header.channel == also);
    if !channel_declared {
        return Err(IpcError::Protocol {
            detail: "wrong channel for the session policy",
        });
    }

    match policy.expected_corr {
        ExpectedCorrelation::Zero => {
            if header.corr != CorrelationId(0) {
                return Err(IpcError::Protocol {
                    detail: "correlation must be 0 for this frame",
                });
            }
        }
        ExpectedCorrelation::NonZero => {
            if header.corr == CorrelationId(0) {
                return Err(IpcError::Protocol {
                    detail: "correlation 0 is reserved",
                });
            }
        }
        ExpectedCorrelation::Exactly(expected) => {
            if header.corr != expected {
                return Err(IpcError::Protocol {
                    detail: "correlation does not match the awaited call",
                });
            }
        }
    }

    match policy.payload {
        PayloadMode::ExactLen(expected) => {
            if header.len != expected {
                return Err(IpcError::Protocol {
                    detail: "payload length does not match the declared exact shape",
                });
            }
        }
        PayloadMode::Empty => {
            if header.len != 0 {
                return Err(IpcError::Protocol {
                    detail: "payload must be empty for this frame",
                });
            }
        }
        PayloadMode::Codec => {}
    }

    Ok(ValidatedFrameHeader(header))
}

/// One monotonic clock carried across accept, retries, header and payload
/// reads (spec §4 deadline model).
///
/// Relative durations are converted once at the owning boundary with checked
/// arithmetic; overflow is `KELD-IPC-006`, never blocking forever. The
/// instant never renews: byte trickle, retries, and per-receive timeouts
/// recompute *remaining* time from the same absolute instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsoluteDeadline(Instant);

impl AbsoluteDeadline {
    /// Mints a deadline `duration` from now.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError::Timeout`] (`KELD-IPC-006`) if the addition
    /// overflows the monotonic clock — a nonsense duration must fail closed,
    /// not wait forever.
    pub fn from_now(duration: Duration) -> Result<Self, IpcError> {
        match Instant::now().checked_add(duration) {
            Some(instant) => Ok(Self(instant)),
            None => Err(IpcError::Timeout),
        }
    }

    /// Wraps an existing absolute instant (e.g. a bootstrap generation
    /// deadline minted by the host).
    #[must_use]
    pub const fn at(instant: Instant) -> Self {
        Self(instant)
    }

    /// The earlier of two absolute deadlines (spec §4: a started frame
    /// expires at the earliest applicable clock).
    #[must_use]
    pub fn earliest(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// Whether the deadline has passed.
    #[must_use]
    pub fn expired(&self) -> bool {
        Instant::now() >= self.0
    }

    /// Remaining time, or `None` once expired. Every blocking OS wait must be
    /// capped by this value, recomputed per wait from the same instant.
    #[must_use]
    pub fn remaining(&self) -> Option<Duration> {
        let now = Instant::now();
        if now >= self.0 {
            None
        } else {
            Some(self.0 - now)
        }
    }

    /// The underlying absolute instant.
    #[must_use]
    pub const fn instant(&self) -> Instant {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FLAG_RAW;
    use crate::{ECHO_CHANNEL, LIFECYCLE_CHANNEL};

    fn header(kind: FrameKind, flags: u16, channel: u16, corr: u32, len: u32) -> FrameHeader {
        FrameHeader {
            kind,
            flags,
            channel: ChannelId(channel),
            corr: CorrelationId(corr),
            len,
        }
    }

    fn detail_of(err: IpcError) -> &'static str {
        match err {
            IpcError::Protocol { detail } => detail,
            other => panic!("expected KELD-IPC-005 Protocol, got {other}"),
        }
    }

    /// Spec §3 criterion 2 and §4 table: every reserved combination on the
    /// echo receiver fails `KELD-IPC-005` with a distinct, stable detail.
    #[test]
    fn echo_receiver_rejects_each_reserved_combination_with_its_own_detail() {
        let policy = ReceivePolicy::echo_receiver();
        let cases: [(FrameHeader, &str); 8] = [
            (
                header(FrameKind::Call, 0, 1, 0, 4),
                "correlation 0 is reserved",
            ),
            (
                header(FrameKind::Call, FLAG_RAW, 1, 7, 4),
                "FLAG_RAW is invalid for a structured session",
            ),
            (
                header(FrameKind::Call, 1 << 1, 1, 7, 4),
                "unknown flag bits are reserved",
            ),
            (
                header(FrameKind::Call, u16::MAX, 1, 7, 4),
                "FLAG_RAW is invalid for a structured session",
            ),
            (
                header(FrameKind::Call, 0, 2, 7, 4),
                "wrong channel for the session policy",
            ),
            (
                header(FrameKind::Reply, 0, 1, 7, 4),
                "frame kind is not declared by the session policy",
            ),
            (
                header(FrameKind::Hello, 0, 1, 7, 4),
                "frame kind is not declared by the session policy",
            ),
            (
                header(FrameKind::StreamOpen, 0, 1, 7, 4),
                "frame kind is not declared by the session policy",
            ),
        ];
        for (bad, expected_detail) in cases {
            let err = validate_received_header(&policy, bad)
                .expect_err("reserved combination must not validate");
            let msg = err.to_string();
            assert!(msg.contains("KELD-IPC-005"), "{bad:?}: {msg}");
            assert_eq!(detail_of(err), expected_detail, "{bad:?}");
        }
    }

    /// Spec §3 criterion 3: the valid structured echo `CALL` admits exactly.
    #[test]
    fn echo_receiver_admits_the_valid_call_and_exposes_its_fields() {
        let policy = ReceivePolicy::echo_receiver();
        let ok = validate_received_header(&policy, header(FrameKind::Call, 0, 1, 7, 12))
            .expect("valid CALL");
        assert_eq!(ok.kind(), FrameKind::Call);
        assert_eq!(ok.flags(), 0);
        assert_eq!(ok.channel(), ECHO_CHANNEL);
        assert_eq!(ok.corr(), CorrelationId(7));
        assert_eq!(ok.len(), 12);
        assert!(!ok.is_empty());
    }

    /// The kind byte being syntactically known never admits it: every
    /// undeclared kind fails on every v0 policy (spec §4: "handlers must not
    /// accept a kind merely because `FrameKind::from_u8` recognizes it").
    #[test]
    fn syntactically_known_kinds_outside_the_policy_are_rejected_everywhere() {
        let policies = [
            ReceivePolicy::server_pre_auth_hello(),
            ReceivePolicy::client_await_hello(),
            ReceivePolicy::echo_receiver(),
            ReceivePolicy::echo_reply_waiter(CorrelationId(5)),
            ReceivePolicy::lifecycle_receiver(),
            ReceivePolicy::lifecycle_event_receiver(),
            ReceivePolicy::lifecycle_reply_waiter(CorrelationId(5)),
            ReceivePolicy::privileged_call_receiver(ChannelId(2)),
        ];
        for policy in &policies {
            for kind_byte in 0..=10u8 {
                let kind = FrameKind::from_u8(kind_byte).expect("valid kind byte");
                let allowed =
                    policy.kinds.contains(kind) || (policy.allow_ping && kind == FrameKind::Ping);
                if allowed {
                    continue;
                }
                // Fields otherwise conformant to the policy: only the kind is wrong.
                let corr = match policy.expected_corr {
                    ExpectedCorrelation::Zero => 0,
                    ExpectedCorrelation::NonZero => 9,
                    ExpectedCorrelation::Exactly(c) => c.0,
                };
                let len = match policy.payload {
                    PayloadMode::ExactLen(n) => n,
                    PayloadMode::Empty => 0,
                    PayloadMode::Codec => 4,
                };
                let err =
                    validate_received_header(policy, header(kind, 0, policy.channel.0, corr, len))
                        .expect_err("undeclared kind must not validate");
                assert_eq!(
                    detail_of(err),
                    "frame kind is not declared by the session policy",
                    "{policy:?} kind {kind:?}"
                );
            }
        }
    }

    /// Spec §3 criterion 4: HELLO admission is shape-checked before any token
    /// comparison — wrong lengths are `KELD-IPC-005`, never `KELD-IPC-007`.
    #[test]
    fn hello_shape_failures_are_protocol_not_auth() {
        for policy in [
            ReceivePolicy::server_pre_auth_hello(),
            ReceivePolicy::client_await_hello(),
        ] {
            for bad_len in [0u32, 31, 33] {
                let err =
                    validate_received_header(&policy, header(FrameKind::Hello, 0, 0, 0, bad_len))
                        .expect_err("wrong HELLO length is a shape failure");
                let msg = err.to_string();
                assert!(msg.contains("KELD-IPC-005"), "{msg}");
                assert!(!msg.contains("KELD-IPC-007"), "{msg}");
            }
            let err =
                validate_received_header(&policy, header(FrameKind::Hello, FLAG_RAW, 0, 0, 32))
                    .expect_err("HELLO flags must be zero");
            assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
            for (channel, corr) in [(1u16, 0u32), (0, 1)] {
                let err = validate_received_header(
                    &policy,
                    header(FrameKind::Hello, 0, channel, corr, 32),
                )
                .expect_err("HELLO reserved fields must be zero");
                assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
            }
            let ok = validate_received_header(&policy, header(FrameKind::Hello, 0, 0, 0, 32))
                .expect("exact HELLO shape");
            assert_eq!(ok.len(), 32);
        }
    }

    /// Spec §3 criterion 5: only the awaited correlation can satisfy a waiter;
    /// `EVENT` needs correlation 0 on its declared channel.
    #[test]
    fn waiter_and_event_correlation_rules_hold() {
        // Lifecycle declares a CallError ERR codec: both kinds are awaited.
        let lifecycle_waiter = ReceivePolicy::lifecycle_reply_waiter(CorrelationId(7));
        for kind in [FrameKind::Reply, FrameKind::Err] {
            validate_received_header(&lifecycle_waiter, header(kind, 0, 3, 7, 4))
                .expect("awaited correlation validates");
            let err = validate_received_header(&lifecycle_waiter, header(kind, 0, 3, 8, 4))
                .expect_err("unrelated correlation must not complete the waiter");
            assert_eq!(
                detail_of(err),
                "correlation does not match the awaited call"
            );
            let err = validate_received_header(&lifecycle_waiter, header(kind, 0, 3, 0, 4))
                .expect_err("correlation 0 must not complete the waiter");
            assert_eq!(
                detail_of(err),
                "correlation does not match the awaited call"
            );
        }
        // Echo declares no ERR codec: only REPLY is awaited (live client rule).
        let echo_waiter = ReceivePolicy::echo_reply_waiter(CorrelationId(7));
        validate_received_header(&echo_waiter, header(FrameKind::Reply, 0, 1, 7, 4))
            .expect("awaited echo REPLY validates");
        let err = validate_received_header(&echo_waiter, header(FrameKind::Err, 0, 1, 7, 4))
            .expect_err("undeclared ERR must not complete the echo waiter");
        assert_eq!(
            detail_of(err),
            "frame kind is not declared by the session policy"
        );
        let events = ReceivePolicy::lifecycle_event_receiver();
        validate_received_header(&events, header(FrameKind::Event, 0, 3, 0, 1))
            .expect("uncorrelated EVENT validates");
        let err = validate_received_header(&events, header(FrameKind::Event, 0, 3, 4, 1))
            .expect_err("correlated EVENT is reserved");
        assert_eq!(detail_of(err), "correlation must be 0 for this frame");
    }

    /// The live v0 `PING` probe stays admissible where it is a positive
    /// pre-KEL-133 vector (criterion 11), but its reserved combinations now
    /// fail explicitly: flags and payload must be zero/empty.
    #[test]
    fn ping_is_admitted_only_where_live_and_only_in_exact_shape() {
        for policy in [
            ReceivePolicy::echo_receiver(),
            ReceivePolicy::lifecycle_receiver(),
            ReceivePolicy::lifecycle_event_receiver(),
        ] {
            // Channel and correlation are echoed, not constrained.
            let ok = validate_received_header(&policy, header(FrameKind::Ping, 0, 42, 9, 0))
                .expect("live PING validates");
            assert_eq!(ok.kind(), FrameKind::Ping);
            let err =
                validate_received_header(&policy, header(FrameKind::Ping, FLAG_RAW, 42, 9, 0))
                    .expect_err("PING flags must be zero");
            assert_eq!(detail_of(err), "PING flags must be 0");
            let err = validate_received_header(&policy, header(FrameKind::Ping, 0, 42, 9, 1))
                .expect_err("PING payload must be empty");
            assert_eq!(detail_of(err), "PING payload must be empty");
        }
        for policy in [
            ReceivePolicy::server_pre_auth_hello(),
            ReceivePolicy::client_await_hello(),
            ReceivePolicy::privileged_call_receiver(ChannelId(2)),
            ReceivePolicy::echo_reply_waiter(CorrelationId(1)),
        ] {
            let err = validate_received_header(&policy, header(FrameKind::Ping, 0, 0, 0, 0))
                .expect_err("PING is not declared on this session");
            assert_eq!(
                detail_of(err),
                "frame kind is not declared by the session policy",
                "{policy:?}"
            );
        }
    }

    /// Spec §4 table row 8: the future privileged receiver admits only the
    /// exact structured `CALL`; the lifecycle receiver behaves identically on
    /// its own channel (row 5).
    #[test]
    fn lifecycle_and_privileged_receivers_pin_channel_and_correlation() {
        let lifecycle = ReceivePolicy::lifecycle_receiver();
        validate_received_header(&lifecycle, header(FrameKind::Call, 0, 3, 2, 1))
            .expect("valid lifecycle CALL");
        assert_eq!(lifecycle.channel, LIFECYCLE_CHANNEL);
        let err = validate_received_header(&lifecycle, header(FrameKind::Call, 0, 1, 2, 1))
            .expect_err("echo channel is wrong for lifecycle");
        assert_eq!(detail_of(err), "wrong channel for the session policy");

        let fs = ReceivePolicy::privileged_call_receiver(ChannelId(2));
        validate_received_header(&fs, header(FrameKind::Call, 0, 2, 3, 8))
            .expect("valid privileged CALL");
        let err = validate_received_header(&fs, header(FrameKind::Call, 0, 2, 0, 8))
            .expect_err("privileged CALL correlation 0 is reserved");
        assert_eq!(detail_of(err), "correlation 0 is reserved");
        let err = validate_received_header(&fs, header(FrameKind::Ping, 0, 2, 0, 0))
            .expect_err("privileged session does not declare PING");
        assert_eq!(
            detail_of(err),
            "frame kind is not declared by the session policy"
        );
    }

    /// Validation order is part of the contract: kind before flags before
    /// channel before correlation before length, so one hostile header maps
    /// to one stable corpus row.
    #[test]
    fn validation_order_is_kind_flags_channel_correlation_length() {
        let policy = ReceivePolicy::echo_receiver();
        // Everything wrong at once: kind wins.
        let err = validate_received_header(&policy, header(FrameKind::Grant, u16::MAX, 9, 0, 3))
            .expect_err("kind is checked first");
        assert_eq!(
            detail_of(err),
            "frame kind is not declared by the session policy"
        );
        // Kind right, everything else wrong: flags win.
        let err = validate_received_header(&policy, header(FrameKind::Call, u16::MAX, 9, 0, 3))
            .expect_err("flags are checked second");
        assert_eq!(
            detail_of(err),
            "FLAG_RAW is invalid for a structured session"
        );
        // Flags right: channel wins over correlation.
        let err = validate_received_header(&policy, header(FrameKind::Call, 0, 9, 0, 3))
            .expect_err("channel is checked third");
        assert_eq!(detail_of(err), "wrong channel for the session policy");
        // Channel right: correlation wins over length.
        let hello = ReceivePolicy::server_pre_auth_hello();
        let err = validate_received_header(&hello, header(FrameKind::Hello, 0, 0, 5, 31))
            .expect_err("correlation is checked before length");
        assert_eq!(detail_of(err), "correlation must be 0 for this frame");
    }

    /// The one multiplexed v0 session: the primary app receiver declares
    /// exactly the echo and lifecycle channels (spec rows 3 and 5) and
    /// nothing else.
    #[test]
    fn primary_app_receiver_declares_exactly_echo_and_lifecycle_channels() {
        let policy = ReceivePolicy::primary_app_receiver();
        for channel in [1u16, 3] {
            validate_received_header(&policy, header(FrameKind::Call, 0, channel, 7, 4))
                .expect("declared channel admits");
        }
        validate_received_header(&policy, header(FrameKind::Ping, 0, 9, 0, 0))
            .expect("live PING admits");
        for channel in [0u16, 2, 4, 9, u16::MAX] {
            let err = validate_received_header(&policy, header(FrameKind::Call, 0, channel, 7, 4))
                .expect_err("undeclared channel must not admit");
            assert_eq!(detail_of(err), "wrong channel for the session policy");
        }
        let err = validate_received_header(&policy, header(FrameKind::Call, 0, 1, 0, 4))
            .expect_err("correlation 0 stays reserved on the primary session");
        assert_eq!(detail_of(err), "correlation 0 is reserved");
        let err = validate_received_header(&policy, header(FrameKind::Call, FLAG_RAW, 3, 7, 4))
            .expect_err("FLAG_RAW stays invalid on the primary session");
        assert_eq!(
            detail_of(err),
            "FLAG_RAW is invalid for a structured session"
        );
        let err = validate_received_header(&policy, header(FrameKind::Event, 0, 3, 0, 1))
            .expect_err("EVENT is host-to-app only; the receiver does not declare it");
        assert_eq!(
            detail_of(err),
            "frame kind is not declared by the session policy"
        );
    }

    #[test]
    fn absolute_deadline_never_renews_and_overflow_fails_closed() {
        let overflow = AbsoluteDeadline::from_now(Duration::MAX)
            .expect_err("saturating a monotonic clock must fail closed");
        assert!(overflow.to_string().contains("KELD-IPC-006"), "{overflow}");

        let deadline = AbsoluteDeadline::from_now(Duration::from_hours(1)).expect("mint");
        assert!(!deadline.expired());
        let first = deadline.remaining().expect("time remains");
        let second = deadline.remaining().expect("time remains");
        assert!(
            second <= first,
            "remaining time must be monotonically non-increasing: {first:?} then {second:?}"
        );

        let past = AbsoluteDeadline::at(Instant::now());
        assert!(past.expired());
        assert_eq!(past.remaining(), None);

        let far = AbsoluteDeadline::from_now(Duration::from_mins(1)).expect("mint");
        let near = AbsoluteDeadline::from_now(Duration::from_secs(1)).expect("mint");
        assert_eq!(near.earliest(far).instant(), near.instant());
        assert_eq!(far.earliest(near).instant(), near.instant());
    }

    /// The policy constructors are `const`: hot-path receivers select their
    /// policy without any runtime construction cost or allocation.
    #[test]
    fn policies_are_const_constructible_and_fixed_size() {
        const ECHO: ReceivePolicy = ReceivePolicy::echo_receiver();
        assert_eq!(ECHO.channel, ECHO_CHANNEL);
        // Fixed-size Copy values only: the validator decision allocates nothing.
        assert!(core::mem::size_of::<ReceivePolicy>() <= 32);
        assert_eq!(
            core::mem::size_of::<ValidatedFrameHeader>(),
            core::mem::size_of::<FrameHeader>()
        );
    }
}
