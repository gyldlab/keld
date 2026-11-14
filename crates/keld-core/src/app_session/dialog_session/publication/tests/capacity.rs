//! Existing Dialog source custody can release actual completed writer capacity
//! while its native wake/failure remains deferred to ordinary source service.
use super::*;
use crate::app_session::primary_writer::RetainedWrite;
use std::sync::mpsc;
use std::thread;

#[test]
fn waiting_value_publisher_settles_completed_dialog_without_native_dispatch() {
    fixture_observed(|router, peer, writer, _progress, native, _owner| {
        settle(router);
        while native.try_recv().is_ok() {}
        let target = router
            .current_primary_writer_target()
            .expect("capture")
            .expect("A");
        let (held, resume) = writer.hold_next_dequeue_for_test();
        router
            .service_dialog_writer_progress()
            .expect("admit real settled source");
        held.recv_timeout(Duration::from_secs(3))
            .expect("actual source write held");
        let (waited, waiting) = mpsc::sync_channel(1);
        writer.observe_next_capacity_wait_for_test(waited);
        let publishing = router.clone();
        let caller = target.caller().clone();
        let original_deadline = Instant::now() + Duration::from_secs(2);
        let reservation_thread = thread::spawn(move || {
            publishing.reserve_primary_value(
                &caller,
                original_deadline,
                "waiting Window value producer",
            )
        });
        let entered = waiting.recv_timeout(Duration::from_secs(1));
        let unlocked = router.current.try_lock().is_ok()
            && router.shutdown.transition.try_lock().is_ok()
            && router
                .dialog
                .as_ref()
                .expect("same source")
                .state
                .try_lock()
                .is_ok();
        resume.send(()).expect("release actual native pipe write");
        assert_save(peer);
        let reserved = reservation_thread
            .join()
            .expect("bounded reservation joins");
        let completed = {
            let state = router
                .dialog
                .as_ref()
                .expect("source")
                .state
                .lock()
                .expect("state");
            matches!(
                state.publication.as_ref().and_then(|p| p.receipt.as_ref()),
                Some(RetainedWrite::Completed(None))
            )
        };
        let no_native_dispatch = native.try_recv().is_err();
        assert!(entered.is_ok(), "actual capacity CV wait was reached");
        assert!(unlocked, "no Core/source guard crosses capacity wait");
        assert!(
            reserved.is_ok(),
            "physical completion must wake same capacity wait"
        );
        assert!(completed, "same source retains deferred completed state");
        assert!(no_native_dispatch, "pure hook cannot dispatch DialogWake");
        drop(reserved);
        router
            .service_dialog_writer_progress()
            .expect("ordinary post-turn source service");
        assert!(matches!(
            native.try_recv(),
            Ok(AppWindowCommand::DialogWake)
        ));
        assert!(
            router
                .dialog
                .as_ref()
                .expect("source")
                .state
                .lock()
                .expect("state")
                .publication
                .is_none()
        );
    });
}

#[test]
fn pure_settlement_rearms_existing_unit_progress_after_prior_marker_acknowledged() {
    fixture_observed(|router, peer, writer, events, native, _owner| {
        settle(router);
        while native.try_recv().is_ok() {}
        router
            .service_dialog_writer_progress()
            .expect("admit settled result");
        assert_save(peer);
        progress(events, writer); // existing event was acknowledged before Window turn
        let caller = router
            .current_primary_writer_target()
            .expect("capture")
            .expect("A")
            .caller()
            .clone();
        let reservation = router
            .reserve_primary_value(
                &caller,
                Instant::now() + Duration::from_secs(2),
                "same-thread Window reservation",
            )
            .expect("completed source frees value without event pumping");
        assert!(native.try_recv().is_err());
        assert!(
            matches!(events.try_recv(), Ok(AppWindowCommand::HostWriterProgress)),
            "deferred completed source must not lose its only normal-service hint"
        );
        assert!(
            events.try_recv().is_err(),
            "still exactly one coalesced unit marker"
        );
        drop(reservation);
        router
            .service_dialog_writer_progress()
            .expect("normal source service after turn");
        assert!(matches!(
            native.try_recv(),
            Ok(AppWindowCommand::DialogWake)
        ));
    });
}

#[test]
fn actual_failed_dialog_receipt_keeps_exact_failure_until_ordinary_service() {
    fixture_observed(|router, peer, writer, _progress, _native, owner| {
        settle(router);
        let (held, resume) = writer.hold_next_dequeue_for_test();
        router
            .service_dialog_writer_progress()
            .expect("admit original result");
        held.recv_timeout(Duration::from_secs(3))
            .expect("actual writer holds original");
        let observation = {
            let state = router
                .dialog
                .as_ref()
                .expect("source")
                .state
                .lock()
                .expect("state");
            let Some(RetainedWrite::Pending(receipt)) =
                state.publication.as_ref().and_then(|p| p.receipt.as_ref())
            else {
                panic!("actual source receipt absent");
            };
            receipt.observation()
        };
        peer.shutdown_app_link()
            .expect("actual original pipe failure");
        resume.send(()).expect("actual write returns unknown");
        observation
            .wait_quiesced()
            .expect("actual failed write returned");
        assert!(
            router
                .settle_dialog_primary_receipt()
                .expect("pure settlement")
        );
        {
            let state = router
                .dialog
                .as_ref()
                .expect("same source")
                .state
                .lock()
                .expect("state");
            assert!(matches!(
                state.publication.as_ref().and_then(|p| p.receipt.as_ref()),
                Some(RetainedWrite::Completed(Some(_)))
            ));
        }
        assert!(
            owner.try_recv().is_err(),
            "pure callback cannot wait on recovery owner"
        );
        let reporting = router.clone();
        let service = thread::spawn(move || reporting.service_dialog_writer_progress());
        let crate::app_session::DirectPrimaryOwnerCommand::FailPrimaryWriter(failure, reply) =
            owner
                .recv_timeout(Duration::from_secs(3))
                .expect("only ordinary service reports exact failure")
        else {
            panic!("wrong failure owner command");
        };
        let denied = std::cell::Cell::new(0);
        let result = router.apply_primary_writer_failure(
            &failure,
            |_| panic!("fixture has not committed initial Ready"),
            || denied.set(denied.get() + 1),
        );
        reply
            .send(result.map(|_| ()))
            .expect("actual owner acknowledges");
        service
            .join()
            .expect("normal service joins")
            .expect("exact failure handled");
        assert_eq!(denied.get(), 1);
        assert!(
            router
                .dialog
                .as_ref()
                .expect("same source")
                .state
                .lock()
                .expect("state")
                .publication
                .is_none()
        );
    });
}
