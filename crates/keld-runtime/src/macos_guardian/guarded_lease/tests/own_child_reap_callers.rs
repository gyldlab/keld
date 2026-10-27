//! GH-824 D1, one contract per Guardian-side parent caller of
//! `retire_own_child_group`: the guarded lease, the supervised lease, and the
//! legacy one-group runner (`run_live_group` and `reject_spawn`). Each caller
//! is its Bun leader's OS parent, so it must decide the group (at most one
//! signal, no in-call wait) and prove absence only after reaping its exact
//! Child. A caller that signals or waits before its own reap burns an in-call
//! absence wait that cannot succeed while its own zombie holds the group.
//!
//! Each caller runs twice: once with the host's fail-safe signal landing
//! before the call (no signal is authorized) and once with it landing between
//! the call's birth check and its one signal (that signal is denied). The
//! test hook, which sits inside `terminate_registered_group`, counts every
//! group `killpg` the product makes, and every denied signal it reconciles.
use std::cell::Cell;
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Child, ExitStatus};
use std::rc::Rc;
use std::time::Instant;

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;

use super::super::super::authenticated_runner::{reject_spawn, run_live_group};
use super::super::super::process_group::{
    GroupIdentity, LocalGroupWitness, TestGroupSignalPoint, install_test_group_signal_hook,
    validate_guardian_child_group,
};
use super::super::super::supervised_lease::SupervisedGuardianLease;
use super::super::super::{GUARDIAN_SHUTDOWN_DEADLINE, RuntimeError, io, lifecycle_error};
use super::own_child_reap::{await_zombie_only, raw, retire_test_child, sleeper_child};
use super::{GenerationLease as _, RevocationCause, locally_enrolled_lease};

/// Where the host's fail-safe `killpg` lands relative to the caller's call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostSignal {
    /// Before the call: the leader is already the caller's own unreaped
    /// zombie, so the decision is ambiguous and sends no signal.
    BeforeCall,
    /// Between the call's birth check and its one signal, which the kernel
    /// then denies (zombie-only EPERM).
    BeforeCallerSignal,
}

/// The caller's result and, when it exposes one, its reaped leader status.
type CallerOutcome = (Result<(), RuntimeError>, Option<ExitStatus>);

/// Holds a test-owned Child until a caller takes it; a test that panics first
/// still kills and reaps it.
struct Unhanded(Option<Child>);

impl Unhanded {
    fn hand_over(mut self) -> Child {
        self.0.take().expect("the leader is handed over once")
    }
}

impl Drop for Unhanded {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            retire_test_child(child);
        }
    }
}

fn assert_caller_decides_only_after_its_own_reap(
    host: HostSignal,
    caller: impl FnOnce(Child, GroupIdentity, LocalGroupWitness) -> CallerOutcome,
) {
    let child = sleeper_child(0);
    let pid = child.id();
    let leader = Unhanded(Some(child));
    let group = raw(pid);
    let identity = validate_guardian_child_group(pid, std::process::id())
        .expect("kernel birth of the owned leader");
    let witness = LocalGroupWitness::default();
    witness
        .register_child(pid)
        .expect("kernel proves the owned direct leader");
    let signals = Rc::new(Cell::new(0_u32));
    let denials = Rc::new(Cell::new(0_u32));
    let (signal_count, denial_count) = (Rc::clone(&signals), Rc::clone(&denials));
    let _hook = install_test_group_signal_hook(
        pid,
        Box::new(move |point| match point {
            TestGroupSignalPoint::BeforeSignal => {
                signal_count.set(signal_count.get() + 1);
                if host == HostSignal::BeforeCallerSignal && signal_count.get() == 1 {
                    killpg(Pid::from_raw(group), Signal::SIGKILL).expect("host fail-safe signal");
                    await_zombie_only(group);
                }
            }
            TestGroupSignalPoint::AfterDeniedSignal => denial_count.set(denial_count.get() + 1),
        }),
    );
    if host == HostSignal::BeforeCall {
        killpg(Pid::from_raw(group), Signal::SIGKILL).expect("host fail-safe signal");
        await_zombie_only(group);
    }

    let started = Instant::now();
    let (result, status) = caller(leader.hand_over(), identity, witness);
    let elapsed = started.elapsed();

    result.expect("the caller's own reap retires the group it parents");
    assert!(
        elapsed < GUARDIAN_SHUTDOWN_DEADLINE / 4,
        "no in-call absence wait before the caller's own reap: {elapsed:?}"
    );
    // Exactly the decision's signals, at most one per call: none when the
    // host already left only the caller's own zombie, else the one
    // birth-checked signal, whose denial is reconciled after the reap. A
    // stray direct signal before the decision would be the one the host
    // pre-empts instead, and the decision would then send none to reconcile.
    let decided = u32::from(host == HostSignal::BeforeCallerSignal);
    assert_eq!(
        (signals.get(), denials.get()),
        (decided, decided),
        "(group signals, reconciled denials)"
    );
    assert_eq!(
        kill(Pid::from_raw(group), None),
        Err(Errno::ESRCH),
        "the caller reaped its own leader"
    );
    assert_eq!(kill(Pid::from_raw(-group), None), Err(Errno::ESRCH));
    if let Some(status) = status {
        assert_eq!(status.signal(), Some(9), "the host's SIGKILL ended it");
    }
}

