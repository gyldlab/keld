//! Same DATA record release orders and actual primary completion. Native facts
//! are explicitly modeled; a real retained graph supplies the checked lease.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use crate::app_session::PrimaryRouterHandle;
use crate::app_session::primary_writer::{ControlMessage, RetainedWrite, admit_locked};
use crate::app_session::window_registry::WindowOwner;
use crate::app_session::window_router::WindowSession;
use keld_wv::{AppWindowCommand, AppWindowEvent};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

fn with_storage(
    run: impl FnOnce(
        &Arc<Mutex<CompatSession>>,
        &PrimaryCallerContext,
        NativeRouteKey,
        NativeDataReservation,
        &std::sync::mpsc::Receiver<RendererBridgeOutcome>,
        &PrimaryRouterHandle,
        &std::sync::mpsc::Receiver<AppWindowCommand>,
    ),
) {
    super::super::owned_tests::fixture(|router, caller, _, _, _, outcomes, commands| {
        let mut windows = WindowSession::new(true);
        let (root, native, route) = windows
            .fixture_original_retirement_route(WindowOwner::new(caller.identity, caller.attempt))
            .expect("same retained graph fixture");
        let key = NativeRouteKey {
            webview: route.document.stamp().view().0,
            navigation: route.document.stamp().observed_generation(),
            message: 77,
        };
        let owner = router.compat.as_ref().expect("same ledger");
        let token = {
            let mut state = owner.lock().expect("actual DATA owner");
            state
                .ledger
                .reserve(Direction::MainToRenderer, key, caller, 512)
                .expect("one DATA slot");
            state.ledger.data[Direction::MainToRenderer.index()]
                .as_mut()
                .expect("record")
                .document = Some(route.document.clone());
            state.ledger.data[Direction::MainToRenderer.index()]
                .as_mut()
                .expect("same DATA")
                .native_window = Some(route.window);
            state
                .ledger
                .lease_native_data(caller, key, &route.document, Arc::clone(owner))
                .expect("same-record storage token")
        };
        run(owner, caller, key, token, outcomes, router, commands);
        drop(native);
        drop(root);
    });
}

#[test]
fn data_storage_drop_wakes_and_clears_same_record_without_control_interest() {
    with_storage(|owner, caller, key, token, outcomes, router, commands| {
        assert!(matches!(commands.try_recv(), Err(TryRecvError::Empty)));
        {
            let mut state = owner.lock().expect("same retained DATA owner");
            let charge = state.ledger.data[Direction::MainToRenderer.index()]
                .as_mut()
                .expect("original native DATA record");
            assert!(charge.key == key && charge.owned_by(caller));
            // Model the two independent receipts, as in the adjacent owner test.
            charge.consumer_done = true;
            charge.native_done = true;
            state
                .release_completed_native_event()
                .expect("same record publishes its one release");
            assert!(state.ledger.data[Direction::MainToRenderer.index()].is_some());
        }
        assert!(matches!(
            outcomes.try_recv(),
            Ok(RendererBridgeOutcome::EventReleased {
                webview,
                navigation,
                request,
            }) if webview.0 == key.webview
                && navigation == u64::from(key.navigation)
                && request == key.message
        ));
        assert!(matches!(outcomes.try_recv(), Err(TryRecvError::Empty)));
        assert!(
            matches!(commands.try_recv(), Err(TryRecvError::Empty)),
            "no unrelated writer or CONTROL-capacity wake precedes DATA drop"
        );

        // The actual same-record token is the only source of the next marker.
        // No native CONTROL interest or additional application call is registered.
        drop(token);
        let command = commands
            .try_recv()
            .expect("native DATA drop must publish HostWriterProgress without CONTROL interest");
        assert!(matches!(command, AppWindowCommand::HostWriterProgress));
        assert!(
            matches!(commands.try_recv(), Err(TryRecvError::Empty)),
            "one physical drop publishes only the existing coalesced unit marker"
        );
        {
            let state = owner.lock().expect("drop does not service the ledger");
            let charge = state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("same DATA remains until ordinary source service");
            assert!(charge.key == key && charge.owned_by(caller));
            assert!(charge.consumer_done && charge.native_done && charge.event_release_sent);
            assert!(
                charge
                    .native_storage
                    .as_ref()
                    .expect("same physical storage flag")
                    .load(Ordering::Acquire)
            );
        }
        assert!(matches!(
            router
                .window_native_event(AppWindowEvent::HostWriterProgress)
                .expect("ordinary handler acknowledges the actual marker and services its source"),
            crate::app_session::window_router::NativePublicationDisposition::Handled
        ));
        assert!(
            owner
                .lock()
                .expect("same DATA after ordinary service")
                .ledger
                .data[Direction::MainToRenderer.index()]
            .is_none(),
            "ordinary progress clears the exact physically released DATA charge"
        );
        assert!(matches!(outcomes.try_recv(), Err(TryRecvError::Empty)));
        assert!(matches!(commands.try_recv(), Err(TryRecvError::Empty)));
    });
}

