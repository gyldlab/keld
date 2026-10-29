// Group-leader disappearance does not establish group disappearance. An
// unrelated process in the same private group can outlive the original leader.
use std::io::Write as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, Stdio};
use std::thread;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;

use super::super::GUARDIAN_SHUTDOWN_DEADLINE;
use super::super::process_group::{
    finish_group_termination, install_test_ambiguous_probe_receipt, observe_group_identity,
    terminate_attested_group, terminate_registered_group, validate_guardian_child_group,
};

mod cleanup_authority;
mod exiting_member_race;
mod native_descendant;
use self::native_descendant::spawn_held_native_descendant;

// Test-owned direct Child identity ensures a failing negative oracle never
// leaves an unrelated native process alive after panic/unwind.
struct TestOwnedGroupChild(Child);

/// Await the kernel's zombie-only state of a group whose test-owned members
/// were killed with `SIGKILL` but not reaped: `kill(-g, 0)` turns from Ok (a member is
/// still exiting) to EPERM (every member is `SZOMB`). Polling is the awaited
/// condition; the existing shutdown deadline is only a kill switch.
fn await_zombie_only_group(raw: i32) {
    let deadline = std::time::Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        match kill(Pid::from_raw(-raw), None) {
            Err(Errno::EPERM) => return,
            Ok(()) => {}
            other => panic!("group left the zombie-only path: {other:?}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "killed members never became a zombie-only group"
        );
        thread::yield_now();
    }
}

impl Drop for TestOwnedGroupChild {
    fn drop(&mut self) {
        // A test may have reaped its owned Child already. Never issue a late
        // numeric-PID kill after wait() could have released that identity.
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn foreign_group_is_not_a_guardian_owned_direct_child() {
    let mut group = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn private, test-owned foreign group");
    let group_pid = group.id();
    let actual_parent = std::process::id();
    let own_child = validate_guardian_child_group(group_pid, actual_parent);
    let forged_parent = validate_guardian_child_group(group_pid, u32::MAX);
    // Reap the test-owned process before any assertion, including red cases.
    group.kill().expect("kill exact test-owned child");
    group.wait().expect("reap exact test-owned child");
    own_child.expect("kernel identifies this actual direct child");
    let error = forged_parent.expect_err("an unrelated Guardian must not own this group");
    assert!(error.to_string().contains("direct child"), "{error}");
}

#[test]
fn reaped_leader_does_not_mask_live_process_group_when_signal_is_denied() {
    let (mut rescue, group_pid, _descendant) = spawn_held_native_descendant();
    let birth = rescue
        .original_birth
        .expect("original leader birth is retained");
    let leader = &mut rescue
        .leader
        .as_mut()
        .expect("owned original group leader")
        .0;
    leader
        .stdin
        .as_mut()
        .expect("held shell exit gate")
        .write_all(b"E\n")
        .expect("release original shell");
    leader.stdin.take();
    let raw_group = i32::try_from(group_pid).expect("group ID fits signed PID");
    assert!(leader.wait().expect("reap shell group leader").success());

    // Real macOS group state refutes the old getpgid(leader) heuristic.
    let leader_gone = nix::unistd::getpgid(Some(Pid::from_raw(raw_group)));
    let group_probe = kill(Pid::from_raw(-raw_group), None);
    let outcome = finish_group_termination(Err(Errno::EPERM), || group_probe);
    // Retain original birth for rescue after the leader has been reaped.
    // The surrounding owner runs it on any earlier panic/unwind as well.
    let cleanup = terminate_attested_group(birth);
    assert_eq!(leader_gone, Err(Errno::ESRCH));
    assert_eq!(group_probe, Ok(()), "descendant still owns group");
    assert!(
        outcome.is_err(),
        "EPERM cannot be suppressed for a live group"
    );
    cleanup.expect("reap isolated test process group");
}

#[test]
fn denied_group_signal_is_only_ignorable_after_group_absence_proof() {
    assert!(finish_group_termination(Err(Errno::EPERM), || Err(Errno::ESRCH)).is_ok());
    assert!(finish_group_termination(Err(Errno::EPERM), || Err(Errno::EPERM)).is_err());
    assert!(finish_group_termination(Err(Errno::EPERM), || Ok(())).is_err());
    assert!(finish_group_termination(Err(Errno::EIO), || Err(Errno::ESRCH)).is_err());
}

#[test]
fn stale_group_birth_cannot_signal_a_live_unrelated_native_group() {
    let mut outsider = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("test owns real live group"),
    );
    let pid = outsider.0.id();
    let birth = observe_group_identity(pid).expect("kernel birth of test-owned group");
    let changed_birth = birth.altered_birth_for_test();
    let refusal = terminate_attested_group(changed_birth)
        .expect_err("a mismatched registered birth cannot authorize SIGKILL");
    let alive = outsider
        .0
        .try_wait()
        .expect("inspect exact live outsider")
        .is_none();
    // Product outcome is sampled BEFORE the test's rescue cleanup.
    terminate_attested_group(birth).expect("terminate actual attested native group");
    let status = outsider.0.wait().expect("reap exact test-owned outsider");
    assert!(
        refusal.to_string().contains("identity changed"),
        "missing native birth-mismatch denial: {refusal}"
    );
    assert!(
        alive,
        "forged birth identity killed an unrelated live group"
    );
    assert_eq!(status.signal(), Some(9));
}

#[test]
fn reaped_registered_group_leader_does_not_strand_live_descendant() {
    let (mut rescue, group, child) = spawn_held_native_descendant();
    let birth = rescue
        .original_birth
        .expect("original native group birth retained");
    let group_pid = Pid::from_raw(i32::try_from(group).expect("valid PGID"));
    assert_eq!(
        nix::unistd::getpgid(Some(child)),
        Ok(group_pid),
        "known live descendant is in original group"
    );
    // Now intentionally let the original leader exit and be reaped. Its
    // descendant stays in the enrolled group to exercise the actual fallback.
    let leader = &mut rescue.leader.as_mut().expect("owned native group leader").0;
    leader
        .stdin
        .as_mut()
        .expect("held exit gate")
        .write_all(b"E\n")
        .expect("release original shell leader");
    leader.stdin.take();
    assert!(leader.wait().expect("reap departed group leader").success());
    assert_eq!(
        nix::unistd::getpgid(Some(group_pid)),
        Err(Errno::ESRCH),
        "kernel no longer exposes original leader birth"
    );
    terminate_attested_group(birth)
        .expect("original registered group must still reap surviving descendants");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if kill(child, None) == Err(Errno::ESRCH) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "host fallback stranded original same-group descendant after leader reap"
        );
        std::thread::yield_now();
    }
    // Product disappearance was proven before attested rescue and Child Drop.
}

