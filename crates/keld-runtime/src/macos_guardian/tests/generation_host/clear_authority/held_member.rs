//! A reaped leader is not absence while a separate enrolled member is a zombie.
use std::fs::File;
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use super::super::super::super::generation_host::GenerationHostAdmission;
use super::super::super::super::guarded_primary::NativeSignalAuthority;
use super::super::super::super::process_group::{
    TestGroupSignalPoint, install_test_group_signal_hook,
};
use super::super::super::test_support::join_before;
use super::super::run_generation_host;
use super::{
    CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REGISTERED,
    CONTROL_REVOKE, CONTROL_REVOKED, CONTROL_SPAWNED, GUARDIAN_SHUTDOWN_DEADLINE,
    GenerationControlRecord, GuardedPrimaryUpdate, NativeChildOwner, RecoveryState,
    RevocationCause, assert_native_clear_absence_timeout, assert_no_clear_bytes_before_host_eof,
    await_held_zombie_group, guardian_exchange, handshake_client, lock_recovery,
    observe_group_identity, parse_app_link, write_generation_control,
};

const SELECTOR: &str = "macos_guardian::tests::generation_host::clear_authority::held_member::authenticated_clear_reaped_leader_retains_held_member";
const CHILD_ENV: &str = "KELD_CLEAR_HELD_MEMBER_CHILD";

#[test]
fn authenticated_clear_reaped_leader_retains_held_member() {
    if std::env::var_os(CHILD_ENV).is_some() {
        run_clear_session(true);
        run_clear_session(false);
        println!("held-member-negative-and-healthy-complete");
        return;
    }
    let root = tempfile::tempdir().expect("isolated native test output");
    let log = root.path().join("child.log");
    let output = File::create(&log).expect("child output file");
    let mut child = NativeChildOwner::new(
        Command::new(std::env::current_exe().expect("native test binary"))
            .args(["--exact", SELECTOR, "--nocapture", "--test-threads=1"])
            .env(CHILD_ENV, "1")
            .stdin(Stdio::null())
            .stdout(output.try_clone().expect("stdout file"))
            .stderr(output)
            .spawn()
            .expect("isolated native CLEAR test"),
    );
    let watchdog = Instant::now() + Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.try_wait().expect("observe owned test process") {
            break status;
        }
        assert!(Instant::now() < watchdog, "isolated CLEAR test hung");
        std::thread::yield_now();
    };
    let text = std::fs::read_to_string(log).expect("native test observations");
    assert!(status.success(), "{status}: {text}");
    assert!(
        text.contains("held-member-negative-and-healthy-complete"),
        "exact selector must execute both scenarios: {text}"
    );
}

fn owned_sleep(group: i32) -> NativeChildOwner {
    NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(group)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("owned group fixture"),
    )
}

// Local instrumentation wraps the same production host entry as the shared
// fixture; the thread-local signal observer must be installed on that thread.
fn start_counted_host(
    host: UnixStream,
    updates: mpsc::Sender<GuardedPrimaryUpdate>,
    current: Arc<Mutex<Option<super::super::super::super::RegisteredPrimaryGeneration>>>,
    recovery: Arc<Mutex<RecoveryState>>,
    group: u32,
    signals: Arc<AtomicUsize>,
) -> std::thread::JoinHandle<Result<(), super::RuntimeError>> {
    std::thread::spawn(move || {
        let _hook = install_test_group_signal_hook(
            group,
            Box::new(move |point| {
                assert_eq!(point, TestGroupSignalPoint::BeforeSignal);
                signals.fetch_add(1, Ordering::SeqCst);
            }),
        );
        run_generation_host(
            GenerationHostAdmission::new(
                std::process::id(),
                Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
            ),
            host,
            &updates,
            &current,
            &recovery,
            None,
            None,
        )
    })
}

fn authenticate_and_revoke(
    guardian: &mut UnixStream,
    updates: &mpsc::Receiver<GuardedPrimaryUpdate>,
    group: u32,
) -> (UnixStream, crate::role::BoundRoleGeneration) {
    let prepared = guardian_exchange(
        guardian,
        &GenerationControlRecord::new(CONTROL_PREPARE, 1),
        CONTROL_PREPARED,
    );
    guardian_exchange(
        guardian,
        &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(group),
        CONTROL_REGISTERED,
    );
    let link = String::from_utf8(prepared.payload).expect("host minted app link");
    let (endpoint, token) = parse_app_link(&link).expect("actual app link");
    let mut app = UnixStream::connect(endpoint).expect("connect app");
    handshake_client(&mut app, &token).expect("authenticate app");
    let bound = loop {
        match updates
            .recv_timeout(Duration::from_secs(5))
            .expect("Bound event")
        {
            GuardedPrimaryUpdate::Bound(bound) => break bound,
            GuardedPrimaryUpdate::Role(_) => {}
        }
    };
    assert_eq!(bound.attempt(), 1);
    guardian_exchange(
        guardian,
        &GenerationControlRecord::new(CONTROL_REVOKE, 1)
            .with_cause(RevocationCause::AdmissionFailed),
        CONTROL_REVOKED,
    );
    (app, bound)
}

