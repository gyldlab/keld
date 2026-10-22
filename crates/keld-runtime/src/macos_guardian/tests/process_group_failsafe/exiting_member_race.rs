//! GH-824 host CLEAR trigger: the last live member of an attested group
//! finishes exiting between the cleanup's checks and its one group SIGKILL.
//!
//! XNU `killpg1` counts an exiting member as found but skips `SZOMB` members
//! and returns EPERM when it found none. The group is then zombie-only: the
//! same ambiguous state the read-only absence owner already handles. These
//! native tests put the member's exit exactly in that window through a
//! test-only fault point, instead of relying on scheduler timing. The tests
//! and fault point follow #1077 (main); here the denied signal is classified
//! by the one signal-phase EPERM classifier and decided by the one absence
//! owner, never by a second signal.
use std::cell::RefCell;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Instant;

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use super::super::super::GUARDIAN_SHUTDOWN_DEADLINE;
use super::super::super::process_group::{
    GroupIdentity, TestGroupSignalPoint, install_test_ambiguous_probe_receipt,
    install_test_group_signal_hook, observe_group_identity, reconcile_denied_signal_after_reap,
    terminate_attested_group,
};
use super::super::test_support::await_held_zombie_group;
use super::TestOwnedGroupChild;

const DENIED_SIGNAL: &str = "macOS guardian process-group signal";

fn sleeper(group: i32) -> TestOwnedGroupChild {
    TestOwnedGroupChild(
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

fn pid_of(child: &TestOwnedGroupChild) -> Pid {
    Pid::from_raw(i32::try_from(child.0.id()).expect("positive native PID"))
}

/// A reaped original leader plus one live, test-owned member of its group.
fn reaped_leader_with_live_member() -> (GroupIdentity, i32, TestOwnedGroupChild) {
    let mut leader = sleeper(0);
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("original kernel group birth");
    let raw = i32::try_from(group).expect("positive native group PID");
    let member = sleeper(raw);
    assert_eq!(
        nix::unistd::getpgid(Some(pid_of(&member))),
        Ok(Pid::from_raw(raw))
    );
    leader.0.kill().expect("kill exact test-owned leader");
    assert_eq!(leader.0.wait().expect("reap leader").signal(), Some(9));
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(raw))),
        Err(Errno::ESRCH)
    );
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Ok(()),
        "live member keeps the group signallable"
    );
    (original, raw, member)
}

/// Fault point body: kill the still-unreaped (so never recycled) test-owned
/// process and await the kernel's zombie-only group state through the shared
/// fixture owner. The poll is the awaited condition; its bound is a kill switch.
fn make_group_zombie_only(raw: i32, last_live: Pid) -> Box<dyn FnMut(TestGroupSignalPoint)> {
    Box::new(move |point| {
        if point == TestGroupSignalPoint::BeforeSignal {
            kill(last_live, Signal::SIGKILL).expect("kill exact test-owned process");
            await_held_zombie_group(Pid::from_raw(raw));
        }
    })
}

/// The test reaps `held` only AFTER the shipping owner has itself observed the
/// real (leader ESRCH, group EPERM) state its own denied signal left behind.
fn reap_after_owner_observes(
    group: u32,
    mut held: TestOwnedGroupChild,
) -> (
    super::super::super::process_group::TestAmbiguousProbeReceipt,
    thread::JoinHandle<(bool, std::process::ExitStatus)>,
) {
    let (observed_tx, observed_rx) = mpsc::channel();
    let receipt = install_test_ambiguous_probe_receipt(group, observed_tx);
    let reaper = thread::spawn(move || {
        let observed = observed_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE).is_ok();
        let status = held.0.wait().expect("reap killed group process");
        (observed, status)
    });
    (receipt, reaper)
}

/// An unproven group keeps the typed signal-phase EPERM as the primary error
/// and the absence owner's own reason as context.
fn assert_denied_signal(error: &super::super::super::RuntimeError, absence_reason: &str) {
    assert!(
        matches!(
            error,
            super::super::super::RuntimeError::Lifecycle { phase, source }
                if *phase == DENIED_SIGNAL
                    && source.kind() == std::io::ErrorKind::PermissionDenied
        ),
        "an unproven group keeps the typed signal-phase EPERM: {error}"
    );
    let text = error.to_string();
    assert!(
        text.contains("Operation not permitted"),
        "the kernel's denial stays visible: {text}"
    );
    assert!(
        text.contains(absence_reason),
        "the absence owner's reason is kept as context: {text}"
    );
}

const BUDGET_SPENT: &str = "original group absence remains unproven after the shutdown budget";

#[test]
fn member_exit_between_probe_and_group_signal_retires_after_reap_without_host_fault() {
    let (original, raw, member) = reaped_leader_with_live_member();
    let _hook =
        install_test_group_signal_hook(original.pid, make_group_zombie_only(raw, pid_of(&member)));
    let (_receipt, reaper) = reap_after_owner_observes(original.pid, member);

    let result = terminate_attested_group(original);
    let (observed, status) = reaper.join().expect("member reaper joins");

    result.expect("a group made zombie-only by its last exit is retired once reaped");
    assert!(observed, "owner awaited the reap instead of guessing");
    assert_eq!(status.signal(), Some(9));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
}

