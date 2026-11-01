//! One native group signal per generation. A *delivered* signal that does not
//! release the generation also spends the authority: a second group-wide kill
//! could only reach processes that joined later or a recycled PGID. Custody
//! is kept (fail closed) until absence is proven; nothing is re-signalled.
use std::os::unix::process::CommandExt as _;
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use super::super::super::super::process_group::{
    TestGroupSignalPoint, install_test_group_signal_hook, require_attested_group_absence,
};
use super::super::super::super::{
    GuardedPrimary, RecoveryState, RegisteredPrimaryGeneration, lock_generation,
};
use super::super::super::test_support::{
    NativeChildOwner, await_held_zombie_group, test_group_identity,
};
use super::{REUSED_LEADER, owned_sleep, primary_with_dead_guardian};

/// A process that joins the original group only AFTER the first signal.
fn late_joiner(raw: i32) -> NativeChildOwner {
    owned_sleep(raw)
}

/// SIGKILL delivery is asynchronous; terminate the still-owned, unreaped
/// joiner with SIGTERM ourselves and read which signal actually killed it.
fn assert_not_resignalled(mut joiner: NativeChildOwner, context: &str) {
    let raw = i32::try_from(joiner.id()).expect("joiner PID fits c_int");
    kill(Pid::from_raw(raw), Signal::SIGTERM).expect("signal the unreaped test joiner");
    assert_eq!(
        joiner.wait().expect("reap the test-owned joiner").signal(),
        Some(Signal::SIGTERM as i32),
        "{context}: a delivered signal must not be followed by a second killpg"
    );
}

#[test]
fn failed_clear_absence_after_delivered_signal_never_resignals_the_group() {
    // Reviewer probe (PR #947 comment 6292499288), replayed step by step.
    let mut leader = owned_sleep(0);
    let raw = i32::try_from(leader.id()).expect("group PID fits c_int");
    let mut generation =
        RegisteredPrimaryGeneration::authenticated(1, test_group_identity(leader.id()));
    // CLEAR step 1: the single armed, birth-attested signal is delivered.
    generation
        .signal_if_armed()
        .expect("armed CLEAR signal is delivered");
    let joiner = late_joiner(raw);
    leader.wait().expect("reap the killed leader");
    // CLEAR step 2: the absence proof fails because the joiner is live.
    let identity = generation.group_identity.expect("attested birth");
    assert!(
        require_attested_group_absence(identity, Instant::now() + Duration::from_millis(200))
            .is_err(),
        "a live joiner keeps the group present"
    );
    // A later host fail-safe for the same retained generation.
    let later = generation.terminate();
    assert!(later.is_err(), "the group is not proven absent: {later:?}");
    assert_not_resignalled(joiner, "fail-safe after failed CLEAR");
}

// GH-824 trigger: the leader passes its birth check, then exits into a
// zombie-only group before the one signal, so killpg is denied (EPERM). The
// denied attempt is classified by the one absence owner and still SPENDS the
// generation's authority: a later fail-safe only probes and never signals a
// process that joined the group afterwards.
#[test]
fn denied_attested_signal_spends_the_generation_authority() {
    use super::super::super::super::guarded_primary::NativeSignalAuthority;
    let mut leader = owned_sleep(0);
    let raw = i32::try_from(leader.id()).expect("group PID fits c_int");
    let leader_pid = Pid::from_raw(raw);
    let mut generation =
        RegisteredPrimaryGeneration::authenticated(1, test_group_identity(leader.id()));
    let _hook = install_test_group_signal_hook(
        leader.id(),
        Box::new(move |point| {
            if point == TestGroupSignalPoint::BeforeSignal {
                kill(leader_pid, Signal::SIGKILL).expect("kill the exact test-owned leader");
                await_held_zombie_group(leader_pid);
            }
        }),
    );
    let denied = generation
        .signal_if_armed()
        .expect_err("a held zombie-only group is not proven absent");
    assert!(
        denied.to_string().contains("process-group signal")
            && denied.to_string().contains("absence remains unproven"),
        "the denied signal keeps its typed signal-phase failure and the absence reason: {denied}"
    );
    assert!(
        matches!(
            generation.native_signal,
            NativeSignalAuthority::ProbeOnly { .. }
        ),
        "the denied attempt spent the authority: {:?}",
        generation.native_signal
    );
    let joiner = late_joiner(raw);
    leader.wait().expect("reap the killed leader");
    let later = generation.terminate();
    assert!(
        later.is_err(),
        "the joiner keeps the group present: {later:?}"
    );
    assert_not_resignalled(joiner, "fail-safe after a denied signal");
}

