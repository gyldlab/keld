//! A retained group birth is custody, not repeated signal authority. After
//! one failed or refused host-side attempt, later fatal-cleanup calls for the
//! same generation may only probe for absence, inside the one reap budget the
//! first attempt started.
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::unistd::{Pid, getpgid};
use std::os::unix::process::ExitStatusExt as _;

use super::super::super::process_group::terminate_registered_group;
use super::super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardedPrimary, RegisteredPrimaryGeneration, RuntimeError,
    lock_generation,
};
use super::super::test_support::{NativeChildOwner, await_held_zombie_group, test_group_identity};
use super::primary_with_dead_guardian;

mod delivered_signal;

const AMBIGUOUS_GROUP: &str = "registered group identity unavailable";
const PROBE_TIMEOUT: &str = "absence remains unproven";

fn owned_sleep(group: i32) -> NativeChildOwner {
    NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(group)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("test-owned native group process"),
    )
}

/// A test-owned leader plus a second test-owned member of the same group.
fn leader_with_member() -> (NativeChildOwner, NativeChildOwner, i32) {
    let leader = owned_sleep(0);
    let raw = i32::try_from(leader.id()).expect("group PID fits c_int");
    let member = owned_sleep(raw);
    let member_raw = i32::try_from(member.id()).expect("member PID fits c_int");
    assert_eq!(
        getpgid(Some(Pid::from_raw(member_raw))),
        Ok(Pid::from_raw(raw)),
        "the member belongs to the leader's native group"
    );
    (leader, member, raw)
}

fn reap_leader(leader: &mut NativeChildOwner, raw: i32) {
    leader.kill().expect("kill only the test-owned leader");
    leader.wait().expect("reap the test-owned leader");
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Err(Errno::ESRCH));
}

fn first_poll_fatal_error(primary: &mut GuardedPrimary) -> RuntimeError {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        if let Err(error) = primary.poll_fatal() {
            return error;
        }
        assert!(
            Instant::now() < deadline,
            "guardian exit observation deadline"
        );
        std::thread::yield_now();
    }
}

/// Kills the group leader and member, then holds the killed member as an
/// unreaped zombie: Darwin then reports the group as (ESRCH, EPERM) until
/// this test reaps it, which is the persistent ambiguous case.
fn zombie_only_group() -> (NativeChildOwner, RegisteredPrimaryGeneration, i32) {
    let (mut leader, member, raw) = leader_with_member();
    let registered =
        RegisteredPrimaryGeneration::authenticated(1, test_group_identity(leader.id()));
    terminate_registered_group(leader.id()).expect("signal only the test-owned group");
    leader.wait().expect("reap the test-owned leader");
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Err(Errno::ESRCH));
    // SIGKILL delivery to the held member is asynchronous; await the observed
    // zombie-only state through the one shared fixture owner.
    await_held_zombie_group(Pid::from_raw(raw));
    (member, registered, raw)
}

fn reap_zombie(mut member: NativeChildOwner, raw: i32) {
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::EPERM),
        "the host neither signalled nor forgot the unproven group"
    );
    assert_eq!(
        member.wait().expect("reap the held group member").signal(),
        Some(9)
    );
}

/// First attempt refuses a mismatched live leader; that leader then exits
/// while a same-UID member stays live, so a later retry sees (leader ESRCH,
/// group signallable) on the retained numeric PGID and must not signal it.
fn assert_refused_retry_never_signals(first_attempt: impl FnOnce(&mut GuardedPrimary)) {
    let (mut leader, mut member, raw) = leader_with_member();
    // The enrolled birth does not match the live leader: the host's first
    // attempt must refuse, exactly as for a reused leader PID.
    let registered = RegisteredPrimaryGeneration::authenticated(
        1,
        test_group_identity(leader.id()).altered_birth_for_test(),
    );
    let mut primary = primary_with_dead_guardian(registered, 17, true);
    first_attempt(&mut primary);
    reap_leader(&mut leader, raw);
    assert_eq!(kill(Pid::from_raw(-raw), None), Ok(()));

    let retry = primary
        .shutdown()
        .expect_err("the retained generation is not proven absent");
    assert!(
        lock_generation(&primary.current_generation).is_some(),
        "an unproven group keeps its custody record: {retry}"
    );
    // SIGKILL delivery is asynchronous, so a live-now check cannot prove the
    // host never signalled. Terminate the still-owned, unreaped member with
    // SIGTERM ourselves: its reaped status names the signal that killed it.
    let member_raw = i32::try_from(member.id()).expect("member PID fits c_int");
    kill(Pid::from_raw(member_raw), Signal::SIGTERM).expect("signal the unreaped test member");
    assert_eq!(
        member.wait().expect("reap the test-owned member").signal(),
        Some(Signal::SIGTERM as i32),
        "a retry after a refused attempt must never killpg a live group: {retry}"
    );
}

