//! Same actual DATA ownership and Runtime installation. Native callback/status
//! inputs are explicitly modeled by the existing debug/profile document fixture.
#![allow(clippy::expect_used, clippy::panic)]
use super::*;
use crate::app_session::compat_router::native_event_retirement::tests::fixture::fixture;
use crate::app_session::primary_writer::ControlMessage;
use keld_ipc::AppLinkDeadlines as _;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

struct Successor {
    router: PrimaryRouterHandle,
    caller: PrimaryCallerContext,
    peer: crate::app_session::BootstrapStream,
}
impl Drop for Successor {
    fn drop(&mut self) {
        let _ = self
            .router
            .retire_generation_after_write_failure(&self.caller);
        let reader = self
            .router
            .readers
            .lock()
            .ok()
            .and_then(|mut readers| readers.remove(&self.caller.attempt));
        if let Some(reader) = reader {
            let _ = reader.join();
        }
    }
}

#[test]
#[cfg_attr(
    windows,
    expect(
        clippy::too_many_lines,
        reason = "Keep actual A retirement, equal-number B installation, retained failed input and B wire bytes in one scenario."
    )
)]
fn stale_failed_event_moves_whole_input_to_same_charge_and_equal_number_b_still_writes() {
    fixture(|mut f| {
        assert!(
            !f.router.window_ready.load(Ordering::Acquire),
            "fixture is before native Ready"
        );
        f.router
            .retire_generation_after_write_failure(f.caller)
            .expect("canonical A retirement");
        let (server, peer) = crate::app_session::tests::primary_test_stream_pair();
        peer.set_app_link_read_deadline(Some(Duration::from_secs(3)))
            .expect("bounded B observer");
        f.router
            .install_generation(f.caller.attempt, server)
            .expect("actual equal-number B installer");
        let target = f
            .router
            .current_primary_writer_target()
            .expect("B target")
            .expect("B live");
        let mut successor = Successor {
            router: f.router.clone(),
            caller: target.caller().clone(),
            peer,
        };
        assert!(
            successor.caller.identity == f.caller.identity
                && successor.caller.attempt == f.caller.attempt
        );
        assert!(!Arc::ptr_eq(
            &successor.caller.reader_stop,
            &f.caller.reader_stop
        ));
        let mut input = Some(f.native.event_quiescence_request(77, f.role(), false));
        assert!(
            input
                .as_ref()
                .expect("original")
                .has_fixed_event_failure_metadata()
        );
        let before = Arc::strong_count(&f.caller.reader_stop);
        let result = f
            .router
            .dispatch_renderer_request_retained(&mut input)
            .expect("typed same-source dispatch");
        assert!(matches!(
            result,
            RendererRequestDisposition::Complete(NativePublicationDisposition::Handled)
        ));
        assert!(input.is_none());
        assert_eq!(
            Arc::strong_count(&f.caller.reader_stop),
            before,
            "ownership transfer cannot destroy the original request"
        );
        {
            let state = f.owner.lock().expect("actual original row");
            let row = state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("same DATA");
            assert!(row.owned_by(f.caller) && !row.owned_by(&successor.caller));
            assert!(row.native_done && !row.consumer_done && row.retained_native_failure.is_some());
            let retained = row
                .retained_native_failure
                .as_ref()
                .expect("same compact input");
            assert_eq!(retained.webview(), WebviewId(f.key.webview));
            assert_eq!(retained.navigation(), u64::from(f.key.navigation));
            assert_eq!(retained.request(), f.key.message);
            assert!(retained.document_lease().same_binding(&f.native.document()));
            assert!(retained.role_witness().same_role(&f.role()));
        }
        // An already occupied retained-input field cannot be overwritten, even
        // by a second fixed-shaped input with the same numbers.
        let mut duplicate = Some(f.native.event_quiescence_request(77, f.role(), false));
        let source = f
            .router
            .capture_renderer_input_source(duplicate.as_ref().expect("duplicate"))
            .expect("exact old source");
        let result = f.router.retain_renderer_input_failure(
            &mut duplicate,
            source.as_ref(),
            app_detail("modeled duplicate failure", "no second transfer"),
        );
        assert!(matches!(
            result,
            RendererRequestDisposition::RetainedFailure {
                disposition: RendererFailureDisposition::RetiredSource,
                ..
            }
        ));
        assert!(duplicate.is_some());
        drop(duplicate);
        drop(source);
        f.router
            .submit_primary_control(
                &successor.caller,
                ControlMessage::Ping {
                    channel: keld_ipc::LIFECYCLE_CHANNEL,
                    correlation: CorrelationId(92),
                },
                Instant::now() + Duration::from_secs(3),
                "actual B continued",
            )
            .expect("same writer progresses B");
        let (header, body) = keld_ipc::link::read_frame(&mut successor.peer).expect("real B Ping");
        assert_eq!(header.kind, FrameKind::Ping);
        assert_eq!(header.corr, CorrelationId(92));
        assert_eq!(body, [] as [u8; 0]);
        assert!(f.no_fatal(), "old A never queued an unfenced Fatal for B");
        f.model_retirement(compat_control::DispatchCertainty::OutcomeUnknown);
        f.router
            .service_compat_writer_progress()
            .expect("same original retirement source");
        assert!(matches!(
            f.outcomes.try_recv(),
            Ok(RendererBridgeOutcome::EventReleased { request: 77, .. })
        ));
        let before_drop = Arc::strong_count(&f.caller.reader_stop);
        assert!(
            f.owner.lock().expect("actual storage held").ledger.data
                [Direction::MainToRenderer.index()]
            .as_ref()
            .expect("same row")
            .retained_native_failure
            .is_some()
        );
        drop(f.storage.take());
        f.router
            .service_compat_writer_progress()
            .expect("actual backing released");
        assert!(
            f.owner.lock().expect("settled").ledger.data[Direction::MainToRenderer.index()]
                .is_none()
        );
        assert_eq!(
            Arc::strong_count(&f.caller.reader_stop) + 2,
            before_drop,
            "only same Charge owner and whole retained request drop after final storage release"
        );
        assert!(f.no_fatal());
    });
}

