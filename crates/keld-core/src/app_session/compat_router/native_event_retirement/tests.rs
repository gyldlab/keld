//! Actual Core records/pipes with explicitly modeled native proof. No test here
//! manufactures a production `NativeEventRetirement` or claims native Close.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use crate::app_session::primary_writer::{
    ControlMessage, ValueFreeTerminal, WriterRefusal, admit_locked,
};
use keld_ipc::codec::decode;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
pub(in crate::app_session::compat_router) mod fixture;
use fixture::fixture;
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}

#[test]
fn native_retirement_matches_exact_role_lease_and_preserves_first_proof_without_receipts() {
    fixture(|f| {
        f.native.revoke();
        let document = f.native.document();
        let role = f.role();
        let foreign = RendererRoleWitness::new(Arc::new(AtomicBool::new(true)));
        let mut state = f.owner.lock().expect("actual DATA");
        assert!(
            !state
                .ledger
                .apply_native_event_retirement(&RetirementInput {
                    delivery: f.key.message,
                    document: &document,
                    role: &foreign,
                    certainty: compat_control::DispatchCertainty::NotDispatched
                })
        );
        assert!(
            !state
                .ledger
                .apply_native_event_retirement(&RetirementInput {
                    delivery: f.key.message + 1,
                    document: &document,
                    role: &role,
                    certainty: compat_control::DispatchCertainty::NotDispatched
                })
        );
        for certainty in [
            compat_control::DispatchCertainty::NotDispatched,
            compat_control::DispatchCertainty::OutcomeUnknown,
        ] {
            assert!(
                state
                    .ledger
                    .apply_native_event_retirement(&RetirementInput {
                        delivery: f.key.message,
                        document: &document,
                        role: &role,
                        certainty
                    })
            );
        }
        let row = state.ledger.data[Direction::MainToRenderer.index()]
            .as_ref()
            .expect("retained");
        assert_eq!(
            row.native_retirement,
            Some(compat_control::DispatchCertainty::NotDispatched)
        );
        assert!(!row.consumer_done && !row.native_done);
        assert_eq!(row.pending_reply, Some(CorrelationId(77)));
    });
}

#[test]
fn first_actual_consumer_or_retirement_selection_is_the_one_written_on_the_real_pipe() {
    for consumer_first in [true, false] {
        fixture(|mut f| {
            let input =
                f.native
                    .event_consumer_request(77, f.role(), WvConsumerDisposition::Consumed);
            if consumer_first {
                f.router
                    .complete_native_event(f.owner, f.caller, f.key, &input)
                    .expect("actual consumer path");
            }
            f.model_retirement(compat_control::DispatchCertainty::NotDispatched);
            if !consumer_first {
                f.router
                    .complete_native_event(f.owner, f.caller, f.key, &input)
                    .expect("late consumer is inert");
            }
            {
                let state = f.owner.lock().expect("source");
                let row = state.ledger.data[Direction::MainToRenderer.index()]
                    .as_ref()
                    .expect("same row");
                assert_eq!(row.consumer_done, consumer_first);
                assert!(!row.native_done);
            }
            f.router
                .service_compat_writer_progress()
                .expect("same terminal source");
            let (header, bytes) = keld_ipc::link::read_frame(f.peer).expect("actual terminal");
            assert_eq!(header.corr, CorrelationId(77));
            let expected = if consumer_first {
                TerminalDisposition::Consumed
            } else {
                TerminalDisposition::RouteRetired {
                    certainty: compat_control::DispatchCertainty::NotDispatched,
                }
            };
            assert_eq!(
                decode::<CompatControlResponse>(&bytes).expect("canonical DTO"),
                CompatControlResponse::Deliver {
                    disposition: expected
                }
            );
            f.wait_writes();
            f.router
                .service_compat_writer_progress()
                .expect("actual completion service");
            assert!(matches!(
                f.outcomes.try_recv(),
                Ok(RendererBridgeOutcome::EventReleased { request: 77, .. })
            ));
            assert!(
                f.owner.lock().expect("storage held").ledger.data
                    [Direction::MainToRenderer.index()]
                .is_some()
            );
            drop(f.storage.take());
            f.router
                .service_compat_writer_progress()
                .expect("actual storage release");
            assert!(
                f.owner.lock().expect("settled").ledger.data[Direction::MainToRenderer.index()]
                    .is_none()
            );
            assert!(f.outcomes.try_recv().is_err());
        });
    }
}

