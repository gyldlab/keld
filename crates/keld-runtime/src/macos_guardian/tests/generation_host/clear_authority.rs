//! A forged/early authenticated KGC1 CLEAR cannot discard a live Bun group.
//! The host must exercise its own attested native fail-safe AND prove whole-group
//! absence before CLEARED; a successful signal is not the absence witness.
use std::io::{self, Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use super::super::super::process_group::{
    GroupIdentity, observe_group_identity, require_attested_group_absence, terminate_attested_group,
};
use super::super::super::{GUARDIAN_SHUTDOWN_DEADLINE, handshake_client, parse_app_link};
use super::super::test_support::{NativeChildOwner, await_held_zombie_group, verified_real_bun};
use super::{
    CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REGISTERED,
    CONTROL_REVOKE, CONTROL_REVOKED, CONTROL_SPAWNED, GenerationControlRecord,
    GuardedPrimaryUpdate, RecoveryState, RevocationCause, RuntimeError, guardian_exchange,
    lock_recovery, read_generation_control, start_generation_host_fixture,
    write_generation_control,
};

struct NativeBunGroup {
    leader: NativeChildOwner,
    original_birth: Option<GroupIdentity>,
}

impl Drop for NativeBunGroup {
    fn drop(&mut self) {
        // The real Bun leader may already have been reaped by the success
        // oracle. A saved numeric PGID and a descendant marker do not prove
        // ownership of any future process assigned that number.
        if let Some(original_birth) = self.original_birth.take() {
            let _ = terminate_attested_group(original_birth);
        }
        // NativeChildOwner follows this Drop and reaps only its original
        // direct Child, including a panic before group-birth observation.
    }
}

mod cleanup_authority;
mod failed_clear_authority;

fn await_bun_descendant(marker: &std::path::Path) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(contents) = std::fs::read_to_string(marker)
            && let Ok(pid) = contents.trim().parse::<i32>()
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "real Bun never spawned its group descendant"
        );
        std::thread::yield_now();
    }
}

fn start_real_bun_group(marker: &std::path::Path) -> NativeBunGroup {
    let bun = verified_real_bun();
    let script = r#"
const { spawn } = require("node:child_process");
const { writeFileSync } = require("node:fs");
const child = spawn("/bin/sleep", ["30"], { stdio: "ignore" });
if (!child.pid) process.exit(72);
writeFileSync(process.env.KELD_CLEAR_TEST_DESCENDANT, String(child.pid));
setInterval(() => {}, 1000);
"#;
    // The test's native rescue must own this group BEFORE any Bun child can
    // fork its same-group descendant. An initial /bin/sh becomes the same PID
    // after exec and does not change the original kernel group birth.
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "IFS= read -r permit || exit 73; [ \"$permit\" = R ] || exit 74; exec \"$@\"",
            "keld-clear-preexec-gate",
        ])
        .arg(&bun)
        .args(["-e", script])
        .env("KELD_CLEAR_TEST_DESCENDANT", marker)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let leader = NativeChildOwner::new(
        command
            .spawn()
            .expect("spawn held isolated Bun group leader"),
    );
    let mut group = NativeBunGroup {
        leader,
        original_birth: None,
    };
    group.original_birth = Some(
        observe_group_identity(group.leader.id())
            .expect("retain original group birth while Bun's pre-exec gate is still closed"),
    );
    group
        .leader
        .stdin
        .as_mut()
        .expect("sole test-owned Bun pre-exec gate")
        .write_all(b"R\n")
        .expect("release Bun only after group birth is retained");
    group.leader.stdin.take();
    group
}

fn descendant_gone(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while kill(Pid::from_raw(pid), None) != Err(Errno::ESRCH) {
        assert!(
            Instant::now() < deadline,
            "Bun descendant escaped the host's CLEAR fail-safe"
        );
        std::thread::yield_now();
    }
}