#[test]
fn current_a_failed_event_queues_exact_fatal_and_keeps_dispatcher_input() {
    fixture(|f| {
        let mut input = Some(f.native.event_quiescence_request(77, f.role(), false));
        let result = f
            .router
            .dispatch_renderer_request_retained(&mut input)
            .expect("typed disposition");
        assert!(matches!(
            result,
            RendererRequestDisposition::RetainedFailure {
                disposition: RendererFailureDisposition::FatalQueued,
                ..
            }
        ));
        assert!(input.is_some());
        let state = f.owner.lock().expect("same original record");
        let row = state.ledger.data[Direction::MainToRenderer.index()]
            .as_ref()
            .expect("DATA retained");
        assert!(row.native_done && !row.consumer_done && row.retained_native_failure.is_none());
        drop(state);
        assert!(matches!(
            f.commands.try_recv(),
            Ok(keld_wv::AppWindowCommand::Fatal)
        ));
        assert!(f.no_fatal(), "no duplicate Fatal");
    });
}

#[test]
fn equal_numbers_with_foreign_role_allocation_cannot_mark_original_native_done() {
    fixture(|f| {
        let foreign = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
        let mut input = Some(f.native.event_quiescence_request(77, foreign, false));
        let result = f
            .router
            .dispatch_renderer_request_retained(&mut input)
            .expect("foreign completion ignored");
        assert!(matches!(
            result,
            RendererRequestDisposition::Complete(NativePublicationDisposition::Handled)
        ));
        assert!(input.is_none());
        let state = f.owner.lock().expect("unchanged original");
        let row = state.ledger.data[Direction::MainToRenderer.index()]
            .as_ref()
            .expect("A remains");
        assert!(!row.native_done && !row.consumer_done && row.retained_native_failure.is_none());
        drop(state);
        assert!(f.no_fatal());
    });
}
