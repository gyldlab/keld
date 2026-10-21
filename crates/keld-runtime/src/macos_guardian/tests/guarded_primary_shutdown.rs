// Root supplies an already-built Host artifact. Guardian stdout remains
// exclusively the existing Q/S ACK pipe, never a libtest output stream.
use std::fs::File;
use std::io::{self, Read as _};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::super::{
    GENERATION_CONTROL_POLL, GUARDIAN_SHUTDOWN_DEADLINE, GuardedPrimary, GuardedPrimaryUpdate,
    GuardianBootstrap, RecoveryState, RegisteredPrimaryGeneration, RoleEvent, RuntimeError,
    SUPERVISED_GUARDIAN_ARG, lifecycle_error, lock_generation, lock_recovery,
};
use super::test_support::NativeChildOwner;

struct PersistentPrimaryFixture {
    primary: GuardedPrimary,
    stderr: File,
    stage_root: std::path::PathBuf,
}

impl PersistentPrimaryFixture {
    fn start() -> Self {
        use std::os::unix::fs::MetadataExt as _;
        // Host's existing fixture owner stages the public application and
        // keeps its ordinary HTTP observer alive. Runtime never stages it.
        let receipt = std::env::var_os("KELD_KEL143_FIXTURE_RECEIPT")
            .expect("Root supplies the existing Host fixture artifact JSON");
        let bytes = std::fs::read(receipt).expect("owned staged fixture receipt");
        assert!(
            !bytes.is_empty() && bytes.len() <= 8192,
            "bounded fixture data"
        );
        let data: serde_json::Value = serde_json::from_slice(&bytes).expect("fixture JSON");
        let host = data["stagedHost"]
            .as_str()
            .expect("exact staged Host artifact");
        let stage = std::path::PathBuf::from(data["stageRoot"].as_str().expect("stage root"));
        let entry = data["entryRelative"]
            .as_str()
            .expect("existing public app entry");
        let retained = File::open(stage.join(entry)).expect("retain exact entry identity");
        let identity = retained.metadata().expect("retained entry metadata");
        let stderr = tempfile::tempfile().expect("owned guardian diagnostics");
        let mut command = Command::new(host);
        command
            .arg(SUPERVISED_GUARDIAN_ARG)
            .arg(&stage)
            .arg(entry)
            .arg(identity.dev().to_string())
            .arg(identity.ino().to_string())
            .env_remove("KELD_DEV_LEASE")
            .process_group(0)
            .stderr(Stdio::from(
                stderr.try_clone().expect("diagnostic capture clone"),
            ));
        let bootstrap =
            GuardianBootstrap::spawn_supervised(command).expect("existing supervised bootstrap");
        drop(retained);
        let primary = bootstrap
            .register_guarded_primary_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("real authenticated guardian registration");
        Self {
            primary,
            stderr,
            stage_root: stage,
        }
    }

    fn ready(&mut self) -> u32 {
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        let bound = loop {
            self.primary
                .poll_fatal()
                .expect("healthy real guardian before Bound");
            if let Some(GuardedPrimaryUpdate::Bound(bound)) =
                self.primary.recv_update(GENERATION_CONTROL_POLL)
            {
                break bound;
            }
            assert!(Instant::now() < deadline, "actual G1 Bound deadline");
        };
        assert_eq!(bound.attempt(), 1);
        let pid = self
            .primary
            .group_pid()
            .expect("actual registered G1 group");
        let mut app = bound.into_stream();
        let ready = keld_ipc::codec::encode(&keld_ipc::lifecycle::LifecycleEvent::Ready)
            .expect("canonical lifecycle Ready payload");
        self.primary.arm_recovery();
        keld_ipc::link::write_frame(
            &mut app,
            keld_ipc::FrameKind::Event,
            0,
            keld_ipc::lifecycle::LIFECYCLE_CHANNEL,
            keld_ipc::CorrelationId(0),
            &ready,
        )
        .expect("existing actual Ready event");
        // The host lease also retains its stream; this alias mints no
        // process authority and no independent child/wait owner.
        pid
    }

