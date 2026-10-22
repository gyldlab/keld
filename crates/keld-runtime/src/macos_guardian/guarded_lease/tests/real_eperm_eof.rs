//! A real Darwin group transition makes the attested Guardian's SIGKILL return
//! EPERM, then closes its authentic KGC1 stream while the host retains custody.
//! Both original Children are owned by this fixture until their exact waits.

use std::io;
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Instant;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::sys::wait::{WaitPidFlag, waitpid};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::super::GenerationLease as _;
use super::super::super::generation_host::GenerationHostAdmission;
use super::super::super::process_group::{
    GroupIdentity, LocalGroupWitness, TestGroupSignalHookOwner, TestGroupSignalPoint,
    install_test_group_signal_hook, observe_group_identity, terminate_attested_group,
    validate_guardian_child_group,
};
use super::super::super::{
    BoundRoleGeneration, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REGISTERED, CONTROL_SPAWNED,
    GENERATION_CONTROL_POLL, GUARDIAN_SHUTDOWN_DEADLINE, GenerationControlPeer,
    GenerationControlReader, GenerationControlRecord, GuardedPrimaryUpdate, RecoveryState,
    RegisteredPrimaryGeneration, RevocationCause, RuntimeError, control_exchange, handshake_client,
    lock_generation, lock_recovery, parse_app_link, run_generation_host,
};
use super::super::GuardedPrimaryLease;
use super::own_child_reap::{assert_classified_as_denied_signal, assert_signal_phase_eperm};

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

// Keep each real KGC1 worker joined even when a native assertion panics.
// SignalRelease is declared after the workers and releases their last barrier
// first, so a failed test never strands a worker with Child custody.
struct OwnedWorker<T>(Option<thread::JoinHandle<T>>);

impl<T> OwnedWorker<T> {
    fn new(worker: thread::JoinHandle<T>) -> Self {
        Self(Some(worker))
    }

    fn join(&mut self) -> thread::Result<T> {
        self.0.take().expect("one original worker join").join()
    }
}

impl<T> Drop for OwnedWorker<T> {
    fn drop(&mut self) {
        if let Some(worker) = self.0.take() {
            let _ = worker.join();
        }
    }
}

// The controller always releases a native signal fence, including on an
// assertion panic. The bounded worker can then finish its original cleanup;
// only this fixture's retained Child handles own the emergency kill/wait.
struct SignalRelease(Option<mpsc::SyncSender<()>>);

impl SignalRelease {
    fn release(&mut self) {
        self.0
            .take()
            .expect("one native signal release")
            .send(())
            .expect("original attested signal worker receives release");
    }
}

impl Drop for SignalRelease {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.send(());
        }
    }
}

fn running_member(group: i32) -> OwnedChild {
    let mut command = Command::new("/bin/sleep");
    command
        .arg("30")
        .process_group(group)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    OwnedChild(command.spawn().expect("one exact test-owned group Child"))
}

// The original group leader. The Guardian lease takes its exact direct Child
// for the shipping own-Child reap, so the fixture cannot also hold a kill
// handle; it keeps only the leader's stdin. Closing that pipe makes the exact
// original leader exit on its own: no PID, PGID or signal authority is used.
fn exiting_leader() -> (OwnedChild, ChildStdin) {
    let mut command = Command::new("/bin/cat");
    command
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut leader = OwnedChild(command.spawn().expect("one exact test-owned group leader"));
    let exit = leader.0.stdin.take().expect("original leader stdin pipe");
    (leader, exit)
}

fn await_native_zombie(pid: u32, deadline: Instant) {
    loop {
        let output = Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .expect("real Darwin process-state observation");
        if output.status.success() && output.stdout.starts_with(b"Z") {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "original retained Child did not reach Darwin zombie state"
        );
        thread::yield_now();
    }
}

// The existing shipping host worker consumes the real fixed KGC1 record and
// validates the test's direct Child. Keep authenticated app/role streams alive
// until the observed EOF; the lease takes sole Guardian writer ownership.
struct NativeKgc1Host {
    control: Option<Arc<Mutex<GenerationControlPeer>>>,
    host_worker: OwnedWorker<Result<(), RuntimeError>>,
    current: Arc<Mutex<Option<RegisteredPrimaryGeneration>>>,
    _updates_rx: mpsc::Receiver<GuardedPrimaryUpdate>,
    _bound: BoundRoleGeneration,
    _app: UnixStream,
}

