//! X11 component proof: real pipe/FIFO and actual retained accounting tokens.
//! Native callbacks and the document fixture are modeled, not COM acceptance.
use super::{Arc, AtomicBool, NativeControlReservation, Ordering, PrimaryCallerContext};
use crate::app_session::compat_router::native_event_retirement::tests::fixture::{
    EventFixture, fixture,
};
use crate::app_session::compat_router::{
    CompatControlResponse, Direction, NativeRouteKey, RendererBridgeOutcome, RendererRoleWitness,
    TerminalDisposition, WindowOwner, WvConsumerDisposition,
};
use crate::app_session::primary_writer::{
    ControlMessage, WriteReceipt, WriterRefusal, admit_locked,
};
use crate::app_session::window_router::WindowSession;
use keld_ipc::frame::{CorrelationId, FrameKind};
use std::sync::mpsc::SyncSender;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
enum ReleaseOrder {
    NativeFirst,
    WriterFirst,
}

/// On a red assertion the existing physical worker is still released before join.
struct HeldWorker(Option<SyncSender<()>>);
impl HeldWorker {
    fn release(&mut self) {
        self.0
            .take()
            .expect("one actual physical release")
            .send(())
            .expect("original worker still owns the hold");
    }
}
impl Drop for HeldWorker {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.try_send(());
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ControlCensus {
    correlation: u32,
    retained: usize,
    allocation: usize,
    released: bool,
    application_done: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct Census {
    data: (NativeRouteKey, usize, usize, bool, bool, bool),
    controls: [Option<ControlCensus>; 4],
    writer_positions: usize,
}

fn census(f: &EventFixture<'_>) -> Census {
    let state = f.owner.lock().expect("same accounting owner");
    let data = state.ledger.data[Direction::MainToRenderer.index()]
        .as_ref()
        .expect("original DATA stays present");
    let storage = data.native_storage.as_ref().expect("original native token");
    Census {
        data: (
            data.key,
            data.retained,
            Arc::as_ptr(storage) as usize,
            storage.load(Ordering::Acquire),
            data.consumer_done,
            data.native_done,
        ),
        controls: std::array::from_fn(|slot| {
            let record = state.ledger.controls[slot].as_ref()?;
            let released = record.native_release.as_ref()?;
            Some(ControlCensus {
                correlation: record.correlation,
                retained: record.retained,
                allocation: Arc::as_ptr(released) as usize,
                released: released.load(Ordering::Acquire),
                application_done: record.application_done,
            })
        }),
        writer_positions: f
            .writer
            .snapshot_target_writes(f.target)
            .expect("same five-position writer")
            .into_iter()
            .flatten()
            .count(),
    }
}

fn saturated_controls(
    f: &EventFixture<'_>,
    deadline: Instant,
) -> (Vec<NativeControlReservation>, Vec<WriteReceipt>) {
    let mut native = Vec::with_capacity(4);
    let mut receipts = Vec::with_capacity(4);
    for correlation in 101..=104 {
        let token = {
            let mut state = f.owner.lock().expect("same CONTROL owner");
            let window = state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("original DATA")
                .native_window;
            state
                .ledger
                .reserve_control(f.caller, correlation, super::MAX_CONTROL_BYTES)
                .expect("one of four original CONTROL records");
            state
                .ledger
                .lease_existing_control_scoped(
                    f.caller,
                    correlation,
                    0,
                    Some(Arc::clone(f.owner)),
                    window,
                )
                .expect("same record acquires its native storage half")
        };
        let frame = f
            .writer
            .try_prepare_control(
                Arc::clone(f.target),
                ControlMessage::Ping {
                    channel: keld_ipc::LIFECYCLE_CHANNEL,
                    correlation: CorrelationId(correlation),
                },
                deadline,
            )
            .expect("one of four actual writer CONTROL positions");
        let receipt = {
            let _transition = f.router.shutdown.transition_guard();
            let current = f.router.current.lock().expect("actual caller fence");
            let mut state = f.owner.lock().expect("same CONTROL source");
            let record = state
                .ledger
                .controls
                .iter_mut()
                .flatten()
                .find(|record| record.correlation == correlation)
                .expect("exact original correlation");
            let receipt = admit_locked(current.as_ref().expect("actual caller"), frame)
                .expect("real FIFO admission");
            record.primary_write = Some(receipt.observation());
            receipt
        };
        native.push(token);
        receipts.push(receipt);
    }
    (native, receipts)
}

fn assert_wrong_releases(
    f: &EventFixture<'_>,
    foreign_document: &keld_wv::renderer_contract::NativeDocumentFixture,
) {
    let before = census(f);
    let original =
        f.native
            .event_consumer_request(f.key.message, f.role(), WvConsumerDisposition::Consumed);
    for key in [
        NativeRouteKey {
            webview: f.key.webview + 1,
            ..f.key
        },
        NativeRouteKey {
            navigation: f.key.navigation + 1,
            ..f.key
        },
        NativeRouteKey {
            message: f.key.message + 1,
            ..f.key
        },
    ] {
        f.router
            .complete_native_event(f.owner, f.caller, key, &original)
            .expect("wrong scalar key is inert");
        assert_eq!(
            census(f),
            before,
            "wrong view/document/delivery changed custody"
        );
    }

    let foreign = PrimaryCallerContext {
        reader_stop: Arc::new(AtomicBool::new(false)),
        ..f.caller.clone()
    };
    f.router
        .complete_native_event(f.owner, &foreign, f.key, &original)
        .expect("same-number foreign caller is inert");
    assert_eq!(census(f), before, "foreign caller changed original DATA");

    let wrong_role = f.native.event_consumer_request(
        f.key.message,
        RendererRoleWitness::new(Arc::new(AtomicBool::new(false))),
        WvConsumerDisposition::Consumed,
    );
    f.router
        .complete_native_event(f.owner, f.caller, f.key, &wrong_role)
        .expect("foreign native role is inert");
    assert_eq!(
        census(f),
        before,
        "foreign native role changed original DATA"
    );

    let wrong_document = foreign_document.event_consumer_request(
        f.key.message,
        f.role(),
        WvConsumerDisposition::Consumed,
    );
    assert_eq!(
        wrong_document
            .document_lease()
            .expect("foreign lease")
            .stamp(),
        original.document_lease().expect("original lease").stamp(),
        "independent owners deliberately share scalar view/generation"
    );
    f.router
        .complete_native_event(f.owner, f.caller, f.key, &wrong_document)
        .expect("foreign irreversible lease is inert");
    assert_eq!(
        census(f),
        before,
        "equal numeric document changed original DATA"
    );

    {
        let mut state = f.owner.lock().expect("same CONTROL source");
        for correlation in 101..=104 {
            state.ledger.release_control(&foreign, correlation);
        }
        state.ledger.release_control(f.caller, 105);
    }
    assert_eq!(
        census(f),
        before,
        "wrong caller/correlation released CONTROL"
    );
}

fn release_application_halves(f: &EventFixture<'_>) {
    let mut state = f.owner.lock().expect("same source correlation owner");
    for correlation in 101..=104 {
        state.ledger.release_control(f.caller, correlation);
    }
    assert_eq!(state.ledger.controls_in_flight(), 4);
}

fn assert_original_data_settles(f: &mut EventFixture<'_>, storage_was_released: bool) {
    let consumed =
        f.native
            .event_consumer_request(f.key.message, f.role(), WvConsumerDisposition::Consumed);
    f.router
        .complete_native_event(f.owner, f.caller, f.key, &consumed)
        .expect("original modeled consumer reaches actual completion owner");
    let returned = f
        .native
        .event_quiescence_request(f.key.message, f.role(), true);
    f.router
        .complete_native_event(f.owner, f.caller, f.key, &returned)
        .expect("independent modeled native completion");
    f.wait_writes();
    let (header, payload) = keld_ipc::link::read_frame(f.peer).expect("actual source terminal");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(77))
    );
    assert_eq!(
        keld_ipc::codec::decode::<CompatControlResponse>(&payload).expect("canonical terminal"),
        CompatControlResponse::Deliver {
            disposition: TerminalDisposition::Consumed
        }
    );
    f.router
        .service_compat_writer_progress()
        .expect("same completion source");
    assert!(matches!(
        f.outcomes.try_recv(),
        Ok(RendererBridgeOutcome::EventReleased { webview, navigation, request })
            if webview.0 == f.key.webview && navigation == u64::from(f.key.navigation)
                && request == f.key.message
    ));
    assert!(
        f.outcomes.try_recv().is_err(),
        "same DATA emits one release"
    );
    assert_eq!(
        f.owner.lock().expect("same DATA").ledger.data[Direction::MainToRenderer.index()].is_none(),
        storage_was_released,
        "native storage owns its independent final half"
    );
    drop(f.storage.take());
    f.router
        .service_compat_writer_progress()
        .expect("physical storage completion");
    let state = f.owner.lock().expect("final actual accounting owner");
    assert!(state.ledger.data.iter().all(Option::is_none));
    assert_eq!(state.ledger.controls_in_flight(), 0);
    assert_eq!(state.ledger.retained_bytes(), 0);
}

