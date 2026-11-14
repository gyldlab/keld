//! Actual Windows pipe plus modeled already-settled native records. These cases
//! qualify Core storage/admission ordering, not `WebView2` completion or Runtime identity.
#![allow(clippy::expect_used, clippy::panic)]

use super::*;
use crate::app_session::primary_writer::{PrimaryWriter, PrimaryWriterWorker, WriterLink};
use keld_ipc::AppLinkDeadlines as _;
use keld_wv::AppWindowCommand;
use std::sync::mpsc::Receiver;
use std::time::Duration;

pub(super) fn fixture(
    run: impl FnOnce(
        &PrimaryRouterHandle,
        &mut keld_ipc::BootstrapStream,
        &PrimaryWriter,
        &Receiver<RendererBridgeOutcome>,
        &Receiver<AppWindowCommand>,
    ),
) {
    let (mut router, mut peer, _guardian, windows) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let (session, outcomes) = super::super::test_fixture::availability_session();
    router.compat = Some(Arc::new(Mutex::new(session)));
    {
        let mut current = router.current.lock().expect("canonical current");
        let active = current.as_mut().expect("actual original pipe");
        let caller = PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        active.writer_link = Some(
            WriterLink::new(active.writer.try_clone().expect("exact pipe clone"), caller)
                .expect("fixed target"),
        );
    }
    let (writer, mut worker) =
        PrimaryWriterWorker::start(router.clone()).expect("one actual writer");
    router.primary_writer = Some(writer.clone());
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("bounded physical fixture");
    run(&router, &mut peer, &writer, &outcomes, &windows);
    worker
        .stop_and_join()
        .expect("worker joins with every canonical lock free");
}