fn owned_group() -> (NativeChildOwner, NativeChildOwner, super::GroupIdentity) {
    let leader = owned_sleep(0);
    let group_pid = leader.id();
    let raw = i32::try_from(group_pid).expect("group PID fits");
    let group = Pid::from_raw(raw);
    assert_ne!(getpgid(None).expect("harness group"), group);
    let member = owned_sleep(raw);
    assert_eq!(
        getpgid(Some(Pid::from_raw(
            i32::try_from(member.id()).expect("member PID")
        ))),
        Ok(group)
    );
    let birth = observe_group_identity(group_pid).expect("native leader birth");
    (leader, member, birth)
}

fn observe_healthy_clear_and_successor(
    guardian: &mut UnixStream,
    recovery: &Mutex<RecoveryState>,
    group_pid: u32,
) {
    let cleared = super::read_generation_control(guardian).expect("healthy CLEAR");
    assert_eq!(
        (cleared.kind, cleared.attempt, cleared.pid),
        (CONTROL_CLEARED, 1, group_pid)
    );
    // CLEAR alone does not grant successor authority; recovery must release it.
    lock_recovery(recovery).arm();
    let prepared = guardian_exchange(
        guardian,
        &GenerationControlRecord::new(CONTROL_PREPARE, 2),
        CONTROL_PREPARED,
    );
    assert_eq!((prepared.kind, prepared.attempt), (CONTROL_PREPARED, 2));
    guardian_exchange(
        guardian,
        &GenerationControlRecord::new(CONTROL_REVOKE, 2).with_cause(RevocationCause::Shutdown),
        CONTROL_REVOKED,
    );
    guardian_exchange(
        guardian,
        &GenerationControlRecord::new(CONTROL_CLEAR, 2),
        CONTROL_CLEARED,
    );
}

fn run_clear_session(hold_member: bool) {
    let (mut leader, mut member, birth) = owned_group();
    let group_pid = leader.id();
    let raw = i32::try_from(group_pid).expect("group PID fits");
    let group = Pid::from_raw(raw);
    let (host, mut guardian) = UnixStream::pair().expect("private control pair");
    guardian
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bounded read");
    let (tx, rx) = mpsc::channel();
    let current = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).arm();
    let signals = Arc::new(AtomicUsize::new(0));
    let worker = start_counted_host(
        host,
        tx,
        Arc::clone(&current),
        Arc::clone(&recovery),
        group_pid,
        Arc::clone(&signals),
    );
    let (app, bound) = authenticate_and_revoke(&mut guardian, &rx, group_pid);
    let started = Instant::now();
    write_generation_control(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_CLEAR, 1).with_pid(group_pid),
    )
    .expect("CLEAR");
    await_held_zombie_group(group);
    assert_eq!(
        leader.wait().expect("reap only exact leader").signal(),
        Some(9)
    );
    assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::EPERM));

    if hold_member {
        // Queue a real successor request behind CLEAR. Any reply byte before
        // native absence is invalid, whether CLEARED or PREPARED.
        write_generation_control(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_PREPARE, 2),
        )
        .expect("queued successor");
        assert_no_clear_bytes_before_host_eof(&mut guardian, group, started);
        let error = join_before(
            worker,
            started + 2 * GUARDIAN_SHUTDOWN_DEADLINE,
            "host did not fail closed",
        )
        .expect("host join")
        .expect_err("held member prevents CLEAR");
        assert_native_clear_absence_timeout(error);
        let retained = (*current.lock().expect("generation lock")).expect("retain original owner");
        assert_eq!(retained.group_identity, Some(birth));
        assert!(matches!(
            retained.native_signal,
            NativeSignalAuthority::Delivered { .. }
        ));
        assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::EPERM));
        assert_eq!(
            member
                .wait()
                .expect("reap held exact member after refusal")
                .signal(),
            Some(9)
        );
    } else {
        assert_eq!(
            member
                .wait()
                .expect("healthy session reaps exact member")
                .signal(),
            Some(9)
        );
        observe_healthy_clear_and_successor(&mut guardian, &recovery, group_pid);
        {
            let mut recovery = lock_recovery(&recovery);
            recovery.deny();
            recovery.shutdown_acknowledged = true;
        }
        drop(bound);
        drop(app);
        drop(guardian);
        join_before(
            worker,
            Instant::now() + 2 * GUARDIAN_SHUTDOWN_DEADLINE,
            "healthy fixture worker did not close",
        )
        .expect("healthy worker join")
        .expect("healthy authenticated KGC1 session exits cleanly");
        assert_eq!(signals.load(Ordering::SeqCst), 1);
        assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
        assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
        return;
    }
    assert_eq!(signals.load(Ordering::SeqCst), 1);
    assert_eq!(getpgid(Some(group)), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
    lock_recovery(&recovery).deny();
    drop(bound);
    drop(app);
    drop(guardian);
}
