//! Host CLEAR, the Guardian's post-reap EPERM reconciliation and the host
//! fallback cleanup must share one read-only group-absence rule: the original
//! leader PID and the whole group both report ESRCH inside the caller's
//! original budget. Each oracle below observes real Darwin process state.
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid, setpgid};

use super::super::super::process_group::{
    observe_group_identity, reconcile_denied_signal_after_reap, require_attested_group_absence,
    require_attested_group_absence_for_test, terminate_attested_group,
};
use super::TestOwnedGroupChild;

const ESCAPE_TARGET_ENV: &str = "KELD_TEST_ESCAPED_LEADER_TARGET_GROUP";
const ESCAPE_ROLE_TEST: &str =
    "macos_guardian::tests::process_group_failsafe::shared_absence::escaped_leader_process_role";

/// Re-executed libtest role: the original group leader joins another
/// test-owned group only after the controller has pinned its kernel birth.
#[test]
fn escaped_leader_process_role() {
    let Some(target) = std::env::var_os(ESCAPE_TARGET_ENV) else {
        return;
    };
    let target = target
        .to_str()
        .and_then(|text| text.parse::<i32>().ok())
        .expect("numeric target process group");
    let mut gate = [0_u8; 2];
    std::io::stdin()
        .read_exact(&mut gate)
        .expect("controller pins original birth before the move");
    setpgid(Pid::from_raw(0), Pid::from_raw(target)).expect("leave the original process group");
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "KELD_ESCAPED_LEADER_MOVED").expect("report the completed move");
    stdout.flush().expect("flush move receipt");
    drop(stdout);
    let mut rest = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut rest);
}

fn denied_group_signal() -> super::super::super::RuntimeError {
    super::super::super::lifecycle_error(
        "macOS guardian process-group signal",
        std::io::Error::from_raw_os_error(nix::libc::EPERM),
    )
}

// Genuine dual ESRCH observed after the original budget cannot authorize
// either CLEAR or a forgiven signal-phase EPERM: one deadline rule, one owner.
#[test]
fn post_reap_reconciliation_shares_the_clear_absence_deadline() {
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
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("pin original native leader birth");
    let raw = i32::try_from(group).expect("positive native group");
    terminate_attested_group(original).expect("signal only the original attested group");
    leader.0.wait().expect("reap exact owned leader");
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));

    let expired = Instant::now()
        .checked_sub(Duration::from_millis(1))
        .expect("representable elapsed deadline");
    require_attested_group_absence(original, expired)
        .expect_err("CLEAR absence cannot be proven after its original budget");
    let refusal = reconcile_denied_signal_after_reap(original, denied_group_signal(), expired)
        .expect_err("post-reap EPERM reconciliation uses the same original budget");
    assert!(
        refusal.to_string().contains("process-group signal"),
        "the original signal failure stays visible: {refusal}"
    );
    let fresh = Instant::now() + Duration::from_secs(1);
    require_attested_group_absence(original, fresh).expect("same real absence, live budget");
    reconcile_denied_signal_after_reap(original, denied_group_signal(), fresh)
        .expect("same real absence reconciles EPERM inside a live budget");
}

