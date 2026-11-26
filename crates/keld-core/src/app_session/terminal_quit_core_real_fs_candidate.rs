//! Component test for the draft terminal cut using the real Core FS worker,
//! writer, lease counter and `SessionShutdownState`. The caller, selection and
//! registry are test fixtures; this does not wire a conditional IPC handler,
//! pause the real reader, invoke the lifecycle tail or create native windows.

use super::super::super::super::{
    FsDispatchSession, PrimaryLinkIdentity, PrimaryRouterHandle, SESSION_LIFECYCLE_QUIT,
    tests::{ReapedRouterProbe, guarded_test_router, write_fs_call},
};
use super::super::super::WindowRegistry;
use super::{HostFacts, Outcome, TerminalQuit, Token, ValidatedExit};
use keld_ipc::codec::decode;
use keld_ipc::link::read_frame;
use keld_ipc::{BootstrapStream, CallError, CorrelationId, FrameKind};
use keld_native::fs::{FS_CHANNEL, FsResponse};
use std::cell::Cell;
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::num::{NonZeroU32, NonZeroU64};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, atomic::Ordering, mpsc};
use std::thread;
use std::time::Duration;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// The actual router's transition/current and FS state are read in their
/// established order. The independent empty registry is local to this test.
/// No other thread mutates it or sends subsequent peer calls during the drain.
fn cut<R>(
    handle: &PrimaryRouterHandle,
    registry: &WindowRegistry,
    fs: &FsDispatchSession,
    run: impl FnOnce(HostFacts) -> R,
) -> TestResult<R> {
    let _transition = handle.shutdown.transition_guard();
    let current = handle
        .current
        .lock()
        .map_err(|_| io::Error::other("current fixture poisoned"))?;
    let active = current
        .as_ref()
        .ok_or_else(|| io::Error::other("current fixture missing"))?;
    let drained = {
        let state = fs
            .state
            .lock()
            .map_err(|_| io::Error::other("FS fixture state poisoned"))?;
        state.in_flight == 0
            && !state.call_outstanding
            && state.pending_call.is_none()
            && state.failed_write_attempt.is_none()
    };
    let empty = !registry.has_live_windows();
    Ok(run(HostFacts {
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
    }))
}

fn assert_actual_terminal_frame(
    peer: &mut BootstrapStream,
    target: &Path,
    denied: bool,
) -> TestResult {
    let (header, payload) = read_frame(peer)?;
    assert_eq!(
        (header.channel, header.corr),
        (FS_CHANNEL, CorrelationId(60))
    );
    if denied {
        assert_eq!(header.kind, FrameKind::Err);
        assert_eq!(decode::<CallError>(&payload)?.code, "KELD-GUARD002");
        assert!(!target.exists());
    } else {
        assert_eq!(header.kind, FrameKind::Reply);
        assert!(matches!(decode::<FsResponse>(&payload)?, FsResponse::Write));
        assert_eq!(std::fs::read(target)?, b"t2");
    }
    Ok(())
}

#[test]
fn conditional_cut_waits_for_actual_fs_reply_and_lease() -> TestResult {
    const CHILD_ENV: &str = "KELD_CORE_TEST_GH451_FS_DRAIN_CHILD";
    const MARKER: &str = "KELD_GH451_FS_DRAIN_OBSERVED";
    const SELECTOR: &str = "app_session::window_registry::tests::terminal_quit_candidate::real_fs::conditional_cut_waits_for_actual_fs_reply_and_lease";
    if std::env::var_os(CHILD_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
        exercise_actual_fs()?;
        println!("{MARKER}");
        return Ok(());
    }
    // Reuse the existing test-only child custodian and its bounded cleanup
    // wait. A regression in drain notification may otherwise hang Drop or
    // join; never modify the production wait merely to make the test finish.
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = ReapedRouterProbe::new(
        Command::new(std::env::current_exe()?)
            .args(["--exact", SELECTOR, "--nocapture", "--test-threads=1"])
            .env(CHILD_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone()?))
            .stderr(Stdio::from(stderr.try_clone()?))
            .spawn()?,
    );
    let status = child.wait_bounded_stderr(&stderr, 64 * 1024);
    drop(child);
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut output = String::new();
    let mut detail = String::new();
    stdout.take(64 * 1024).read_to_string(&mut output)?;
    stderr.take(64 * 1024).read_to_string(&mut detail)?;
    assert!(
        status.as_ref().is_ok_and(std::process::ExitStatus::success)
            && output.contains(MARKER)
            && output.contains("test result: ok. 1 passed; 0 failed;"),
        "isolated FS drain component failed: status={status:?}, stdout={output}, stderr={detail}"
    );
    Ok(())
}

