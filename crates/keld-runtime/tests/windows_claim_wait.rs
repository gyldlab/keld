//! Real Windows proof, at the crate level with a child stand-in for the candidate, of the
//! two mechanisms that end the owner's claim wait (KEL-53 §4 "Candidate connect-back",
//! *Margin and deadlines*; §7 "8, 20 (profile handover)" h13): the retained launch handle,
//! through `WindowsLaunchedProcess::wait_until_exited` on a watcher thread driving the
//! endpoint's claim canceller, and the claim deadline; and one negative control per
//! mechanism, each with the other mechanism alone, so the row detects each missing
//! mechanism on its own.
//!
//! Oracles: the typed `keld-ipc` outcome; the elapsed time on this process's monotonic
//! clock against the test's own floor and deadline stand-ins (the production floor is
//! S6c2d's constant, and `accept_claim` takes its deadline as a parameter, so nothing here
//! shortens a production deadline); the launch handle's state, read independently after
//! the wait; and the watcher's own report of what ended it.

#![cfg(windows)]
#![allow(unsafe_code)] // isolated test-only own-session query for the endpoint's per-user form
#![allow(clippy::expect_used, clippy::panic)] // process fixture invariants must abort loudly
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use keld_ipc::{
    SessionToken, WindowsAttemptClaimCanceller, WindowsAttemptEndpoint,
    WindowsAttemptEndpointSecurity, WindowsAttemptExchangeError, WindowsAttemptOwnerChannel,
    WindowsPeerTokenFacts,
};
use keld_runtime::windows_job::{WindowsLaunchedProcess, WindowsProcessJob, WindowsProcessPeer};
use keld_runtime::windows_lpac::WindowsSuspendedChild;
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;

/// The kill switch of every wait here: far above any expected step, never a
/// synchronization.
const KILL_SWITCH: Duration = Duration::from_secs(20);
/// The claim floor's stand-in: the deadline a cell passes when the handle must end the
/// wait first, and the time the negative controls must reach.
const FLOOR: Duration = Duration::from_secs(5);
/// The watcher's slice: it reads its stop condition between bounded waits, so it outlives
/// the wait it serves by at most one slice.
const SLICE: Duration = Duration::from_millis(50);
/// "At once": the bound on the handle mechanism's latency, from the return of the call
/// that ends the candidate to the owner's return. That call, `terminate_and_wait`,
/// returns only once the candidate's family has exited, so from its return the launch
/// handle is signaled and only the handle mechanism remains. The call's own duration is
/// the OS's process-kill latency, which is unbounded under load and is not the property:
/// a hosted runner spent 260 ms from the call to the owner's return, and 0 ms of it after
/// the call returned. S6c2b measured the whole span over twenty runs of this file
/// while the full `keld-runtime` suite ran concurrently as load (its evidence record,
/// `flake-load1.log`, on this machine: maximum 5 ms, median 1 ms); four times that
/// maximum is 20 ms, and the bound takes the rule's floor instead, sixteen default
/// timer ticks (16 x 15.625 ms), for a loaded two-core runner. It stays twenty times
/// below [`FLOOR`] (checked at compile time below), so the negative control still
/// separates the two mechanisms.
const AT_ONCE: Duration = Duration::from_millis(250);
// "At once" must stay well below the floor, or a wait that ends at the floor could pass
// as one that ends at once.
const _: () = assert!(AT_ONCE.as_millis() * 20 <= FLOOR.as_millis());
/// `TerminateJobObject`'s exit code in `WindowsProcessJob::terminate_and_wait`.
const JOB_TERMINATION_EXIT: u32 = 1;

static NONCE: AtomicU32 = AtomicU32::new(0);

fn system32() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set")).join("System32")
}