fn guarded_lease(
    child: Child,
    _identity: GroupIdentity,
    witness: LocalGroupWitness,
) -> CallerOutcome {
    let mut leader = Unhanded(Some(child));
    let child = leader.0.as_mut().expect("held leader");
    let result = locally_enrolled_lease(child.id(), witness)
        .revoke_with_child(RevocationCause::Shutdown, child);
    (result, child.try_wait().expect("cached reap status"))
}

fn supervised_lease(
    child: Child,
    _identity: GroupIdentity,
    witness: LocalGroupWitness,
) -> CallerOutcome {
    let mut leader = Unhanded(Some(child));
    let child = leader.0.as_mut().expect("held leader");
    let result = SupervisedGuardianLease::locally_enrolled_for_test(child.id(), witness)
        .revoke_with_child(RevocationCause::Shutdown, child);
    (result, child.try_wait().expect("cached reap status"))
}

fn live_group_runner(
    child: Child,
    identity: GroupIdentity,
    _witness: LocalGroupWitness,
) -> CallerOutcome {
    let (liveness, host) = UnixStream::pair().expect("host-liveness pair");
    drop(host);
    match run_live_group(child, identity, liveness, || Ok(())) {
        Ok(report) => (Ok(()), Some(report.leader_status())),
        Err(error) => (Err(error), None),
    }
}

fn rejected_spawn_runner(
    child: Child,
    identity: GroupIdentity,
    _witness: LocalGroupWitness,
) -> CallerOutcome {
    let primary = || {
        lifecycle_error(
            "macOS guardian pre-exec registration",
            io::Error::other("test-rejected registration"),
        )
    };
    let error = reject_spawn(primary(), None, Some(identity), Some(child), || Ok(()));
    // The rejection is the primary error alone: any cleanup failure would be
    // collapsed into it.
    let result = if error.to_string() == primary().to_string() {
        Ok(())
    } else {
        Err(error)
    };
    (result, None)
}

#[test]
fn guarded_lease_reaps_before_deciding_a_group_the_host_made_zombie_only() {
    assert_caller_decides_only_after_its_own_reap(HostSignal::BeforeCall, guarded_lease);
}

#[test]
fn guarded_lease_reconciles_its_denied_signal_only_after_its_own_reap() {
    assert_caller_decides_only_after_its_own_reap(HostSignal::BeforeCallerSignal, guarded_lease);
}

#[test]
fn supervised_lease_reaps_before_deciding_a_group_the_host_made_zombie_only() {
    assert_caller_decides_only_after_its_own_reap(HostSignal::BeforeCall, supervised_lease);
}

#[test]
fn supervised_lease_reconciles_its_denied_signal_only_after_its_own_reap() {
    assert_caller_decides_only_after_its_own_reap(HostSignal::BeforeCallerSignal, supervised_lease);
}

#[test]
fn live_group_runner_reaps_before_deciding_a_group_the_host_made_zombie_only() {
    assert_caller_decides_only_after_its_own_reap(HostSignal::BeforeCall, live_group_runner);
}

#[test]
fn live_group_runner_reconciles_its_denied_signal_only_after_its_own_reap() {
    assert_caller_decides_only_after_its_own_reap(
        HostSignal::BeforeCallerSignal,
        live_group_runner,
    );
}

#[test]
fn rejected_spawn_reaps_before_deciding_a_group_the_host_made_zombie_only() {
    assert_caller_decides_only_after_its_own_reap(HostSignal::BeforeCall, rejected_spawn_runner);
}

#[test]
fn rejected_spawn_reconciles_its_denied_signal_only_after_its_own_reap() {
    assert_caller_decides_only_after_its_own_reap(
        HostSignal::BeforeCallerSignal,
        rejected_spawn_runner,
    );
}
