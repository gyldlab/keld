//! KEL-53 §4 "Profile handover", steps 3b and 3c, through the router: when the quit
//! deadline D starts, and what keeps a `Quit` the application sent inside D its claim
//! (PR #722 review findings P1-1 and P1-2).
//!
//! Each cell drives the real `complete_profile_handover` on a stand-in main thread, the
//! real lifecycle-Quit handler on a stand-in reader thread and the real filesystem
//! drain, with a test D of [`TEST_D`] instead of the host's 10 s. The application side
//! is the test's end of the session link; the primary owner is a thread that answers
//! the accepted-shutdown and shutdown requests as the live owner does.

use super::{
    AppWindowCommand, BootstrapStream, CorrelationId, FS_CHANNEL, FrameKind, FsDispatchSession,
    FsInFlight, HandoverClock, HandoverWait, HostAppError, LIFECYCLE_CHANNEL, LifecycleEvent,
    ManifestError, PERMISSIONS_FILE, PendingFsCall, PrimaryRouterHandle, SESSION_HANDOVER,
    SESSION_LIFECYCLE_QUIT, SessionShutdownState, TestPrimaryOwnerCommand, app_ipc,
    complete_profile_handover, initial_ready_router_with_window_loop, load_verified_manifest,
    primary_test_stream_pair, write_frame,
};
use std::fs::{self, File};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The cells' quit deadline: long enough that a `Quit` sent right after the
/// application reads `LastWindowClosed` lands inside it on a loaded runner (each cell
/// asserts that precondition from its own instants), short enough to pass quickly.
const TEST_D: Duration = Duration::from_secs(1);
/// How far past the deadline a cell looks before it lets the in-flight work go.
const PAST_DEADLINE: Duration = Duration::from_millis(250);
/// Kill switch over every join and poll, never a synchronization wait.
const KILL_SWITCH: Duration = Duration::from_secs(30);
const FS_CORRELATION: CorrelationId = CorrelationId(90);
const QUIT_CORRELATION: CorrelationId = CorrelationId(77);

/// One frame the application read: its kind, `"lifecycle"`, `"fs"` or `"other"`, and
/// its correlation.
type SeenFrame = (FrameKind, &'static str, u32);

/// Answers the direct owner's accepted-shutdown and shutdown requests, as the live
/// owner does, so either tail runs to its end.
fn answering_owner(owner_rx: Receiver<TestPrimaryOwnerCommand>) -> JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(command) = owner_rx.recv_timeout(KILL_SWITCH) {
            match command {
                TestPrimaryOwnerCommand::PrepareAcceptedShutdown(reply)
                | TestPrimaryOwnerCommand::Shutdown(reply) => {
                    let _ = reply.send(Ok(()));
                }
                _ => {}
            }
        }
    })
}

fn read_lifecycle_event(client: &mut BootstrapStream) -> LifecycleEvent {
    try_read_lifecycle_event(client).expect("lifecycle Event")
}

/// The next lifecycle event, or `None` when the link ended first.
fn try_read_lifecycle_event(client: &mut BootstrapStream) -> Option<LifecycleEvent> {
    let (header, payload) = keld_ipc::link::read_frame(client).ok()?;
    assert_eq!(header.kind, FrameKind::Event);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    Some(keld_ipc::codec::decode::<LifecycleEvent>(&payload).expect("lifecycle payload"))
}

/// The application's side of the session link after `LastWindowClosed`: reads every
/// frame to the link's end. On the `Quit` reply it closes its end of the `Quit`
/// reader's pair, as a real client closes its link after the reply (the Windows Quit
/// tail awaits that peer close before its link close).
fn application_reader(
    mut client: BootstrapStream,
    quit_peer: BootstrapStream,
) -> JoinHandle<Vec<SeenFrame>> {
    thread::spawn(move || {
        let mut peer = Some(quit_peer);
        let mut frames = Vec::new();
        while let Ok((header, _)) = keld_ipc::link::read_frame(&mut client) {
            let channel = if header.channel == LIFECYCLE_CHANNEL {
                "lifecycle"
            } else if header.channel == FS_CHANNEL {
                "fs"
            } else {
                "other"
            };
            if header.kind == FrameKind::Reply
                && channel == "lifecycle"
                && header.corr.0 == QUIT_CORRELATION.0
            {
                drop(peer.take());
            }
            frames.push((header.kind, channel, header.corr.0));
            if frames.len() > 16 {
                break;
            }
        }
        frames
    })
}