impl NativeKgc1Host {
    fn register(group_pid: u32) -> Self {
        let (host, guardian) = UnixStream::pair().expect("actual KGC1 byte stream");
        guardian
            .set_read_timeout(Some(GENERATION_CONTROL_POLL))
            .expect("real Guardian control read deadline");
        guardian
            .set_write_timeout(Some(GENERATION_CONTROL_POLL))
            .expect("real Guardian control write deadline");
        let (updates_tx, updates_rx) = mpsc::channel();
        let current = Arc::new(Mutex::new(None));
        let recovery = Arc::new(Mutex::new(RecoveryState::new()));
        lock_recovery(&recovery).arm();
        let (host_current, host_recovery) = (Arc::clone(&current), Arc::clone(&recovery));
        let host_worker = OwnedWorker::new(thread::spawn(move || {
            run_generation_host(
                GenerationHostAdmission::new(
                    std::process::id(),
                    Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
                ),
                host,
                &updates_tx,
                &host_current,
                &host_recovery,
                None,
                None,
            )
        }));
        let control = Arc::new(Mutex::new(GenerationControlPeer {
            stream: guardian,
            reader: GenerationControlReader::new(),
        }));
        let prepared = control_exchange(
            &control,
            &GenerationControlRecord::new(CONTROL_PREPARE, 1),
            CONTROL_PREPARED,
            1,
        )
        .expect("host minted an authenticated G1 link");
        control_exchange(
            &control,
            &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(group_pid),
            CONTROL_REGISTERED,
            1,
        )
        .expect("actual host-validated native generation registration");
        let app_link = String::from_utf8(prepared.payload).expect("authenticated app link");
        let (endpoint, token) = parse_app_link(&app_link).expect("host-provisioned identity");
        let mut app = UnixStream::connect(endpoint).expect("bind real registered app link");
        handshake_client(&mut app, &token).expect("real role authentication");
        let bound = loop {
            match updates_rx
                .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
                .expect("native registration event")
            {
                GuardedPrimaryUpdate::Bound(bound) => break bound,
                GuardedPrimaryUpdate::Role(_) => {}
            }
        };
        assert_eq!(bound.attempt(), 1);
        Self {
            control: Some(control),
            host_worker,
            current,
            _updates_rx: updates_rx,
            _bound: bound,
            _app: app,
        }
    }

    fn take_guardian_control(&mut self) -> Arc<Mutex<GenerationControlPeer>> {
        self.control
            .take()
            .expect("sole original Guardian KGC1 writer")
    }

    fn assert_fatal_eof_and_retained_birth(&mut self, group_pid: u32, original: GroupIdentity) {
        let eof = self
            .host_worker
            .join()
            .expect("authenticated KGC1 worker joins")
            .expect_err("Guardian failure closes KGC1 with retained host generation");
        assert!(
            matches!(
                eof,
                RuntimeError::Lifecycle {
                    phase: "macOS guardian generation control read",
                    ref source
                } if source.kind() == io::ErrorKind::UnexpectedEof
                    && source.to_string() == "generation-control peer closed mid-session"
            ),
            "actual EOF must remain a fatal typed KGC1 error: {eof}"
        );
        assert!(
            lock_generation(&self.current)
                .as_ref()
                .is_some_and(|group| group.attempt == 1
                    && group.group_pid == group_pid
                    && group.group_identity == Some(original)),
            "failed Guardian signal and control EOF cannot authorize host CLEAR"
        );
    }
}

fn hold_exact_original_zombies(leader_exit: ChildStdin, member: &mut OwnedChild, group_pid: u32) {
    // The same birth-attested leader is held by the Guardian lease as its
    // exact direct Child. Kill only the member's exact Child object and end
    // the leader through its own stdin; neither a stored numeric PID nor the
    // original PGID grants signal authority to this test.
    member
        .0
        .kill()
        .expect("kill exact original group member Child");
    drop(leader_exit);
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    await_native_zombie(group_pid, deadline);
    await_native_zombie(member.0.id(), deadline);
    let raw = i32::try_from(group_pid).expect("original kernel group");
    assert_eq!(
        getpgid(Some(Pid::from_raw(raw))),
        Err(Errno::ESRCH),
        "Darwin no longer exposes the original zombie leader to getpgid"
    );
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::EPERM),
        "two original unreaped native zombies retain an ambiguous group"
    );
}

fn assert_original_guardian_eperm(
    guardian_worker: &mut OwnedWorker<Result<(), RuntimeError>>,
    signal_released_at: Instant,
    group_pid: i32,
) -> RuntimeError {
    let guardian_error = guardian_worker
        .join()
        .expect("Guardian lease worker joins")
        .expect_err("actual native EPERM must remain a failed Guardian revocation");
    // The stack's D3 error contract: the typed signal-phase EPERM carries
    // the absence owner's reason as context, so the kernel's raw EPERM is the
    // wrapped source rather than `raw_os_error()` of the outer error.
    assert_signal_phase_eperm(&guardian_error);
    assert!(
        matches!(
            guardian_error,
            RuntimeError::Lifecycle {
                phase: "macOS guardian process-group signal",
                ref source
            } if source
                .get_ref()
                .and_then(|context| context.source())
                .and_then(|denied| denied.downcast_ref::<io::Error>())
                .and_then(io::Error::raw_os_error)
                == Some(nix::libc::EPERM)
        ),
        "retain exact original native signal-phase EPERM: {guardian_error}"
    );
    // The test-only fence precedes the real signal. The shipping lease starts
    // its original five-second post-signal deadline AFTER that signal; a
    // premature EPERM return must fail this lower-bound witness.
    assert!(
        signal_released_at.elapsed() >= GUARDIAN_SHUTDOWN_DEADLINE,
        "reconciliation accepted an ambiguous group before the original five-second deadline"
    );
    assert!(
        matches!(
            waitpid(Pid::from_raw(group_pid), Some(WaitPidFlag::WNOHANG)),
            Err(Errno::ECHILD)
        ),
        "Guardian must reap the original direct Child before returning signal-phase EPERM"
    );
    guardian_error
}