fn assert_no_premature_cleared(guardian: &mut UnixStream, group: Pid) {
    // Read just one byte: any early ACK is a violation. On success, the
    // original framed KGC1 record is left untouched for the later read.
    guardian
        .set_read_timeout(Some(Duration::from_millis(20)))
        .expect("bounded KGC1 ACK observation");
    let no_ack_deadline = Instant::now() + Duration::from_millis(150);
    let mut first_byte = [0];
    while Instant::now() < no_ack_deadline {
        match guardian.read(&mut first_byte) {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Ok(0) => panic!("host closed KGC1 before native group absence"),
            Ok(_) => panic!("host sent CLEARED before native zombie was reaped"),
            Err(error) => panic!("KGC1 ACK observation failed: {error}"),
        }
        assert_ne!(
            kill(Pid::from_raw(-group.as_raw()), None),
            Err(Errno::ESRCH),
            "test-owned group unexpectedly disappeared before direct Child reap"
        );
        std::thread::yield_now();
    }
    guardian
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("restore KGC1 framing after native absence witness");
}

fn assert_no_clear_bytes_before_host_eof(
    guardian: &mut UnixStream,
    group: Pid,
    clear_started_at: Instant,
) {
    // Full-budget negative KGC1 observation, independent of the direct
    // absence-helper test. A byte before native group reap is a failed oracle.
    guardian
        .set_read_timeout(Some(Duration::from_millis(20)))
        .expect("bounded KGC1 negative observation");
    let watchdog = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE + Duration::from_secs(2);
    let mut first_byte = [0_u8];
    loop {
        assert!(
            Instant::now() < watchdog,
            "host did not fail closed within the existing shutdown deadline"
        );
        match guardian.read(&mut first_byte) {
            Ok(0) => break,
            Ok(_) => panic!("host emitted KGC1 bytes while original zombie group persisted"),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => panic!("KGC1 negative observation failed: {error}"),
        }
        assert_eq!(
            kill(Pid::from_raw(-group.as_raw()), None),
            Err(Errno::EPERM),
            "test-owned original zombie group must persist until direct Child reap"
        );
    }
    assert!(
        clear_started_at.elapsed() >= GUARDIAN_SHUTDOWN_DEADLINE,
        "host fabricated an early group-absence timeout before its existing budget expired"
    );
}

fn assert_native_clear_absence_timeout(error: RuntimeError) {
    assert!(
        matches!(
            error,
            RuntimeError::Lifecycle {
                phase: "macOS guardian generation clear group absence",
                source
            } if source.kind() == io::ErrorKind::TimedOut
        ),
        "only a native group-absence timeout can close this KGC1 transaction"
    );
}

fn assert_live_bun_group_before_control(owned: &mut NativeBunGroup, descendant: i32, group: Pid) {
    assert_eq!(
        getpgid(Some(Pid::from_raw(descendant))),
        Ok(group),
        "real Bun descendant belongs to the exact registered process group"
    );
    observe_group_identity(owned.leader.id())
        .expect("kernel still holds original Bun leader birth");
    assert!(
        owned
            .leader
            .try_wait()
            .expect("inspect real Bun before CLEAR")
            .is_none(),
        "premature Bun exit cannot satisfy the forged CLEAR negative control"
    );
}

#[test]
fn held_unreaped_bun_group_exhausts_absence_budget_before_native_reap() {
    let root = tempfile::tempdir().expect("owned negative-control Bun/CLEAR root");
    let mut owned = start_real_bun_group(&root.path().join("descendant.pid"));
    let descendant = await_bun_descendant(&root.path().join("descendant.pid"));
    let group = Pid::from_raw(i32::try_from(owned.leader.id()).expect("group pid fits"));
    let original_birth = owned.original_birth.expect("retain real group birth");
    assert_eq!(getpgid(Some(Pid::from_raw(descendant))), Ok(group));

    terminate_attested_group(original_birth).expect("one birth-attested SIGKILL");
    await_held_zombie_group(group);
    // The original test also proves host-side CLEAR wiring. This additional
    // independent negative control holds the actual zombie beyond a complete
    // injected 350ms polling budget. A bogus 200ms delayed Ok must fail it.
    let error =
        require_attested_group_absence(original_birth, Instant::now() + Duration::from_millis(350))
            .expect_err("a held native zombie group must not satisfy absence");
    assert!(
        matches!(error, RuntimeError::Lifecycle { phase: "macOS guardian generation clear group absence", source } if source.kind() == io::ErrorKind::TimedOut),
        "absence must fail closed with a typed timeout"
    );
    assert_eq!(
        kill(Pid::from_raw(-group.as_raw()), None),
        Err(Errno::EPERM)
    );

    let status = owned.leader.wait().expect("reap exact native group leader");
    assert_eq!(status.signal(), Some(9));
    descendant_gone(descendant);
    require_attested_group_absence(original_birth, Instant::now() + Duration::from_secs(5))
        .expect("real dual ESRCH permits absence only after exact native reap");
}