/// A verified manifest's filesystem dispatch session, as the router owns it.
fn fs_session() -> (tempfile::TempDir, Arc<FsDispatchSession>) {
    let temp = tempfile::tempdir().expect("manifest root");
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).expect("allowed root");
    let scope = allowed.display().to_string().replace('\\', "/");
    let text = format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"]}}}}}}"#);
    let path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&path, &text).expect("write manifest");
    // The loader's integrity refusal names the digest of the exact bytes it read.
    let Err(ManifestError::IntegrityMismatch { actual: digest, .. }) = load_verified_manifest(
        File::open(&path).expect("open manifest for its digest"),
        path.clone(),
        [0_u8; 32],
    ) else {
        panic!("a zero digest must be refused with the actual digest");
    };
    let verified = load_verified_manifest(File::open(&path).expect("open manifest"), path, digest)
        .expect("verify manifest");
    (temp, Arc::new(FsDispatchSession::new(verified)))
}

/// The filesystem call the application wrote before its `Quit`, admitted and in flight.
fn admit_fs_call(fs: &Arc<FsDispatchSession>) -> FsInFlight {
    fs.begin_call(PendingFsCall {
        attempt: 1,
        correlation: FS_CORRELATION,
    })
    .expect("admit the FS call")
    .expect("admission open")
}

/// Whether the filesystem session has no admitted call left.
fn fs_drained(fs: &FsDispatchSession) -> bool {
    fs.state.lock().is_ok_and(|state| state.in_flight == 0)
}

/// Polls `ready` until it holds, under the kill switch.
fn poll_until(label: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + KILL_SWITCH;
    while !ready() {
        assert!(Instant::now() < deadline, "{label}: kill switch");
        thread::park_timeout(Duration::from_millis(1));
    }
}

/// Joins `thread` under the kill switch, so a hung tail fails the cell.
fn join<T>(label: &str, thread: JoinHandle<T>) -> T {
    poll_until(label, || thread.is_finished());
    thread.join().unwrap_or_else(|_| panic!("{label} panicked"))
}

fn pending_quit(handle: &PrimaryRouterHandle) -> Option<CorrelationId> {
    handle
        .current
        .lock()
        .expect("generation lock")
        .as_ref()
        .and_then(|active| active.pending_quit)
}

fn set(instant: &OnceLock<Instant>) -> bool {
    instant.get().is_some()
}

/// The instant the handover's `LastWindowClosed` write returned, once recorded.
fn handover_write(clock: &HandoverClock) -> Instant {
    poll_until("the handover's LastWindowClosed write", || {
        set(&clock.last_window_closed_written)
    });
    *clock
        .last_window_closed_written
        .get()
        .expect("polled above")
}

/// Requests the handover and starts the stand-in main thread once the loop's `Quit`
/// arrived: it writes `LastWindowClosed` and waits up to [`TEST_D`] for the
/// application's `Quit`. Returns the instant just before the write could start.
fn start_handover(
    handle: &PrimaryRouterHandle,
    window_rx: &Receiver<AppWindowCommand>,
) -> (JoinHandle<Result<(), HostAppError>>, Instant) {
    assert!(handle.request_handover().expect("handover request"));
    assert_eq!(
        window_rx.recv_timeout(Duration::from_secs(2)),
        Ok(AppWindowCommand::Quit)
    );
    let main = handle.clone();
    let before_write = Instant::now();
    let waiter = thread::spawn(move || complete_profile_handover(&main, &main.shutdown, TEST_D));
    (waiter, before_write)
}

/// The application's `Quit`, read by the lifecycle-Quit handler on its own thread;
/// returns the handler's thread and the application's end of the `Quit` reader's pair.
fn send_quit(
    handle: &PrimaryRouterHandle,
) -> (JoinHandle<Result<(), HostAppError>>, BootstrapStream) {
    let quit_handle = handle.clone();
    let (mut quit_reader, quit_peer) = primary_test_stream_pair();
    let quit = thread::spawn(move || {
        quit_handle.lifecycle_quit(1, QUIT_CORRELATION, &[0], &mut quit_reader)
    });
    (quit, quit_peer)
}