#[test]
fn member_exit_between_probe_and_group_signal_never_clears_an_unreaped_group() {
    let (original, raw, mut member) = reaped_leader_with_live_member();
    let _hook =
        install_test_group_signal_hook(original.pid, make_group_zombie_only(raw, pid_of(&member)));

    let started = Instant::now();
    let error = terminate_attested_group(original)
        .expect_err("EPERM alone is never proof that the group is gone");
    let elapsed = started.elapsed();

    assert_denied_signal(&error, BUDGET_SPENT);
    assert!(
        elapsed < GUARDIAN_SHUTDOWN_DEADLINE,
        "the absence probe stays inside the existing shutdown budget: {elapsed:?}"
    );
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::EPERM),
        "the owner neither forgot nor re-signalled the unproven group"
    );
    assert_eq!(member.0.wait().expect("reap member").signal(), Some(9));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
}

// After its one denied signal, the call never signals again, even when a
// process joins the group and makes it signallable while the owner polls.
#[test]
fn denied_group_signal_is_never_repeated_when_the_group_becomes_signallable() {
    let (original, raw, mut member) = reaped_leader_with_live_member();
    let late_member = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&late_member);
    let mut zombie_only = make_group_zombie_only(raw, pid_of(&member));
    let _hook = install_test_group_signal_hook(
        original.pid,
        Box::new(move |point| {
            zombie_only(point);
            if point == TestGroupSignalPoint::AfterDeniedSignal {
                let joiner = sleeper(raw);
                assert_eq!(kill(Pid::from_raw(-raw), None), Ok(()));
                *slot.borrow_mut() = Some(joiner);
            }
        }),
    );

    let error = terminate_attested_group(original)
        .expect_err("a group signallable again after the denial is not absent");
    let mut joiner = late_member
        .borrow_mut()
        .take()
        .expect("a process joined right after the denied signal");

    assert_denied_signal(&error, BUDGET_SPENT);
    assert!(
        joiner.0.try_wait().expect("inspect joiner").is_none(),
        "the call must not signal the group a second time"
    );
    kill(pid_of(&joiner), Signal::SIGTERM).expect("signal the test-owned joiner");
    assert_eq!(
        joiner.0.wait().expect("reap joiner").signal(),
        Some(Signal::SIGTERM as i32),
        "only the test's SIGTERM ended the joiner"
    );
    assert_eq!(member.0.wait().expect("reap member").signal(), Some(9));
}

/// A live, birth-matched original leader alone in its group.
fn attested_live_leader() -> (GroupIdentity, i32, TestOwnedGroupChild) {
    let leader = sleeper(0);
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("original kernel group birth");
    let raw = i32::try_from(group).expect("positive native group PID");
    assert_eq!(kill(Pid::from_raw(-raw), None), Ok(()));
    (original, raw, leader)
}

// Birth-matched arm: the leader passed the identity check, then became a
// zombie before the one signal. The denied signal goes to the same owner.
#[test]
fn attested_leader_exit_before_group_signal_retires_after_reap_without_host_fault() {
    let (original, raw, leader) = attested_live_leader();
    let _hook =
        install_test_group_signal_hook(original.pid, make_group_zombie_only(raw, pid_of(&leader)));
    let (_receipt, reaper) = reap_after_owner_observes(original.pid, leader);

    let result = terminate_attested_group(original);
    let (observed, status) = reaper.join().expect("leader reaper joins");

    result.expect("a leader that exited into a zombie-only group is retired once reaped");
    assert!(observed, "owner awaited the reap instead of guessing");
    assert_eq!(status.signal(), Some(9));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
}

#[test]
fn attested_leader_exit_before_group_signal_never_clears_an_unreaped_group() {
    let (original, raw, mut leader) = attested_live_leader();
    let _hook =
        install_test_group_signal_hook(original.pid, make_group_zombie_only(raw, pid_of(&leader)));

    let error = terminate_attested_group(original)
        .expect_err("a held zombie leader is never proof that the group is gone");

    assert_denied_signal(&error, BUDGET_SPENT);
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::EPERM));
    assert_eq!(leader.0.wait().expect("reap leader").signal(), Some(9));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
}

// The absence owner's own reason survives a denied signal: here the live
// leader at the PID has another birth (PID reuse). The kernel's EPERM stays
// the typed primary error, and the one classifier still recognises that
// exact denial, so the Guardian's post-reap reconciliation can retire the
// group once it is really gone.
#[test]
fn unproven_denied_signal_keeps_the_eperm_and_the_absence_reason() {
    let (original, raw, mut leader) = attested_live_leader();
    let denied = || {
        super::super::super::lifecycle_error(
            DENIED_SIGNAL,
            std::io::Error::from_raw_os_error(nix::libc::EPERM),
        )
    };
    let error = reconcile_denied_signal_after_reap(
        original.altered_birth_for_test(),
        denied(),
        Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4,
    )
    .expect_err("a live leader with another birth is not absence");
    assert_denied_signal(&error, "group leader birth changed");

    leader.0.kill().expect("kill exact test-owned leader");
    assert_eq!(leader.0.wait().expect("reap leader").signal(), Some(9));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
    reconcile_denied_signal_after_reap(
        original,
        error,
        Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
    )
    .expect("the contexted denial is still the exact signal-phase EPERM");
}

