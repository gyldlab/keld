//! Actual pipe controls for Ready commit versus an accepted terminal owner.
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use super::{Fixture, fixture};
use crate::app_session::primary_writer::{ControlMessage, WriteCompletion};
use crate::app_session::{
    AppWindowCommand, CorrelationId, DirectPrimaryOwnerCommand, SESSION_CLI_LEASE_LOST,
    read_primary_frames,
};

#[expect(
    clippy::needless_pass_by_value,
    reason = "This worker owns the actual receiver and router through its acknowledgement loop; borrowing would change their thread and disposal custody."
)]
fn tail_owner(
    owner: Receiver<DirectPrimaryOwnerCommand>,
    router: crate::app_session::PrimaryRouterHandle,
) -> Vec<&'static str> {
    let mut observations = Vec::new();
    while let Ok(command) = owner.recv_timeout(Duration::from_secs(5)) {
        match command {
            DirectPrimaryOwnerCommand::ArmRecovery(_, reply) => {
                observations.push("arm");
                crate::app_session::acknowledge_direct_recovery_arm(
                    Some(&router),
                    Ok(()),
                    |result| reply.send(result).expect("actual arm acknowledgement"),
                );
            }
            DirectPrimaryOwnerCommand::PrepareAcceptedShutdown(reply) => {
                observations.push("prepare");
                reply.send(Ok(())).expect("accepted-shutdown preparation");
            }
            DirectPrimaryOwnerCommand::Shutdown(reply) => {
                observations.push("shutdown");
                reply.send(Ok(())).expect("actual tail acknowledgement");
                break;
            }
            _ => panic!("unexpected owner command in the actual Quit sequence"),
        }
    }
    observations
}

#[test]
#[cfg_attr(
    windows,
    expect(
        clippy::too_many_lines,
        reason = "Keep Ready return, accepted Quit, stopped-reader Drain and original owner joins in one ordered test."
    )
)]
fn written_ready_commits_before_quit_and_stopped_reader_still_owes_its_drain_reply() {
    let Fixture {
        router,
        mut peer,
        mut reader,
        caller,
        owner,
        windows,
        mut worker,
    } = fixture();
    let owner_router = router.clone();
    let owner = thread::spawn(move || tail_owner(owner, owner_router));
    let (written, written_rx) = mpsc::sync_channel(1);
    let (ready_resume, ready_resume_rx) = mpsc::sync_channel(1);
    let (waiting, waiting_rx) = mpsc::sync_channel(1);
    {
        let mut hooks = router
            .quit_drain_hooks
            .lock()
            .expect("same lifecycle hooks");
        hooks.ready_written = Some((written, ready_resume_rx));
        hooks.quit_ready_wait = Some(waiting);
    }
    let ready = router.clone();
    let ready = thread::spawn(move || ready.signal_ready());
    assert_eq!(
        keld_ipc::link::read_frame(&mut peer)
            .expect("physical Ready")
            .1,
        [0]
    );
    written_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("Written before commit");
    let payload =
        keld_ipc::codec::encode(&keld_ipc::LifecycleRequest::Quit).expect("canonical Quit");
    keld_ipc::link::write_frame(
        &mut peer,
        keld_ipc::FrameKind::Call,
        0,
        keld_ipc::LIFECYCLE_CHANNEL,
        CorrelationId(61),
        &payload,
    )
    .expect("actual Quit frame");
    let caller_for_reader = caller.clone();
    let reader_handle = router.clone();
    let quit =
        thread::spawn(move || read_primary_frames(&mut reader, &reader_handle, &caller_for_reader));
    waiting_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("Quit waits at exact Ready commit");
    assert!(router.shutdown.is_running());
    assert!(!router.window_ready.load(Ordering::Acquire));
    assert!(router.current.try_lock().is_ok());
    assert!(router.shutdown.transition.try_lock().is_ok());
    let writer = router.primary_writer_owner().expect("one primary writer");
    let (quit_dequeued, quit_write) = writer.hold_next_dequeue_for_test();
    ready_resume.send(()).expect("allow canonical Ready commit");
    ready
        .join()
        .expect("Ready source joins")
        .expect("Ready commits");
    quit_dequeued
        .recv_timeout(Duration::from_secs(3))
        .expect("Quit uses the sole physical writer");
    let reader_stopped = caller.reader_stop.load(Ordering::Acquire);
    let current_unlocked = router.current.try_lock().is_ok();
    let transition_unlocked = router.shutdown.transition.try_lock().is_ok();
    let no_ui_exit = windows.try_recv().is_err();
    // A red assertion must not strand the physical worker at this barrier.
    quit_write.send(()).expect("release actual Quit write");
    let (header, body) = keld_ipc::link::read_frame(&mut peer).expect("physical Quit reply");
    assert!(
        reader_stopped,
        "actual accepted Quit stops its registered reader"
    );
    assert!(current_unlocked && transition_unlocked);
    assert!(
        no_ui_exit,
        "UI exit cannot overtake the owed physical reply"
    );
    assert_eq!(header.corr, CorrelationId(61));
    assert_eq!(
        keld_ipc::codec::decode::<keld_ipc::LifecycleResponse>(&body).expect("canonical reply"),
        keld_ipc::LifecycleResponse::Quit
    );
    let (drain_dequeued, drain_write) = writer.hold_next_dequeue_for_test();
    keld_ipc::link::write_frame(
        &mut peer,
        keld_ipc::FrameKind::Call,
        0,
        keld_ipc::LIFECYCLE_CHANNEL,
        CorrelationId(62),
        &payload,
    )
    .expect("later real Call");
    drain_dequeued
        .recv_timeout(Duration::from_secs(3))
        .expect("ERR024 uses the same writer after reader stop");
    assert!(router.current.try_lock().is_ok());
    assert!(router.shutdown.transition.try_lock().is_ok());
    drain_write.send(()).expect("release actual drain reply");
    let (header, body) =
        keld_ipc::link::read_frame(&mut peer).expect("physical drained Call error");
    assert_eq!(header.kind, keld_ipc::FrameKind::Err);
    assert_eq!(header.corr, CorrelationId(62));
    assert_eq!(
        keld_ipc::codec::decode::<keld_ipc::CallError>(&body)
            .expect("canonical error")
            .code,
        "KELD-IPC-024"
    );
    drop(peer);
    quit.join()
        .expect("actual primary reader joins")
        .expect("accepted Quit tail");
    assert!(matches!(
        windows.recv_timeout(Duration::from_secs(3)),
        Ok(AppWindowCommand::Quit)
    ));
    assert_eq!(
        owner.join().expect("same owner joins"),
        ["arm", "prepare", "shutdown"]
    );
    assert!(router.current.lock().expect("retired current").is_none());
    worker.stop_and_join().expect("physical writer joins");
    drop(worker);
    drop(router);
    drop(windows);
}

