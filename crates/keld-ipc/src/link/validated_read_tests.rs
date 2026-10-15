use super::{
    APP_LINK_IO_DEADLINE, AtomicBool, ChannelId, CorrelationId, FrameHeader, FrameKind, HEADER_LEN,
    Instant, IpcError, MAX_FRAME_LEN, read_primary_app_frame_interruptible_with_privileged_call,
    read_primary_app_frame_interruptible_with_privileged_calls,
    read_quit_drain_frame_with_privileged_calls, read_validated_frame,
};
use crate::receive::ReceivePolicy;

mod capacity;

fn frame_bytes(kind: FrameKind, flags: u16, channel: u16, corr: u32, payload: &[u8]) -> Vec<u8> {
    let header = FrameHeader {
        kind,
        flags,
        channel: ChannelId(channel),
        corr: CorrelationId(corr),
        len: u32::try_from(payload.len()).expect("test payload fits"),
    };
    let mut bytes = header.encode().to_vec();
    bytes.extend_from_slice(payload);
    bytes
}

/// Spec kel133 §3 criterion 1: the semantic validator rejects before the
/// payload buffer is allocated or read. The cursor position is the
/// falsifiable oracle — validating after the payload read would consume
/// the payload bytes and move the cursor past `HEADER_LEN`.
#[test]
fn semantic_rejection_happens_before_the_payload_is_read() {
    let policy = ReceivePolicy::echo_receiver();
    let hostile = frame_bytes(FrameKind::Call, 0, 1, 0, &[0xAA; 64]); // corr 0
    let mut cursor = std::io::Cursor::new(hostile);
    let err = read_validated_frame(&mut cursor, &policy).expect_err("CALL corr 0 must not admit");
    assert!(err.to_string().contains("KELD-IPC-005"), "{err}");
    assert_eq!(
        cursor.position(),
        HEADER_LEN as u64,
        "payload bytes must remain unread on semantic rejection"
    );
}

/// Criterion order: the envelope cap (`KELD-IPC-004`) fires before the
/// semantic validator even when the header is also semantically invalid.
#[test]
fn envelope_cap_precedes_semantic_validation() {
    let policy = ReceivePolicy::echo_receiver();
    let mut header = FrameHeader {
        kind: FrameKind::Call,
        flags: crate::frame::FLAG_RAW, // also semantically invalid
        channel: ChannelId(9),         // also wrong channel
        corr: CorrelationId(0),        // also reserved correlation
        len: u32::try_from(MAX_FRAME_LEN).expect("cap fits u32") + 1,
    }
    .encode()
    .to_vec();
    header.extend_from_slice(&[0u8; 4]);
    let mut cursor = std::io::Cursor::new(header);
    let err =
        read_validated_frame(&mut cursor, &policy).expect_err("oversized envelope must not admit");
    assert!(err.to_string().contains("KELD-IPC-004"), "{err}");
}

/// The valid structured CALL still admits and returns its exact payload.
#[test]
fn valid_call_admits_with_payload() {
    let policy = ReceivePolicy::echo_receiver();
    let payload = crate::codec::encode(&crate::EchoRequest {
        message: "kipc".to_owned(),
        count: 2,
    })
    .expect("encode");
    let bytes = frame_bytes(FrameKind::Call, 0, 1, 7, &payload);
    let mut cursor = std::io::Cursor::new(bytes);
    let (validated, got) = read_validated_frame(&mut cursor, &policy).expect("valid CALL admits");
    assert_eq!(validated.kind(), FrameKind::Call);
    assert_eq!(validated.corr(), CorrelationId(7));
    assert_eq!(got, payload);
}

/// KEL-102/T3 permits one privileged CALL outstanding. The trusted
/// pending state must reject a second FS header before its payload is
/// consumed; queue saturation is too late to enforce this boundary.
#[test]
fn primary_privileged_reader_rejects_second_call_before_payload() {
    let mut bytes = frame_bytes(FrameKind::Call, 0, 2, 7, &[]);
    bytes.extend_from_slice(&frame_bytes(FrameKind::Call, 0, 2, 8, &[0xAA; 64]));
    let mut cursor = std::io::Cursor::new(bytes);
    let stop = AtomicBool::new(false);

    read_primary_app_frame_interruptible_with_privileged_call(
        &mut cursor,
        &stop,
        || None,
        &crate::channel_table::FS,
        || false,
    )
    .expect("first privileged CALL admits")
    .expect("reader remains active");
    let error = read_primary_app_frame_interruptible_with_privileged_call(
        &mut cursor,
        &stop,
        || None,
        &crate::channel_table::FS,
        || true,
    )
    .expect_err("second outstanding privileged CALL must fail");

    assert!(error.to_string().contains("KELD-IPC-005"), "{error}");
    assert_eq!(
        cursor.position(),
        (HEADER_LEN * 2) as u64,
        "second privileged payload must remain unread"
    );
}