#[test]
fn early_kgc1_clear_held_zombie_never_acknowledges_before_host_deadline() {
    let root = tempfile::tempdir().expect("owned host-integration zombie test root");
    let marker = root.path().join("descendant.pid");
    let mut owned = start_real_bun_group(&marker);
    let group_pid = owned.leader.id();
    let original_birth = owned
        .original_birth
        .expect("retain enrolled native group birth");
    let group = Pid::from_raw(i32::try_from(group_pid).expect("group pid fits"));
    let descendant = await_bun_descendant(&marker);
    assert_live_bun_group_before_control(&mut owned, descendant, group);

    // Exercise the real KGC1 host path, not the absence helper in isolation:
    // a mutant that drops the helper or sleeps 250ms before CLEARED must fail.
    let (host, mut guardian) = UnixStream::pair().expect("private native KGC1 control pair");
    guardian
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bounded KGC1 setup");
    let (updates_tx, updates_rx) = mpsc::channel::<GuardedPrimaryUpdate>();
    let current = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).arm();
    let host_worker = start_generation_host_fixture(
        host,
        updates_tx,
        Arc::clone(&current),
        Arc::clone(&recovery),
    );
    let prepared = guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_PREPARE, 1),
        CONTROL_PREPARED,
    );
    let app_link = String::from_utf8(prepared.payload).expect("host-minted app link");
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(group_pid),
        CONTROL_REGISTERED,
    );
    let (endpoint, token) = parse_app_link(&app_link).expect("authenticated link");
    let mut app = UnixStream::connect(endpoint).expect("connect actual app link");
    handshake_client(&mut app, &token).expect("bind real Bun generation");
    let bound = loop {
        match updates_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded generation authentication")
        {
            GuardedPrimaryUpdate::Bound(bound) => break bound,
            GuardedPrimaryUpdate::Role(_) => {}
        }
    };
    assert_eq!(bound.attempt(), 1);
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_REVOKE, 1)
            .with_cause(RevocationCause::AdmissionFailed),
        CONTROL_REVOKED,
    );
    assert!(
        owned
            .leader
            .try_wait()
            .expect("inspect pre-CLEAR Bun leader")
            .is_none(),
        "REVOKED did not prove native termination"
    );
    let clear_started_at = Instant::now();
    write_generation_control(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_CLEAR, 1).with_pid(group_pid),
    )
    .expect("send early authenticated CLEAR");
    await_held_zombie_group(group);

    // Keep the exact native Child unreaped for the ENTIRE production shutdown
    // budget. EOF without CLEARED is acceptable only with the typed host
    // timeout and retained original group owner checked below.
    assert_no_clear_bytes_before_host_eof(&mut guardian, group, clear_started_at);
    let host_error = host_worker
        .join()
        .expect("host worker terminates after original native budget")
        .expect_err("host must reject unreaped zombie CLEAR");
    assert_native_clear_absence_timeout(host_error);
    assert_eq!(
        current
            .lock()
            .expect("current generation mutex")
            .as_ref()
            .map(|g| (g.attempt, g.group_pid, g.group_identity)),
        Some((1, group_pid, Some(original_birth))),
        "failed CLEAR may not release the original birth-attested host owner"
    );
    assert_eq!(
        kill(Pid::from_raw(-group.as_raw()), None),
        Err(Errno::EPERM)
    );

    let status = owned
        .leader
        .wait()
        .expect("reap exact owned zombie after host EOF");
    assert_eq!(status.signal(), Some(9));
    descendant_gone(descendant);
    lock_recovery(&recovery).deny();
    drop(bound);
    drop(app);
    drop(guardian);
}