#[test]
#[cfg_attr(
    windows,
    expect(
        clippy::too_many_lines,
        reason = "Keep the accepted terminal and its actually admitted frame together through return and owner teardown."
    )
)]
fn known_written_ready_cannot_close_an_accepted_tails_admitted_frame() {
    let Fixture {
        router,
        mut peer,
        reader,
        caller,
        owner,
        windows,
        mut worker,
    } = fixture();
    let (written, written_rx) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::sync_channel(1);
    router
        .quit_drain_hooks
        .lock()
        .expect("lifecycle hook")
        .ready_written = Some((written, resume_rx));
    let ready = router.clone();
    let ready = thread::spawn(move || ready.signal_ready());
    assert_eq!(
        keld_ipc::link::read_frame(&mut peer)
            .expect("actual Ready")
            .1,
        [0]
    );
    written_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("actual Written");
    let target = router
        .current_primary_writer_target()
        .expect("target")
        .expect("original A");
    let writer = router.primary_writer_owner().expect("same writer");
    let deadline = Instant::now() + Duration::from_secs(3);
    let frame = writer
        .prepare_control(
            &caller,
            &router.shutdown,
            Arc::clone(&target),
            ControlMessage::QuitDrained {
                channel: keld_ipc::LIFECYCLE_CHANNEL,
                correlation: CorrelationId(65),
            },
            deadline,
        )
        .expect("fixed already-owned terminal");
    let (dequeued, release) = writer.hold_next_dequeue_for_test();
    {
        let _transition = router.shutdown.transition_guard();
        assert!(router.shutdown.claim_guarded(SESSION_CLI_LEASE_LOST));
        assert!(router.shutdown.begin_tail());
    }
    let run = router.shutdown.tail_run();
    let tail = super::super::tail::AcceptedTail {
        target,
        cause: SESSION_CLI_LEASE_LOST,
    };
    let receipt = router
        .admit_tail_control(&tail, frame)
        .expect("tail admission")
        .expect("exact A");
    dequeued
        .recv_timeout(Duration::from_secs(3))
        .expect("admitted original terminal");
    resume
        .send(())
        .expect("known Written Ready observes accepted terminal owner");
    let ready_result = ready.join().expect("Ready source joins");
    let retained = router
        .current
        .lock()
        .expect("current")
        .as_ref()
        .is_some_and(|active| Arc::ptr_eq(&active.reader_stop, &caller.reader_stop));
    let premature_owner_action = owner.try_recv().is_ok();
    let pending_ready = router.shutdown.transition_guard().pending_ready.is_some();
    // Release the real I/O owner even for the deliberate negative mutation, so
    // a failed assertion cannot strand a test thread at this physical barrier.
    release
        .send(())
        .expect("tail owns release of its actual frame");
    let completion = receipt.wait(deadline);
    let received = keld_ipc::link::read_frame(&mut peer);
    drop(receipt);
    let cleanup = router.retire_accepted_tail_with_writer(&tail);
    drop(run);
    drop(reader);
    worker.stop_and_join().expect("one physical writer drains");
    assert!(
        ready_result.is_ok(),
        "known Written cancellation must not report unknown I/O: {ready_result:?}"
    );
    assert!(
        retained,
        "the accepted tail retains A until its actual terminal drains"
    );
    assert!(!premature_owner_action);
    assert!(!pending_ready);
    assert!(matches!(completion, Ok(WriteCompletion::Written)));
    let (header, bytes) = received.expect("tail's actual terminal survives Ready cancellation");
    assert_eq!(header.corr, CorrelationId(65));
    assert_eq!(
        keld_ipc::codec::decode::<keld_ipc::CallError>(&bytes)
            .expect("tail error")
            .code,
        "KELD-IPC-024"
    );
    cleanup.expect("tail closes exact A after real completion");
    drop(worker);
    drop(router);
    drop(windows);
}
