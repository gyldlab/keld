//! Actual canonical reader/cursor controls, independent of Core/native grants.
use super::*;
use crate::HEADER_LEN;
use crate::link::write_frame;
use std::cell::Cell;
use std::io::Cursor;

fn frame(kind: FrameKind, correlation: u32, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_frame(
        &mut bytes,
        kind,
        0,
        crate::ECHO_CHANNEL,
        CorrelationId(correlation),
        body,
    )
    .unwrap();
    bytes
}

#[test]
fn matching_numeric_oversize_reply_and_err_refuse_before_any_body_byte() {
    for kind in [FrameKind::Reply, FrameKind::Err] {
        let mut stream = Cursor::new(frame(kind, 7, &[17; 4097]));
        let mut admitted = 0;
        let result = read_primary_app_frame_interruptible_with_payload_admission(
            &mut stream,
            &AtomicBool::new(false),
            || Some(CorrelationId(7)),
            &[],
            |_, _| GuardedCallAdmission::Admit,
            |header| {
                admitted += 1;
                assert_eq!(header.kind(), kind);
                assert_eq!(header.corr(), CorrelationId(7));
                if header.len() > 4096 {
                    return Err(IpcError::PayloadTooLarge);
                }
                Ok(())
            },
        );
        assert!(matches!(result, Err(IpcError::PayloadTooLarge)));
        assert_eq!(admitted, 1);
        assert_eq!(stream.position(), HEADER_LEN as u64);
    }
}

#[test]
fn unrelated_correlation_is_rejected_before_the_original_owner_is_consulted() {
    let mut stream = Cursor::new(frame(FrameKind::Reply, 9, &[17; 32]));
    let consulted = Cell::new(false);
    let result = read_primary_app_frame_interruptible_with_payload_admission(
        &mut stream,
        &AtomicBool::new(false),
        || Some(CorrelationId(7)),
        &[],
        |_, _| GuardedCallAdmission::Admit,
        |_| {
            consulted.set(true);
            Ok(())
        },
    );
    assert!(matches!(result, Err(IpcError::Protocol { .. })));
    assert!(!consulted.get());
    assert_eq!(stream.position(), HEADER_LEN as u64);
}

#[test]
fn accepted_boundary_reads_exact_body_and_does_not_reapply_the_cut() {
    let body = vec![255; 4096];
    let mut stream = Cursor::new(frame(FrameKind::Reply, 7, &body));
    let mut admissions = 0;
    let actual = read_primary_app_frame_interruptible_with_payload_admission(
        &mut stream,
        &AtomicBool::new(false),
        || Some(CorrelationId(7)),
        &[],
        |_, _| GuardedCallAdmission::Admit,
        |header| {
            admissions += 1;
            assert_eq!(header.len(), 4096);
            Ok(())
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(actual.payload, body);
    assert_eq!(admissions, 1);
    assert_eq!(stream.position(), (HEADER_LEN + 4096) as u64);
}

#[test]
fn existing_non_numeric_call_retains_its_original_envelope_policy() {
    let body = vec![23; 4097];
    let mut stream = Cursor::new(frame(FrameKind::Call, 7, &body));
    let actual = read_primary_app_frame_interruptible_with_payload_admission(
        &mut stream,
        &AtomicBool::new(false),
        || None,
        &[],
        |_, _| GuardedCallAdmission::Admit,
        |header| {
            if matches!(header.kind(), FrameKind::Reply | FrameKind::Err) && header.len() > 4096 {
                return Err(IpcError::PayloadTooLarge);
            }
            Ok(())
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(actual.payload, body);
}