/// Drops the cell's last router handle, which closes a link no tail closed, and keeps
/// the session cause and the handover clock for the assertions.
fn release(handle: PrimaryRouterHandle) -> (u8, Arc<HandoverClock>) {
    let observed = (
        handle.shutdown.cause(),
        Arc::clone(&handle.shutdown.handover_clock),
    );
    drop(handle);
    observed
}

fn position(frames: &[SeenFrame], wanted: SeenFrame) -> Option<usize> {
    frames.iter().position(|frame| *frame == wanted)
}

/// P1-1. A user closes the last window and the application keeps the session alive
/// windowless; more than D later the host hands the profile over. D runs from the
/// handover's own `LastWindowClosed` write (KEL-53 §4 step 3b), so a `Quit` the
/// application sends right after that write claims the session under lifecycle-Quit
/// and is answered. Before the fix the natural close's instant started D, so the
/// forced claim followed the handover's write at once.
#[test]
fn a_natural_last_window_close_does_not_start_the_handover_deadline() {
    let (handle, mut client, owner_rx, window_rx) = initial_ready_router_with_window_loop();
    let owner = answering_owner(owner_rx);
    let _natural = handle
        .signal_last_window_closed()
        .expect("the user's own close of the last window");
    assert_eq!(
        read_lifecycle_event(&mut client),
        LifecycleEvent::LastWindowClosed
    );
    let natural_returned = Instant::now();
    // Only time passing on the monotonic clock can put the natural close more than D
    // before the handover's write; bounded by TEST_D, never a synchronization wait.
    thread::sleep(TEST_D + PAST_DEADLINE);
    assert!(
        handle.shutdown.is_running(),
        "the windowless session still runs"
    );

    let (waiter, before_write) = start_handover(&handle, &window_rx);
    // Asserted below, after the cause: a forced end closes the link at once.
    let handover_event = try_read_lifecycle_event(&mut client);
    let (quit, quit_peer) = send_quit(&handle);
    let application = application_reader(client, quit_peer);
    poll_until("the Quit handler's entry", || {
        pending_quit(&handle) == Some(QUIT_CORRELATION) || !handle.shutdown.is_running()
    });
    let quit_entered = Instant::now();

    let quit_result = join("the Quit handler", quit);
    let handover_result = join("the stand-in main thread", waiter);
    let (cause, clock) = release(handle);
    let frames = join("the application reader", application);
    eprintln!(
        "P1-1 cell: cause={cause} handover_event={handover_event:?} quit={quit_result:?} \
         handover={handover_result:?} forced={} quit_entry_after_write_start_ms={} frames={frames:?}",
        set(&clock.forced),
        quit_entered
            .saturating_duration_since(before_write)
            .as_millis()
    );
    assert!(
        quit_entered < before_write + TEST_D,
        "precondition: the Quit entered inside D of the handover's write"
    );
    assert!(
        !set(&clock.forced),
        "the deadline claim did not run: D started at the handover's write"
    );
    assert_eq!(cause, SESSION_LIFECYCLE_QUIT);
    assert_eq!(
        handover_event,
        Some(LifecycleEvent::LastWindowClosed),
        "the handover writes its own LastWindowClosed"
    );
    quit_result.expect("the Quit tail");
    handover_result.expect("the handover");
    assert!(
        position(&frames, (FrameKind::Reply, "lifecycle", QUIT_CORRELATION.0)).is_some(),
        "the application's Quit is answered: {frames:?}"
    );
    let written = *clock
        .last_window_closed_written
        .get()
        .expect("the handover's write is recorded");
    assert!(
        written > natural_returned,
        "the clock records the handover's write, not the natural close"
    );
    join("the owner", owner);
}

