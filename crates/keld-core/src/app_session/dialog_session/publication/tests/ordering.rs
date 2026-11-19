use super::{assert_save, fixture, fixture_observed, progress, settle, source_pointer};
use crate::app_session::dialog_session::{
    DialogFault, DialogOwner, DialogReplyV1, DialogUiOutcome, DialogWorkOptions, WebviewId,
};
use keld_ipc::codec::{decode, encode};
use keld_ipc::link::read_frame;
use keld_ipc::{CallError, CorrelationId, FrameKind};
use keld_native::dialog::{CancelDisposition, DialogRequestV1};
use std::io::Read as _;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn observed_renderer_failure_precedes_pending_dialog_publication() {
    use crate::app_session::{
        AppReadyTrigger, AppWindowEvent, app_detail, coordinate_window_events,
    };

    fixture(|router, _peer, _writer, _events| {
        settle(router);
        let original = source_pointer(router);
        let terminal = OnceLock::from(Err(app_detail(
            "renderer dispatch fixture",
            "original renderer outcome failed",
        )));
        let (events, received) = mpsc::channel();
        events
            .send(AppWindowEvent::HostWriterProgress)
            .expect("deliver the coalesced Core progress");
        let failure = coordinate_window_events(
            &received,
            router,
            &router.window_commands,
            AppReadyTrigger::NavigationReady,
            |_| Ok(()),
            None,
            Some(&terminal),
        )
        .expect_err("renderer terminal must stop the coordinator");
        assert!(
            failure
                .to_string()
                .contains("original renderer outcome failed")
        );
        assert_eq!(
            source_pointer(router),
            original,
            "the actual pending Dialog source must remain outside the writer"
        );
    });
}

#[test]
fn duplicate_correlation_closes_exact_link_before_fresh_admission() {
    fixture_observed(|router, peer, _writer, _events, native, owner| {
        let original = settle(router);
        while native.try_recv().is_ok() {}
        let target = router
            .current_primary_writer_target()
            .expect("capture")
            .expect("live original link");
        let error = router
            .dispatch_dialog(target.caller(), original.correlation, &[255])
            .expect_err("same correlation rejected before decoding");
        assert!(error.detail.contains("KELD-IPC-005"), "{error}");
        assert!(target.caller().reader_stop.load(Ordering::Acquire));
        assert!(router.current.lock().expect("generation lock").is_none());
        assert_eq!(
            peer.read(&mut [0]).expect("actual EOF"),
            0,
            "no competing refusal is written"
        );
        assert!(router.dialog.as_ref().expect("dialog").fully_drained());
        assert!(native.try_recv().is_ok(), "retirement wakes native custody");
        assert!(owner.try_recv().is_err(), "no numeric recovery authority");
    });
}

#[test]
fn duplicate_after_enqueue_drains_the_owned_terminal_without_a_second_frame() {
    fixture(|router, peer, writer, _events| {
        let original = settle(router);
        let target = router
            .current_primary_writer_target()
            .expect("capture")
            .expect("live original link");
        let (dequeued, resume) = writer.hold_next_dequeue_for_test();
        router
            .service_dialog_writer_progress()
            .expect("enqueue original terminal");
        dequeued
            .recv_timeout(Duration::from_secs(2))
            .expect("actual writer entered the held position");

        let caller = target.caller().clone();
        thread::scope(|scope| {
            let duplicate =
                scope.spawn(move || router.dispatch_dialog(&caller, original.correlation, &[255]));
            let cutoff = Instant::now() + Duration::from_secs(2);
            while !target.caller().reader_stop.load(Ordering::Acquire) && Instant::now() < cutoff {
                thread::yield_now();
            }
            let revoked_before_write = target.caller().reader_stop.load(Ordering::Acquire);
            resume.send(()).expect("allow actual original write");
            let error = duplicate
                .join()
                .expect("reader joins")
                .expect_err("reuse refused");
            assert!(
                revoked_before_write,
                "dequeue alone released the original correlation"
            );
            assert!(error.detail.contains("KELD-IPC-005"), "{error}");
        });

        assert_save(peer);
        assert_eq!(
            peer.read(&mut [0]).expect("actual old-link EOF"),
            0,
            "original link closes after its admitted write"
        );
        router
            .service_dialog_writer_progress()
            .expect("collect actual completion");
        assert!(router.dialog.as_ref().expect("dialog").fully_drained());
    });
}

