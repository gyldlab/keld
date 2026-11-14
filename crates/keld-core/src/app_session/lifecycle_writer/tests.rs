//! Actual Windows pipe controls for lifecycle publication and failed-write retirement.
#![allow(clippy::expect_used, clippy::panic)]

use std::io::Read as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use super::super::primary_writer::{PrimaryWriterWorker, WriterLink};
use super::super::tests::initial_ready_router_with_window_loop;
use super::super::{
    AppWindowCommand, BootstrapStream, CorrelationId, DirectPrimaryOwnerCommand,
    PrimaryCallerContext, PrimaryRouterHandle, SESSION_LIFECYCLE_QUIT, read_primary_frames,
};

struct Fixture {
    router: PrimaryRouterHandle,
    peer: BootstrapStream,
    reader: BootstrapStream,
    caller: PrimaryCallerContext,
    owner: Receiver<DirectPrimaryOwnerCommand>,
    windows: Receiver<AppWindowCommand>,
    worker: PrimaryWriterWorker,
}

fn fixture() -> Fixture {
    let (mut router, peer, owner, windows) = initial_ready_router_with_window_loop();
    // Production generation installation registers this exact cancellation Arc
    // with the existing shutdown owner before any reader/writer retains it.
    let reader_stop = router.shutdown.register_generation_reader();
    let (reader, caller) = {
        let mut current = router.current.lock().expect("fixture current");
        let active = current.as_mut().expect("fixture generation");
        active.reader_stop = reader_stop;
        let caller = PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        let reader = active.writer.try_clone().expect("actual same pipe reader");
        active.writer_link = Some(
            WriterLink::new(
                active.writer.try_clone().expect("actual same pipe writer"),
                caller.clone(),
            )
            .expect("retained exact link"),
        );
        (reader, caller)
    };
    let (writer, worker) = PrimaryWriterWorker::start(router.clone()).expect("physical writer");
    router.primary_writer = Some(writer);
    Fixture {
        router,
        peer,
        reader,
        caller,
        owner,
        windows,
        worker,
    }
}

/// The actual Core owner predicates and arm acknowledgement, with only the
/// external Runtime gate/restart modeled here. This is not a Supervisor probe.
#[expect(
    clippy::needless_pass_by_value,
    reason = "This worker owns the actual receiver and router through its acknowledgement loop; borrowing would change their thread and disposal custody."
)]
fn owner_loop(
    owner: Receiver<DirectPrimaryOwnerCommand>,
    failure_owner: Option<PrimaryRouterHandle>,
) -> Vec<&'static str> {
    let mut observed = Vec::new();
    let mut gate_armed = false;
    while let Ok(command) = owner.recv_timeout(Duration::from_secs(5)) {
        match command {
            DirectPrimaryOwnerCommand::ArmRecovery(_, reply) => {
                gate_armed = true;
                observed.push("arm");
                super::super::acknowledge_direct_recovery_arm(
                    failure_owner.as_ref(),
                    Ok(()),
                    |result| {
                        reply
                            .send(result)
                            .expect("actual recovery arm acknowledgement");
                    },
                );
            }
            DirectPrimaryOwnerCommand::FailPrimaryReader(failure, reply) => {
                let router = failure_owner.as_ref().expect("actual EOF caller owner");
                let decision = std::cell::Cell::new(None);
                let applied = router.apply_primary_read_failure(&failure, |attempt| {
                    assert_eq!(attempt, 1);
                    decision.set(Some(if gate_armed {
                        "failed"
                    } else {
                        "startup-failed"
                    }));
                });
                // Model the subsequent owned Runtime revocation through its
                // actual Core path, outside the callback's canonical guards.
                if decision.get().is_some() {
                    router
                        .retire_generation(1)
                        .expect("actual modeled owner revocation");
                }
                reply
                    .send(applied.map(|_| ()))
                    .expect("actual EOF owner decision");
                if let Some(decision) = decision.get() {
                    observed.push(decision);
                }
                return observed;
            }
            DirectPrimaryOwnerCommand::FailPrimaryWriter(failure, reply) => {
                let router = failure_owner.as_ref().expect("actual Ready failure owner");
                let decision = std::cell::Cell::new(None);
                let applied = router.apply_primary_writer_failure(
                    &failure,
                    |attempt| {
                        assert_eq!(attempt, 1);
                        decision.set(Some("failed"));
                    },
                    || decision.set(Some("deny")),
                );
                reply
                    .send(applied.map(|_| ()))
                    .expect("actual canonical failure decision");
                if let Some(decision) = decision.get() {
                    observed.push(decision);
                }
                return observed;
            }
            DirectPrimaryOwnerCommand::DenyRecovery => observed.push("deny"),
            _ => panic!("unexpected lifecycle owner command"),
        }
    }
    observed
}

