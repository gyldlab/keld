//! Component test for the draft terminal cut using the real Core FS worker,
//! writer, lease counter and `SessionShutdownState`. The caller, selection and
//! registry are test fixtures; this does not wire a conditional IPC handler,
//! pause the real reader, invoke the lifecycle tail or create native windows.

use super::super::super::super::{
    FsDispatchSession, PrimaryLinkIdentity, PrimaryRouterHandle, SESSION_LIFECYCLE_QUIT,
    tests::{guarded_test_router, write_fs_call},
};
use super::super::super::WindowRegistry;
use super::{HostFacts, Outcome, TerminalQuit};
use keld_ipc::codec::decode;
use keld_ipc::link::read_frame;
use keld_ipc::{BootstrapStream, CallError, CorrelationId, FrameKind};
use keld_native::fs::{FS_CHANNEL, FsResponse};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Arc, atomic::Ordering, mpsc};
use std::thread;
use std::time::Duration;

/// The actual router's transition/current and FS state are read in their
/// established order. The independent empty registry is local to this test.
/// No other thread mutates it or sends subsequent peer calls during the drain.
fn cut<R>(
    handle: &PrimaryRouterHandle,
    registry: &WindowRegistry,
    fs: &FsDispatchSession,
    run: impl FnOnce(HostFacts) -> R,
) -> R {
    let _transition = handle.shutdown.transition_guard();
    let current = handle.current.lock().expect("current fixture");
    let active = current.as_ref().expect("current generation");
    let drained = {
        let state = fs.state.lock().expect("FS state");
        state.in_flight == 0
            && !state.call_outstanding
            && state.pending_call.is_none()
            && state.failed_write_attempt.is_none()
    };
    let empty = !registry.has_live_windows();
    run(HostFacts {
        generation: NonZeroU32::new(active.attempt),
        qualified: active.identity == PrimaryLinkIdentity::Fixture
            && active.attempt == 1
            && !active.reader_stop.load(Ordering::Acquire)
            && registry.ready
            && !registry.ending,
        running: handle.shutdown.is_running(),
        registry_empty: empty,
        destruction_complete: empty,
        fs_terminal_drained: drained,
    })
}

fn assert_actual_terminal_frame(peer: &mut BootstrapStream, target: &Path, denied: bool) {
    let (header, payload) = read_frame(peer).expect("read actual terminal frame");
    assert_eq!(
        (header.channel, header.corr),
        (FS_CHANNEL, CorrelationId(60))
    );
    if denied {
        assert_eq!(header.kind, FrameKind::Err);
        assert_eq!(
            decode::<CallError>(&payload)
                .expect("actual typed refusal")
                .code,
            "KELD-GUARD002"
        );
        assert!(!target.exists());
    } else {
        assert_eq!(header.kind, FrameKind::Reply);
        assert!(matches!(
            decode::<FsResponse>(&payload).expect("actual FS response"),
            FsResponse::Write
        ));
        assert_eq!(std::fs::read(target).expect("actual FS effect"), b"t2");
    }
}

#[test]
fn conditional_cut_waits_for_actual_fs_reply_and_lease() {
    for denied in [false, true] {
        let (fixture, mut peer) = guarded_test_router();
        let handle = fixture.router.handle();
        let fs = &fixture.snapshot.fs;
        let mut registry = WindowRegistry::new();
        assert!(registry.mark_ready());
        let mut quit = TerminalQuit::new();
        let Outcome::Prepared(token) = cut(&handle, &registry, fs, |facts| quit.prepare(facts))
        else {
            panic!("expected fixture preparation");
        };

        let target = if denied {
            fixture
                .allowed
                .parent()
                .expect("fixture root")
                .join("denied.txt")
        } else {
            fixture.allowed.join("held.txt")
        };
        let (written_tx, written_rx) = mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = mpsc::channel();
        fs.hold_next_terminal_write(written_tx, resume_rx);
        write_fs_call(&mut peer, 60, &target);
        fixture
            .taken
            .recv_timeout(Duration::from_secs(5))
            .expect("FS worker entered");

        assert_eq!(
            cut(&handle, &registry, fs, |facts| quit
                .begin_commit(token, facts)),
            Outcome::DrainRequired
        );
        let (waiting_tx, waiting_rx) = mpsc::sync_channel(1);
        fs.observe_next_drain_wait(waiting_tx);
        let draining_fs = Arc::clone(fs);
        let drain = thread::spawn(move || draining_fs.drain());
        waiting_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("real drain waits");
        // No reservation before the worker actually writes its terminal frame.
        assert_eq!(
            cut(&handle, &registry, fs, |facts| quit.finish_commit(
                token,
                facts,
                || { panic!("reserved before owed terminal reply") }
            )),
            Outcome::AwaitingDrain
        );
        assert!(handle.shutdown.is_running());
        fixture
            .release
            .send(())
            .expect("release actual FS operation");
        written_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("terminal frame written");
        assert_actual_terminal_frame(&mut peer, &target, denied);
        // The real writer has already retired the correlation but still owns
        // the lease. Receiving the frame is insufficient to reserve terminal.
        assert_eq!(
            cut(&handle, &registry, fs, |facts| quit.finish_commit(
                token,
                facts,
                || { panic!("reserved before actual FS lease drop") }
            )),
            Outcome::AwaitingDrain
        );
        assert!(!drain.is_finished());
        assert!(handle.shutdown.is_running());
        assert!(!handle.shutdown.reader_stop.load(Ordering::Acquire));

        resume_tx.send(()).expect("release actual terminal lease");
        drain
            .join()
            .expect("drain thread")
            .expect("actual FS drained");
        assert_eq!(
            cut(&handle, &registry, fs, |facts| quit.finish_commit(
                token,
                facts,
                || { handle.shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT) }
            )),
            Outcome::Committed
        );
        assert!(!handle.shutdown.is_running());
        assert!(handle.shutdown.reader_stop.load(Ordering::Acquire));
        {
            let _transition = handle.shutdown.transition_guard();
            assert!(!handle.shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT));
        }
        // No new conditional wire or tail is wired. Dispose only this fixture
        // through the existing router owner instead of calling legacy Quit.
        fixture
            .router
            .shutdown()
            .expect("dispose component fixture");
    }
}
