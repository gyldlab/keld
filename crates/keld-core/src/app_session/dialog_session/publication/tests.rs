//! Real Windows pipe/writer plus modeled, already quiescent native dialog.
//! These prove Core publication, not an OS panel or Runtime admission receipt.
#![allow(clippy::expect_used, clippy::panic)]

use super::super::{
    CallError, CorrelationId, DialogUiOutcome, DialogWorkOptions, SaveDialogOptions, WebviewId,
};
use super::*;
use crate::app_session::primary_writer::{
    ControlMessage, PrimaryWriter, PrimaryWriterWorker, WriterLink,
};
use keld_ipc::AppLinkDeadlines as _;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::time::Duration;

mod capacity;

fn fixture(
    run: impl FnOnce(
        &PrimaryRouterHandle,
        &mut keld_ipc::BootstrapStream,
        &PrimaryWriter,
        &Receiver<AppWindowCommand>,
    ),
) {
    fixture_observed(|router, peer, writer, progress, _native, _owner| {
        run(router, peer, writer, progress);
    });
}

fn fixture_observed(
    run: impl FnOnce(
        &PrimaryRouterHandle,
        &mut keld_ipc::BootstrapStream,
        &PrimaryWriter,
        &Receiver<AppWindowCommand>,
        &Receiver<AppWindowCommand>,
        &Receiver<crate::app_session::DirectPrimaryOwnerCommand>,
    ),
) {
    let (mut router, mut peer, guardian, progress) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let (dialog, native, _temp) =
        super::super::tests::with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    router.dialog = Some(Arc::new(dialog));
    {
        let mut current = router.current.lock().expect("canonical caller");
        let active = current.as_mut().expect("initial actual pipe");
        let caller = PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        active.writer_link = Some(
            WriterLink::new(
                active.writer.try_clone().expect("actual pipe clone"),
                caller,
            )
            .expect("fixed target"),
        );
    }
    let (writer, mut worker) =
        PrimaryWriterWorker::start(router.clone()).expect("one physical writer");
    router.primary_writer = Some(writer.clone());
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("bounded wire oracle");
    run(&router, &mut peer, &writer, &progress, &native, &guardian);
    worker
        .stop_and_join()
        .expect("join actual writer outside all Core locks");
}

fn settle(router: &PrimaryRouterHandle) -> DialogOwner {
    let target = router
        .current_primary_writer_target()
        .expect("capture")
        .expect("actual live target");
    let caller = target.caller().clone();
    let owner = DialogOwner {
        identity: caller.identity,
        attempt: caller.attempt,
        correlation: CorrelationId(77),
        nonce: 9,
        parent: None,
    };
    let dialog = router.dialog.as_ref().expect("same source owner");
    dialog
        .reserve_for_caller(
            owner,
            DialogWorkOptions::Save(SaveDialogOptions {
                default_path: None,
                filters: Vec::new(),
                security_scoped_bookmarks: false,
            }),
            WebviewId(3),
            caller,
        )
        .expect("same guarded reservation after modeled admission");
    dialog
        .take_work()
        .expect("modeled native owner")
        .expect("one work");
    dialog
        .record_native_outcome(
            owner,
            DialogUiOutcome::FileResult,
            Some(DialogReplyV1::Save {
                canceled: false,
                file_path: r"D:\fixture\original-result.drawio".to_owned(),
            }),
        )
        .expect("modeled actual native result wins");
    dialog
        .complete_for_writer(owner.attempt, owner.nonce, DialogUiOutcome::FileResult)
        .expect("physical native drain retains original publication");
    owner
}

fn source_pointer(router: &PrimaryRouterHandle) -> usize {
    let state = router
        .dialog
        .as_ref()
        .expect("source owner")
        .state
        .lock()
        .expect("same record");
    let record = state.publication.as_ref().expect("retained result");
    let Some(Ok(DialogReplyV1::Save { file_path, .. })) = record.result.as_ref() else {
        panic!("original source path missing");
    };
    file_path.as_ptr() as usize
}

fn progress(events: &Receiver<AppWindowCommand>, writer: &PrimaryWriter) {
    assert!(matches!(
        events
            .recv_timeout(Duration::from_secs(2))
            .expect("actual writer progress"),
        AppWindowCommand::HostWriterProgress
    ));
    writer
        .acknowledge_progress()
        .expect("one coalesced acknowledgement");
}

