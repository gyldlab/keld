//! Real Windows pipe and fixed-storage controls. No native delivery claim.
#![allow(clippy::expect_used, clippy::panic)]

use super::super::{PrimaryRouterHandle, tests::initial_ready_router_with_window_loop};
use super::*;
use keld_ipc::AppLinkDeadlines as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

mod capacity;
mod quiescence;
mod retirement_observation;
mod schedule;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}
fn target(router: &PrimaryRouterHandle) -> Arc<WriterLink> {
    let current = router.current.lock().expect("fixture current");
    let active = current.as_ref().expect("fixture active");
    WriterLink::new(
        active.writer.try_clone().expect("exact pipe"),
        PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        },
    )
    .expect("retained exact writer link")
}
fn frame(writer: &PrimaryWriter, link: &Arc<WriterLink>, bytes: &[u8]) -> PreparedFrame {
    writer
        .try_reserve_value(deadline())
        .expect("one value position")
        .copy_value(
            Arc::clone(link),
            FrameKind::Reply,
            keld_ipc::ECHO_CHANNEL,
            CorrelationId(9),
            bytes,
        )
        .expect("charged exact copy")
}
fn send(frame: PreparedFrame) -> WriteReceipt {
    frame
        .try_send()
        .unwrap_or_else(|_| panic!("actual fixed FIFO refused"))
}
fn count(writer: &PrimaryWriter) -> usize {
    writer.shared.state.lock().expect("writer state").charged()
}

#[test]
fn positions_are_one_value_four_control_across_queue_and_retained_receipts() {
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    let writer = PrimaryWriter::new(router.window_commands.clone());
    let value = frame(&writer, &link, &[7; 32]);
    let pointer = value.packet.as_ref().expect("packet").payload.as_ptr();
    let mut receipts = Vec::new();
    receipts.push(send(value));
    for sequence in 1..=4 {
        receipts.push(send(
            writer
                .try_prepare_control(
                    Arc::clone(&link),
                    ControlMessage::Ping {
                        channel: keld_ipc::LIFECYCLE_CHANNEL,
                        correlation: CorrelationId(sequence),
                    },
                    deadline(),
                )
                .expect("one of four reserved controls"),
        ));
    }
    assert_eq!(count(&writer), 5);
    assert!(matches!(
        writer.try_reserve_value(deadline()),
        Err(WriterRefusal::Full)
    ));
    assert!(matches!(
        writer.reserve(Class::Control, None, deadline()),
        Err(WriterRefusal::Full)
    ));
    let (position, packet) = writer
        .shared
        .state
        .lock()
        .expect("dequeue")
        .dequeue()
        .expect("fixed dequeue")
        .expect("one packet");
    assert_eq!(
        packet.payload.as_ptr(),
        pointer,
        "the encoded Box moves without a second copy"
    );
    assert_eq!(count(&writer), 5, "dequeue releases no credit");
    drop(packet);
    writer
        .shared
        .state
        .lock()
        .expect("complete")
        .complete(position, WriteCompletion::Written);
    assert_eq!(
        receipts[0].poll().expect("actual receipt"),
        Some(WriteCompletion::Written)
    );
    assert_eq!(
        count(&writer),
        5,
        "completion metadata remains charged while retained"
    );
    drop(receipts.remove(0));
    assert_eq!(count(&writer), 4);
    drop(
        writer
            .try_reserve_value(deadline())
            .expect("only now can the value position be reused"),
    );
    for expected in 1..=4 {
        let (position, packet) = writer
            .shared
            .state
            .lock()
            .expect("dequeue control")
            .dequeue()
            .expect("fixed dequeue")
            .expect("one control");
        assert_eq!(
            packet.header.corr,
            CorrelationId(expected),
            "admission FIFO order"
        );
        drop(packet);
        writer
            .shared
            .state
            .lock()
            .expect("complete control")
            .complete(position, WriteCompletion::Written);
    }
    assert_eq!(count(&writer), 4);
    drop(receipts);
    assert_eq!(count(&writer), 0);
}

#[test]
fn actual_try_send_full_and_disconnected_return_the_same_charged_allocation() {
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let link = target(&router);
    for failure in [WriterRefusal::Full, WriterRefusal::Disconnected] {
        let writer = PrimaryWriter::new(router.window_commands.clone());
        let prepared = frame(&writer, &link, &[1, 2, 3]);
        let pointer = prepared.packet.as_ref().expect("packet").payload.as_ptr();
        writer
            .shared
            .state
            .lock()
            .expect("actual send seam")
            .fail_next_send(failure);
        #[expect(
            clippy::manual_let_else,
            reason = "Keep the full charged success/error carrier in this explicit match so the panic arm retains its existing disposal scope."
        )]
        let returned = match (failure, prepared.try_send()) {
            (WriterRefusal::Full, Err(TrySendError::Full(frame)))
            | (WriterRefusal::Disconnected, Err(TrySendError::Disconnected(frame))) => frame,
            _ => panic!("wrong actual try-send branch"),
        };
        assert_eq!(
            returned
                .packet
                .as_ref()
                .expect("returned packet")
                .payload
                .as_ptr(),
            pointer
        );
        assert!(returned.targets(&link));
        assert_eq!(count(&writer), 1);
        assert!(
            writer
                .shared
                .state
                .lock()
                .expect("no queued bytes")
                .dequeue()
                .expect("queue")
                .is_none()
        );
        drop(returned);
        assert_eq!(
            count(&writer),
            0,
            "only the returned attempted reservation is released"
        );
    }
}