/// The stand-in's minimal complete environment block.
fn environment() -> Vec<(OsString, OsString)> {
    ["SystemRoot", "WINDIR"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
        .collect()
}

/// A same-token candidate stand-in that holds until it is ended: `waitfor.exe` waiting
/// up to its maximum for a signal nobody sends, spawned directly (no shell, so no
/// grandchild), recorded, assigned to its own attempt Job and resumed, as the candidate
/// is before the owner's claim wait. The signal name is letters and digits only, the
/// only characters `waitfor` admits; a refused name exits at once, which the fixture
/// check below would report.
fn held_candidate() -> (WindowsProcessJob, WindowsLaunchedProcess) {
    let system32 = system32();
    let signal = format!(
        "kelds6c2bp{}n{}",
        std::process::id(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    );
    let child = WindowsSuspendedChild::spawn_same_token(
        &system32.join("waitfor.exe"),
        &[
            OsString::from("/T"),
            OsString::from("99999"),
            OsString::from(signal),
        ],
        &environment(),
        &system32,
    )
    .expect("create the held candidate suspended");
    let mut job = WindowsProcessJob::create().expect("create the attempt Job");
    let mut launched = WindowsLaunchedProcess::record(child).expect("record the launch");
    let membership = job
        .assign_child(launched.child())
        .expect("assign the suspended candidate to the attempt Job");
    launched.resume(&membership).expect("resume the candidate");
    // Fixture check: the stand-in holds after its resume; it is not the oracle.
    assert!(
        !launched
            .wait_until_exited(Duration::from_millis(100))
            .expect("query the held candidate"),
        "the stand-in must hold until it is ended"
    );
    (job, launched)
}

fn own_session() -> u32 {
    let mut session_id = 0_u32;
    // SAFETY: this process's own ID names a live process; the output is writable.
    assert_ne!(
        unsafe { ProcessIdToSessionId(std::process::id(), &raw mut session_id) },
        0,
        "query this process's session: {}",
        std::io::Error::last_os_error()
    );
    session_id
}

/// A fresh connect-back endpoint of this user, under random IDs so concurrent cells
/// never share a name.
fn own_endpoint() -> WindowsAttemptEndpoint {
    let own =
        WindowsProcessPeer::open(std::process::id(), own_session()).expect("pin this process");
    let security =
        WindowsAttemptEndpointSecurity::per_user_connect_back(&own.token_facts().user_sid)
            .expect("the per-user connect-back form");
    let fresh = || *SessionToken::random().expect("random ID").as_bytes();
    WindowsAttemptEndpoint::create_connect_back(&fresh(), &fresh(), &fresh(), &security)
        .expect("create the connect-back endpoint")
}

type Outcome = Result<WindowsAttemptOwnerChannel<WindowsProcessPeer>, WindowsAttemptExchangeError>;

/// Runs the owner's claim wait on its own thread; nothing connects, so the claimant check
/// never runs. Reports the instant the wait ended.
fn spawn_owner(
    endpoint: WindowsAttemptEndpoint,
    deadline: Instant,
) -> (thread::JoinHandle<Outcome>, mpsc::Receiver<Instant>) {
    let (ended, ended_at) = mpsc::channel();
    let owner = thread::spawn(move || {
        let outcome = endpoint
            .accept_claim(deadline, |_: u32, _: u32, _: &WindowsPeerTokenFacts| {
                None::<WindowsProcessPeer>
            });
        let _ = ended.send(Instant::now());
        outcome
    });
    (owner, ended_at)
}

/// What ended the watcher.
#[derive(Debug, PartialEq, Eq)]
enum WatchEnd {
    /// The launch handle signaled; the watcher cancelled the claim wait.
    HandleSignaled,
    /// The owner's wait had ended; the watcher stopped within one slice.
    Stopped,
}

/// The watcher the S6c2e coordinator composes: a shared borrow of the launch record,
/// bounded slices on its handle, the stop condition read between them, and the claim
/// canceller driven once the handle signals.
fn watch(
    launched: &WindowsLaunchedProcess,
    stop: &AtomicBool,
    canceller: &WindowsAttemptClaimCanceller,
) -> WatchEnd {
    let kill_switch = Instant::now() + KILL_SWITCH * 2;
    loop {
        if stop.load(Ordering::Acquire) {
            return WatchEnd::Stopped;
        }
        if launched
            .wait_until_exited(SLICE)
            .expect("wait on the launch handle")
        {
            canceller.cancel().expect("cancel the claim wait");
            return WatchEnd::HandleSignaled;
        }
        assert!(Instant::now() < kill_switch, "watcher kill switch");
    }
}

fn join_owner(owner: thread::JoinHandle<Outcome>) -> Outcome {
    owner.join().expect("the owner thread")
}

/// h13, the handle half: a candidate killed after its resume and before it connects ends
/// the claim wait at once, by its signaled launch handle, well before the floor; the
/// outcome is the typed cancel, not the deadline.
#[test]
fn a_candidate_killed_after_its_resume_ends_the_claim_wait_at_once_through_its_launch_handle() {
    let (job, mut launched) = held_candidate();
    let endpoint = own_endpoint();
    let canceller = endpoint.claim_canceller();
    let started = Instant::now();
    let (owner, ended) = spawn_owner(endpoint, started + FLOOR);
    let stop = AtomicBool::new(false);
    let (watch_end, from_kill_call, from_kill_return) = thread::scope(|scope| {
        let watcher = scope.spawn(|| watch(&launched, &stop, &canceller));
        let kill_called_at = Instant::now();
        job.terminate_and_wait(launched.child(), KILL_SWITCH)
            .expect("end the candidate family");
        let kill_returned_at = Instant::now();
        let ended_at = ended
            .recv_timeout(KILL_SWITCH)
            .expect("the claim wait ends within the kill switch");
        stop.store(true, Ordering::Release);
        (
            watcher.join().expect("the watcher thread"),
            ended_at.duration_since(kill_called_at),
            ended_at.saturating_duration_since(kill_returned_at),
        )
    });
    let outcome = join_owner(owner);
    println!(
        "S6C2B_LATENCY handle_cancel_from_kill_call_ms={} from_kill_return_ms={}",
        from_kill_call.as_millis(),
        from_kill_return.as_millis()
    );
    assert!(
        matches!(outcome, Err(WindowsAttemptExchangeError::ClaimCancelled)),
        "the handle, not the deadline, ended the wait: {outcome:?}"
    );
    assert_eq!(watch_end, WatchEnd::HandleSignaled);
    assert!(
        launched.has_exited().expect("query the launch"),
        "the launch handle is signaled"
    );
    // Measured from the kill's return, when the candidate is known dead: the kill call's
    // own latency belongs to the OS, not to the claim wait (see [`AT_ONCE`]).
    assert!(
        from_kill_return < AT_ONCE,
        "at once once the candidate is dead, not at the floor: {from_kill_return:?} \
         after the kill returned ({from_kill_call:?} after the kill call)"
    );
    assert_eq!(
        launched.wait(10_000).expect("reap the killed candidate"),
        JOB_TERMINATION_EXIT
    );
}

/// h13, the deadline half: a candidate that lives but never connects ends the claim wait
/// at the claim deadline, with the handle watcher wired and silent; the watcher stops
/// within its slice once the wait has ended.
#[test]
fn a_live_candidate_that_never_connects_ends_the_claim_wait_at_the_deadline() {
    let (job, launched) = held_candidate();
    let endpoint = own_endpoint();
    let canceller = endpoint.claim_canceller();
    let deadline_in = Duration::from_millis(1500);
    let started = Instant::now();
    let (owner, ended) = spawn_owner(endpoint, started + deadline_in);
    let stop = AtomicBool::new(false);
    let (watch_end, ended_at, watcher_stop) = thread::scope(|scope| {
        let watcher = scope.spawn(|| watch(&launched, &stop, &canceller));
        let ended_at = ended
            .recv_timeout(KILL_SWITCH)
            .expect("the claim wait ends within the kill switch");
        stop.store(true, Ordering::Release);
        let stopped_at = Instant::now();
        let watch_end = watcher.join().expect("the watcher thread");
        (watch_end, ended_at, stopped_at.elapsed())
    });
    let outcome = join_owner(owner);
    let elapsed = ended_at.duration_since(started);
    println!(
        "S6C2B_LATENCY deadline_overshoot_ms={} watcher_stop_ms={}",
        elapsed.saturating_sub(deadline_in).as_millis(),
        watcher_stop.as_millis()
    );
    assert!(
        matches!(outcome, Err(WindowsAttemptExchangeError::ClaimDeadline)),
        "the deadline ended the wait: {outcome:?}"
    );
    assert_eq!(watch_end, WatchEnd::Stopped, "the handle never signaled");
    assert!(
        !launched.has_exited().expect("query the launch"),
        "the candidate still lives"
    );
    assert!(
        elapsed >= deadline_in,
        "not before the deadline: {elapsed:?}"
    );
    assert!(
        elapsed < deadline_in + AT_ONCE,
        "at the deadline, not later: {elapsed:?}"
    );
    assert!(
        watcher_stop < SLICE + AT_ONCE,
        "the watcher outlives the wait by at most one slice: {watcher_stop:?}"
    );
    job.terminate_and_wait(launched.child(), KILL_SWITCH)
        .expect("end the live candidate");
}

/// h13, negative control for the handle mechanism: with the handle watcher not wired,
/// so that only the deadline ends the wait, the killed candidate's wait ends at the floor
/// instead of at once, so the row detects the missing mechanism.
#[test]
fn without_the_handle_watcher_a_killed_candidate_ends_the_claim_wait_only_at_the_floor() {
    let (job, mut launched) = held_candidate();
    let endpoint = own_endpoint();
    let floor = Instant::now() + FLOOR;
    let (owner, ended) = spawn_owner(endpoint, floor);
    job.terminate_and_wait(launched.child(), KILL_SWITCH)
        .expect("end the candidate family");
    let kill_returned_at = Instant::now();
    let ended_at = ended
        .recv_timeout(KILL_SWITCH)
        .expect("the claim wait ends within the kill switch");
    let outcome = join_owner(owner);
    // Fixture check, the handle cell's own precondition: the kill returned before the
    // floor, so the owner was still waiting on a dead candidate. A later return would make
    // this run the live-candidate case, which proves nothing about a killed one.
    assert!(
        kill_returned_at < floor,
        "the kill returned {:?} after the floor",
        kill_returned_at.saturating_duration_since(floor)
    );
    assert!(
        launched.has_exited().expect("query the launch"),
        "the launch handle is signaled, and nothing watched it"
    );
    assert!(
        matches!(outcome, Err(WindowsAttemptExchangeError::ClaimDeadline)),
        "only the deadline ended the wait: {outcome:?}"
    );
    // At the floor, not at once. The origin is the handle cell's, the kill's return; the
    // bound is the floor itself, not "the kill's return plus `AT_ONCE`". The kill's own
    // latency is the OS's (see [`AT_ONCE`]): a kill that returns within `AT_ONCE` of the
    // floor still leaves a correct deadline-only wait ending at the floor, and the typed
    // outcome above, not the clock, then separates it from the handle cell.
    assert!(
        ended_at >= floor,
        "at the floor, not before: {:?} after the kill returned",
        ended_at.saturating_duration_since(kill_returned_at)
    );
    assert_eq!(
        launched.wait(10_000).expect("reap the killed candidate"),
        JOB_TERMINATION_EXIT
    );
}

/// h13, negative control for the deadline mechanism: with the deadline disabled (set past
/// the test's kill switch), so that only the handle could end the wait, the live
/// never-connecting candidate leaves the owner still waiting past the floor; the harness
/// then ends the wait through the canceller, so the row detects the missing mechanism.
#[test]
fn without_the_deadline_a_live_never_connecting_candidate_leaves_the_owner_waiting_past_the_floor()
{
    let (job, launched) = held_candidate();
    let endpoint = own_endpoint();
    let canceller = endpoint.claim_canceller();
    let started = Instant::now();
    let (owner, ended) = spawn_owner(endpoint, started + KILL_SWITCH * 4);
    let stop = AtomicBool::new(false);
    let (watch_end, ended_at) = thread::scope(|scope| {
        let watcher = scope.spawn(|| watch(&launched, &stop, &canceller));
        assert!(
            matches!(
                ended.recv_timeout(FLOOR + AT_ONCE),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "with no deadline and a live candidate, nothing ends the wait by the floor"
        );
        assert!(
            !launched.has_exited().expect("query the launch"),
            "the candidate still lives"
        );
        canceller.cancel().expect("the harness ends the wait");
        let ended_at = ended
            .recv_timeout(KILL_SWITCH)
            .expect("the cancel ends the wait within the kill switch");
        stop.store(true, Ordering::Release);
        (watcher.join().expect("the watcher thread"), ended_at)
    });
    let outcome = join_owner(owner);
    assert!(
        matches!(outcome, Err(WindowsAttemptExchangeError::ClaimCancelled)),
        "{outcome:?}"
    );
    assert_eq!(watch_end, WatchEnd::Stopped, "the handle never signaled");
    assert!(
        ended_at.duration_since(started) >= FLOOR,
        "still waiting past the floor"
    );
    job.terminate_and_wait(launched.child(), KILL_SWITCH)
        .expect("end the live candidate");
}