#[test]
fn primary_guarded_channels_keep_independent_admission_before_payload() {
    let stop = AtomicBool::new(false);
    for (entry, blocked_entry) in [
        (&crate::channel_table::AUTH, &crate::channel_table::FS),
        (&crate::channel_table::FS, &crate::channel_table::AUTH),
    ] {
        let payload = [0x11, 0x22];
        let mut cursor = std::io::Cursor::new(frame_bytes(
            FrameKind::Call,
            0,
            entry.wire_id(),
            7,
            &payload,
        ));
        let (header, got) = read_primary_app_frame_interruptible_with_privileged_calls(
            &mut cursor,
            &stop,
            || None,
            &[&crate::channel_table::FS, &crate::channel_table::AUTH],
            |matched, corr| {
                assert_eq!(matched.id(), entry.id());
                assert_eq!(corr, CorrelationId(7));
                matched.id() == blocked_entry.id()
            },
        )
        .expect("the other channel's occupied owner cannot block this CALL")
        .expect("active reader");
        assert_eq!(header.channel(), entry.id());
        assert_eq!(got, payload);
    }
    // Framing does not collapse broker single-flight into one pending CALL.
    for corr in [7, 8] {
        let mut cursor = std::io::Cursor::new(frame_bytes(
            FrameKind::Call,
            0,
            crate::channel_table::AUTH.wire_id(),
            corr,
            &[],
        ));
        read_primary_app_frame_interruptible_with_privileged_calls(
            &mut cursor,
            &stop,
            || None,
            &[&crate::channel_table::AUTH],
            |_, _| false,
        )
        .expect("available auth capacity admits both nonzero ids")
        .expect("active reader");
    }
    for rejected_corr in [7, 8] {
        let mut cursor = std::io::Cursor::new(frame_bytes(
            FrameKind::Call,
            0,
            crate::channel_table::AUTH.wire_id(),
            rejected_corr,
            &[0xAA; 64],
        ));
        let error = read_primary_app_frame_interruptible_with_privileged_calls(
            &mut cursor,
            &stop,
            || None,
            &[&crate::channel_table::AUTH],
            |entry, corr| {
                entry.id() == crate::channel_table::AUTH.id()
                    && corr == CorrelationId(rejected_corr)
            },
        )
        .expect_err("trusted unavailable correlation or capacity rejects");
        assert!(matches!(error, IpcError::Protocol { .. }));
        assert_eq!(cursor.position(), HEADER_LEN as u64);
    }
}

#[test]
fn primary_auth_selection_cannot_widen_canonical_header_rules() {
    let stop = AtomicBool::new(false);
    let auth = crate::channel_table::AUTH.wire_id();
    for selected in [&[][..], &[&crate::channel_table::FS][..]] {
        let mut cursor =
            std::io::Cursor::new(frame_bytes(FrameKind::Call, 0, auth, 7, &[0xAA; 64]));
        read_primary_app_frame_interruptible_with_privileged_calls(
            &mut cursor,
            &stop,
            || None,
            selected,
            |_, _| false,
        )
        .expect_err("unselected auth cannot select its own policy");
        assert_eq!(cursor.position(), HEADER_LEN as u64);
    }
    for (kind, flags, channel, corr) in [
        (FrameKind::Call, 0, auth, 0),
        (FrameKind::Call, crate::frame::FLAG_RAW, auth, 7),
        (FrameKind::Call, 0x8000, auth, 7),
        (FrameKind::Call, 0, auth + 1, 7),
        (FrameKind::Call, 0, u16::MAX, 7),
        (FrameKind::Event, 0, auth, 0),
        (FrameKind::Reply, 0, auth, 7),
        (FrameKind::Err, 0, auth, 7),
        (FrameKind::StreamOpen, 0, auth, 7),
        (FrameKind::StreamChunk, 0, auth, 7),
        (FrameKind::StreamClose, 0, auth, 7),
        (FrameKind::Cancel, 0, auth, 7),
        (FrameKind::Grant, 0, auth, 7),
    ] {
        let mut cursor = std::io::Cursor::new(frame_bytes(kind, flags, channel, corr, &[0xAA; 64]));
        let error = read_primary_app_frame_interruptible_with_privileged_calls(
            &mut cursor,
            &stop,
            || None,
            &[&crate::channel_table::FS, &crate::channel_table::AUTH],
            |_, _| false,
        )
        .expect_err("bad or future structured header remains rejected");
        assert!(matches!(error, IpcError::Protocol { .. }));
        assert_eq!(cursor.position(), HEADER_LEN as u64);
    }
    let mut cursor = std::io::Cursor::new(frame_bytes(
        FrameKind::Call,
        0,
        crate::channel_table::ECHO.wire_id(),
        7,
        &[0xAA; 64],
    ));
    read_primary_app_frame_interruptible_with_privileged_calls(
        &mut cursor,
        &stop,
        || None,
        &[&crate::channel_table::ECHO],
        |_, _| false,
    )
    .expect_err("host-internal entry cannot become a guarded policy");
    assert_eq!(cursor.position(), HEADER_LEN as u64);
}