fn hold_actual_value(f: &EventFixture<'_>, deadline: Instant) -> (HeldWorker, WriteReceipt) {
    let (dequeued, resume) = f.writer.hold_next_dequeue_for_test();
    let held = HeldWorker(Some(resume));
    let frame = f
        .writer
        .try_reserve_value(deadline)
        .expect("sole VALUE")
        .copy_value(
            Arc::clone(f.target),
            FrameKind::Reply,
            keld_ipc::ECHO_CHANNEL,
            CorrelationId(9),
            &[7; 32],
        )
        .expect("one charged exact physical payload");
    let value = {
        let _transition = f.router.shutdown.transition_guard();
        let current = f.router.current.lock().expect("original caller");
        admit_locked(current.as_ref().expect("actual active link"), frame)
            .expect("actual first FIFO packet")
    };
    dequeued
        .recv_timeout(Duration::from_secs(2))
        .expect("real worker dequeued VALUE");
    (held, value)
}

fn read_original_fifo(f: &mut EventFixture<'_>) {
    let (header, payload) = keld_ipc::link::read_frame(f.peer).expect("real VALUE bytes");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(9))
    );
    assert_eq!(
        payload, [7; 32],
        "same bounded payload reaches the actual pipe once"
    );
    for correlation in 101..=104 {
        let (header, payload) = keld_ipc::link::read_frame(f.peer).expect("real FIFO CONTROL");
        assert_eq!(
            (header.kind, header.corr),
            (FrameKind::Ping, CorrelationId(correlation))
        );
        assert!(payload.is_empty(), "canonical Ping has no payload");
    }
}