#[test]
fn actual_ready_written_then_reader_eof_waits_for_canonical_ready_commit() {
    let Fixture {
        router,
        mut peer,
        mut reader,
        caller,
        owner,
        windows,
        mut worker,
    } = fixture();
    let failure_owner = router.clone();
    let owner = thread::spawn(move || owner_loop(owner, Some(failure_owner)));
    let (written, written_rx) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::sync_channel(1);
    let (waiting, waiting_rx) = mpsc::sync_channel(1);
    {
        let mut hooks = router
            .quit_drain_hooks
            .lock()
            .expect("existing lifecycle hooks");
        hooks.ready_written = Some((written, resume_rx));
        hooks.ready_failure_wait = Some(waiting);
    }
    let producer = router.clone();
    let producer = thread::spawn(move || producer.signal_ready());
    let (header, bytes) = keld_ipc::link::read_frame(&mut peer).expect("physical Ready frame");
    assert_eq!(header.kind, keld_ipc::FrameKind::Event);
    assert_eq!(header.channel, keld_ipc::LIFECYCLE_CHANNEL);
    assert_eq!(bytes, [0], "literal canonical Ready event");
    written_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("actual Written before commit");
    let eof_handle = router.clone();
    let (reader_done, done_rx) = mpsc::sync_channel(1);
    let eof = thread::spawn(move || {
        let result = read_primary_frames(&mut reader, &eof_handle, &caller);
        reader_done.send(()).expect("reader completion witness");
        result
    });
    drop(peer);
    waiting_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("EOF waits for the Ready commit");
    {
        let transition = router.shutdown.transition_guard();
        assert!(transition.pending_ready.is_some());
        assert!(
            router.current.try_lock().is_ok(),
            "no current guard across the wait"
        );
        assert!(!router.window_ready.load(Ordering::Acquire));
        assert!(!router.pre_ready_link_failed.load(Ordering::Acquire));
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
    resume.send(()).expect("allow canonical Ready commit");
    producer
        .join()
        .expect("Ready producer joins")
        .expect("Ready commits");
    eof.join()
        .expect("real reader joins")
        .expect("EOF delivered to existing owner");
    assert!(router.window_ready.load(Ordering::Acquire));
    assert!(router.recovery_armed.load(Ordering::Acquire));
    assert!(!router.pre_ready_link_failed.load(Ordering::Acquire));
    assert!(router.shutdown.transition_guard().pending_ready.is_none());
    worker.stop_and_join().expect("physical writer drains");
    drop(worker);
    drop(router);
    drop(windows);
    assert_eq!(owner.join().expect("owner joins"), ["arm", "failed"]);
}

#[test]
fn deadline_wins_after_physical_ready_and_late_commit_cannot_arm_recovery() {
    let Fixture {
        router,
        mut peer,
        mut reader,
        caller,
        owner,
        windows,
        mut worker,
    } = fixture();
    let failure_owner = router.clone();
    let owner = thread::spawn(move || owner_loop(owner, Some(failure_owner)));
    let (written, written_rx) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::sync_channel(1);
    let (waiting, waiting_rx) = mpsc::sync_channel(1);
    {
        let mut hooks = router.quit_drain_hooks.lock().expect("lifecycle hooks");
        hooks.ready_written = Some((written, resume_rx));
        hooks.ready_failure_wait = Some(waiting);
    }
    let producer = router.clone();
    let producer = thread::spawn(move || producer.signal_ready());
    assert_eq!(
        keld_ipc::link::read_frame(&mut peer)
            .expect("actual Ready")
            .1,
        [0]
    );
    written_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("actual Written");
    let eof_handle = router.clone();
    let eof = thread::spawn(move || read_primary_frames(&mut reader, &eof_handle, &caller));
    drop(peer);
    waiting_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("reader is in pending-Ready wait");
    {
        let mut transition = router.shutdown.transition_guard();
        transition
            .pending_ready
            .as_mut()
            .expect("the actual pending record")
            .deadline = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("past deadline");
        router.shutdown.changed.notify_all();
    }
    eof.join()
        .expect("deadline releases EOF")
        .expect("existing owner accepts failure");
    assert!(!router.window_ready.load(Ordering::Acquire));
    assert!(router.pre_ready_link_failed.load(Ordering::Acquire));
    assert!(router.shutdown.transition_guard().pending_ready.is_none());
    resume.send(()).expect("late producer resumes");
    producer
        .join()
        .expect("late producer joins")
        .expect("the existing EOF owner already retired A; late Ready cannot re-arm it");
    assert!(!router.recovery_armed.load(Ordering::Acquire));
    assert!(!router.window_ready.load(Ordering::Acquire));
    assert!(router.current.lock().expect("current").is_none());
    worker.stop_and_join().expect("physical writer drains");
    drop(worker);
    drop(router);
    drop(windows);
    assert_eq!(owner.join().expect("owner joins"), ["startup-failed"]);
}

#[test]
fn failed_writer_retires_accepted_quit_without_appending_its_pending_terminal() {
    let Fixture {
        router,
        mut peer,
        reader,
        caller,
        owner,
        windows,
        mut worker,
    } = fixture();
    {
        let _transition = router.shutdown.transition_guard();
        assert!(router.shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT));
        assert!(
            router
                .shutdown
                .accept_reserved_guarded(SESSION_LIFECYCLE_QUIT)
        );
        router
            .current
            .lock()
            .expect("current")
            .as_mut()
            .expect("active")
            .pending_quit = Some(CorrelationId(41));
    }
    router
        .retire_generation_after_write_failure(&caller)
        .expect("canonical non-writing retirement");
    assert!(router.current.lock().expect("current").is_none());
    assert!(caller.reader_stop.load(Ordering::Acquire));
    let mut byte = [0];
    match peer.read(&mut byte) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::NotConnected
            ) => {}
        result => panic!("failed stream received another terminal or did not close: {result:?}"),
    }
    assert!(matches!(owner.try_recv(), Err(mpsc::TryRecvError::Empty)));
    drop(reader);
    worker
        .stop_and_join()
        .expect("writer owner joins without self-wait");
    drop(worker);
    drop(router);
    drop(windows);
}