pub(super) fn settle(router: &PrimaryRouterHandle, disposition: ConsumerDisposition) {
    let caller = {
        let current = router.current.lock().expect("actual caller");
        let active = current.as_ref().expect("actual role");
        PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    let mut state = router
        .compat
        .as_ref()
        .expect("same ledger")
        .lock()
        .expect("record");
    state
        .ledger
        .reserve(
            Direction::MainToRenderer,
            NativeRouteKey {
                webview: 1,
                navigation: 2,
                message: 77,
            },
            &caller,
            4096,
        )
        .expect("existing upstream DATA lane");
    let record = state.ledger.data[Direction::MainToRenderer.index()]
        .as_mut()
        .expect("original native record");
    record.consumer_done = true;
    record.native_done = true;
    record.pending_reply = Some(CorrelationId(77));
    record.terminal = Some(disposition);
    record.terminal_deadline = Some(Instant::now() + Duration::from_secs(3));
}
fn diagnostic_pointer(router: &PrimaryRouterHandle) -> usize {
    let state = router
        .compat
        .as_ref()
        .expect("ledger")
        .lock()
        .expect("record");
    let record = state.ledger.data[Direction::MainToRenderer.index()]
        .as_ref()
        .expect("retained record");
    let Some(ConsumerDisposition::Refused { error }) = record.terminal.as_ref() else {
        panic!("same source diagnostic missing");
    };
    error.message.as_ptr() as usize
}
fn refused() -> ConsumerDisposition {
    ConsumerDisposition::Refused {
        error: CallError {
            code: "KELD-WV-011".to_owned(),
            message: "original consumer refused the delivery".to_owned(),
        },
    }
}
fn progress(windows: &Receiver<AppWindowCommand>, writer: &PrimaryWriter) {
    assert!(matches!(
        windows
            .recv_timeout(Duration::from_secs(2))
            .expect("actual bounded writer wake"),
        AppWindowCommand::HostWriterProgress
    ));
    writer
        .acknowledge_progress()
        .expect("one wake acknowledgement");
}

#[test]
fn value_terminal_full_restores_exact_source_then_keeps_credit_through_actual_write() {
    fixture(|router, peer, writer, outcomes, windows| {
        settle(router, refused());
        let source_pointer = diagnostic_pointer(router);
        let occupied = writer
            .try_reserve_value(Instant::now() + Duration::from_secs(3))
            .expect("one occupied value position");
        router
            .service_compat_writer_progress()
            .expect("Full retains settled record");
        assert_eq!(
            diagnostic_pointer(router),
            source_pointer,
            "no cloned source or payload queue"
        );
        assert!(outcomes.try_recv().is_err(), "Full is not native release");
        assert!(
            windows.try_recv().is_err(),
            "free CONTROL positions cannot wake a blocked VALUE interest"
        );
        let (dequeued, resume) = writer.hold_next_dequeue_for_test();
        drop(occupied);
        progress(windows, writer);
        router
            .service_compat_writer_progress()
            .expect("actual final admission");
        dequeued
            .recv_timeout(Duration::from_secs(2))
            .expect("worker dequeued before physical write");
        {
            let state = router
                .compat
                .as_ref()
                .expect("ledger")
                .lock()
                .expect("charged native record");
            let record = state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("charge remains");
            assert!(record.pending_reply.is_none());
            assert!(record.primary_write.is_some());
            assert!(!record.terminal_preparing);
            assert_eq!(record.retained, 4096);
        }
        router
            .service_compat_writer_progress()
            .expect("pending receipt inspection");
        assert!(
            outcomes.try_recv().is_err(),
            "dequeue is not physical completion"
        );
        assert!(matches!(
            writer.try_reserve_value(Instant::now() + Duration::from_secs(2)),
            Err(WriterRefusal::Full)
        ));
        assert!(router.shutdown.transition.try_lock().is_ok());
        assert!(router.current.try_lock().is_ok());
        resume.send(()).expect("allow original physical writer");
        let (header, bytes) =
            keld_ipc::link::read_frame(peer).expect("actual original pipe terminal");
        assert_eq!(header.corr, CorrelationId(77));
        assert_eq!(
            compat_control::decode_response(&bytes).expect("canonical response"),
            CompatControlResponse::Deliver {
                disposition: terminal(refused())
            }
        );
        progress(windows, writer);
        router
            .service_compat_writer_progress()
            .expect("actual completion drains record");
        assert!(matches!(
            outcomes
                .recv_timeout(Duration::from_secs(2))
                .expect("separate physical release"),
            RendererBridgeOutcome::EventReleased { request: 77, .. }
        ));
        assert!(
            router
                .compat
                .as_ref()
                .expect("ledger")
                .lock()
                .expect("released")
                .ledger
                .data[Direction::MainToRenderer.index()]
            .is_none()
        );
        drop(
            writer
                .try_reserve_value(Instant::now() + Duration::from_secs(2))
                .expect("only physical completion releases capacity"),
        );
    });
}

#[test]
fn actual_try_send_full_keeps_record_and_source_until_a_real_retry() {
    fixture(|router, peer, writer, outcomes, windows| {
        settle(router, refused());
        let source_pointer = diagnostic_pointer(router);
        writer.fail_next_send_for_test(WriterRefusal::Full);
        router
            .service_compat_writer_progress()
            .expect("actual final try_send Full");
        assert_eq!(diagnostic_pointer(router), source_pointer);
        {
            let state = router
                .compat
                .as_ref()
                .expect("ledger")
                .lock()
                .expect("source custody");
            let record = state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("same record");
            assert_eq!(record.pending_reply, Some(CorrelationId(77)));
            assert!(record.primary_write.is_none());
            assert!(!record.terminal_preparing);
        }
        assert!(outcomes.try_recv().is_err());
        progress(windows, writer);
        router
            .service_compat_writer_progress()
            .expect("retry exact settled record");
        let (header, _) = keld_ipc::link::read_frame(peer).expect("only admitted retry wrote");
        assert_eq!(header.corr, CorrelationId(77));
        progress(windows, writer);
        router
            .service_compat_writer_progress()
            .expect("complete exact record");
        assert!(matches!(
            outcomes
                .recv_timeout(Duration::from_secs(2))
                .expect("physical release"),
            RendererBridgeOutcome::EventReleased { request: 77, .. }
        ));
    });
}

#[test]
fn fixed_consumer_terminal_uses_reserved_control_while_value_position_is_full() {
    fixture(|router, peer, writer, outcomes, windows| {
        settle(router, ConsumerDisposition::Consumed);
        let occupied = writer
            .try_reserve_value(Instant::now() + Duration::from_secs(3))
            .expect("occupied ordinary class");
        router
            .service_compat_writer_progress()
            .expect("fixed terminal uses reserved control");
        let (header, bytes) = keld_ipc::link::read_frame(peer).expect("actual CONTROL pipe frame");
        assert_eq!(header.corr, CorrelationId(77));
        assert_eq!(
            compat_control::decode_response(&bytes).expect("canonical fixed terminal"),
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::Consumed
            }
        );
        progress(windows, writer);
        router
            .service_compat_writer_progress()
            .expect("actual control completion");
        assert!(matches!(
            outcomes
                .recv_timeout(Duration::from_secs(2))
                .expect("native release"),
            RendererBridgeOutcome::EventReleased { request: 77, .. }
        ));
        assert!(
            matches!(
                writer.try_reserve_value(Instant::now() + Duration::from_secs(2)),
                Err(WriterRefusal::Full)
            ),
            "CONTROL completion cannot release unrelated VALUE"
        );
        drop(occupied);
    });
}
