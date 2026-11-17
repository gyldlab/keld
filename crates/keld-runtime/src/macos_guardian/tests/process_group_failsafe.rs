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
    GroupIdentity, finish_group_termination, install_test_ambiguous_probe_receipt,
    observe_group_identity, reconcile_denied_signal_after_reap,
    reconcile_denied_signal_after_reap_for_test, terminate_attested_group,
    terminate_registered_group, validate_guardian_child_group,
};

use super::test_support::await_held_zombie_group;

mod cleanup_authority;
mod exiting_member_race;
mod native_descendant;
mod shared_absence;
use self::native_descendant::spawn_held_native_descendant;

// Test-owned direct Child identity ensures a failing negative oracle never
// leaves an unrelated native process alive after panic/unwind.
struct TestOwnedGroupChild(Child);

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

// Regression for the Guardian's distinct signal-before-Child-reap cut.
// The synthetic EPERM is the exact prior signal-phase error; the independent
// kernel observations below are real Darwin process-group states. A separate
// test-owned parent retains the killed group member until the product probe
// witnesses ESRCH + EPERM, so removing the post-reap retry makes this RED.
#[test]
fn guardian_signal_eperm_reconciles_only_after_native_group_absence() {
    let mut leader = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own original native group leader"),
    );
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("attest original kernel birth");
    let raw = i32::try_from(group).expect("positive native group");
    let mut member = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(raw)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own another real same-group process"),
    );
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(
            i32::try_from(member.0.id()).expect("member PID")
        ))),
        Ok(Pid::from_raw(raw)),
        "second Child must really be in the registered group"
    );
    let denied = || {
        super::super::lifecycle_error(
            "macOS guardian process-group signal",
            std::io::Error::from_raw_os_error(nix::libc::EPERM),
        )
    };
    assert!(
        reconcile_denied_signal_after_reap(
            original,
            denied(),
            std::time::Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4,
        )
        .is_err(),
        "EPERM cannot be forgiven for an executing group"
    );
    assert!(
        leader.0.try_wait().expect("inspect live leader").is_none(),
        "read-only reconciliation must not kill a live outsider"
    );

    terminate_registered_group(group).expect("SIGKILL only the test-owned group");
    assert_eq!(
        leader.0.wait().expect("reap exact original Child").signal(),
        Some(9)
    );
    assert_unreaped_native_member_remains_denied(original, raw, denied);

    let (observed_tx, observed_rx) = std::sync::mpsc::channel();
    let _receipt = install_test_ambiguous_probe_receipt(group, observed_tx);
    let reaper = thread::spawn(move || {
        observed_rx
            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("product observed actual EPERM zombie-group ambiguity");
        member.0.wait().expect("reap independent group member")
    });
    let resolved = reconcile_denied_signal_after_reap(
        original,
        denied(),
        std::time::Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4,
    );
    assert_eq!(
        reaper
            .join()
            .expect("independent member-reap worker")
            .signal(),
        Some(9)
    );
    resolved.expect("only genuine post-reap ESRCH of leader and group can reconcile EPERM");
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::ESRCH),
        "the native group was absent before any fixture rescue"
    );
    assert_other_native_failures_remain_denied(original);
}

// Keep one original native same-PGID member an unreaped zombie for the entire
// read-only retry budget. The negative oracle is independent of the later
// receipt-driven reaper, and detects an illegal early-Ok on ESRCH + EPERM.
fn assert_unreaped_native_member_remains_denied(
    original: GroupIdentity,
    raw: i32,
    denied: impl Fn() -> super::super::RuntimeError,
) {
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(raw))),
        Err(Errno::ESRCH)
    );
    // The other parent's held zombie must make the group ambiguous.
    await_held_zombie_group(Pid::from_raw(raw));
    assert!(
        reconcile_denied_signal_after_reap(original, denied(), std::time::Instant::now()).is_err(),
        "reaping only the original leader cannot prove a whole group is gone"
    );
    // A zero-remaining-budget check alone cannot catch a false-positive
    // success in the FUTURE-deadline EPERM retry branch. Keep the separate
    // native member deliberately unreaped for an existing bounded interval:
    // even after the retry budget expires, an occupied zombie-only group
    // is not absence and must never authorize a successful CLEAR.
    assert!(
        reconcile_denied_signal_after_reap(
            original,
            denied(),
            std::time::Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4,
        )
        .is_err(),
        "future-budget EPERM ambiguity cannot be reconciled before the other member is reaped"
    );
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::EPERM),
        "the original same-group member must remain an unreaped native zombie"
    );
}