#[test]
fn full_control_restores_same_retirement_source_and_actual_release_rearms_progress() {
    fixture(|mut f| {
        let mut full: Vec<_> = (0..4)
            .map(|_| {
                f.writer
                    .try_reserve_control(deadline())
                    .expect("four actual positions")
            })
            .collect();
        f.model_retirement(compat_control::DispatchCertainty::NotDispatched);
        f.router
            .service_compat_writer_progress()
            .expect("Full retains original");
        {
            let state = f.owner.lock().expect("same source");
            let row = state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("retained");
            assert!(!row.terminal_preparing && row.primary_write.is_none());
            assert_eq!(row.pending_reply, Some(CorrelationId(77)));
        }
        assert!(f.no_fatal());
        drop(full.pop());
        assert!(matches!(
            f.commands.try_recv(),
            Ok(keld_wv::AppWindowCommand::HostWriterProgress)
        ));
        f.router
            .service_compat_writer_progress()
            .expect("one released position");
        let (header, bytes) = keld_ipc::link::read_frame(f.peer).expect("actual retired terminal");
        assert_eq!(header.corr, CorrelationId(77));
        assert!(matches!(
            decode::<CompatControlResponse>(&bytes).expect("canonical"),
            CompatControlResponse::Deliver {
                disposition: TerminalDisposition::RouteRetired {
                    certainty: compat_control::DispatchCertainty::NotDispatched
                }
            }
        ));
        f.wait_writes();
        drop(full);
        f.router
            .service_compat_writer_progress()
            .expect("same receipt");
        drop(f.storage.take());
        f.router
            .service_compat_writer_progress()
            .expect("same storage");
    });
}

#[test]
fn selected_diagnostic_survives_full_and_role_loss_without_becoming_fixed_control() {
    fixture(|mut f| {
        let input = f.native.event_consumer_request(
            77,
            f.role(),
            WvConsumerDisposition::Refused {
                code: String::from("KELD-IPC-017"),
                message: String::from("original retained diagnostic"),
            },
        );
        f.router
            .complete_native_event(f.owner, f.caller, f.key, &input)
            .expect("real consumer path");
        let value = f
            .writer
            .try_reserve_value(deadline())
            .expect("sole VALUE occupied");
        f.model_retirement(compat_control::DispatchCertainty::OutcomeUnknown);
        f.router
            .service_compat_writer_progress()
            .expect("diagnostic Full remains source");
        let pointer = {
            let state = f.owner.lock().expect("same row");
            let row = state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("source");
            let Some(ConsumerDisposition::Refused { error }) = &row.terminal else {
                panic!("original diagnostic")
            };
            error.message.as_ptr()
        };
        let refusal = {
            let _transition = f.router.shutdown.transition_guard();
            let _current = f.router.current.lock().expect("canonical retirement");
            let Err(error) = f.router.compat_retirement_reply_locked(f.caller) else {
                panic!("a diagnostic cannot be rewritten into CONTROL OutcomeUnknown");
            };
            error
        };
        assert_eq!(refusal.code, "KELD-CORE-037");
        assert_eq!(refusal.phase, "compat retirement snapshot");
        assert_eq!(
            refusal.detail,
            "selected original diagnostic/source cannot be replaced by a fixed retirement"
        );
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        assert_eq!(
            PrimaryRouterHandle::observed_selected_retirement_refusal(&refusal),
            Some("selected-diagnostic"),
            "cause comes from the actual canonical source refusal after guards end"
        );
        drop(value);
        let retired = f.router.retire_generation_after_write_failure(f.caller);
        assert!(
            retired.is_err(),
            "this ambiguous selected-source race stays explicitly unqualified"
        );
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        if let Err(error) = &retired {
            assert_eq!(
                PrimaryRouterHandle::observed_selected_retirement_refusal(error),
                None,
                "a distinct actual failed-write retirement error is not either exact refusal"
            );
        }
        let state = f.owner.lock().expect("retained after role loss");
        let row = state.ledger.data[Direction::MainToRenderer.index()]
            .as_ref()
            .expect("same source");
        let Some(ConsumerDisposition::Refused { error }) = &row.terminal else {
            panic!("original diagnostic retained")
        };
        assert_eq!(error.message.as_ptr(), pointer);
        assert_eq!(row.pending_reply, Some(CorrelationId(77)));
        assert!(row.primary_write.is_none() && row.retirement_write.is_none());
    });
}