// One-shot receipt on the shared `BeforeSignal` test hook: it runs on the
// Guardian worker thread after the call's birth check and before its one
// native group signal. The receipt re-reads the kernel birth itself, so it can
// only fire for the exact original live leader, then holds the real signal
// until the controller has produced the native zombie-only group. It
// substitutes no PID, syscall, return value or clock.
fn install_attested_signal_fence(
    original: GroupIdentity,
    observed: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
) -> TestGroupSignalHookOwner {
    let mut fence = Some((observed, release));
    install_test_group_signal_hook(
        original.pid,
        Box::new(move |point| {
            if point != TestGroupSignalPoint::BeforeSignal {
                return;
            }
            let Some((observed, release)) = fence.take() else {
                return;
            };
            assert!(
                observe_group_identity(original.pid).is_ok_and(|seen| seen == original),
                "test signal observation must match the verified native birth"
            );
            observed.send(()).expect("original group birth checked");
            release
                .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
                .expect("release authentic native group signal");
        }),
    )
}

#[test]
fn native_attested_signal_eperm_causes_kgc1_eof_without_clearing_held_group() {
    let (mut leader, leader_exit) = exiting_leader();
    let pid = leader.0.id();
    let raw = i32::try_from(pid).expect("native group identifier");
    assert!(
        raw > 1 && Pid::from_raw(raw) != getpgrp(),
        "private native group"
    );
    let mut member = running_member(raw);
    assert!(
        getpgid(Some(Pid::from_raw(
            i32::try_from(member.0.id()).expect("member PID")
        ))) == Ok(Pid::from_raw(raw)),
        "second original Child belongs to exactly the same group"
    );
    let original = observe_group_identity(pid).expect("capture original native leader birth");
    assert!(
        validate_guardian_child_group(pid, std::process::id()).is_ok_and(|seen| seen == original),
        "the original birth must belong to this fixture's direct Child"
    );
    let witness = LocalGroupWitness::default();
    witness
        .register_child(pid)
        .expect("local pre-exec birth witness");
    let mut host = NativeKgc1Host::register(pid);

    let lease = GuardedPrimaryLease {
        attempt: 1,
        control: host.take_guardian_control(),
        host_unavailable: Arc::new(AtomicBool::new(false)),
        group_pid: Some(pid),
        group_witness: witness,
        admission: None,
        exec_withheld: false,
        host_closed_registration: false,
        refused_registration_retired: Arc::new(AtomicBool::new(false)),
    };
    let (checked_tx, checked_rx) = mpsc::sync_channel(1);
    let (signal_tx, signal_rx) = mpsc::sync_channel(1);
    let mut guardian_worker = OwnedWorker::new(thread::spawn(move || {
        let _phase = install_attested_signal_fence(original, checked_tx, signal_rx);
        // The shipping supervisor path: the lease owns the exact direct Child
        // and decides a denied group signal only after that Child's reap.
        lease.revoke_with_child(RevocationCause::Shutdown, &mut leader.0)
    }));
    // This guard is declared after the worker so it releases the signal
    // barrier first during test unwinding, before the original Child owners
    // can drop and reap their native group.
    let mut signal_release = SignalRelease(Some(signal_tx));

    checked_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("actual REVOKED ACK and original birth verification before signal");
    // The only forced scheduling cut follows a real, matching birth check.
    hold_exact_original_zombies(leader_exit, &mut member, pid);
    let signal_released_at = Instant::now();
    signal_release.release();
    let guardian_error =
        assert_original_guardian_eperm(&mut guardian_worker, signal_released_at, raw);

    host.assert_fatal_eof_and_retained_birth(pid, original);
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::EPERM),
        "the exact unreaped group member remains a native zombie through EOF"
    );

    // Only the fixture's original Child wait releases the last zombie. A
    // subsequent read-only absence check must then see both native ESRCH.
    assert_eq!(
        member
            .0
            .wait()
            .expect("reap exact original group member")
            .signal(),
        Some(9)
    );
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Err(Errno::ESRCH));
    assert_eq!(kill(Pid::from_raw(-raw), None), Err(Errno::ESRCH));
    terminate_attested_group(original).expect("native dual ESRCH after original Child reaps");
    // Once the group is really gone, the one classifier accepts this very
    // error as the exact signal-phase denial.
    assert_classified_as_denied_signal(original, guardian_error);
}
