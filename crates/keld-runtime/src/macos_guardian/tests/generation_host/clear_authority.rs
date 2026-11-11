//! A forged/early authenticated KGC1 CLEAR cannot discard a live Bun group.
//! The host must exercise its own attested native fail-safe before CLEARED.
use std::io::Write as _;
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use super::super::super::process_group::{
    GroupIdentity, TestGroupSignalHookOwner, TestGroupSignalPoint, install_test_group_signal_hook,
    observe_group_identity, terminate_attested_group,
};
use super::super::super::{handshake_client, parse_app_link};
use super::super::test_support::{NativeChildOwner, verified_real_bun};
use super::{
    CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REGISTERED,
    CONTROL_REVOKE, CONTROL_REVOKED, CONTROL_SPAWNED, GenerationControlRecord,
    GuardedPrimaryUpdate, RecoveryState, RevocationCause, await_bound_generation,
    guardian_exchange, lock_recovery, start_generation_host_fixture_with_setup,
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

fn assert_live_bun_group(
    owned: &mut NativeBunGroup,
    group_pid: u32,
    marker: &std::path::Path,
) -> (Pid, i32) {
    let group = Pid::from_raw(i32::try_from(group_pid).expect("group pid fits"));
    let descendant = await_bun_descendant(marker);
    assert_eq!(
        getpgid(Some(Pid::from_raw(descendant))),
        Ok(group),
        "real Bun descendant belongs to the exact registered process group"
    );
    observe_group_identity(group_pid).expect("kernel still holds original Bun leader birth");
    assert!(
        owned
            .leader
            .try_wait()
            .expect("inspect real Bun before CLEAR")
            .is_none(),
        "premature Bun exit cannot satisfy the forged CLEAR negative control"
    );
    (group, descendant)
}

fn assert_native_absence(
    drained: &crate::primary::RoleProcessDrained,
    original_principal: crate::primary::RolePrincipal,
    group: Pid,
    leader_status: std::process::ExitStatus,
    current: &Mutex<Option<super::super::super::RegisteredPrimaryGeneration>>,
) {
    assert_eq!(drained.principal(), original_principal);
    assert_eq!(drained.attempt(), 1);
    assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
    assert_eq!(
        kill(Pid::from_raw(-group.as_raw()), None),
        Err(Errno::ESRCH)
    );
    assert_eq!(
        leader_status.signal(),
        Some(9),
        "host must SIGKILL still-live Bun at CLEAR"
    );
    assert!(current.lock().expect("current generation mutex").is_none());
}

fn assert_unreaped_clear_refusal(
    guardian: &mut UnixStream,
    host_worker: std::thread::JoinHandle<Result<(), super::RuntimeError>>,
    updates_rx: &mpsc::Receiver<GuardedPrimaryUpdate>,
) -> super::RuntimeError {
    assert!(
        super::read_generation_control(guardian).is_err(),
        "an unreaped original Bun must never receive CLEARED"
    );
    let error = host_worker
        .join()
        .expect("host worker terminates on missing proof")
        .expect_err("physical absence is unproven");
    assert!(error.to_string().contains("absence"), "{error}");
    assert!(
        !updates_rx.try_iter().any(|update| matches!(
            update,
            GuardedPrimaryUpdate::Role(super::RoleEvent::ProcessDrained { .. })
        )),
        "kill delivery, Revoked and EOF cannot mint physical drain"
    );
    error
}

#[test]
fn early_kgc1_clear_reaps_live_bun_and_same_group_descendant_before_cleared() {
    exercise_clear_absence_witness(true);
}

#[test]
fn unreaped_bun_cannot_publish_physical_drain_or_release_cleared() {
    exercise_clear_absence_witness(false);
}

fn clear_reap_fence(
    group: u32,
    enabled: bool,
    reaped: mpsc::Receiver<()>,
) -> Option<TestGroupSignalHookOwner> {
    enabled.then(|| {
        install_test_group_signal_hook(
            group,
            Box::new(move |point| {
                if point == TestGroupSignalPoint::AfterDelivery {
                    reaped
                        .recv_timeout(Duration::from_secs(5))
                        .expect("test parent reaps the killed group before the absence probe");
                }
            }),
        )
    })
}

fn exercise_clear_absence_witness(reap_before_ack: bool) {
    let root = tempfile::tempdir().expect("owned native Bun/CLEAR test root");
    let marker = root.path().join("descendant.pid");
    let mut owned = start_real_bun_group(&marker);
    let group_pid = owned.leader.id();
    let (group, descendant) = assert_live_bun_group(&mut owned, group_pid, &marker);

    let (host, mut guardian) = UnixStream::pair().expect("private KGC1 generation-control pair");
    guardian
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bounded real Guardian read");
    let (updates_tx, updates_rx) = mpsc::channel::<GuardedPrimaryUpdate>();
    let current = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).arm();
    let (reaped_tx, reaped_rx) = mpsc::channel();
    let host_worker = start_generation_host_fixture_with_setup(
        host,
        updates_tx,
        Arc::clone(&current),
        Arc::clone(&recovery),
        move || clear_reap_fence(group_pid, reap_before_ack, reaped_rx),
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
    let bound = await_bound_generation(
        &updates_rx,
        "receive actual bounded generation authentication",
    );
    assert_eq!(
        bound.attempt(),
        1,
        "host must really bind the admitted Bun generation"
    );
    let original_principal = bound.principal();
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
    // Compromised/buggy Guardian falsely claims CLEAR despite still-live Bun
    // and descendant. Without the production CLEAR fail-safe, this receives
    // CLEARED while both continue to execute (failure-first negative oracle).
    super::write_generation_control(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_CLEAR, 1).with_pid(group_pid),
    )
    .expect("request native CLEAR without claiming the child was reaped");
    if !reap_before_ack {
        // SIGKILL may be delivered, but this real OS parent deliberately retains
        // the direct child's zombie. Only actual disappearance may mint proof.
        let _error = assert_unreaped_clear_refusal(&mut guardian, host_worker, &updates_rx);
        let _ = owned
            .leader
            .wait()
            .expect("test parent now reaps its exact Bun");
        descendant_gone(descendant);
        return;
    }
    // The real Guardian performs this exact Child reap BEFORE sending CLEAR.
    // This early-CLEAR adversary instead waits while Host's fail-safe kills,
    // then reaps before releasing the test fence at the actual signal boundary.
    // The production absence check is intentionally not a wait for a live leader.
    let leader_status = owned.leader.wait().expect("host cleared native Bun group");
    descendant_gone(descendant);
    reaped_tx
        .send(())
        .expect("release absence proof only after real child and descendant disappearance");
    let cleared = super::read_generation_control(&mut guardian)
        .expect("CLEARED only after actual native absence");
    assert_eq!(cleared.kind, CONTROL_CLEARED);
    let drained = updates_rx
        .try_iter()
        .find_map(|update| match update {
            GuardedPrimaryUpdate::Role(super::RoleEvent::ProcessDrained { retirement }) => {
                Some(retirement)
            }
            _ => None,
        })
        .expect("actual old Bun/group disappearance publishes physical witness");
    assert_native_absence(&drained, original_principal, group, leader_status, &current);
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
