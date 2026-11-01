//! A real host KGC1 CLEAR whose single signal is delivered but whose absence
//! proof fails must persist the spent signal authority in the shared
//! generation slot. A later fail-safe for that retained generation may only
//! probe; it must never SIGKILL a process that joined the group afterwards.
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::unistd::{Pid, getpgid};

use super::super::super::super::GUARDIAN_SHUTDOWN_DEADLINE;
use super::super::super::super::guarded_primary::NativeSignalAuthority;
use super::super::super::test_support::join_before;
use super::{
    CONTROL_CLEAR, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REGISTERED, CONTROL_REVOKE,
    CONTROL_REVOKED, CONTROL_SPAWNED, GenerationControlRecord, GuardedPrimaryUpdate,
    NativeChildOwner, RecoveryState, RevocationCause, assert_native_clear_absence_timeout,
    await_held_zombie_group, guardian_exchange, lock_recovery, observe_group_identity,
    start_generation_host_fixture, write_generation_control,
};

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

#[test]
fn failed_host_clear_persists_spent_authority_and_later_failsafe_never_resignals() {
    // The leader is a direct child of this test process, which the fixture
    // treats as the Guardian. Keeping it unreaped holds a zombie-only group,
    // so the host's CLEAR signal is delivered but its absence proof fails.
    let mut leader = owned_sleep(0);
    let group_pid = leader.id();
    let raw = i32::try_from(group_pid).expect("group PID fits c_int");
    let group = Pid::from_raw(raw);
    let original_birth = observe_group_identity(group_pid).expect("original leader birth");

    let (host, mut guardian) = UnixStream::pair().expect("private native KGC1 control pair");
    guardian
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bounded KGC1 setup");
    let (updates_tx, _updates_rx) = mpsc::channel::<GuardedPrimaryUpdate>();
    let current = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).arm();
    let host_worker = start_generation_host_fixture(
        host,
        updates_tx,
        Arc::clone(&current),
        Arc::clone(&recovery),
    );
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_PREPARE, 1),
        CONTROL_PREPARED,
    );
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(group_pid),
        CONTROL_REGISTERED,
    );
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_REVOKE, 1)
            .with_cause(RevocationCause::AdmissionFailed),
        CONTROL_REVOKED,
    );
    // The host's CLEAR budget is one shutdown deadline from its read of
    // CLEAR; twice that from the write is only a kill switch. A host that
    // wrongly CLEARs keeps waiting for the next KGC1 record, which must fail
    // this test rather than hang it.
    let clear_kill_switch = Instant::now() + 2 * GUARDIAN_SHUTDOWN_DEADLINE;
    write_generation_control(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_CLEAR, 1).with_pid(group_pid),
    )
    .expect("send authenticated CLEAR");
    await_held_zombie_group(group);
    let host_error = join_before(
        host_worker,
        clear_kill_switch,
        "host worker still running after its CLEAR budget: a held zombie group was CLEARED",
    )
    .expect("host worker terminates after its CLEAR budget")
    .expect_err("held zombie group cannot be CLEARED");
    assert_native_clear_absence_timeout(host_error);

    // The real CLEAR path wrote the spent authority back into the slot.
    let retained = (*current.lock().expect("current generation mutex"))
        .expect("failed CLEAR keeps the original birth-attested owner");
    assert_eq!(retained.group_identity, Some(original_birth));
    assert!(
        matches!(
            retained.native_signal,
            NativeSignalAuthority::Delivered { .. }
        ),
        "the delivered CLEAR signal spent the authority: {:?}",
        retained.native_signal
    );

    // A process joins the retained group AFTER the single signal; then the
    // leader is reaped, so the group number answers but has no leader.
    let mut joiner = owned_sleep(raw);
    leader.wait().expect("reap the killed leader");
    assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw), None), Ok(()));

    // Every later host fail-safe goes through RegisteredPrimaryGeneration::
    // terminate on the shared slot.
    let later = current
        .lock()
        .expect("current generation mutex")
        .as_mut()
        .expect("retained generation")
        .terminate();
    assert!(later.is_err(), "the group is not proven absent: {later:?}");

    // SIGKILL delivery is asynchronous: terminate the unreaped joiner with
    // SIGTERM ourselves and read which signal actually killed it.
    let joiner_raw = i32::try_from(joiner.id()).expect("joiner PID fits c_int");
    kill(Pid::from_raw(joiner_raw), Signal::SIGTERM).expect("signal the unreaped joiner");
    assert_eq!(
        joiner.wait().expect("reap the test-owned joiner").signal(),
        Some(Signal::SIGTERM as i32),
        "a later fail-safe after a failed host CLEAR must not killpg the group again"
    );
    lock_recovery(&recovery).deny();
    drop(guardian);
}