fn assert_save(peer: &mut keld_ipc::BootstrapStream) {
    let (header, bytes) = keld_ipc::link::read_frame(peer).expect("actual original terminal");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Reply,
            keld_ipc::channel_table::DIALOG.id(),
            CorrelationId(77)
        )
    );
    assert_eq!(
        keld_ipc::codec::decode::<DialogReplyV1>(&bytes).expect("canonical shared result"),
        DialogReplyV1::Save {
            canceled: false,
            file_path: r"D:\fixture\original-result.drawio".to_owned(),
        }
    );
}

#[test]
fn full_dialog_value_keeps_exact_source_original_and_charge_through_actual_write() {
    fixture(|router, peer, writer, events| {
        let owner = settle(router);
        let dialog = router.dialog.as_ref().expect("source");
        let pointer = source_pointer(router);
        let occupied = writer
            .try_reserve_value(Instant::now() + Duration::from_secs(3))
            .expect("one occupied VALUE");
        router
            .service_dialog_writer_progress()
            .expect("Full retains result");
        assert_eq!(
            source_pointer(router),
            pointer,
            "no source clone or extra payload queue"
        );
        assert_eq!(
            dialog.pending_original_for(owner.identity, owner.attempt),
            Some(owner.correlation)
        );
        assert!(!dialog.has_capacity());
        assert!(
            events.try_recv().is_err(),
            "free controls do not wake VALUE"
        );
        let (dequeued, resume) = writer.hold_next_dequeue_for_test();
        drop(occupied);
        progress(events, writer);
        router
            .service_dialog_writer_progress()
            .expect("admit exact original");
        dequeued
            .recv_timeout(Duration::from_secs(2))
            .expect("actual dequeue before physical write");
        assert_eq!(
            dialog.pending_original_for(owner.identity, owner.attempt),
            None,
            "enqueue owns the sole original terminal"
        );
        assert!(!dialog.has_capacity(), "dequeue is not actual completion");
        assert!(matches!(
            writer.try_reserve_value(Instant::now() + Duration::from_secs(2)),
            Err(WriterRefusal::Full)
        ));
        assert!(router.shutdown.transition.try_lock().is_ok());
        assert!(router.current.try_lock().is_ok());
        router
            .service_dialog_writer_progress()
            .expect("pending receipt inspection");
        assert!(!dialog.has_capacity());
        resume.send(()).expect("allow actual original pipe write");
        assert_save(peer);
        progress(events, writer);
        router
            .service_dialog_writer_progress()
            .expect("actual completion releases source slot");
        assert!(dialog.has_capacity());
        assert!(dialog.fully_drained());
        drop(
            writer
                .try_reserve_value(Instant::now() + Duration::from_secs(2))
                .expect("actual write and receipt release VALUE"),
        );
    });
}

#[test]
fn actual_final_try_send_full_does_not_consume_dialog_original() {
    fixture(|router, peer, writer, events| {
        let owner = settle(router);
        let pointer = source_pointer(router);
        writer.fail_next_send_for_test(WriterRefusal::Full);
        router
            .service_dialog_writer_progress()
            .expect("actual final Full");
        assert_eq!(source_pointer(router), pointer);
        assert_eq!(
            router
                .dialog
                .as_ref()
                .expect("source")
                .pending_original_for(owner.identity, owner.attempt),
            Some(owner.correlation)
        );
        progress(events, writer);
        router
            .service_dialog_writer_progress()
            .expect("same original retry");
        assert_save(peer);
        progress(events, writer);
        router
            .service_dialog_writer_progress()
            .expect("actual result completed once");
        assert!(router.dialog.as_ref().expect("source").fully_drained());
    });
}