// Each deadline guard of the one absence owner refuses on its own. The kernel
// reads are real dual ESRCH; only the monotonic clock is injected, so removing
// EITHER the before-read guard OR the after-read guard alone turns this red.
#[test]
fn each_absence_deadline_guard_refuses_without_the_other() {
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
    let group = leader.0.id();
    let original = observe_group_identity(group).expect("pin original native leader birth");
    let raw = i32::try_from(group).expect("positive native group");
    terminate_attested_group(original).expect("signal only the original attested group");
    leader.0.wait().expect("reap exact owned leader");
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));

    let deadline = Instant::now() + Duration::from_secs(1);
    let before = deadline
        .checked_sub(Duration::from_millis(1))
        .expect("representable pre-deadline instant");
    let after = deadline + Duration::from_millis(1);
    let real_probe = |native: i32| {
        let reads = (
            getpgid(Some(Pid::from_raw(native))),
            kill(Pid::from_raw(-native), None),
        );
        assert_eq!(reads, (Err(Errno::ESRCH), Err(Errno::ESRCH)));
        reads
    };

    // Before-read guard alone: an already expired budget issues no kernel read.
    let mut expired_probes = 0;
    require_attested_group_absence_for_test(
        original,
        deadline,
        || after,
        |native| {
            expired_probes += 1;
            real_probe(native)
        },
    )
    .expect_err("an expired budget cannot prove absence");
    assert_eq!(
        expired_probes, 0,
        "the before-read guard must refuse before any kernel observation"
    );

    // After-read guard alone: a budget live at the read but expired at the
    // decision cannot turn that real dual ESRCH into success.
    let mut clock_reads = 0;
    let mut late_probes = 0;
    require_attested_group_absence_for_test(
        original,
        deadline,
        || {
            clock_reads += 1;
            if clock_reads == 1 { before } else { after }
        },
        |native| {
            late_probes += 1;
            real_probe(native)
        },
    )
    .expect_err("dual ESRCH decided after the budget is not absence");
    assert_eq!(late_probes, 1, "the real kernel reads executed once");
    assert_eq!(
        clock_reads, 2,
        "the decision re-reads the clock after the probe"
    );

    // Positive control: the same real absence inside the budget succeeds.
    require_attested_group_absence_for_test(original, deadline, || before, real_probe)
        .expect("the same real absence inside a live budget");
}

// A live original leader that left its PGID leaves that numeric group empty
// (`kill(-PGID, 0) == ESRCH`) while `getpgid(leader)` still succeeds. That is
// not dual-ESRCH absence: the fallback must neither signal anything nor report
// that the attested group was cleaned up while the original process lives.
#[test]
fn escaped_original_leader_is_not_reported_as_group_cleanup() {
    let mut holder = TestOwnedGroupChild(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own unrelated destination group"),
    );
    let holder_group = i32::try_from(holder.0.id()).expect("positive holder group");
    let mut leader = TestOwnedGroupChild(
        Command::new(std::env::current_exe().expect("native libtest binary"))
            .args(["--exact", ESCAPE_ROLE_TEST, "--nocapture"])
            .env(ESCAPE_TARGET_ENV, holder_group.to_string())
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("own original native group leader"),
    );
    let group = leader.0.id();
    let raw = i32::try_from(group).expect("positive original group");
    let original = observe_group_identity(group).expect("pin original leader birth before move");
    leader
        .0
        .stdin
        .as_mut()
        .expect("move gate")
        .write_all(b"M\n")
        .expect("permit move after birth is pinned");
    let mut receipt = BufReader::new(leader.0.stdout.take().expect("move receipt"));
    let mut line = String::new();
    loop {
        line.clear();
        let read = receipt.read_line(&mut line).expect("read move receipt");
        assert_ne!(read, 0, "escaped-leader role exited before moving");
        if line.trim_end() == "KELD_ESCAPED_LEADER_MOVED" {
            break;
        }
    }
    assert_eq!(
        getpgid(Some(Pid::from_raw(raw))),
        Ok(Pid::from_raw(holder_group)),
        "the original leader is live in another group"
    );
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));

    let cleanup = terminate_attested_group(original);
    assert!(
        leader
            .0
            .try_wait()
            .expect("inspect original leader")
            .is_none(),
        "fallback must not signal the escaped original leader"
    );
    assert!(
        holder.0.try_wait().expect("inspect holder").is_none(),
        "fallback must not signal the unrelated destination group"
    );
    let error = cleanup.expect_err("an empty PGID with a live original leader is not cleanup");
    assert!(
        error
            .to_string()
            .contains("registered group identity unavailable"),
        "{error}"
    );
}