    fn await_cleared(&mut self) {
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        let mut revoked = false;
        loop {
            self.primary
                .poll_fatal()
                .expect("healthy guardian during real retirement");
            if let Some(update) = self.primary.recv_update(GENERATION_CONTROL_POLL) {
                match update {
                    GuardedPrimaryUpdate::Role(RoleEvent::Revoked { attempt: 1, .. }) => {
                        revoked = true;
                    }
                    GuardedPrimaryUpdate::Bound(bound) => {
                        panic!("closed permit bound G{}", bound.attempt())
                    }
                    GuardedPrimaryUpdate::Role(_) => {}
                }
            }
            let recovery = lock_recovery(&self.primary.recovery_decision);
            if revoked && recovery.cleared_attempt == Some(1) && recovery.expected_attempt == 2 {
                assert!(recovery.release_attempt.is_none());
                break;
            }
            drop(recovery);
            assert!(
                Instant::now() < deadline,
                "actual G1 revoke/Cleared deadline"
            );
        }
        assert!(self.primary.group_pid().is_none());
    }

    fn assert_live_for_budget(&mut self, phase: &str) {
        let guardian = self.primary.guardian_pid();
        let began = Instant::now();
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE + GENERATION_CONTROL_POLL;
        let mut observations = 0_u64;
        // This measures survival while an owned release/EOF is withheld;
        // it is the contract under test, not a synchronization sleep.
        while Instant::now() < deadline {
            self.primary
                .poll_fatal()
                .expect("persistent guardian remains healthy before host EOF");
            assert!(
                self.primary
                    .child
                    .try_wait()
                    .expect("owned guardian status")
                    .is_none()
            );
            assert_eq!(self.primary.guardian_pid(), guardian);
            assert!(self.primary.group_pid().is_none());
            observations += 1;
            if let Some(GuardedPrimaryUpdate::Bound(bound)) =
                self.primary.recv_update(Duration::ZERO)
            {
                panic!("closed native permit bound G{}", bound.attempt());
            }
            std::thread::yield_now();
        }
        println!(
            "KEL143_RUNTIME_LIVE phase={phase} guardian={guardian} observations={observations} elapsed_ns={} group=None",
            began.elapsed().as_nanos()
        );
    }

    fn diagnostics(&mut self) -> String {
        use std::io::Seek as _;
        self.stderr.rewind().expect("rewind owned diagnostics");
        let mut text = String::new();
        (&self.stderr)
            .take(65_536)
            .read_to_string(&mut text)
            .expect("bounded diagnostics");
        text
    }
}

#[test]
#[ignore = "explicit real macOS staged Host guardian shutdown proof"]
fn guarded_primary_attributed_stop_keeps_guardian_live_until_host_eof() {
    let mut fixture = PersistentPrimaryFixture::start();
    let group = fixture.ready();
    println!(
        "KEL143_RUNTIME_OWNED host={} guardian={} g1_group={group}",
        std::process::id(),
        fixture.primary.guardian_pid()
    );
    fixture
        .primary
        .fail_current_generation(1)
        .expect("only actual registered live G1");
    fixture.await_cleared();
    fixture.assert_live_for_budget("runtime-permit-held");
    fixture
        .primary
        .accept_shutdown()
        .expect("actual Q ACK before host tail");
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    while fixture
        .primary
        .worker
        .as_ref()
        .is_some_and(|worker| !worker.is_finished())
    {
        fixture
            .primary
            .poll_fatal()
            .expect("Q-attributed guardian health");
        assert!(
            Instant::now() < deadline,
            "actual cancelled control EOF deadline"
        );
        std::thread::yield_now();
    }
    fixture
        .primary
        .join_worker()
        .expect("attributed resource-free control EOF");
    println!(
        "KEL143_RUNTIME_CONTROL_EOF guardian={} ack={}",
        fixture.primary.guardian_pid(),
        lock_recovery(&fixture.primary.recovery_decision).shutdown_acknowledged
    );
    fixture.assert_live_for_budget("q-stopped-before-host-eof");
    let status = fixture
        .primary
        .shutdown()
        .expect("host EOF and exact guardian reap");
    let diagnostics = fixture.diagnostics();
    println!(
        "KEL143_RUNTIME_REAP guardian={} raw_status={} group={:?}",
        fixture.primary.guardian_pid(),
        status.into_raw(),
        fixture.primary.group_pid()
    );
    assert!(
        status.success(),
        "attributed guardian final status: {status:?}; {diagnostics}"
    );
    assert!(fixture.primary.group_pid().is_none());
    assert!(
        fixture
            .primary
            .child
            .try_wait()
            .expect("retained terminal status")
            .is_some()
    );
    assert!(group > 1);
    assert!(
        !fixture.stage_root.exists(),
        "existing guardian completed its owned stage cleanup"
    );
}