/// P1-2. The application wrote a filesystem call, then its `Quit` inside D; the Quit
/// handler waits in the filesystem drain past D; then the call completes. The `Quit`
/// keeps its claim through the drain: the session ends under lifecycle-Quit, the
/// filesystem reply reaches the application before the `Quit` reply, and the deadline
/// claim never runs. Before the fix the deadline claimed the handover while the
/// `Quit` drained, the filesystem reply was suppressed and the `Quit` got no reply.
#[test]
fn a_quit_admitted_inside_d_keeps_its_claim_through_its_fs_drain() {
    let (mut handle, mut client, owner_rx, window_rx) = initial_ready_router_with_window_loop();
    let owner = answering_owner(owner_rx);
    let (_temp, fs) = fs_session();
    handle.fs = Some(Arc::downgrade(&fs));
    let admitted = admit_fs_call(&fs);

    let (waiter, before_write) = start_handover(&handle, &window_rx);
    assert_eq!(
        read_lifecycle_event(&mut client),
        LifecycleEvent::LastWindowClosed
    );
    let (quit, quit_peer) = send_quit(&handle);
    let application = application_reader(client, quit_peer);
    poll_until("the Quit handler's entry", || {
        pending_quit(&handle) == Some(QUIT_CORRELATION)
    });
    let quit_entered = Instant::now();

    let deadline = handover_write(&handle.shutdown.handover_clock) + TEST_D;
    // Time must pass the deadline while the Quit drains; bounded, not a sync wait.
    thread::sleep(deadline.saturating_duration_since(Instant::now()) + PAST_DEADLINE);
    let running_past_deadline = handle.shutdown.is_running();
    let forced_past_deadline = set(&handle.shutdown.handover_clock.forced);

    // The filesystem call completes: its terminal reply, then its lease's release.
    let fs_write = handle.submit_fs_terminal_write(admitted, 1, |writer| {
        write_frame(
            writer,
            FrameKind::Reply,
            0,
            FS_CHANNEL,
            FS_CORRELATION,
            b"{}",
        )
        .map_err(|source| app_ipc("FS reply", &source))
    })
    // The direct path never defers a failed write's recovery.
    .map(|deferred_recovery| deferred_recovery.is_some());
    let quit_result = join("the Quit handler", quit);
    let handover_result = join("the stand-in main thread", waiter);
    let (cause, clock) = release(handle);
    let frames = join("the application reader", application);
    eprintln!(
        "P1-2 cell: running_past_deadline={running_past_deadline} \
         forced_past_deadline={forced_past_deadline} cause={cause} fs_write={fs_write:?} \
         quit={quit_result:?} handover={handover_result:?} frames={frames:?}"
    );
    assert!(
        quit_entered < before_write + TEST_D,
        "precondition: the Quit entered inside D of the handover's write"
    );
    assert!(
        running_past_deadline && !forced_past_deadline,
        "a Quit admitted inside D holds the deadline claim through its drain"
    );
    assert_eq!(cause, SESSION_LIFECYCLE_QUIT);
    assert!(!set(&clock.forced), "the deadline claim never ran");
    assert!(!fs_write.expect("the FS terminal write"));
    quit_result.expect("the Quit tail");
    handover_result.expect("the handover");
    let fs_reply = position(&frames, (FrameKind::Reply, "fs", FS_CORRELATION.0));
    let quit_reply = position(&frames, (FrameKind::Reply, "lifecycle", QUIT_CORRELATION.0));
    assert!(
        matches!((fs_reply, quit_reply), (Some(fs), Some(quit)) if fs < quit),
        "the FS reply reaches the application before the Quit reply: {frames:?}"
    );
    join("the owner", owner);
}