#[test]
fn last_window_closed_keeps_physical_completion_when_its_producer_is_delayed() {
    let Fixture {
        router,
        mut peer,
        reader,
        caller: _,
        owner,
        windows,
        mut worker,
    } = fixture();
    let (written, written_rx) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::sync_channel(1);
    router
        .quit_drain_hooks
        .lock()
        .expect("clock hook")
        .last_window_written = Some((written, resume_rx));
    let producer = router.clone();
    let producer = thread::spawn(move || producer.signal_last_window_closed());
    let physical = written_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("physical completion");
    let (header, payload) = keld_ipc::link::read_frame(&mut peer).expect("actual lifecycle frame");
    assert_eq!(header.channel, keld_ipc::LIFECYCLE_CHANNEL);
    assert_eq!(payload, [1], "literal LastWindowClosed event");
    let released = Instant::now();
    resume.send(()).expect("release delayed producer");
    let returned = producer
        .join()
        .expect("producer joins")
        .expect("actual lifecycle write");
    assert_eq!(
        returned, physical,
        "handover D starts at the actual completed write"
    );
    assert!(returned < released, "producer scheduling cannot move D");
    assert!(router.last_window_closed.load(Ordering::Acquire));
    assert!(router.shutdown.transition.try_lock().is_ok());
    assert!(router.current.try_lock().is_ok());
    assert!(matches!(owner.try_recv(), Err(mpsc::TryRecvError::Empty)));
    drop(reader);
    worker.stop_and_join().expect("physical owner joins");
    drop(worker);
    drop(router);
    drop(windows);
}

#[test]
fn four_full_controls_wait_outside_core_fences_then_publish_on_release() {
    use super::super::primary_writer::ControlMessage;
    for ready in [true, false] {
        let Fixture {
            router,
            mut peer,
            reader,
            caller: _,
            owner,
            windows,
            mut worker,
        } = fixture();
        let owner = thread::spawn(move || owner_loop(owner, None));
        let writer = router.primary_writer_owner().expect("same physical owner");
        let target = router
            .current_primary_writer_target()
            .expect("target")
            .expect("active");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut held: [Option<_>; 4] = std::array::from_fn(|index| {
            Some(
                writer
                    .try_prepare_control(
                        Arc::clone(&target),
                        ControlMessage::Ping {
                            channel: keld_ipc::LIFECYCLE_CHANNEL,
                            correlation: CorrelationId(
                                u32::try_from(index + 1).expect("four fixed controls"),
                            ),
                        },
                        deadline,
                    )
                    .expect("occupy one actual CONTROL position"),
            )
        });
        let (waiting, waiting_rx) = mpsc::sync_channel(1);
        writer.observe_next_capacity_wait_for_test(waiting);
        let producer = router.clone();
        let producer = thread::spawn(move || {
            if ready {
                producer.signal_ready()
            } else {
                producer.signal_last_window_closed().map(|_| ())
            }
        });
        waiting_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("actual writer capacity wait");
        assert!(
            router.shutdown.transition.try_lock().is_ok(),
            "capacity waits outside transition"
        );
        assert!(
            router.current.try_lock().is_ok(),
            "capacity waits outside current"
        );
        assert!(router.current.lock().expect("current").is_some());
        assert!(!router.pre_ready_link_failed.load(Ordering::Acquire));
        assert!(!target.caller().reader_stop.load(Ordering::Acquire));
        drop(held[0].take());
        let (header, payload) =
            keld_ipc::link::read_frame(&mut peer).expect("frame after real slot release");
        assert_eq!(header.channel, keld_ipc::LIFECYCLE_CHANNEL);
        assert_eq!(payload, [u8::from(!ready)]);
        producer
            .join()
            .expect("unblocked source joins")
            .expect("capacity was transient");
        assert!(router.current.lock().expect("current").is_some());
        assert!(!target.caller().reader_stop.load(Ordering::Acquire));
        drop(held);
        drop(reader);
        worker.stop_and_join().expect("actual writer joins");
        drop(worker);
        drop(router);
        drop(windows);
        assert_eq!(
            owner.join().expect("owner joins"),
            if ready { vec!["arm"] } else { vec![] }
        );
    }
}

mod quit;
mod retirement;
