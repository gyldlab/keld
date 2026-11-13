//! A previously revoked KGC1 generation never gains native OS signal
//! authority from a later, syntactically valid SPAWNED under recovery denial.
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt as _;
use std::process::Child;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use crate::macos_guardian::process_group::{
    GroupIdentity, observe_group_identity, terminate_attested_group,
};

use super::{
    CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REVOKE, CONTROL_REVOKED, CONTROL_SPAWNED,
    GenerationControlRecord, GuardedPrimaryUpdate, RecoveryState, RevocationCause,
    guardian_exchange, lock_recovery, long_running_command, start_generation_host_fixture,
    write_generation_control,
};

struct OwnedNativeGroup {
    child: Child,
    identity: GroupIdentity,
}

impl Drop for OwnedNativeGroup {
    fn drop(&mut self) {
        let _ = terminate_attested_group(self.identity);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn revoked_prepared_kgc1_attempt_cannot_signal_a_later_live_guardian_child() {
    let (host, mut guardian) = UnixStream::pair().expect("real private Unix stream pair");
    guardian
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bounded authentic generation exchange");
    let (updates_tx, _updates_rx) = mpsc::channel::<GuardedPrimaryUpdate>();
    let current = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).arm();
    let worker = start_generation_host_fixture(
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
        &GenerationControlRecord::new(CONTROL_REVOKE, 1)
            .with_cause(RevocationCause::AdmissionFailed),
        CONTROL_REVOKED,
    );
    // This is a real kernel child process group owned by the deliberately
    // test-owned Guardian role (the test parent). A stale KGC1 record must
    // not be able to kill it after its prepared lease was consumed.
    let mut command = long_running_command();
    command.process_group(0);
    let child = command
        .spawn()
        .expect("start real live isolated native group");
    let group_pid = child.id();
    let mut group = OwnedNativeGroup {
        child,
        identity: observe_group_identity(group_pid).expect("live original group birth"),
    };
    lock_recovery(&recovery).deny();
    write_generation_control(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(group_pid),
    )
    .expect("write real post-revocation fixed SPAWNED bytes");
    let failure = worker
        .join()
        .expect("generation host worker must terminate after invalid order")
        .expect_err("revoked generation cannot mint new signal authority");
    assert!(
        failure.to_string().contains("invalid spawned transition"),
        "missing-lease must be denied before native group identity/signal: {failure}"
    );
    let pid = Pid::from_raw(i32::try_from(group_pid).expect("kernel group PID fits"));
    assert_eq!(
        getpgid(Some(pid)),
        Ok(pid),
        "native group must remain itself"
    );
    assert_eq!(
        kill(pid, None),
        Ok(()),
        "live native group survives malformed sequence"
    );
    assert!(
        group
            .child
            .try_wait()
            .expect("observed native child status")
            .is_none(),
        "post-revocation SPAWNED signaled a still-owned native group"
    );
    assert!(
        !matches!(kill(Pid::from_raw(-pid.as_raw()), None), Err(Errno::ESRCH)),
        "live owned process group must be independently observable"
    );
}