#[test]
fn reaped_leader_with_temporarily_unreaped_dead_member_clears_without_false_host_fault() {
    let mut leader = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("test-owned original group leader"),
    );
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("original kernel group birth");
    let group_raw = i32::try_from(group).expect("group fits positive PID");
    let mut member = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(group_raw)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("another test-owned member of the exact group"),
    );
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(
            i32::try_from(member.0.id()).expect("member PID")
        ))),
        Ok(Pid::from_raw(group_raw)),
        "the un-reaped member belongs to the enrolled group"
    );
    terminate_registered_group(group).expect("signal only the test-owned group");
    assert_eq!(
        leader.0.wait().expect("reap original leader").signal(),
        Some(9)
    );
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(group_raw))),
        Err(Errno::ESRCH),
        "original leader birth is gone before host CLEAR"
    );
    // Darwin must report a temporarily zombie-only group as ambiguous. A
    // SIGKILLed member is still *exiting* (found, so Ok) for a moment before
    // it becomes SZOMB; await that kernel state rather than racing it.
    await_zombie_only_group(group_raw);
    // A separate actual OS Child owner holds the killed group member until
    // the *shipping* fallback has observed its own first ESRCH + EPERM pair.
    // This exact receipt, rather than a sleep or scheduler hope, prevents a
    // mutant without the retry from accidentally passing after an early reap.
    let (observed_tx, observed_rx) = std::sync::mpsc::channel();
    let _probe_receipt = install_test_ambiguous_probe_receipt(group, observed_tx);
    let reaper = thread::spawn(move || {
        observed_rx
            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("host observed real ambiguous zombie-only group before reap");
        member.0.wait().expect("reap held killed group member")
    });
    let result = terminate_attested_group(original);
    assert_eq!(
        reaper.join().expect("group-member reaper joins").signal(),
        Some(9)
    );
    result.expect("originally enrolled group becomes provably absent after OS reap");
    assert_eq!(kill(Pid::from_raw(-group_raw), None), Err(Errno::ESRCH));
}

#[test]
fn reaped_leader_with_persistently_unreaped_member_must_not_claim_clear() {
    let mut leader = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("test-owned originally enrolled group leader"),
    );
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("original group kernel birth");
    let raw = i32::try_from(group).expect("positive native group PID");
    let mut member = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(raw)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("test-owned second member"),
    );
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(
            i32::try_from(member.0.id()).expect("native member PID")
        ))),
        Ok(Pid::from_raw(raw))
    );
    terminate_registered_group(group).expect("signal original real group");
    assert_eq!(
        leader.0.wait().expect("reap original leader").signal(),
        Some(9)
    );
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(raw))),
        Err(Errno::ESRCH)
    );
    await_zombie_only_group(raw);

    let error = terminate_attested_group(original)
        .expect_err("unreaped zombie-only group is not a proven-empty native group");
    assert!(
        error
            .to_string()
            .contains("registered group identity unavailable"),
        "permanent ambiguous group must remain a fatal owner-state fault: {error}"
    );
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::EPERM),
        "the host did not signal or forget an unproven group"
    );
    assert_eq!(
        member
            .0
            .wait()
            .expect("reap final native group member")
            .signal(),
        Some(9)
    );
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
}
