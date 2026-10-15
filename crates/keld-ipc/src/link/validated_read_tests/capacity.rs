use super::super::{
    Duration, GuardedCallAdmission, read_frame_interruptible_validated_with,
    read_primary_app_frame_interruptible_with_admission,
    validate_primary_app_header_with_privileged_calls,
};
use super::{AtomicBool, CorrelationId, FrameKind, HEADER_LEN, Instant, IpcError, frame_bytes};

#[test]
fn capacity_refusal_consumes_complete_frame_and_keeps_next_frame_aligned() {
    let stop = AtomicBool::new(false);
    let mut bytes = frame_bytes(
        FrameKind::Call,
        0,
        crate::channel_table::AUTH.wire_id(),
        7,
        &[0xff, 0xaa, 0x55],
    );
    bytes.extend(frame_bytes(FrameKind::Ping, 0, 1, 9, &[]));
    let mut cursor = std::io::Cursor::new(bytes);
    let frame = read_primary_app_frame_interruptible_with_admission(
        &mut cursor,
        &stop,
        || None,
        &[&crate::channel_table::AUTH],
        |_, _| GuardedCallAdmission::AtCapacity,
    )
    .expect("complete refused frame")
    .expect("frame");
    assert_eq!(frame.admission, GuardedCallAdmission::AtCapacity);
    assert_eq!(frame.payload, [0xff, 0xaa, 0x55]);
    assert_eq!(cursor.position(), (HEADER_LEN + 3) as u64);
    let next = read_primary_app_frame_interruptible_with_admission(
        &mut cursor,
        &stop,
        || None,
        &[&crate::channel_table::AUTH],
        |_, _| panic!("PING must not sample guarded admission"),
    )
    .expect("same stream healthy")
    .expect("next frame");
    assert_eq!(next.header.kind(), FrameKind::Ping);
    assert_eq!(next.header.corr(), CorrelationId(9));
    assert_eq!(next.admission, GuardedCallAdmission::Admit);
}

#[test]
fn capacity_result_keeps_original_clock_after_header_admission() {
    let stop = AtomicBool::new(false);
    let mut cursor = std::io::Cursor::new(frame_bytes(
        FrameKind::Call,
        0,
        crate::channel_table::AUTH.wire_id(),
        7,
        &[0xaa],
    ));
    let error = read_frame_interruptible_validated_with(
        &mut cursor,
        &stop,
        None,
        None,
        Duration::from_millis(2),
        |header| {
            let validated = validate_primary_app_header_with_privileged_calls(
                None,
                &[&crate::channel_table::AUTH],
                header,
            )?;
            // Clock fault stimulus, not sleep-sync: hold admission until an
            // observed absolute instant beyond the original first-byte bound.
            let end = Instant::now() + Duration::from_millis(20);
            while Instant::now() < end {
                std::thread::yield_now();
            }
            Ok((validated, GuardedCallAdmission::AtCapacity))
        },
    )
    .expect_err("capacity cannot renew the first-byte clock before the body");
    assert!(matches!(error, IpcError::Timeout));
    assert_eq!(
        cursor.position(),
        HEADER_LEN as u64,
        "expired body must not be consumed or acknowledged"
    );
}

#[test]
fn capacity_refusal_never_returns_a_truncated_body_or_hides_bad_header() {
    let stop = AtomicBool::new(false);
    let complete = frame_bytes(
        FrameKind::Call,
        0,
        crate::channel_table::AUTH.wire_id(),
        7,
        &[0xaa, 0xbb],
    );
    let mut cursor = std::io::Cursor::new(complete[..complete.len() - 1].to_vec());
    let error = read_primary_app_frame_interruptible_with_admission(
        &mut cursor,
        &stop,
        || None,
        &[&crate::channel_table::AUTH],
        |_, _| GuardedCallAdmission::AtCapacity,
    )
    .expect_err("partial body must be terminal, never a refusal result");
    assert!(
        matches!(error, IpcError::Io(ref source) if source.kind() == std::io::ErrorKind::UnexpectedEof)
    );
    let mut cursor = std::io::Cursor::new(frame_bytes(
        FrameKind::Call,
        crate::frame::FLAG_RAW,
        crate::channel_table::AUTH.wire_id(),
        7,
        &[0xaa; 64],
    ));
    let error = read_primary_app_frame_interruptible_with_admission(
        &mut cursor,
        &stop,
        || None,
        &[&crate::channel_table::AUTH],
        |_, _| panic!("header semantics must precede capacity"),
    )
    .expect_err("invalid header remains terminal");
    assert!(matches!(error, IpcError::Protocol { .. }));
    assert_eq!(cursor.position(), HEADER_LEN as u64);
    let mut cursor = std::io::Cursor::new(frame_bytes(
        FrameKind::Call,
        0,
        crate::channel_table::AUTH.wire_id(),
        7,
        &[0xaa; 64],
    ));
    let error = read_primary_app_frame_interruptible_with_admission(
        &mut cursor,
        &stop,
        || None,
        &[&crate::channel_table::AUTH],
        |_, _| GuardedCallAdmission::RejectSession,
    )
    .expect_err("duplicate/invalid session must stay terminal");
    assert!(matches!(error, IpcError::Protocol { .. }));
    assert_eq!(cursor.position(), HEADER_LEN as u64);
}
