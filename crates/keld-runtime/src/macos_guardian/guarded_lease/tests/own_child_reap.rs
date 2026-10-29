//! GH-824 Guardian side: the Guardian is its Bun leader's OS parent and
//! reaps that exact Child only after the group decision. When the host's
//! fail-safe signal lands just before the Guardian's revoke, the leader is
//! the Guardian's own unreaped zombie and keeps the group zombie-only
//! (`kill(-g, 0)` EPERM) until that reap. Each test runs the real lease
//! revoke SEQUENTIALLY on one thread (no concurrent reaper): the revoke must
//! reap its own Child first and only then prove absence, without first
//! burning an in-call wait that cannot succeed.
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;

use super::super::super::process_group::{
    LocalGroupWitness, TestGroupSignalPoint, install_test_group_signal_hook,
    retire_own_child_group, validate_guardian_child_group,
};
use super::super::super::{GUARDIAN_SHUTDOWN_DEADLINE, RuntimeError};
use super::{GenerationLease as _, RevocationCause, locally_enrolled_lease};

/// The test is the leader's OS parent, exactly as the Guardian is.
struct OwnedLeader(Child);

impl Drop for OwnedLeader {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn sleeper(group: i32) -> OwnedLeader {
    OwnedLeader(
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

fn raw(pid: u32) -> i32 {
    i32::try_from(pid).expect("positive native PID")
}

/// Await the kernel's zombie-only group state after a SIGKILL; the poll is
/// the awaited condition, the existing deadline only a kill switch.
fn await_zombie_only(group: i32) {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    while kill(Pid::from_raw(-group), None) != Err(Errno::EPERM) {
        assert!(Instant::now() < deadline, "group never became zombie-only");
        thread::yield_now();
    }
}

fn enrolled(leader: &OwnedLeader) -> LocalGroupWitness {
    let witness = LocalGroupWitness::default();
    witness
        .register_child(leader.0.id())
        .expect("kernel proves the owned direct leader");
    witness
}

/// The host's fail-safe `killpg` lands BEFORE the Guardian revoke: the
/// Guardian finds its own leader already a zombie (no inspectable birth,
/// group zombie-only) and must not report a failed revoke.
#[test]
fn guardian_revoke_retires_a_group_the_host_made_zombie_only_after_its_own_reap() {
    let mut leader = sleeper(0);
    let group = raw(leader.0.id());
    let witness = enrolled(&leader);
    killpg(Pid::from_raw(group), Signal::SIGKILL).expect("host fail-safe signal");
    await_zombie_only(group);

    let started = Instant::now();
    let result = locally_enrolled_lease(leader.0.id(), witness)
        .revoke_with_child(RevocationCause::Shutdown, &mut leader.0);
    let elapsed = started.elapsed();

    result.expect("the Guardian's own reap retires the group it parents");
    assert!(
        elapsed < GUARDIAN_SHUTDOWN_DEADLINE / 4,
        "no in-call absence wait before the Guardian's own reap: {elapsed:?}"
    );
    assert_eq!(kill(Pid::from_raw(-group), None), Err(Errno::ESRCH));
    assert_eq!(
        leader
            .0
            .try_wait()
            .expect("reaped status")
            .map(|status| status.signal()),
        Some(Some(9))
    );
}

/// The host's signal lands between the Guardian's birth check and its one
/// signal: the Guardian's own `killpg` is denied (zombie-only EPERM).
#[test]
fn guardian_revoke_reconciles_its_denied_signal_only_after_its_own_reap() {
    let mut leader = sleeper(0);
    let group = raw(leader.0.id());
    let witness = enrolled(&leader);
    let _hook = install_test_group_signal_hook(
        leader.0.id(),
        Box::new(move |point| {
            if point == TestGroupSignalPoint::BeforeSignal {
                killpg(Pid::from_raw(group), Signal::SIGKILL).expect("host fail-safe signal");
                await_zombie_only(group);
            }
        }),
    );

    let started = Instant::now();
    let result = locally_enrolled_lease(leader.0.id(), witness)
        .revoke_with_child(RevocationCause::Shutdown, &mut leader.0);
    let elapsed = started.elapsed();

    result.expect("a denied signal is reconciled after the Guardian's own reap");
    assert!(
        elapsed < GUARDIAN_SHUTDOWN_DEADLINE / 4,
        "no in-call absence wait before the Guardian's own reap: {elapsed:?}"
    );
    assert_eq!(kill(Pid::from_raw(-group), None), Err(Errno::ESRCH));
}

/// Negative control: another member's zombie that the Guardian does NOT
/// parent keeps the group zombie-only after the Guardian's reap. That is
/// not absence: the revoke must fail closed (no CLEAR), and only the real
/// parent's reap makes the group gone.
#[test]
fn guardian_revoke_never_clears_a_group_held_by_a_zombie_it_does_not_reap() {
    let mut leader = sleeper(0);
    let group = raw(leader.0.id());
    let mut other = sleeper(group);
    let witness = enrolled(&leader);
    killpg(Pid::from_raw(group), Signal::SIGKILL).expect("host fail-safe signal");
    await_zombie_only(group);

    let error = locally_enrolled_lease(leader.0.id(), witness)
        .revoke_with_child(RevocationCause::Shutdown, &mut leader.0)
        .expect_err("a zombie this Guardian does not reap is not absence");

    assert!(
        error
            .to_string()
            .contains("registered group identity unavailable"),
        "fail closed with the unproven-identity error: {error}"
    );
    assert_eq!(
        kill(Pid::from_raw(-group), None),
        Err(Errno::EPERM),
        "the group was neither signalled again nor forgotten"
    );
    assert_eq!(
        other.0.wait().expect("reap the other member").signal(),
        Some(9)
    );
    assert_eq!(kill(Pid::from_raw(-group), None), Err(Errno::ESRCH));
}

/// The denied-signal path's error contract: the one signal's typed
/// signal-phase EPERM.
fn assert_signal_phase_eperm(error: &RuntimeError) {
    assert!(
        matches!(
            error,
            RuntimeError::Lifecycle {
                phase: "macOS guardian process-group signal",
                source,
            } if source.raw_os_error() == Some(nix::libc::EPERM)
        ),
        "typed signal-phase EPERM: {error:?}"
    );
}

/// Negative control for the DENIED path: the Guardian's own signal is denied
/// and, after its own reap, a zombie it does not parent still holds the group.
/// That is not absence: fail closed with the typed signal-phase EPERM.
#[test]
fn guardian_denied_signal_never_clears_a_group_held_by_a_zombie_it_does_not_reap() {
    let mut leader = sleeper(0);
    let group = raw(leader.0.id());
    let mut other = sleeper(group);
    let witness = enrolled(&leader);
    let _hook = install_test_group_signal_hook(
        leader.0.id(),
        Box::new(move |point| {
            if point == TestGroupSignalPoint::BeforeSignal {
                killpg(Pid::from_raw(group), Signal::SIGKILL).expect("host fail-safe signal");
                await_zombie_only(group);
            }
        }),
    );

    let error = locally_enrolled_lease(leader.0.id(), witness)
        .revoke_with_child(RevocationCause::Shutdown, &mut leader.0)
        .expect_err("a zombie this Guardian does not reap is not absence");

    assert_signal_phase_eperm(&error);
    assert_eq!(
        kill(Pid::from_raw(-group), None),
        Err(Errno::EPERM),
        "the group was neither signalled again nor forgotten"
    );
    assert_eq!(
        other.0.wait().expect("reap the other member").signal(),
        Some(9)
    );
    assert_eq!(kill(Pid::from_raw(-group), None), Err(Errno::ESRCH));
}

/// A fresh live process joins the group (kept in existence by a zombie the
/// Guardian does not parent) during the Guardian's own reap. The call made
/// its one decision before the reap, so it must never signal again: the
/// joiner survives until the test's own SIGTERM, and the call fails closed.
///
/// Oracle: the joiner's exit signal. Sampling liveness right after the call
/// could race the delivery of a late SIGKILL; an earlier product SIGKILL
/// always wins over the test's later SIGTERM, so `signal() == SIGTERM` proves
/// no second group signal reached it.
fn assert_no_signal_reaches_a_process_that_joins_during_the_reap(denied: bool) {
    let mut leader = sleeper(0);
    let group = raw(leader.0.id());
    let mut other = sleeper(group);
    let identity = validate_guardian_child_group(leader.0.id(), std::process::id())
        .expect("kernel birth of the owned leader");
    let signals: Arc<Mutex<u32>> = Arc::default();
    let count = Arc::clone(&signals);
    let _hook = install_test_group_signal_hook(
        leader.0.id(),
        Box::new(move |point| {
            if point == TestGroupSignalPoint::BeforeSignal {
                *count.lock().expect("signal count") += 1;
                killpg(Pid::from_raw(group), Signal::SIGKILL).expect("host fail-safe signal");
                await_zombie_only(group);
            }
        }),
    );
    if !denied {
        // The host's signal lands before the call: no signal is authorized.
        killpg(Pid::from_raw(group), Signal::SIGKILL).expect("host fail-safe signal");
        await_zombie_only(group);
    }
    let mut joiner = None;
    let (retired, reaped) = retire_own_child_group(identity, |_| {
        leader.0.wait().expect("the Guardian's own reap");
        let fresh = sleeper(group);
        assert_eq!(
            nix::unistd::getpgid(Some(Pid::from_raw(raw(fresh.0.id())))),
            Ok(Pid::from_raw(group)),
            "fresh process joined the still-existing group"
        );
        joiner = Some(fresh);
        Ok(())
    });
    let mut joiner = joiner.expect("the reap added a joiner");
    kill(Pid::from_raw(raw(joiner.0.id())), Signal::SIGTERM).expect("terminate joiner");
    let joiner_signal = joiner.0.wait().expect("reap joiner").signal();
    assert_eq!(
        other.0.wait().expect("reap the other member").signal(),
        Some(9)
    );

    reaped.expect("the Guardian's own reap succeeded");
    assert_eq!(
        joiner_signal,
        Some(15),
        "a second group signal hit the fresh joiner"
    );
    assert_eq!(
        *signals.lock().expect("signal count"),
        u32::from(denied),
        "exactly the decision's signals were prepared"
    );
    let error = retired.expect_err("a group that turned live after the reap is not ours");
    if denied {
        assert_signal_phase_eperm(&error);
    } else {
        assert!(
            error
                .to_string()
                .contains("registered group identity unavailable"),
            "{error}"
        );
    }
}

#[test]
fn guardian_denied_signal_never_signals_a_process_that_joins_during_its_reap() {
    assert_no_signal_reaches_a_process_that_joins_during_the_reap(true);
}

#[test]
fn guardian_ambiguous_group_never_signals_a_process_that_joins_during_its_reap() {
    assert_no_signal_reaches_a_process_that_joins_during_the_reap(false);
}