#[test]
fn successful_fail_current_generation_spends_authority_before_clear() {
    let mut leader = owned_sleep(0);
    let raw = i32::try_from(leader.id()).expect("group PID fits c_int");
    let registered =
        RegisteredPrimaryGeneration::authenticated(1, test_group_identity(leader.id()));
    let mut primary = primary_with_dead_guardian(registered, 0, true);
    primary
        .fail_current_generation(1)
        .expect("one attested signal is delivered");
    // A repeated Core failure report for the same attempt is idempotent:
    // the signal was already delivered, so it neither re-signals nor waits.
    let started = Instant::now();
    primary
        .fail_current_generation(1)
        .expect("repeat report after a delivered signal");
    assert!(started.elapsed() < Duration::from_millis(250));
    let joiner = late_joiner(raw);
    leader
        .wait()
        .expect("Guardian-side reap of the killed leader");
    // KGC1 CLEAR step 1 for the same retained generation.
    let clear_signal = lock_generation(&primary.current_generation)
        .as_mut()
        .expect("generation stays registered until CLEAR")
        .signal_if_armed();
    assert!(clear_signal.is_ok(), "{clear_signal:?}");
    assert_not_resignalled(joiner, "CLEAR after fail_current_generation");
    // The group is now absent; the retained record still releases cleanly.
    primary.liveness_writer.take();
    lock_generation(&primary.current_generation)
        .as_mut()
        .expect("retained generation")
        .terminate()
        .expect("read-only absence proof releases an empty group");
}

#[test]
fn deadline_path_guardian_exit_reports_the_earlier_failure_without_a_second_call() {
    // Guardian stand-in ignores liveness EOF, so shutdown takes the
    // deadline branch: one native fail-safe, then kill/wait of the Guardian.
    let (mut leader_owner, registered) = {
        let leader = owned_sleep(0);
        let identity = test_group_identity(leader.id()).altered_birth_for_test();
        (
            leader,
            RegisteredPrimaryGeneration::authenticated(1, identity),
        )
    };
    let mut guardian = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("own unresponsive guardian stand-in");
    let writer = guardian.stdin.take().expect("host liveness writer");
    let ack = guardian.stdout.take().expect("host acknowledgment reader");
    let (_updates_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::new(Mutex::new(Some(registered))),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: true,
        shutdown_failed: false,
        orderly_acknowledged: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    };
    let error = primary
        .shutdown()
        .expect_err("unresponsive Guardian and refused group signal")
        .to_string();
    assert_eq!(
        error.matches(REUSED_LEADER).count(),
        1,
        "the one native attempt is reported once: {error}"
    );
    assert!(
        error.contains("failed earlier in this shutdown"),
        "the GuardianExited report must not re-run the fail-safe: {error}"
    );
    assert!(
        leader_owner
            .try_wait()
            .expect("inspect test-owned leader")
            .is_none(),
        "a mismatched birth never authorizes a signal"
    );
}

#[test]
fn clear_honours_a_failed_attempts_spent_budget_but_not_a_delivered_one() {
    use super::super::super::super::guarded_primary::NativeSignalAuthority;
    let leader = owned_sleep(0);
    let mut generation =
        RegisteredPrimaryGeneration::authenticated(1, test_group_identity(leader.id()));
    let own = Instant::now() + Duration::from_secs(5);
    assert_eq!(generation.clear_absence_deadline(own), own, "armed");
    let spent = Instant::now();
    generation.native_signal = NativeSignalAuthority::Delivered {
        reap_deadline: spent,
    };
    assert_eq!(
        generation.clear_absence_deadline(own),
        own,
        "normal CLEAR after a delivered signal keeps its own window"
    );
    generation.native_signal = NativeSignalAuthority::ProbeOnly {
        reap_deadline: spent,
    };
    assert!(
        generation.clear_absence_deadline(own) < Instant::now() + Duration::from_millis(250),
        "after a failed attempt CLEAR gets no fresh window"
    );
}