fn exercise(order: ReleaseOrder) {
    fixture(|mut f| {
        let mut foreign_windows = WindowSession::new(true);
        let (_foreign_root, foreign_native, _) = foreign_windows
            .fixture_original_retirement_route(WindowOwner::new(
                f.caller.identity,
                f.caller.attempt,
            ))
            .expect("separate admitted graph and irreversible document owner");
        let deadline = Instant::now() + Duration::from_secs(3);
        let (mut held, value) = hold_actual_value(&f, deadline);
        let (mut native, receipts) = saturated_controls(&f, deadline);
        let full = census(&f);
        assert_eq!(full.writer_positions, 5);
        assert_eq!(full.controls.iter().flatten().count(), 4);
        assert!(!full.data.3 && !full.data.4 && !full.data.5);
        assert!(matches!(
            f.writer.try_reserve_value(deadline),
            Err(WriterRefusal::Full)
        ));
        assert!(matches!(
            f.writer.try_reserve_control(deadline),
            Err(WriterRefusal::Full)
        ));
        assert!(
            f.owner
                .lock()
                .expect("same native CONTROL ledger")
                .reserve_native_control(f.caller, 1, 1025, Arc::clone(f.owner))
                .is_none()
        );
        assert_wrong_releases(&f, &foreign_native);

        let native_first = matches!(order, ReleaseOrder::NativeFirst);
        if native_first {
            release_application_halves(&f);
            native.clear();
            drop(f.storage.take());
            f.router
                .service_compat_writer_progress()
                .expect("ordinary progress retains the pending physical CONTROL");
            let state = f.owner.lock().expect("physical packet still held");
            assert_eq!(
                state.ledger.controls_in_flight(),
                4,
                "both logical halves cannot refund the pending physical writes"
            );
            assert!(state.ledger.data[Direction::MainToRenderer.index()].is_some());
        }
        held.release();
        read_original_fifo(&mut f);
        value
            .observation()
            .wait_quiesced()
            .expect("actual VALUE return");
        for receipt in &receipts {
            receipt
                .observation()
                .wait_quiesced()
                .expect("actual CONTROL return");
        }
        drop(value);
        drop(receipts);
        if !native_first {
            release_application_halves(&f);
            native.clear();
        }
        f.router
            .service_compat_writer_progress()
            .expect("ordinary source progress reaps the completed CONTROL records");
        {
            let state = f.owner.lock().expect("same actual completion owner");
            assert_eq!(state.ledger.controls_in_flight(), 0);
        }
        assert_original_data_settles(&mut f, native_first);
        assert!(
            f.writer
                .snapshot_target_writes(f.target)
                .expect("same writer")
                .iter()
                .all(Option::is_none)
        );
        drop(
            f.writer
                .try_reserve_value(deadline)
                .expect("actual VALUE can be reused"),
        );
        drop(
            f.writer
                .try_reserve_control(deadline)
                .expect("actual CONTROL can be reused"),
        );
        assert!(f.no_fatal());
    });
}

#[test]
fn full_data_and_four_controls_ignore_wrong_releases_until_native_then_writer_disposal() {
    exercise(ReleaseOrder::NativeFirst);
}

#[test]
fn full_data_and_four_controls_ignore_wrong_releases_until_writer_then_native_disposal() {
    exercise(ReleaseOrder::WriterFirst);
}