#[test]
fn primary_multi_guarded_selection_preserves_echo_lifecycle_and_ping() {
    let stop = AtomicBool::new(false);
    for entry in [
        &crate::channel_table::ECHO,
        &crate::channel_table::LIFECYCLE,
    ] {
        let mut cursor =
            std::io::Cursor::new(frame_bytes(FrameKind::Call, 0, entry.wire_id(), 7, &[1]));
        let (_, payload) = read_primary_app_frame_interruptible_with_privileged_calls(
            &mut cursor,
            &stop,
            || Some(CorrelationId(17)),
            &[&crate::channel_table::FS, &crate::channel_table::AUTH],
            |_, _| panic!("guarded callback must not gate internal calls"),
        )
        .expect("legacy internal call admits")
        .expect("active reader");
        assert_eq!(payload, [1]);
    }
    for kind in [FrameKind::Reply, FrameKind::Err] {
        for corr in [16, 18] {
            let mut cursor = std::io::Cursor::new(frame_bytes(
                kind,
                0,
                crate::channel_table::ECHO.wire_id(),
                corr,
                &[0xAA; 64],
            ));
            read_primary_app_frame_interruptible_with_privileged_calls(
                &mut cursor,
                &stop,
                || Some(CorrelationId(17)),
                &[&crate::channel_table::FS, &crate::channel_table::AUTH],
                |_, _| false,
            )
            .expect_err("stale or future echo reply remains rejected");
            assert_eq!(cursor.position(), HEADER_LEN as u64);
        }
        let mut cursor = std::io::Cursor::new(frame_bytes(
            kind,
            0,
            crate::channel_table::ECHO.wire_id(),
            17,
            &[1],
        ));
        read_primary_app_frame_interruptible_with_privileged_calls(
            &mut cursor,
            &stop,
            || Some(CorrelationId(17)),
            &[&crate::channel_table::FS, &crate::channel_table::AUTH],
            |_, _| false,
        )
        .expect("exact pending echo terminal admits")
        .expect("active reader");
    }
    for (flags, payload, accepted) in [
        (0, &[][..], true),
        (crate::frame::FLAG_RAW, &[][..], false),
        (0, &[1][..], false),
    ] {
        let mut cursor =
            std::io::Cursor::new(frame_bytes(FrameKind::Ping, flags, u16::MAX, 0, payload));
        let result = read_primary_app_frame_interruptible_with_privileged_calls(
            &mut cursor,
            &stop,
            || None,
            &[&crate::channel_table::FS, &crate::channel_table::AUTH],
            |_, _| panic!("PING must not inspect guarded slots"),
        );
        assert_eq!(result.is_ok(), accepted);
        if !accepted {
            assert_eq!(cursor.position(), HEADER_LEN as u64);
        }
    }
}

#[test]
fn primary_multi_guarded_envelope_cap_and_quit_drain_stay_canonical() {
    let stop = AtomicBool::new(false);
    let header = FrameHeader {
        kind: FrameKind::Call,
        flags: crate::frame::FLAG_RAW,
        channel: crate::channel_table::AUTH.id(),
        corr: CorrelationId(0),
        len: u32::try_from(MAX_FRAME_LEN).expect("frame cap fits") + 1,
    };
    let mut cursor = std::io::Cursor::new(header.encode());
    let error = read_primary_app_frame_interruptible_with_privileged_calls(
        &mut cursor,
        &stop,
        || None,
        &[&crate::channel_table::AUTH],
        |_, _| false,
    )
    .expect_err("envelope cap precedes semantic error before allocation");
    assert!(matches!(error, IpcError::PayloadTooLarge));
    assert_eq!(cursor.position(), HEADER_LEN as u64);
    for entry in [&crate::channel_table::FS, &crate::channel_table::AUTH] {
        let mut cursor =
            std::io::Cursor::new(frame_bytes(FrameKind::Call, 0, entry.wire_id(), 7, &[1]));
        let (header, got) = read_quit_drain_frame_with_privileged_calls(
            &mut cursor,
            &[&crate::channel_table::FS, &crate::channel_table::AUTH],
            Instant::now() + APP_LINK_IO_DEADLINE,
            Instant::now() + APP_LINK_IO_DEADLINE,
        )
        .expect("quiesced declared guarded CALL admits for terminal024")
        .expect("frame");
        assert_eq!(header.channel(), entry.id());
        assert_eq!(got, [1]);
    }
}

/// Syntax stays first: a bad header byte is `KELD-IPC-002`, not 005.
#[test]
fn header_syntax_precedes_semantic_validation() {
    let policy = ReceivePolicy::echo_receiver();
    let mut bytes = frame_bytes(FrameKind::Call, 0, 1, 7, &[]);
    bytes[0] = b'X'; // bad magic
    let mut cursor = std::io::Cursor::new(bytes);
    let err = read_validated_frame(&mut cursor, &policy).expect_err("bad magic must not admit");
    assert!(err.to_string().contains("KELD-IPC-002"), "{err}");
}
