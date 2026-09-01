//! Deterministic raw-byte regressions (KEL-133 spec §3 criterion 12, the
//! CI-fixture half).
//!
//! These run in every named `check` matrix row. The bounded `cargo-fuzz`
//! campaign is an additional T1 gate reported separately in the T1 artifact;
//! every input it retains must be minimized into a named test here. The
//! properties are criterion 12's: header/session decode terminates, never
//! panics, admits nothing the selected policy does not declare, and cannot be
//! forced into an unbounded allocation by a forged length.

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are the assertion oracles

use std::io::Cursor;

use keld_ipc::link::read_validated_frame;
use keld_ipc::receive::{ReceivePolicy, validate_received_header};
use keld_ipc::{ChannelId, CorrelationId, FrameHeader, FrameKind, HEADER_LEN, MAX_FRAME_LEN};

/// Deterministic LCG so the byte soup is reproducible in every matrix row
/// with no rand dependency (numerical recipes constants).
struct Lcg(u64);

impl Lcg {
    #[allow(clippy::cast_possible_truncation)] // deliberate low-byte extraction
    fn next_byte(&mut self) -> u8 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u8
    }
}

fn policies() -> [ReceivePolicy; 9] {
    [
        ReceivePolicy::primary_app_receiver(),
        ReceivePolicy::server_pre_auth_hello(),
        ReceivePolicy::client_await_hello(),
        ReceivePolicy::echo_receiver(),
        ReceivePolicy::echo_reply_waiter(CorrelationId(7)),
        ReceivePolicy::lifecycle_receiver(),
        ReceivePolicy::lifecycle_event_receiver(),
        ReceivePolicy::lifecycle_reply_waiter(CorrelationId(7)),
        ReceivePolicy::privileged_call_receiver(ChannelId(2)),
    ]
}

/// An admitted header must satisfy every rule its policy declares — the
/// self-check every raw-byte case runs instead of trusting the reject path.
fn assert_policy_invariants(policy: &ReceivePolicy, bytes: &[u8]) {
    let header_bytes: &[u8; HEADER_LEN] = bytes[..HEADER_LEN].try_into().expect("header slice");
    let header = FrameHeader::decode(header_bytes).expect("admitted implies syntactic");
    let Ok(validated) = validate_received_header(policy, header) else {
        panic!("read admitted a frame the validator rejects");
    };
    if validated.kind() == FrameKind::Ping {
        assert!(policy.allow_ping);
        assert_eq!(validated.flags(), 0);
        assert_eq!(validated.len(), 0);
    } else {
        assert!(policy.kinds.contains(validated.kind()));
        assert_eq!(validated.flags(), 0, "v0 structured flags mask is zero");
        assert_eq!(validated.channel(), policy.channel);
    }
}

/// Every kind byte 0..=255 with otherwise-valid fields: decode admits only
/// the eleven defined kinds; the validator then admits only declared ones.
/// Removing kind validation is caught here (criterion 12's named mutation).
#[test]
fn exhaustive_kind_byte_sweep_terminates_and_admits_only_declared_kinds() {
    for policy in &policies() {
        for kind_byte in 0..=u8::MAX {
            let mut bytes = FrameHeader {
                kind: FrameKind::Ping,
                flags: 0,
                channel: policy.channel,
                corr: CorrelationId(0),
                len: 0,
            }
            .encode()
            .to_vec();
            bytes[3] = kind_byte;
            let mut cursor = Cursor::new(bytes.clone());
            match read_validated_frame(&mut cursor, policy) {
                Ok(_) => assert_policy_invariants(policy, &bytes),
                Err(e) => {
                    // Terminates with a classified error, never a panic.
                    let _ = e.to_string();
                }
            }
        }
    }
}