/// Interaction of P1-2: the deadline passes while the admitted `Quit` drains, and then
/// the `Quit` fails (its drain finds the filesystem state poisoned). The `Quit`
/// releases its admission on that error path, so the main thread claims the handover
/// then and runs the forced tail: no claim is lost and nothing hangs.
#[test]
fn a_quit_that_fails_in_its_fs_drain_past_d_lets_the_deadline_claim() {
    let (mut handle, mut client, owner_rx, window_rx) = initial_ready_router_with_window_loop();
    let owner = answering_owner(owner_rx);
    let (_temp, fs) = fs_session();
    handle.fs = Some(Arc::downgrade(&fs));
    let admitted = admit_fs_call(&fs);

    let (waiter, _) = start_handover(&handle, &window_rx);
    assert_eq!(
        read_lifecycle_event(&mut client),
        LifecycleEvent::LastWindowClosed
    );
    let (quit, quit_peer) = send_quit(&handle);
    let application = application_reader(client, quit_peer);
    poll_until("the Quit handler's entry", || {
        pending_quit(&handle) == Some(QUIT_CORRELATION)
    });
    // The drain releases the state lock while it waits; poison it there.
    let poisoner = Arc::clone(&fs);
    let poison_outcome = thread::spawn(move || {
        let _state = poisoner.state.lock().expect("FS state");
        panic!("poison the FS dispatch state under the parked drain");
    })
    .join();
    assert!(poison_outcome.is_err(), "the poisoning thread panicked");

    let deadline = handover_write(&handle.shutdown.handover_clock) + TEST_D;
    // Time must pass the deadline while the Quit drains; bounded, not a sync wait.
    thread::sleep(deadline.saturating_duration_since(Instant::now()) + PAST_DEADLINE);
    let running_past_deadline = handle.shutdown.is_running();

    // Releasing the lease wakes the drain, which now fails.
    drop(admitted);
    let quit_result = join("the Quit handler", quit);
    let handover_result = join("the stand-in main thread", waiter);
    // Neither failed tail closed the link; the last router handle's drop does.
    let (cause, clock) = release(handle);
    let frames = join("the application reader", application);
    eprintln!(
        "P1-2 failure cell: running_past_deadline={running_past_deadline} cause={cause} \
         forced={} quit={quit_result:?} handover={handover_result:?} frames={frames:?}",
        set(&clock.forced)
    );
    assert!(
        running_past_deadline,
        "the admitted Quit held the deadline claim while it drained"
    );
    let quit_error = quit_result.expect_err("the Quit's drain fails");
    assert!(
        quit_error.to_string().contains("in-flight lock poisoned"),
        "{quit_error}"
    );
    assert_eq!(
        cause, SESSION_HANDOVER,
        "the main thread claimed once the failed Quit released its admission"
    );
    assert!(set(&clock.forced), "the deadline claim ran");
    let tail_error =
        handover_result.expect_err("the forced tail meets the same poison_outcome state");
    assert!(
        tail_error.to_string().contains("in-flight lock poisoned"),
        "{tail_error}"
    );
    assert!(
        position(&frames, (FrameKind::Reply, "lifecycle", QUIT_CORRELATION.0)).is_none(),
        "the failed Quit is not answered: {frames:?}"
    );
    join("the owner", owner);
}

/// Control for P1-2: the generation of an admitted `Quit` is retired while the `Quit`
/// drains. That `Quit` can no longer claim (it is answered `KELD-IPC-023`), so it no
/// longer holds the deadline: the main thread claims the handover at D, while the
/// filesystem call is still in flight, and the forced tail drains it.
#[test]
fn a_quit_retired_during_its_fs_drain_no_longer_holds_the_deadline() {
    let (mut handle, mut client, owner_rx, window_rx) = initial_ready_router_with_window_loop();
    let owner = answering_owner(owner_rx);
    let (_temp, fs) = fs_session();
    handle.fs = Some(Arc::downgrade(&fs));
    let admitted = admit_fs_call(&fs);

    let (waiter, _) = start_handover(&handle, &window_rx);
    assert_eq!(
        read_lifecycle_event(&mut client),
        LifecycleEvent::LastWindowClosed
    );
    let (quit, quit_peer) = send_quit(&handle);
    let application = application_reader(client, quit_peer);
    poll_until("the Quit handler's entry", || {
        pending_quit(&handle) == Some(QUIT_CORRELATION)
    });
    handle
        .retire_generation(1)
        .expect("retire the Quit's generation");

    let deadline = handover_write(&handle.shutdown.handover_clock) + TEST_D;
    poll_until("the deadline claim", || {
        set(&handle.shutdown.handover_clock.forced)
    });
    let claimed_after_deadline = Instant::now().saturating_duration_since(deadline);
    let fs_in_flight_at_claim = !fs_drained(&fs);

    drop(admitted);
    let quit_result = join("the Quit handler", quit);
    let handover_result = join("the stand-in main thread", waiter);
    let (cause, _clock) = release(handle);
    let frames = join("the application reader", application);
    eprintln!(
        "P1-2 retirement control: claim_after_deadline_ms={} \
         fs_in_flight_at_claim={fs_in_flight_at_claim} cause={cause} quit={quit_result:?} \
         handover={handover_result:?} frames={frames:?}",
        claimed_after_deadline.as_millis()
    );
    assert!(
        fs_in_flight_at_claim,
        "the claim came at D while the FS call was in flight"
    );
    assert_eq!(cause, SESSION_HANDOVER);
    quit_result.expect("a retired Quit returns quietly");
    handover_result.expect("the forced tail");
    // The retirement's KELD-IPC-023 answers are best effort by contract
    // (`answer_pending_calls_locked`): its link close may discard what the
    // application has not read yet, so only the absent reply is asserted.
    assert!(
        position(&frames, (FrameKind::Reply, "lifecycle", QUIT_CORRELATION.0)).is_none(),
        "a retired Quit gets no reply: {frames:?}"
    );
    join("the owner", owner);
}