/// #1077: the denial is the call's only signal. If the group becomes
/// signallable again before the next probe (here: a fresh test-owned process
/// joins the still-existing zombie-only group), the call must fail closed and
/// leave that process untouched: it is not proven to be ours. `first` is the
/// group's last live process; it exits just before the one signal. With the
/// stack's polling absence owner the call fails only at its own D/4 budget,
/// with the typed denial and the spent budget as context.
fn assert_denial_is_the_calls_only_signal(
    original: GroupIdentity,
    raw: i32,
    mut first: TestOwnedGroupChild,
) {
    let joiner: Arc<Mutex<Option<TestOwnedGroupChild>>> = Arc::default();
    let joiner_slot = Arc::clone(&joiner);
    let signals: Arc<Mutex<u32>> = Arc::default();
    let signal_count = Arc::clone(&signals);
    let mut exit_member = make_group_zombie_only(raw, pid_of(&first));
    let _hook = install_test_group_signal_hook(
        original.pid,
        Box::new(move |point| match point {
            TestGroupSignalPoint::BeforeSignal => {
                let mut count = signal_count.lock().expect("signal count");
                *count += 1;
                // Only the first, authorized signal gets the exit window. A
                // second one would hit the fresh joiner; let it, so the
                // oracle below sees the actual effect.
                if *count == 1 {
                    exit_member(point);
                }
            }
            TestGroupSignalPoint::AfterDeniedSignal => {
                let fresh = sleeper(raw);
                assert_eq!(
                    nix::unistd::getpgid(Some(pid_of(&fresh))),
                    Ok(Pid::from_raw(raw)),
                    "fresh process joined the zombie-only group"
                );
                *joiner_slot.lock().expect("joiner slot") = Some(fresh);
            }
        }),
    );

    let result = terminate_attested_group(original);
    let mut joiner = joiner
        .lock()
        .expect("joiner slot")
        .take()
        .expect("denied signal reached the after-denial fault point");
    // Sample the product outcome before the test's own cleanup.
    let joiner_alive = joiner.0.try_wait().expect("inspect joiner").is_none();
    joiner.0.kill().expect("kill exact test-owned joiner");
    assert_eq!(joiner.0.wait().expect("reap joiner").signal(), Some(9));
    assert_eq!(first.0.wait().expect("reap first victim").signal(), Some(9));

    assert_eq!(
        *signals.lock().expect("signal count"),
        1,
        "the owner prepared a second group signal in the same call"
    );
    assert!(joiner_alive, "the owner signalled the group a second time");
    let error = result.expect_err("a group that turned live after a denial is not ours to clear");
    assert_denied_signal(&error, BUDGET_SPENT);
}

/// Reaped-leader arm: the probe saw the group live, then its last member
/// exited before the one signal.
#[test]
fn denied_group_signal_is_never_followed_by_a_second_signal_in_the_same_call() {
    let (original, raw, member) = reaped_leader_with_live_member();
    assert_denial_is_the_calls_only_signal(original, raw, member);
}

/// Birth-matched arm: the live, birth-matched leader is the group's only
/// member and exits between the identity read and the one signal.
#[test]
fn denied_birth_matched_signal_is_never_followed_by_a_second_signal_in_the_same_call() {
    let (original, raw, leader) = attested_live_leader();
    assert_denial_is_the_calls_only_signal(original, raw, leader);
}

/// #1077: the identity-matched arm has the same window: the birth-matched
/// live leader (the group's last member) exits between the identity read and
/// the one signal, so `killpg` sees a zombie-only group.
#[test]
fn leader_exit_between_birth_check_and_group_signal_retires_after_reap_without_host_fault() {
    let (original, raw, leader) = attested_live_leader();
    let _hook =
        install_test_group_signal_hook(original.pid, make_group_zombie_only(raw, pid_of(&leader)));
    let (_receipt, reaper) = reap_after_owner_observes(original.pid, leader);

    let result = terminate_attested_group(original);
    let (observed, status) = reaper.join().expect("leader reaper joins");

    result.expect("a leader that became a zombie before the one signal is retired once reaped");
    assert!(observed, "owner awaited the reap instead of guessing");
    assert_eq!(status.signal(), Some(9));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
}

// The fault point's owner clears its thread-local slot when dropped, also on
// unwind, so a failing test can never leak its hook into later code on the
// same thread.
#[test]
fn group_signal_hook_owner_clears_its_slot_on_unwind() {
    let unwound = std::panic::catch_unwind(|| {
        let _hook = install_test_group_signal_hook(1, Box::new(|_| {}));
        panic!("deterministic test failure while the hook is installed");
    });
    assert!(unwound.is_err(), "the cut must unwind");
    let _again = install_test_group_signal_hook(1, Box::new(|_| {}));
}