#[test]
fn allowed_cancel_ack_precedes_original_terminal_when_native_finishes_during_capacity_wait() {
    fixture_observed(|router, peer, writer, events, native, _owner| {
        let target = router
            .current_primary_writer_target()
            .expect("capture")
            .expect("live link");
        let caller = target.caller();
        let occupied = writer
            .try_reserve_value(Instant::now() + Duration::from_secs(3))
            .expect("ordinary writer occupied");
        let original = CorrelationId(72);
        let cancel = CorrelationId(73);
        let nonce = 9;
        let effect = DialogOwner {
            identity: caller.identity,
            attempt: caller.attempt,
            correlation: original,
            nonce,
            parent: None,
        };
        let dialog = router.dialog.as_ref().expect("dialog");
        dialog
            .reserve_for_caller(
                effect,
                DialogWorkOptions::Message(crate::app_session::dialog_session::tests::options()),
                WebviewId(3),
                caller.clone(),
            )
            .expect("model an admitted original before exercising the actual guarded Cancel");
        assert!(matches!(
            native.recv_timeout(Duration::from_secs(2)),
            Ok(keld_wv::AppWindowCommand::DialogWake)
        ));
        let work = dialog
            .take_work()
            .expect("admitted")
            .expect("one native effect");
        assert_eq!(work.owner.correlation, original);

        let (waited, waiting) = mpsc::sync_channel(1);
        writer.observe_next_capacity_wait_for_test(waited);
        thread::scope(|scope| {
            let acknowledgement = scope.spawn(move || {
                router.dispatch_dialog(
                    caller,
                    cancel,
                    &encode(&DialogRequestV1::Cancel { nonce }).expect("bounded Cancel"),
                )
            });
            waiting
                .recv_timeout(Duration::from_secs(2))
                .expect("Cancel waiting for its slot");
            dialog
                .record_native_outcome(work.owner, DialogUiOutcome::Dismissed, None)
                .expect("already canceled native effect returns");
            router
                .dialog_native_finished(work.owner.attempt, nonce, DialogUiOutcome::Dismissed)
                .expect("original waits for its earlier Cancel ACK");
            assert_eq!(
                dialog.pending_original_for(caller.identity, caller.attempt),
                Some(original)
            );
            // The source is unavailable even before the writer checks its slot.
            assert!(
                super::super::DialogSource::take(dialog)
                    .expect("original admission gate")
                    .is_none()
            );
            drop(occupied);
            acknowledgement
                .join()
                .expect("primary reader joins")
                .expect("physical Cancel ACK returned");
        });

        let (header, bytes) = read_frame(peer).expect("first terminal is the actual ACK");
        assert_eq!((header.kind, header.corr), (FrameKind::Reply, cancel));
        assert_eq!(
            decode::<DialogReplyV1>(&bytes).expect("Cancel reply wire shape"),
            DialogReplyV1::Cancel {
                disposition: CancelDisposition::Requested
            }
        );
        progress(events, writer);
        router
            .service_dialog_writer_progress()
            .expect("native result can publish after ACK");
        let (header, bytes) = read_frame(peer).expect("separate original terminal");
        assert_eq!((header.kind, header.corr), (FrameKind::Err, original));
        assert_eq!(
            decode::<CallError>(&bytes)
                .expect("original error wire shape")
                .code,
            DialogFault::Cancelled.code()
        );
        progress(events, writer);
        router
            .service_dialog_writer_progress()
            .expect("actual original receipt");
        assert!(dialog.fully_drained());
    });
}

#[test]
fn distinct_link_cannot_cancel_a_same_number_original() {
    fixture(|router, _original_peer, _writer, _events| {
        let target = router
            .current_primary_writer_target()
            .expect("capture")
            .expect("active original");
        let caller = target.caller();
        let effect = DialogOwner {
            identity: caller.identity,
            attempt: caller.attempt,
            correlation: CorrelationId(81),
            nonce: 9,
            parent: None,
        };
        let dialog = router.dialog.as_ref().expect("same source");
        dialog
            .reserve_for_caller(
                effect,
                DialogWorkOptions::Message(crate::app_session::dialog_session::tests::options()),
                WebviewId(3),
                caller.clone(),
            )
            .expect("one original reservation, still eligible for native use");
        let (successor, mut peer) = super::replace_with_equal_number(router);
        let attempted_cancel = CorrelationId(82);
        router
            .dispatch_dialog(
                &successor,
                attempted_cancel,
                &encode(&DialogRequestV1::Cancel {
                    nonce: effect.nonce,
                })
                .expect("bounded Cancel"),
            )
            .expect("a distinct caller receives the ordinary dialog refusal");
        let (header, bytes) = read_frame(&mut peer).expect("actual successor link refusal");
        assert_eq!(
            (header.kind, header.corr),
            (FrameKind::Err, attempted_cancel)
        );
        assert_eq!(
            decode::<CallError>(&bytes).expect("typed rejection").code,
            DialogFault::InvalidRequest.code()
        );
        assert!(
            dialog
                .may_present(effect)
                .expect("actual original remains eligible")
        );
    });
}
