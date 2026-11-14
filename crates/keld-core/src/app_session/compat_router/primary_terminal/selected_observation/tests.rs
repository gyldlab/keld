//! Actual source ownership and canonical target checks with modeled native settlement.
//! These component schedules do not qualify `WebView2` or physical VALUE completion.
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "Fixture setup and failing assertions are deliberate."
)]

use super::super::tests::{fixture, settle};
use super::*;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

fn stopped_source(router: &PrimaryRouterHandle) -> ChargeSnapshot {
    settle(
        router,
        ConsumerDisposition::Refused {
            error: keld_ipc::CallError {
                code: "KELD-COMPAT-103".to_owned(),
                message: "original selected diagnostic".to_owned(),
            },
        },
    );
    let mut state = router
        .compat
        .as_ref()
        .expect("same owner")
        .lock()
        .expect("record");
    let charge = state.ledger.data[Direction::MainToRenderer.index()]
        .as_mut()
        .expect("actual reserved DATA record");
    charge.owner.reader_stop.store(true, Ordering::Release);
    charge.retired = true;
    charge.native_retirement = Some(keld_ipc::compat_control::DispatchCertainty::OutcomeUnknown);
    charge.native_storage = Some(Arc::new(AtomicBool::new(false)));
    ChargeSnapshot::from_charge(charge)
}

#[test]
fn stopped_preparation_handoff_restores_original_source_after_moved_snapshot() {
    fixture(|router, _peer, writer, outcomes, _windows| {
        let original = stopped_source(router);
        let owner = router.compat.as_ref().expect("actual ledger owner");
        let owner_count = Arc::strong_count(owner);
        let source = TerminalSource::take(owner)
            .expect("take")
            .expect("actual moved source");
        let first = source
            .selected_diagnostic()
            .expect("bounded actual preparation");
        assert_eq!(Some(first.diagnostic), original.selected);
        let during = {
            let state = owner.lock().expect("same final observation cut");
            charge_snapshots(&state)[Direction::MainToRenderer.index()].expect("charged")
        };
        assert_eq!(during.source_attempt, Some(first.attempt));
        assert!(during.terminal_preparing);
        assert!(
            during.selected.is_none(),
            "strings really moved into TerminalSource"
        );
        assert!(during.reader_stopped && during.retired);
        assert!(
            router
                .primary_writer_target(&source.caller)
                .expect("canonical target")
                .is_none()
        );
        let restored = source
            .finish_denied_target()
            .expect("actual post-restore handoff");
        assert_eq!(
            Arc::strong_count(owner),
            owner_count,
            "source owner really dropped"
        );
        assert_eq!(restored.preparation, first);
        assert_eq!(
            restored.charges[Direction::MainToRenderer.index()],
            Some(original),
            "actual same-source Drop restoration"
        );

        // Retain the scalar capture while the same allocations move again. A
        // previous restoration cannot prove this distinct active preparation.
        let source = TerminalSource::take(owner)
            .expect("second take")
            .expect("same source");
        let second = source
            .selected_diagnostic()
            .expect("second bounded attempt");
        assert_eq!(first.diagnostic, second.diagnostic);
        assert_ne!(
            first.attempt, second.attempt,
            "a repeated source move has a new attempt"
        );
        assert_eq!(
            restored.charges[Direction::MainToRenderer.index()],
            Some(original)
        );
        let during_second = {
            let state = owner.lock().expect("second final cut");
            charge_snapshots(&state)[Direction::MainToRenderer.index()].expect("same charge")
        };
        assert_eq!(during_second.source_attempt, Some(second.attempt));
        assert_ne!(
            during_second.source_attempt,
            Some(restored.preparation.attempt)
        );
        assert!(
            router
                .primary_writer_target(&source.caller)
                .expect("stopped target")
                .is_none()
        );
        let restored_second = source
            .finish_denied_target()
            .expect("second actual restore");
        assert_eq!(restored_second.preparation, second);
        assert_eq!(
            restored_second.charges[Direction::MainToRenderer.index()],
            Some(original)
        );
        assert_eq!(Arc::strong_count(owner), owner_count);
        let final_state = owner.lock().expect("actual final ledger");
        assert_eq!(
            charge_snapshots(&final_state)[Direction::MainToRenderer.index()],
            Some(original)
        );
        drop(final_state);
        assert!(
            outcomes.try_recv().is_err(),
            "restoration is not native release"
        );
        drop(
            writer
                .try_reserve_value(Instant::now() + Duration::from_secs(2))
                .expect("no denied preparation was admitted to VALUE"),
        );
    });
}

#[test]
fn preparation_observation_exhaustion_never_reuses_identity_or_changes_custody() {
    fixture(|router, _peer, _writer, outcomes, _windows| {
        let original = stopped_source(router);
        let owner = router.compat.as_ref().expect("same owner");
        for expected in 1..=130 {
            let source = TerminalSource::take(owner)
                .expect("take")
                .expect("eligibility unchanged");
            let observed = source.selected_diagnostic();
            if expected <= 128 {
                assert_eq!(observed.expect("bounded sequence").attempt, expected);
            } else {
                assert!(observed.is_none(), "exhaustion must never reuse an attempt");
            }
            assert!(
                router
                    .primary_writer_target(&source.caller)
                    .expect("canonical denial")
                    .is_none()
            );
            drop(source);
            let state = owner.lock().expect("ordinary Drop restored actual source");
            assert_eq!(
                charge_snapshots(&state)[Direction::MainToRenderer.index()],
                Some(original)
            );
            let charge = state.ledger.data[Direction::MainToRenderer.index()]
                .as_ref()
                .expect("retained");
            assert_eq!(charge.selected_source_sequence, expected.min(129));
        }
        assert!(
            outcomes.try_recv().is_err(),
            "diagnostic exhaustion is not release"
        );
    });
}

#[test]
fn denied_source_handoff_observes_actual_reap_after_quiescent_release() {
    fixture(|router, _peer, _writer, outcomes, _windows| {
        let original = stopped_source(router);
        let owner = router.compat.as_ref().expect("original owner");
        let source = TerminalSource::take(owner)
            .expect("take")
            .expect("actual moved source");
        let selected = source.selected_diagnostic().expect("actual preparation");
        assert_eq!(Some(selected.diagnostic), original.selected);
        assert!(
            router
                .primary_writer_target(&source.caller)
                .expect("stopped target")
                .is_none()
        );
        {
            let mut state = owner
                .lock()
                .expect("modeled cancellation and native disposal");
            let charge = state.ledger.data[Direction::MainToRenderer.index()]
                .as_mut()
                .expect("original charge remains while preparing");
            // Only these native/cancellation prerequisites are modeled. The
            // following restore, release publication and reap are actual owners.
            charge.pending_reply = None;
            charge
                .native_storage
                .as_ref()
                .expect("original backing")
                .store(true, Ordering::Release);
            assert!(charge.terminal_preparing && charge.terminal.is_none());
        }
        let restored = source
            .finish_denied_target()
            .expect("actual post-helper capture");
        assert_eq!(restored.preparation, selected);
        assert!(
            restored.charges[Direction::MainToRenderer.index()].is_none(),
            "handoff capture follows actual quiescent release and reap"
        );
        assert!(matches!(
            outcomes.try_recv().expect("actual release publication"),
            keld_wv::renderer_bridge::RendererBridgeOutcome::EventReleased { request: 77, .. }
        ));
        assert!(
            outcomes.try_recv().is_err(),
            "one original release publication"
        );
        let state = owner.lock().expect("actual reaped ledger");
        assert!(state.ledger.data[Direction::MainToRenderer.index()].is_none());
    });
}
