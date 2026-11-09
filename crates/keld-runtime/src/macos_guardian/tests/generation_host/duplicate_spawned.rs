//! One SPAWNED per KGC1 attempt. After the host's fail-safe has spent and
//! released a registered generation (its one group signal delivered), a
//! duplicate authenticated SPAWNED for the SAME attempt must not mint a second,
//! armed signal authority: not through the lease registration site, and not
//! through the denied-recovery custody site.
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::super::super::GUARDIAN_SHUTDOWN_DEADLINE;
use super::super::test_support::{NativeChildOwner, join_before};
use super::{
    CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REGISTERED, CONTROL_SPAWNED,
    GenerationControlRecord, GuardedPrimaryUpdate, RecoveryState, RuntimeError, guardian_exchange,
    lock_recovery, start_generation_host_fixture, write_generation_control,
};

fn own_group_leader() -> NativeChildOwner {
    NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("test-owned native group leader"),
    )
}

struct SpentGeneration {
    guardian: UnixStream,
    host_worker: JoinHandle<Result<(), RuntimeError>>,
    current: Arc<Mutex<Option<super::super::super::RegisteredPrimaryGeneration>>>,
    recovery: Arc<Mutex<RecoveryState>>,
}

/// Attempt 1 registers a real group; then the host fail-safe makes the
/// generation's one armed signal and releases the slot, exactly as
/// `GuardedPrimary`'s fail-safe does after a delivered signal. The KGC1
/// worker is still running with attempt 1 current and its lease held.
fn spent_and_released_generation() -> SpentGeneration {
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
    let mut first = own_group_leader();
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(first.id()),
        CONTROL_REGISTERED,
    );
    {
        let mut slot = current.lock().expect("current generation mutex");
        slot.as_mut()
            .expect("attempt 1 registered")
            .terminate()
            .expect("the one armed signal is delivered");
        *slot = None;
    }
    assert_eq!(
        first.wait().expect("reap the signalled leader").signal(),
        Some(9)
    );
    SpentGeneration {
        guardian,
        host_worker,
        current,
        recovery,
    }
}

fn assert_no_second_authority(
    mut spent: SpentGeneration,
    deny_recovery: bool,
) -> (RuntimeError, NativeChildOwner) {
    let mut second = own_group_leader();
    if deny_recovery {
        lock_recovery(&spent.recovery).deny();
    }
    let kill_switch = Instant::now() + 2 * GUARDIAN_SHUTDOWN_DEADLINE;
    write_generation_control(
        &mut spent.guardian,
        &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(second.id()),
    )
    .expect("send a duplicate authenticated SPAWNED for attempt 1");
    let error = join_before(
        spent.host_worker,
        kill_switch,
        "host worker still running after a duplicate SPAWNED",
    )
    .expect("host worker joins")
    .expect_err("a duplicate SPAWNED for a spent attempt is refused");
    assert!(
        second.try_wait().expect("inspect second leader").is_none(),
        "a duplicate SPAWNED must not signal any group: {error}"
    );
    assert!(
        spent
            .current
            .lock()
            .expect("current generation mutex")
            .is_none(),
        "no second generation authority is retained for attempt 1"
    );
    lock_recovery(&spent.recovery).deny();
    drop(spent.guardian);
    let _ = second.kill();
    (error, second)
}

// The denied-recovery custody site creates Armed authority without the lease's
// one-shot admission. Before the fix, a duplicate SPAWNED reached it and the
// host SIGKILLed the named group: a second signal for attempt 1.
#[test]
fn duplicate_spawned_after_released_generation_cannot_rearm_denied_custody() {
    let (error, _second) = assert_no_second_authority(spent_and_released_generation(), true);
    assert!(
        error.to_string().contains("invalid spawned transition"),
        "the KGC1 state machine refuses the duplicate before any custody: {error}"
    );
}

// The lease registration site cannot be reached twice: the lease's one-shot
// bootstrap admission is already consumed. The duplicate is refused before it.
#[test]
fn duplicate_spawned_after_released_generation_cannot_reach_lease_registration() {
    let (error, _second) = assert_no_second_authority(spent_and_released_generation(), false);
    assert!(
        error.to_string().contains("invalid spawned transition"),
        "the KGC1 state machine refuses the duplicate before the lease: {error}"
    );
}