#[test]
fn full_dialog_result_retirement_sends_exactly_one_original_terminal() {
    fixture(|router, peer, writer, _events| {
        let owner = settle(router);
        let occupied = writer
            .try_reserve_value(Instant::now() + Duration::from_secs(3))
            .expect("full VALUE");
        router
            .service_dialog_writer_progress()
            .expect("retained original before retirement");
        assert_eq!(
            router
                .dialog
                .as_ref()
                .expect("source")
                .pending_original_for(owner.identity, owner.attempt),
            Some(owner.correlation)
        );
        router
            .retire_generation(owner.attempt)
            .expect("canonical fixed retirement and exact old close");
        let (header, bytes) =
            keld_ipc::link::read_frame(peer).expect("independent original retirement frame");
        assert_eq!(
            (header.kind, header.channel, header.corr),
            (
                FrameKind::Err,
                keld_ipc::channel_table::DIALOG.id(),
                owner.correlation
            )
        );
        assert_eq!(
            keld_ipc::codec::decode::<CallError>(&bytes).expect("canonical original refusal"),
            CallError::generation_retired()
        );
        assert!(
            keld_ipc::link::read_frame(peer).is_err(),
            "exact old pipe closes without a duplicate original"
        );
        let dialog = router.dialog.as_ref().expect("same source");
        assert!(dialog.fully_drained());
        assert_eq!(
            dialog.pending_original_for(owner.identity, owner.attempt),
            None
        );
        drop(occupied);
    });
}

#[test]
fn same_numeric_successor_cannot_authorize_a_captured_dialog_result() {
    fixture(|router, peer, _writer, _events| {
        let owner = settle(router);
        let pointer = source_pointer(router);
        let dialog = router.dialog.as_ref().expect("original source");
        let temporary = DialogSource::take(dialog)
            .expect("one preparing source")
            .expect("source");
        let successor = {
            let mut current = router.current.lock().expect("canonical current");
            let active = current.as_mut().expect("actual old link");
            // Deliberately retain all numeric facts. Only the actual role
            // allocation distinguishes this adversarial replacement.
            active.reader_stop = Arc::new(AtomicBool::new(false));
            let caller = PrimaryCallerContext {
                attempt: active.attempt,
                identity: active.identity,
                reader_stop: Arc::clone(&active.reader_stop),
            };
            active.writer_link = Some(
                WriterLink::new(
                    active.writer.try_clone().expect("same physical test pipe"),
                    caller.clone(),
                )
                .expect("different exact role target"),
            );
            caller
        };
        drop(temporary);
        router
            .service_dialog_writer_progress()
            .expect("stale capture refuses publication");
        assert_eq!(
            source_pointer(router),
            pointer,
            "current-role lookup cannot consume old source"
        );
        assert_eq!(
            dialog.pending_original_for(owner.identity, owner.attempt),
            Some(owner.correlation)
        );
        dialog.retire(owner.identity, owner.attempt);
        assert!(dialog.fully_drained());
        router
            .submit_primary_control(
                &successor,
                ControlMessage::Ping {
                    channel: keld_ipc::ECHO_CHANNEL,
                    correlation: CorrelationId(91),
                },
                Instant::now() + Duration::from_secs(2),
                "healthy successor control",
            )
            .expect("genuine new target writes");
        let (header, bytes) =
            keld_ipc::link::read_frame(peer).expect("first actual frame belongs to successor");
        assert_eq!(
            (header.kind, header.corr),
            (FrameKind::Ping, CorrelationId(91))
        );
        assert_eq!(bytes, [] as [u8; 0]);
    });
}

#[test]
fn guard_refusal_uses_the_same_writer_before_native_decode() {
    fixture(|router, peer, _writer, _events| {
        let (denied, _native, _temp) =
            super::super::tests::with_manifest(r#"{"app":{"system":[]}}"#);
        let mut guarded = router.clone();
        guarded.dialog = Some(Arc::new(denied));
        let target = guarded
            .current_primary_writer_target()
            .expect("actual role")
            .expect("target");
        guarded
            .dispatch_dialog(target.caller(), CorrelationId(49), &[255])
            .expect("actual writer carries Guard refusal");
        let (header, bytes) = keld_ipc::link::read_frame(peer).expect("correlated guarded refusal");
        assert_eq!(
            (header.kind, header.corr),
            (FrameKind::Err, CorrelationId(49))
        );
        assert_eq!(
            keld_ipc::codec::decode::<CallError>(&bytes)
                .expect("canonical error")
                .code,
            "KELD-GUARD001"
        );
        assert!(guarded.dialog.as_ref().expect("source").fully_drained());
    });
}