#[test]
fn poll_fatal_rejects_owned_guardian_zero_even_after_shutdown_attribution() {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("owned zero-status health fixture");
    let writer = child.stdin.take().expect("owned liveness writer");
    let ack = child.stdout.take().expect("owned acknowledgment reader");
    let (_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::new(Mutex::new(None)),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: true,
        shutdown_failed: false,
        shutdown_sent: None,
        #[cfg(test)]
        shutdown_denial_fence: None,
    };
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let error = loop {
        if let Err(error) = primary.poll_fatal() {
            break error;
        }
        assert!(
            Instant::now() < deadline,
            "owned zero-status inspection deadline"
        );
        std::thread::yield_now();
    };
    assert!(
        matches!(
            error,
            RuntimeError::GuardianExited {
                exit_code: Some(0),
                ..
            }
        ),
        "attribution hid unexpected guardian zero: {error}"
    );
    assert!(primary.liveness_writer.is_none());
    assert!(
        primary
            .child
            .try_wait()
            .expect("already-reaped status retained")
            .is_some()
    );
}

#[test]
fn shutdown_worker_error_reaps_registered_group_after_zero_guardian_exit() {
    let mut group = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own isolated process group"),
    );
    let group_pid = group.id();
    let raw_pid = i32::try_from(group_pid).expect("group PID fits c_int");
    assert_eq!(
        nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(raw_pid)))
            .expect("registered group leader exists")
            .as_raw(),
        raw_pid,
        "fixture must not signal the test runner's process group"
    );

    let mut guardian = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("owned guardian stand-in exits zero after EOF");
    let writer = guardian.stdin.take().expect("owned liveness writer");
    let ack = guardian.stdout.take().expect("owned acknowledgment reader");
    let current_generation = Arc::new(Mutex::new(Some(RegisteredPrimaryGeneration {
        attempt: 1,
        group_pid,
        group_identity: Some(super::test_support::test_group_identity(group_pid)),
    })));
    let (_tx, updates_rx) = mpsc::channel();
    let mut primary = GuardedPrimary {
        child: guardian,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::clone(&current_generation),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: Some(thread::spawn(|| {
            Err(lifecycle_error(
                "test generation worker failure",
                io::Error::other("injected control-channel failure"),
            ))
        })),
        shutdown_attributed: true,
        shutdown_failed: false,
        shutdown_sent: None,
        #[cfg(test)]
        shutdown_denial_fence: None,
    };

    let error = primary.shutdown().expect_err("worker failure stays fatal");
    let guardian_status = primary
        .child
        .try_wait()
        .expect("owned guardian reaped")
        .expect("guardian exited");
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let mut group_status = group.try_wait().expect("inspect owned group");
    while group_status.is_none() && Instant::now() < deadline {
        thread::yield_now();
        group_status = group.try_wait().expect("observe group retirement");
    }
    let fail_safe_reaped = group_status.is_some();
    if !fail_safe_reaped {
        // This fixture has only one test-owned /bin/sleep group member.
        // Rescue the exact unreaped Child, never a possibly recycled PGID.
        group.kill().expect("kill exact retained test-owned child");
        group.wait().expect("reap exact retained test-owned child");
    }
    assert!(
        guardian_status.success(),
        "test requires zero guardian exit"
    );
    assert!(
        fail_safe_reaped,
        "worker failure left a registered Bun process group alive: {error}"
    );
    assert!(
        lock_generation(&current_generation).is_none(),
        "worker failure retained stale group authority"
    );
    let second = primary.shutdown();
    assert!(
        matches!(second, Err(RuntimeError::Lifecycle { ref source, .. })
            if source.kind() == io::ErrorKind::NotConnected),
        "a previously failed Guardian shutdown cannot return success on retry: first={error}; second={second:?}"
    );
}