#[test]
fn data_storage_and_consumer_native_receipts_are_independent_in_both_orders() {
    for storage_first in [false, true] {
        with_storage(|owner, _, _, token, outcomes, _, _| {
            let mut token = Some(token);
            if storage_first {
                drop(token.take());
            }
            {
                let mut state = owner.lock().expect("same record");
                state.ledger.clear_if_complete(Direction::MainToRenderer);
                assert!(
                    state.ledger.data[Direction::MainToRenderer.index()].is_some(),
                    "storage release is no consumer/native fact"
                );
                state.ledger.data[Direction::MainToRenderer.index()]
                    .as_mut()
                    .expect("record")
                    .consumer_done = true;
                state
                    .release_completed_native_event()
                    .expect("consumer half only");
                assert!(outcomes.try_recv().is_err());
                state.ledger.data[Direction::MainToRenderer.index()]
                    .as_mut()
                    .expect("record")
                    .native_done = true;
                state
                    .release_completed_native_event()
                    .expect("actual prerequisite conjunction");
                state
                    .release_completed_native_event()
                    .expect("idempotent source service");
                assert_eq!(
                    state.ledger.data[Direction::MainToRenderer.index()].is_some(),
                    !storage_first
                );
            }
            assert!(matches!(
                outcomes.try_recv(),
                Ok(RendererBridgeOutcome::EventReleased { request: 77, .. })
            ));
            assert!(
                outcomes.try_recv().is_err(),
                "EventReleased is sent once by the same record"
            );
            drop(token.take());
            let mut state = owner.lock().expect("late actual backing release");
            state
                .release_completed_native_event()
                .expect("same record settles");
            assert!(state.ledger.data[Direction::MainToRenderer.index()].is_none());
            assert!(outcomes.try_recv().is_err());
        });
    }
}

#[test]
fn native_data_growth_is_additive_bounded_and_preserves_original_charge_on_failure() {
    with_storage(|owner, _, _, token, _, _, _| {
        let retained = || {
            owner.lock().expect("record").ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("same DATA")
                .retained
        };
        let original = retained();
        assert_eq!(original, 512 + DATA_STORAGE_METADATA_BYTES);
        assert!(token.try_extend_storage(4096, 12_290));
        assert_eq!(retained(), original + 12_290);
        let grown = retained();
        assert!(!token.try_extend_storage(keld_wv::wv_link::MAX_WV_LINK_ENVELOPE_LEN + 1, 1));
        assert!(!token.try_extend_storage(1, usize::MAX));
        assert!(!token.try_extend_storage(1, MAX_DATA_RETAINED_BYTES));
        assert_eq!(retained(), grown);
        assert!(token.try_extend_storage(4096, MAX_DATA_RETAINED_BYTES - grown));
        assert_eq!(retained(), MAX_DATA_RETAINED_BYTES);
        assert!(!token.try_extend_storage(1, 1));
        drop(token);
        let state = owner.lock().expect("only storage released");
        let record = state.ledger.data[Direction::MainToRenderer.index()]
            .as_ref()
            .expect("native/consumer halves still retained");
        assert!(!record.native_done && !record.consumer_done);
    });
}