#[test]
fn early_kgc1_clear_reaps_live_bun_and_same_group_descendant_before_cleared() {
    let root = tempfile::tempdir().expect("owned native Bun/CLEAR test root");
    let marker = root.path().join("descendant.pid");
    let mut owned = start_real_bun_group(&marker);
    let group_pid = owned.leader.id();
    let group = Pid::from_raw(i32::try_from(group_pid).expect("group pid fits"));
    let descendant = await_bun_descendant(&marker);
    assert_live_bun_group_before_control(&mut owned, descendant, group);

    let (host, mut guardian) = UnixStream::pair().expect("private KGC1 generation-control pair");
    guardian
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bounded real Guardian read");
    let (updates_tx, updates_rx) = mpsc::channel::<GuardedPrimaryUpdate>();
    let current = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).arm();
    let host_worker = start_generation_host_fixture(
        host,
        updates_tx,
        Arc::clone(&current),
        Arc::clone(&recovery),
    );
    let prepared = guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_PREPARE, 1),
        CONTROL_PREPARED,
    );
    let app_link = String::from_utf8(prepared.payload).expect("actual host-minted link");
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(group_pid),
        CONTROL_REGISTERED,
    );
    let (endpoint, token) = parse_app_link(&app_link).expect("authenticated link contents");
    let mut app = UnixStream::connect(endpoint).expect("connect real generation app link");
    handshake_client(&mut app, &token).expect("authenticate actual generation before revoke");
    let bound = loop {
        match updates_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("receive actual bounded generation authentication")
        {
            GuardedPrimaryUpdate::Bound(bound) => break bound,
            GuardedPrimaryUpdate::Role(_) => {}
        }
    };
    assert_eq!(
        bound.attempt(),
        1,
        "host must really bind the admitted Bun generation"
    );
    guardian_exchange(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_REVOKE, 1)
            .with_cause(RevocationCause::AdmissionFailed),
        CONTROL_REVOKED,
    );
    assert!(
        owned
            .leader
            .try_wait()
            .expect("inspect live group after REVOKE")
            .is_none(),
        "REVOKED alone must not be miscounted as native group termination"
    );
    // A faulty but authenticated Guardian claims CLEAR while Bun and its
    // descendant are still executing. Retain the exact direct Child UNREAPED
    // to force the kernel's zombie-only group state. A successful host SIGKILL
    // is not sufficient to send CLEARED while the group is still visible.
    assert_eq!(kill(Pid::from_raw(-group.as_raw()), None), Ok(()));
    write_generation_control(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_CLEAR, 1).with_pid(group_pid),
    )
    .expect("send early authenticated KGC1 CLEAR");
    await_held_zombie_group(group);
    assert_no_premature_cleared(&mut guardian, group);
    let leader_status = owned
        .leader
        .wait()
        .expect("reap exact SIGKILLed Bun leader");
    // Reaping the direct Child and observing the descendant's exit are native
    // facts independent of the KGC1 acknowledgment under test.
    descendant_gone(descendant);
    let cleared = read_generation_control(&mut guardian)
        .expect("host acknowledges CLEAR only after native absence");
    assert_eq!(
        (cleared.kind, cleared.attempt, cleared.pid),
        (CONTROL_CLEARED, 1, group_pid)
    );
    assert_eq!(
        leader_status.signal(),
        Some(9),
        "host must SIGKILL still-live Bun at CLEAR"
    );
    assert!(current.lock().expect("current generation mutex").is_none());
    lock_recovery(&recovery).deny();
    drop(bound);
    drop(app);
    drop(guardian);
    // This is a negative control; terminating authenticated control mid-run
    // may be an unattributed EOF. Do not count a failed worker as a clean exit.
    let _ = host_worker
        .join()
        .expect("host control worker joins after test shutdown");
}