/// Control for both cells: with no earlier natural close and no filesystem call in
/// flight, a `Quit` sent right after the handover's write claims the session under
/// lifecycle-Quit and is answered; the deadline claim never runs.
#[test]
fn a_quit_inside_d_claims_the_session_and_is_answered() {
    let (mut handle, mut client, owner_rx, window_rx) = initial_ready_router_with_window_loop();
    let owner = answering_owner(owner_rx);
    let (_temp, fs) = fs_session();
    handle.fs = Some(Arc::downgrade(&fs));

    let (waiter, _) = start_handover(&handle, &window_rx);
    assert_eq!(
        read_lifecycle_event(&mut client),
        LifecycleEvent::LastWindowClosed
    );
    let (quit, quit_peer) = send_quit(&handle);
    let application = application_reader(client, quit_peer);
    let quit_result = join("the Quit handler", quit);
    let handover_result = join("the stand-in main thread", waiter);
    let (cause, clock) = release(handle);
    let frames = join("the application reader", application);
    assert_eq!(cause, SESSION_LIFECYCLE_QUIT);
    assert!(!set(&clock.forced), "the deadline claim never ran");
    quit_result.expect("the Quit tail");
    handover_result.expect("the handover");
    assert!(
        position(&frames, (FrameKind::Reply, "lifecycle", QUIT_CORRELATION.0)).is_some(),
        "the application's Quit is answered: {frames:?}"
    );
    join("the owner", owner);
}

/// The admission alone, on the session state: an expired deadline does not claim while
/// a `Quit` is admitted; dropping the admission (any non-claiming return of the
/// handler) or releasing it under the guard lets the waiter claim at once; a stale
/// generation's clear never releases a newer generation's admission.
#[test]
fn an_admitted_quit_holds_an_expired_deadline_until_it_is_released() {
    let shutdown = SessionShutdownState::new();
    let admission = {
        let _transition = shutdown.transition_guard();
        shutdown.admit_quit_guarded(2)
    };
    {
        let _transition = shutdown.transition_guard();
        shutdown.clear_quit_admission_guarded(1);
    }
    let waiter_state = shutdown.clone();
    let waiter = thread::spawn(move || waiter_state.await_quit_or_claim_handover(Instant::now()));
    // Bounded observation that the expired deadline does not claim; not a sync wait.
    thread::sleep(Duration::from_millis(200));
    assert!(
        !waiter.is_finished() && shutdown.is_running(),
        "an admitted Quit holds the expired deadline, and a stale clear released nothing"
    );
    drop(admission);
    assert_eq!(join("the waiter", waiter), HandoverWait::ClaimedHandover);
    assert_eq!(shutdown.cause(), SESSION_HANDOVER);

    let released = SessionShutdownState::new();
    let admission = {
        let _transition = released.transition_guard();
        released.admit_quit_guarded(1)
    };
    {
        let _transition = released.transition_guard();
        admission.release_guarded();
    }
    assert_eq!(
        released.await_quit_or_claim_handover(Instant::now()),
        HandoverWait::ClaimedHandover
    );
}