/// 4096 seeded pseudo-random buffers per policy shape class: decode always
/// terminates, never panics, and every admission satisfies the policy.
#[test]
fn seeded_byte_soup_never_panics_and_never_admits_undeclared_frames() {
    let mut lcg = Lcg(0x4B49_3133_0000_0001); // "KI" KEL-133, fixed seed
    let policies = policies();
    for case in 0u32..4096 {
        let len = (lcg.next_byte() as usize) % 64;
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(lcg.next_byte());
        }
        // Half the cases get a valid magic/version prefix so the sweep spends
        // its budget past the first two checks.
        if case.is_multiple_of(2) && bytes.len() >= 3 {
            bytes[0] = b'K';
            bytes[1] = b'I';
            bytes[2] = 2;
        }
        let policy = &policies[(case as usize) % policies.len()];
        let mut cursor = Cursor::new(bytes.clone());
        match read_validated_frame(&mut cursor, policy) {
            Ok((validated, payload)) => {
                assert_policy_invariants(policy, &bytes);
                assert_eq!(
                    u32::try_from(payload.len()).expect("payload fits"),
                    validated.len()
                );
            }
            Err(e) => {
                let _ = e.to_string();
            }
        }
    }
}

/// Every truncation point of a valid frame is EOF or an admission at exactly
/// the full length — a partially consumed frame can never admit.
#[test]
fn every_truncation_of_a_valid_frame_fails_closed() {
    let policy = ReceivePolicy::echo_receiver();
    let payload = keld_ipc::codec::encode(&keld_ipc::EchoRequest {
        message: "kipc".to_owned(),
        count: 3,
    })
    .expect("encode");
    let mut frame = FrameHeader {
        kind: FrameKind::Call,
        flags: 0,
        channel: ChannelId(1),
        corr: CorrelationId(7),
        len: u32::try_from(payload.len()).expect("len"),
    }
    .encode()
    .to_vec();
    frame.extend_from_slice(&payload);

    for cut in 0..frame.len() {
        let mut cursor = Cursor::new(frame[..cut].to_vec());
        let err =
            read_validated_frame(&mut cursor, &policy).expect_err("truncated frame must not admit");
        assert!(
            err.to_string().starts_with("KELD-IPC-001"),
            "cut at {cut}: {err}"
        );
    }
    let mut cursor = Cursor::new(frame);
    read_validated_frame(&mut cursor, &policy).expect("full frame admits");
}

/// Every single-bit flip of a valid header terminates in a classified result;
/// any admission still satisfies the policy (correlation bits may flip and
/// stay valid — that is the declared `NonZero` rule, not a defect).
#[test]
fn every_header_bit_flip_terminates_classified() {
    let policy = ReceivePolicy::echo_receiver();
    let payload = keld_ipc::codec::encode(&keld_ipc::EchoRequest {
        message: "kipc".to_owned(),
        count: 3,
    })
    .expect("encode");
    let base = FrameHeader {
        kind: FrameKind::Call,
        flags: 0,
        channel: ChannelId(1),
        corr: CorrelationId(7),
        len: u32::try_from(payload.len()).expect("len"),
    }
    .encode();

    for bit in 0..(HEADER_LEN * 8) {
        let mut header = base;
        header[bit / 8] ^= 1 << (bit % 8);
        let mut bytes = header.to_vec();
        bytes.extend_from_slice(&payload);
        let mut cursor = Cursor::new(bytes.clone());
        match read_validated_frame(&mut cursor, &policy) {
            Ok(_) => assert_policy_invariants(&policy, &bytes),
            Err(e) => {
                let _ = e.to_string();
            }
        }
    }
}

/// A forged length field cannot force an allocation: everything above
/// `MAX_FRAME_LEN` is rejected at the cap with no payload read, all the way
/// up to `u32::MAX`.
#[test]
fn forged_lengths_reject_at_the_cap_without_consuming_payload() {
    let policy = ReceivePolicy::echo_receiver();
    for forged in [
        u32::try_from(MAX_FRAME_LEN).expect("cap fits") + 1,
        u32::try_from(MAX_FRAME_LEN).expect("cap fits") * 2,
        u32::MAX / 2,
        u32::MAX,
    ] {
        let bytes = FrameHeader {
            kind: FrameKind::Call,
            flags: 0,
            channel: ChannelId(1),
            corr: CorrelationId(7),
            len: forged,
        }
        .encode()
        .to_vec();
        let mut cursor = Cursor::new(bytes);
        let err =
            read_validated_frame(&mut cursor, &policy).expect_err("forged length must not admit");
        assert!(err.to_string().starts_with("KELD-IPC-004"), "{err}");
        assert_eq!(
            cursor.position(),
            HEADER_LEN as u64,
            "rejection happens before any payload read"
        );
    }
}