// This separate error-class oracle runs *after* the real two-member native
// group above has been proved absent. Replacing the EPERM-only classifier
// with unconditional success must fail both assertions.
fn assert_other_native_failures_remain_denied(original: GroupIdentity) {
    for (phase, errno) in [
        ("macOS guardian process-group signal", nix::libc::EIO),
        (
            "macOS guardian registered group identity changed",
            nix::libc::EPERM,
        ),
    ] {
        assert!(
            reconcile_denied_signal_after_reap(
                original,
                super::super::lifecycle_error(phase, std::io::Error::from_raw_os_error(errno)),
                std::time::Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4
            )
            .is_err(),
            "unrelated native failure {phase} ({errno}) must not be reconciled"
        );
    }
}

// A later, genuine dual-ESRCH observation must not retroactively reconcile a
// signal-phase EPERM after the original revocation deadline has passed. The
// original group is truly absent, so this is not an ambiguous-zombie oracle.
#[test]
fn expired_post_reap_reconciliation_cannot_accept_late_native_absence() {
    let mut leader = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own isolated native leader"),
    );
    let group_pid = leader.0.id();
    let original = observe_group_identity(group_pid).expect("pin original native leader birth");
    let raw = i32::try_from(group_pid).expect("positive group id");
    assert_ne!(Pid::from_raw(raw), nix::unistd::getpgrp());

    terminate_attested_group(original).expect("signal only original attested group");
    assert_eq!(
        leader
            .0
            .wait()
            .expect("reap original owned leader")
            .signal(),
        Some(9)
    );
    assert_eq!(
        nix::unistd::getpgid(Some(Pid::from_raw(raw))),
        Err(Errno::ESRCH)
    );
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::ESRCH),
        "the kernel must confirm genuine original group absence before the call"
    );

    let signal_denied = || {
        super::super::lifecycle_error(
            "macOS guardian process-group signal",
            std::io::Error::from_raw_os_error(nix::libc::EPERM),
        )
    };
    let expired = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_millis(1))
        .expect("clock represents a deadline already elapsed");
    let refusal = reconcile_denied_signal_after_reap(original, signal_denied(), expired)
        .expect_err("late group absence cannot extend the original revocation deadline");
    assert!(
        refusal.to_string().contains("process-group signal"),
        "keep the original denied-signal error on expired reconciliation: {refusal}"
    );
    let mut expired_probes = 0;
    reconcile_denied_signal_after_reap_for_test(
        original,
        signal_denied(),
        expired,
        std::time::Instant::now,
        |native_group| {
            expired_probes += 1;
            (
                nix::unistd::getpgid(Some(Pid::from_raw(native_group))),
                kill(Pid::from_raw(-native_group), None),
            )
        },
    )
    .expect_err("expired budget must deny BEFORE issuing the read-only probes");
    assert_eq!(
        expired_probes, 0,
        "already expired reconciliation must not observe after its original deadline"
    );

    // The kernel observations below are still real dual ESRCH. Only the
    // monotonic-clock cut is controlled: a valid pre-probe budget expires
    // before the result is considered. Removing ONLY the post-probe guard
    // must make this negative control RED even with the pre-check intact.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let before = deadline
        .checked_sub(std::time::Duration::from_millis(1))
        .expect("pre-observation instant");
    let after = deadline + std::time::Duration::from_millis(1);
    let mut clock_reads = 0;
    let mut kernel_probes = 0;
    reconcile_denied_signal_after_reap_for_test(
        original,
        signal_denied(),
        deadline,
        || {
            clock_reads += 1;
            if clock_reads == 1 { before } else { after }
        },
        |native_group| {
            kernel_probes += 1;
            let leader = nix::unistd::getpgid(Some(Pid::from_raw(native_group)));
            let group = kill(Pid::from_raw(-native_group), None);
            assert_eq!(leader, Err(Errno::ESRCH));
            assert_eq!(group, Err(Errno::ESRCH));
            (leader, group)
        },
    )
    .expect_err("dual ESRCH after deadline cannot retroactively forgive EPERM");
    assert_eq!(kernel_probes, 1, "real native probes must execute");
    assert_eq!(clock_reads, 2, "post-observation deadline must be checked");

    reconcile_denied_signal_after_reap(
        original,
        signal_denied(),
        std::time::Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
    )
    .expect("the same real absence is acceptable inside a fresh, unexpired budget");
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
    // Darwin must report a temporarily zombie-only group as ambiguous.
    await_held_zombie_group(Pid::from_raw(group_raw));
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
    await_held_zombie_group(Pid::from_raw(raw));

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
