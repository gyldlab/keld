//! GH-824 Host hang trigger: the last live member of an attested group
//! finishes exiting between the kernel check that authorized the one group
//! SIGKILL and that signal.
//!
//! XNU `killpg1` counts an exiting member as found but skips `SZOMB` members
//! and returns EPERM when it found none. The group is then zombie-only: the
//! same ambiguous state the read-only absence probe already handles. These
//! native tests put the exit exactly in that window through test-only fault
//! points, instead of relying on scheduler timing.
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Instant;

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use super::super::super::process_group::{
    GroupIdentity, TestGroupSignalPoint, install_test_ambiguous_probe_receipt,
    install_test_group_signal_hook, observe_group_identity, terminate_attested_group,
};
use super::super::super::{GUARDIAN_SHUTDOWN_DEADLINE, RuntimeError};
use super::{TestOwnedGroupChild, await_zombie_only_group};

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

/// The shared contract with the Guardian's post-reap reconciliation (and the
/// #945/#947 stack): an unproven group after a denied signal returns the
/// one signal's typed signal-phase EPERM, never a collapsed or other error.
fn assert_typed_signal_phase_eperm(error: &RuntimeError, context: &str) {
    assert!(
        matches!(
            error,
            RuntimeError::Lifecycle {
                phase: "macOS guardian process-group signal",
                source,
            } if source.raw_os_error() == Some(nix::libc::EPERM)
        ),
        "{context}: {error:?}"
    );
}

/// Fault point body for [`TestGroupSignalPoint::BeforeSignal`]: kill the
/// still-unreaped (so never recycled) test-owned process and await the
/// kernel's zombie-only group state.
fn exit_last_member_before_signal(
    raw: i32,
    member: Pid,
) -> impl FnMut(TestGroupSignalPoint) + 'static {
    move |point| {
        if point == TestGroupSignalPoint::BeforeSignal {
            kill(member, Signal::SIGKILL).expect("kill exact test-owned member");
            await_zombie_only_group(raw);
        }
    }
}

#[test]
fn member_exit_between_probe_and_group_signal_retires_after_reap_without_host_fault() {
    let (original, raw, mut member) = reaped_leader_with_live_member();
    let _hook = install_test_group_signal_hook(
        original.pid,
        Box::new(exit_last_member_before_signal(raw, pid_of(&member))),
    );
    // The member's parent (this test) reaps it only AFTER the shipping owner
    // has itself observed the real (leader ESRCH, group EPERM) state that its
    // own denied signal left behind.
    let (observed_tx, observed_rx) = mpsc::channel();
    let _receipt = install_test_ambiguous_probe_receipt(original.pid, observed_tx);
    let reaper = thread::spawn(move || {
        let observed = observed_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE).is_ok();
        let status = member.0.wait().expect("reap killed group member");
        (observed, status)
    });

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
    let _hook = install_test_group_signal_hook(
        original.pid,
        Box::new(exit_last_member_before_signal(raw, pid_of(&member))),
    );

    let started = Instant::now();
    let error = terminate_attested_group(original)
        .expect_err("EPERM alone is never proof that the group is gone");
    let elapsed = started.elapsed();

    assert_typed_signal_phase_eperm(
        &error,
        "a persistent zombie-only group keeps the one signal's typed EPERM",
    );
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

/// The denial is the call's only signal. If the group becomes signallable
/// again before the next probe (here: a fresh test-owned process joins the
/// still-existing zombie-only group), the call must fail closed and leave
/// that process untouched: it is not proven to be ours. `first` is the
/// group's last live process; it exits just before the one signal.
fn assert_denial_is_the_calls_only_signal(
    original: GroupIdentity,
    raw: i32,
    mut first: TestOwnedGroupChild,
) {
    let joiner: Arc<Mutex<Option<TestOwnedGroupChild>>> = Arc::default();
    let joiner_slot = Arc::clone(&joiner);
    let signals: Arc<Mutex<u32>> = Arc::default();
    let signal_count = Arc::clone(&signals);
    let mut exit_member = exit_last_member_before_signal(raw, pid_of(&first));
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
    assert_typed_signal_phase_eperm(
        &error,
        "a group that turned live after the denial fails closed with the denial",
    );
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
    let leader = sleeper(0);
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("original kernel group birth");
    let raw = i32::try_from(group).expect("positive native group PID");
    assert_denial_is_the_calls_only_signal(original, raw, leader);
}

/// The identity-matched arm has the same window: the birth-matched live
/// leader (the group's last member) exits between the identity read and the
/// one signal, so `killpg` sees a zombie-only group.
#[test]
fn leader_exit_between_birth_check_and_group_signal_retires_after_reap_without_host_fault() {
    let mut leader = sleeper(0);
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("original kernel group birth");
    let raw = i32::try_from(group).expect("positive native group PID");
    let _hook = install_test_group_signal_hook(
        group,
        Box::new(exit_last_member_before_signal(raw, pid_of(&leader))),
    );
    let (observed_tx, observed_rx) = mpsc::channel();
    let _receipt = install_test_ambiguous_probe_receipt(group, observed_tx);
    let reaper = thread::spawn(move || {
        let observed = observed_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE).is_ok();
        let status = leader.0.wait().expect("reap killed original leader");
        (observed, status)
    });

    let result = terminate_attested_group(original);
    let (observed, status) = reaper.join().expect("leader reaper joins");

    result.expect("a leader that became a zombie before the one signal is retired once reaped");
    assert!(observed, "owner awaited the reap instead of guessing");
    assert_eq!(status.signal(), Some(9));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
}