const REUSED_LEADER: &str = "process-group leader PID was reused";

#[test]
fn refused_cleanup_retry_never_signals_a_later_live_group_on_the_same_pgid() {
    assert_refused_retry_never_signals(|primary| {
        let first = first_poll_fatal_error(primary);
        assert!(
            matches!(&first, RuntimeError::GuardianExited { cleanup_error: Some(cleanup), .. }
                if cleanup.to_string().contains(REUSED_LEADER)),
            "first attempt must refuse the mismatched birth: {first}"
        );
    });
}

#[test]
fn refused_fail_current_generation_spends_signal_authority_for_later_retry() {
    assert_refused_retry_never_signals(|primary| {
        let first = primary
            .fail_current_generation(1)
            .expect_err("mismatched birth is refused");
        assert!(
            first.to_string().contains(REUSED_LEADER),
            "first attempt must refuse the mismatched birth: {first}"
        );
    });
}

#[test]
fn first_attempt_still_cleans_a_live_member_after_its_leader_is_reaped() {
    // Negative control: a generation with no failed attempt keeps the
    // original group-wide cleanup for a reaped leader with a live member.
    let (mut leader, mut member, raw) = leader_with_member();
    let registered =
        RegisteredPrimaryGeneration::authenticated(1, test_group_identity(leader.id()));
    reap_leader(&mut leader, raw);
    let mut primary = primary_with_dead_guardian(registered, 17, true);
    let first = first_poll_fatal_error(&mut primary);
    assert!(
        matches!(
            first,
            RuntimeError::GuardianExited {
                cleanup_error: None,
                ..
            }
        ),
        "the first attested cleanup must succeed: {first}"
    );
    assert_eq!(
        member.wait().expect("reap the killed member").signal(),
        Some(9),
        "the first attempt signals the original group"
    );
    assert!(lock_generation(&primary.current_generation).is_none());
}

#[test]
fn one_shutdown_reports_one_native_attempt_for_an_ambiguous_group() {
    let (member, registered, raw) = zombie_only_group();
    let mut primary = primary_with_dead_guardian(registered, 23, true);
    let started = Instant::now();
    let error = primary
        .shutdown()
        .expect_err("ambiguous group cannot be released");
    let elapsed = started.elapsed();
    let text = error.to_string();
    assert_eq!(
        text.matches(AMBIGUOUS_GROUP).count(),
        1,
        "one shutdown transaction makes one native attempt: {text}"
    );
    assert!(
        !text.contains(PROBE_TIMEOUT),
        "no second, probe-only native call in the same shutdown: {text}"
    );
    assert!(
        elapsed < GUARDIAN_SHUTDOWN_DEADLINE / 2,
        "one shutdown must not stack reap windows: {elapsed:?}"
    );
    assert!(lock_generation(&primary.current_generation).is_some());
    reap_zombie(member, raw);
}

#[test]
fn shutdown_after_failed_poll_fatal_shares_the_first_reap_budget() {
    let (member, registered, raw) = zombie_only_group();
    let mut primary = primary_with_dead_guardian(registered, 23, true);
    let first = first_poll_fatal_error(&mut primary);
    assert!(
        first.to_string().contains(AMBIGUOUS_GROUP),
        "first attempt observes the ambiguous group: {first}"
    );
    let started = Instant::now();
    let retry = primary
        .shutdown()
        .expect_err("ambiguous group cannot be released");
    let elapsed = started.elapsed();
    assert!(
        elapsed < (GUARDIAN_SHUTDOWN_DEADLINE / 4).saturating_sub(Duration::from_millis(250)),
        "the first attempt's reap budget is already spent: {elapsed:?}; {retry}"
    );
    assert!(
        !retry.to_string().contains(AMBIGUOUS_GROUP),
        "the native error is reported once, by the first attempt: {retry}"
    );
    assert!(lock_generation(&primary.current_generation).is_some());
    reap_zombie(member, raw);
}