fn assert_both_waiting(
    handle: &PrimaryRouterHandle,
    registry: &WindowRegistry,
    fs: &FsDispatchSession,
    quit: &mut TerminalQuit,
    token: Token,
    exit: ValidatedExit,
) -> TestResult {
    let premature = Cell::new(0);
    let reserve_probe = || {
        premature.set(premature.get() + 1);
        true
    };
    assert_eq!(
        cut(handle, registry, fs, |facts| quit.finish_commit(
            token,
            facts,
            reserve_probe
        ))?,
        Outcome::AwaitingDrain
    );
    assert_eq!(
        cut(handle, registry, fs, |facts| quit.admit_validated_exit(
            exit,
            facts,
            reserve_probe
        ))?,
        Outcome::AwaitingDrain
    );
    assert_eq!(premature.get(), 0);
    Ok(())
}

fn exercise_actual_fs() -> TestResult {
    for denied in [false, true] {
        let (fixture, mut peer) = guarded_test_router();
        let handle = fixture.router.handle();
        let fs = &fixture.snapshot.fs;
        let mut registry = WindowRegistry::new();
        assert!(registry.mark_ready());
        let mut quit = TerminalQuit::new();
        let Outcome::Prepared(token) = cut(&handle, &registry, fs, |facts| quit.prepare(facts))?
        else {
            return Err(io::Error::other("expected fixture preparation").into());
        };
        let exit = ValidatedExit {
            generation: NonZeroU32::new(1).ok_or_else(|| io::Error::other("fixture generation"))?,
            request: NonZeroU64::new(42).ok_or_else(|| io::Error::other("fixture request"))?,
            code: 7,
        };

        let target = if denied {
            fixture
                .allowed
                .parent()
                .ok_or_else(|| io::Error::other("fixture root missing"))?
                .join("denied.txt")
        } else {
            fixture.allowed.join("held.txt")
        };
        let (written_tx, written_rx) = mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = mpsc::channel();
        fs.hold_next_terminal_write(written_tx, resume_rx);
        write_fs_call(&mut peer, 60, &target);
        fixture.taken.recv_timeout(Duration::from_secs(5))?;

        assert_eq!(
            cut(&handle, &registry, fs, |facts| quit
                .begin_commit(token, facts))?,
            Outcome::DrainRequired
        );
        let (waiting_tx, waiting_rx) = mpsc::sync_channel(1);
        fs.observe_next_drain_wait(waiting_tx);
        let draining_fs = Arc::clone(fs);
        let drain = thread::spawn(move || draining_fs.drain());
        waiting_rx.recv_timeout(Duration::from_secs(5))?;
        // No reservation before the worker actually writes its terminal frame.
        assert_both_waiting(&handle, &registry, fs, &mut quit, token, exit)?;
        assert!(handle.shutdown.is_running());
        fixture.release.send(())?;
        written_rx.recv_timeout(Duration::from_secs(5))?;
        assert_actual_terminal_frame(&mut peer, &target, denied)?;
        // The real writer has already retired the correlation but still owns
        // the lease. Receiving the frame is insufficient to reserve terminal.
        assert_both_waiting(&handle, &registry, fs, &mut quit, token, exit)?;
        assert!(!drain.is_finished());
        assert!(handle.shutdown.is_running());
        assert!(!handle.shutdown.reader_stop.load(Ordering::Acquire));

        resume_tx.send(())?;
        drain
            .join()
            .map_err(|_| io::Error::other("drain thread panicked"))??;
        assert_eq!(
            cut(&handle, &registry, fs, |facts| quit.finish_commit(
                token,
                facts,
                || { handle.shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT) }
            ))?,
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
        fixture.router.shutdown()?;
    }
    Ok(())
}