#[test]
fn retirement_and_storage_release_never_refund_an_actual_write_held_past_observer_deadline() {
    for storage_first in [true, false] {
        fixture(|mut f| {
            let (dequeued, resume) = f.writer.hold_next_dequeue_for_test();
            let frame = f
                .writer
                .try_prepare_control(
                    Arc::clone(f.target),
                    ControlMessage::CompatTerminal {
                        correlation: CorrelationId(77),
                        disposition: ValueFreeTerminal::Retired(
                            compat_control::DispatchCertainty::OutcomeUnknown,
                        ),
                    },
                    deadline(),
                )
                .expect("actual fixed terminal");
            let receipt = {
                let _transition = f.router.shutdown.transition_guard();
                let current = f.router.current.lock().expect("same A");
                let mut state = f.owner.lock().expect("same record");
                let receipt =
                    admit_locked(current.as_ref().expect("A"), frame).expect("real admission");
                let row = state.ledger.data[Direction::MainToRenderer.index()]
                    .as_mut()
                    .expect("source");
                row.pending_reply = None;
                row.retired = true;
                row.retirement_write = Some(receipt.observation());
                receipt
            };
            let observation = receipt.observation();
            let entered = dequeued.recv_timeout(Duration::from_secs(2));
            f.model_retirement(compat_control::DispatchCertainty::OutcomeUnknown);
            let timed_out = receipt.wait(Instant::now() + Duration::from_millis(30));
            drop(receipt);
            if storage_first {
                drop(f.storage.take());
            }
            let retained = {
                let mut state = f.owner.lock().expect("held source");
                state
                    .release_quiescent_retired_terminal()
                    .expect("physical pending");
                state.ledger.data[Direction::MainToRenderer.index()].is_some()
                    && f.outcomes.try_recv().is_err()
            };
            resume
                .send(())
                .expect("release actual writer before assertions");
            entered.expect("actual dequeue barrier");
            assert_eq!(timed_out, Err(WriterRefusal::Deadline));
            assert!(retained);
            let (header, _) = keld_ipc::link::read_frame(f.peer).expect("actual terminal bytes");
            assert_eq!(header.corr, CorrelationId(77));
            observation.wait_quiesced().expect("actual return");
            {
                let mut state = f.owner.lock().expect("same completion");
                state
                    .release_quiescent_retired_terminal()
                    .expect("actual quiescence");
                assert_eq!(
                    state.ledger.data[Direction::MainToRenderer.index()].is_some(),
                    !storage_first
                );
            }
            assert!(matches!(
                f.outcomes.try_recv(),
                Ok(RendererBridgeOutcome::EventReleased { request: 77, .. })
            ));
            drop(f.storage.take());
            f.router
                .service_compat_writer_progress()
                .expect("last storage");
            assert!(
                f.owner.lock().expect("settled").ledger.data[Direction::MainToRenderer.index()]
                    .is_none()
            );
            assert!(f.outcomes.try_recv().is_err());
        });
    }
}