#[test]
fn retired_same_number_successor_cannot_replace_the_original_native_storage_record() {
    with_storage(|owner, caller, key, token, outcomes, _, _| {
        let successor = PrimaryCallerContext {
            reader_stop: Arc::new(AtomicBool::new(false)),
            ..caller.clone()
        };
        let mut state = owner.lock().expect("same actual ledger");
        let document = state.ledger.data[Direction::MainToRenderer.index()]
            .as_ref()
            .expect("A DATA")
            .document
            .as_ref()
            .expect("lease")
            .clone();
        assert!(
            state
                .ledger
                .lease_native_data(&successor, key, &document, Arc::clone(owner))
                .is_none()
        );
        state.ledger.role_retired(caller);
        caller.reader_stop.store(true, Ordering::Release);
        assert!(
            state
                .ledger
                .reserve(Direction::MainToRenderer, key, &successor, 1)
                .is_err()
        );
        drop(state);
        drop(token); // late native backing alone cannot release A or manufacture B facts
        let mut state = owner.lock().expect("late original token");
        state
            .release_completed_native_event()
            .expect("not complete");
        let record = state.ledger.data[Direction::MainToRenderer.index()]
            .as_mut()
            .expect("A still owns same record");
        assert!(record.owned_by(caller) && !record.owned_by(&successor));
        assert!(!record.consumer_done && !record.native_done);
        record.consumer_done = true;
        record.native_done = true;
        state
            .release_completed_native_event()
            .expect("modeled final native/consumer facts");
        assert!(state.ledger.data[Direction::MainToRenderer.index()].is_none());
        assert!(matches!(
            outcomes.try_recv(),
            Ok(RendererBridgeOutcome::EventReleased { request: 77, .. })
        ));
    });
}

#[test]
fn event_release_waits_for_the_actual_terminal_write_and_its_same_record_settlement() {
    super::super::owned_tests::fixture(|router, caller, target, peer, writer, outcomes, _| {
        let owner = router.compat.as_ref().expect("ledger");
        let (dequeued, resume) = writer.hold_next_dequeue_for_test();
        let prepared = writer
            .try_prepare_control(
                Arc::clone(target),
                ControlMessage::CompatReleased {
                    correlation: CorrelationId(77),
                },
                Instant::now() + Duration::from_secs(3),
            )
            .expect("actual fixed terminal frame");
        let observation;
        {
            let _transition = router.shutdown.transition_guard();
            let current = router.current.lock().expect("actual final fence");
            let mut state = owner.lock().expect("existing source record");
            state
                .ledger
                .reserve(
                    Direction::MainToRenderer,
                    NativeRouteKey {
                        webview: 7,
                        navigation: 1,
                        message: 77,
                    },
                    caller,
                    512,
                )
                .expect("modeled settled DATA");
            let receipt =
                admit_locked(current.as_ref().expect("same A"), prepared).expect("actual enqueue");
            observation = receipt.observation();
            let record = state.ledger.data[Direction::MainToRenderer.index()]
                .as_mut()
                .expect("source");
            record.consumer_done = true;
            record.native_done = true;
            record.primary_write = Some(RetainedWrite::new(receipt));
        }
        let waited = dequeued.recv_timeout(Duration::from_secs(2));
        let before = {
            let mut state = owner
                .lock()
                .expect("retained while physical writer is held");
            state
                .release_completed_native_event()
                .expect("pending is no release");
            state.ledger.data[Direction::MainToRenderer.index()].is_some()
                && outcomes.try_recv().is_err()
        };
        resume
            .send(())
            .expect("release exact physical worker before assertions");
        waited.expect("actual dequeue barrier");
        assert!(before);
        let (header, _) = keld_ipc::link::read_frame(peer).expect("actual terminal bytes");
        assert_eq!(header.corr, CorrelationId(77));
        observation.wait_quiesced().expect("actual write returned");
        let mut state = owner.lock().expect("same owned receipt");
        state
            .release_completed_native_event()
            .expect("physical completion alone is not source settlement");
        assert!(outcomes.try_recv().is_err());
        let record = state.ledger.data[Direction::MainToRenderer.index()]
            .as_mut()
            .expect("same pending source");
        assert!(
            record
                .primary_write
                .as_mut()
                .expect("receipt")
                .settle()
                .expect("actual poll")
        );
        assert!(matches!(
            RetainedWrite::take_completed(&mut record.primary_write),
            Some(None)
        ));
        state
            .release_completed_native_event()
            .expect("actual source settlement");
        assert!(matches!(
            outcomes.try_recv(),
            Ok(RendererBridgeOutcome::EventReleased { request: 77, .. })
        ));
        assert!(state.ledger.data[Direction::MainToRenderer.index()].is_none());
    });
}