#[test]
fn exact_canonical_encoding_is_charged_before_both_serializer_passes() {
    struct Observed<'a> {
        writer: &'a PrimaryWriter,
        passes: &'a AtomicUsize,
    }
    impl serde::Serialize for Observed<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            assert_eq!(
                count(self.writer),
                1,
                "reservation precedes counting and encoding"
            );
            self.passes.fetch_add(1, Ordering::SeqCst);
            serializer.serialize_str("canonical bounded bytes")
        }
    }
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let writer = PrimaryWriter::new(router.window_commands.clone());
    let link = target(&router);
    let passes = AtomicUsize::new(0);
    let value = Observed {
        writer: &writer,
        passes: &passes,
    };
    let prepared = writer
        .try_reserve_value(deadline())
        .expect("reserve before count")
        .encode_value(
            link,
            FrameKind::Reply,
            keld_ipc::ECHO_CHANNEL,
            CorrelationId(1),
            &value,
            64,
        )
        .expect("same canonical codec");
    assert_eq!(passes.load(Ordering::SeqCst), 2);
    let packet = prepared.packet.as_ref().expect("packet");
    let expected = keld_ipc::codec::encode(&"canonical bounded bytes").expect("existing codec");
    assert_eq!(&*packet.payload, expected);
    assert_eq!(
        packet.payload.len(),
        expected.len(),
        "no maximum-sized ordinary allocation"
    );
    drop(prepared);
    assert_eq!(count(&writer), 0);
}

#[test]
fn changed_or_oversized_canonical_encoding_never_reaches_the_fifo() {
    struct Changed(AtomicUsize);
    impl serde::Serialize for Changed {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                serializer.serialize_u8(1)
            } else {
                serializer.serialize_str("larger on the second pass")
            }
        }
    }
    let (router, _peer, _owner, _windows) = initial_ready_router_with_window_loop();
    let writer = PrimaryWriter::new(router.window_commands.clone());
    let link = target(&router);
    assert!(
        writer
            .try_reserve_value(deadline())
            .expect("reserve")
            .encode_value(
                Arc::clone(&link),
                FrameKind::Reply,
                keld_ipc::ECHO_CHANNEL,
                CorrelationId(1),
                &Changed(AtomicUsize::new(0)),
                64
            )
            .is_err()
    );
    assert_eq!(count(&writer), 0);
    assert!(
        writer
            .try_reserve_value(deadline())
            .expect("reserve")
            .encode_value(
                link,
                FrameKind::Reply,
                keld_ipc::ECHO_CHANNEL,
                CorrelationId(1),
                &"oversized",
                2
            )
            .is_err()
    );
    assert_eq!(count(&writer), 0);
    assert!(
        writer
            .shared
            .state
            .lock()
            .expect("no admission")
            .dequeue()
            .expect("queue")
            .is_none()
    );
}

#[test]
fn unit_progress_is_coalesced_and_class_specific_without_payload_or_route_ids() {
    let (tx, rx) = mpsc::channel();
    let writer = PrimaryWriter::new(tx);
    let value = writer
        .try_reserve_value(deadline())
        .expect("value position");
    writer
        .register_value_interest()
        .expect("retained value needs capacity");
    assert!(
        rx.try_recv().is_err(),
        "unused controls cannot spuriously wake a blocked value"
    );
    let mut controls: Vec<_> = (0..4)
        .map(|_| {
            writer
                .reserve(Class::Control, None, deadline())
                .expect("control position")
        })
        .collect();
    writer
        .register_control_interest()
        .expect("control interest");
    assert!(rx.try_recv().is_err());
    drop(controls.pop());
    writer
        .register_control_interest()
        .expect("another retained control");
    drop(value);
    assert!(matches!(
        rx.try_recv(),
        Ok(AppWindowCommand::HostWriterProgress)
    ));
    assert!(
        rx.try_recv().is_err(),
        "only one outstanding marker while UI is blocked"
    );
    writer.acknowledge_progress().expect("UI owns first marker");
    assert!(matches!(
        rx.try_recv(),
        Ok(AppWindowCommand::HostWriterProgress)
    ));
    assert!(rx.try_recv().is_err());
    writer
        .acknowledge_progress()
        .expect("all actual records serviced");
    assert!(rx.try_recv().is_err());
    drop(controls);
}
