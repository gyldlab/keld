use std::fs::File;
use std::io::{self, Read as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use super::super::process_group::{GroupIdentity, observe_group_identity};
use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::signal::kill;
use nix::unistd::{Pid, pipe};

/// Await the real kernel state in which every remaining member of `group` is
/// a killed but unreaped zombie (`kill(-PGID, 0) == EPERM`). SIGKILL delivery
/// does not mean the member has already exited, so fixtures must observe this
/// state rather than assert it immediately. The three-second bound is only a
/// kill switch; the loop ends on the observed condition.
pub(super) fn await_held_zombie_group(group: Pid) {
    let signal_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match kill(Pid::from_raw(-group.as_raw()), None) {
            Err(Errno::EPERM) => break, // zombie-only process group, not absent
            Err(Errno::ESRCH) => panic!("whole group disappeared before direct Child reap"),
            Ok(()) if Instant::now() < signal_deadline => std::thread::yield_now(),
            probe => panic!("host did not create a held zombie group: {probe:?}"),
        }
    }
}

/// Join a test worker, failing with the fixed `expectation` instead of
/// hanging when it is still running at `deadline`. The loop ends on the
/// observed `is_finished`; the deadline is only a kill switch, so a mutation
/// that leaves the worker blocked turns the test red rather than stalling it.
pub(super) fn join_before<T>(
    worker: thread::JoinHandle<T>,
    deadline: Instant,
    expectation: &'static str,
) -> thread::Result<T> {
    while !worker.is_finished() {
        assert!(Instant::now() < deadline, "{expectation}");
        thread::yield_now();
    }
    worker.join()
}

/// Tests construct a real owned native group before installing a host owner.
pub(super) fn test_group_identity(pid: u32) -> GroupIdentity {
    observe_group_identity(pid).expect("live native group leader has a kernel birth identity")
}

/// Own one actual direct OS child from the instant spawn returns. Tests that
/// exercise fail-closed shutdown may panic before they can install a product
/// owner; their emergency cleanup must not send a stale numeric PID/PGID.
pub(super) struct NativeChildOwner(Option<Child>);

impl NativeChildOwner {
    pub(super) fn new(child: Child) -> Self {
        Self(Some(child))
    }

    /// Transfer to the product owner exactly once, leaving Drop disarmed.
    pub(super) fn take(&mut self) -> Child {
        self.0.take().expect("transfer the original owned Child")
    }
}

impl std::ops::Deref for NativeChildOwner {
    type Target = Child;

    fn deref(&self) -> &Child {
        self.0.as_ref().expect("borrow original owned native Child")
    }
}

impl std::ops::DerefMut for NativeChildOwner {
    fn deref_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("borrow original owned native Child")
    }
}

impl Drop for NativeChildOwner {
    fn drop(&mut self) {
        let Some(child) = self.0.as_mut() else {
            return;
        };
        // No other owner may wait this retained Child. After it has been
        // reaped, avoid a late numeric kill of a potentially recycled PID.
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

pub(super) fn long_running_command() -> Command {
    let mut command = Command::new("/usr/bin/tail");
    command
        .args(["-f", "/dev/null"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

pub(super) fn liveness_pipe() -> (File, File) {
    let (reader, writer) = pipe().expect("create host-liveness pipe");
    fcntl(&reader, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))
        .expect("make liveness reader non-inheritable");
    fcntl(&writer, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))
        .expect("make liveness writer non-inheritable");
    (File::from(reader), File::from(writer))
}

pub(super) fn assert_gone(pid: Option<u32>) {
    let pid = pid.expect("child registered");
    let pid = i32::try_from(pid).expect("pid fits i32");
    assert_eq!(kill(Pid::from_raw(pid), None), Err(Errno::ESRCH));
}

/// Resolve the actual Bun executable for multiple native Guardian crash cuts.
/// No shell stand-in can prove the pre-exec/descendant security contract.
pub(super) fn verified_real_bun() -> std::path::PathBuf {
    let explicit = std::env::var_os("BUN_BIN").map(std::path::PathBuf::from);
    let on_path = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("bun"))
            .find(|candidate| candidate.is_file())
    });
    let installed = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".bun/bin/bun"))
        .filter(|candidate| candidate.is_file());
    let bun = explicit
        .or(on_path)
        .or(installed)
        .expect("real Bun binary required for macOS guardian crash proof");
    let version = Command::new(&bun)
        .arg("--version")
        .output()
        .expect("execute real Bun preflight");
    assert!(
        version.status.success(),
        "real Bun version preflight failed: {version:?}"
    );
    bun
}

/// Actual test-only reporter receipt from `SpawnAdmission`, before the
/// authenticated KGR1/KGC1 registration exchange. Both crash scenarios use
/// this observer; the product still owns the pre-exec child and permit.
pub(super) fn await_spawned_gate(listener: &UnixListener) -> (UnixStream, i32, u32) {
    await_spawned_gate_until(listener, Instant::now() + Duration::from_secs(10), |_| {})
}

/// Private native fixture observation only; no alternate registration owner.
#[derive(Clone, Copy, Debug)]
enum SpawnedGateStage {
    BeforeAccept,
    AfterAccept,
    BeforeByte { received: usize },
    AfterByte { byte: u8 },
}

fn await_spawned_gate_until(
    listener: &UnixListener,
    deadline: Instant,
    mut observe: impl FnMut(SpawnedGateStage),
) -> (UnixStream, i32, u32) {
    loop {
        assert!(
            Instant::now() < deadline,
            "Guardian never reached pre-exec gate before original deadline"
        );
        observe(SpawnedGateStage::BeforeAccept);
        match listener.accept() {
            Ok((mut stream, _)) => {
                observe(SpawnedGateStage::AfterAccept);
                // The native accept can complete after the original caller's
                // ten-second deadline even when its last poll began in time.
                assert!(
                    Instant::now() < deadline,
                    "native gate accept completed after original deadline"
                );
                stream
                    .set_nonblocking(true)
                    .expect("bounded record observer");
                let mut bytes = Vec::with_capacity(80);
                loop {
                    assert!(
                        Instant::now() < deadline,
                        "pre-exec PID receipt exceeded original deadline before native byte"
                    );
                    observe(SpawnedGateStage::BeforeByte {
                        received: bytes.len(),
                    });
                    let mut byte = [0_u8; 1];
                    match stream.read(&mut byte) {
                        Ok(1) if byte[0] == b'\n' => {
                            observe(SpawnedGateStage::AfterByte { byte: byte[0] });
                            // A successful LF read is still a native operation
                            // and must not bypass the original shared clock.
                            assert!(
                                Instant::now() < deadline,
                                "native pre-exec LF completed after original deadline"
                            );
                            break;
                        }
                        Ok(1) => {
                            observe(SpawnedGateStage::AfterByte { byte: byte[0] });
                            bytes.push(byte[0]);
                            assert!(bytes.len() <= 80, "bounded native PID record");
                        }
                        Ok(0) => panic!("truncated native pre-exec PID record"),
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                            ) => {}
                        Err(error) => panic!("read pre-exec PID receipt: {error}"),
                        Ok(_) => unreachable!("one-byte receipt read"),
                    }
                    assert!(
                        Instant::now() < deadline,
                        "native pre-exec byte poll exceeded original deadline"
                    );
                    thread::yield_now();
                }
                let receipt = std::str::from_utf8(&bytes).expect("UTF-8 PID receipt");
                let mut words = receipt.split_whitespace();
                assert_eq!(words.next(), Some("SPAWNED"), "{receipt}");
                let pid: i32 = words.next().expect("process group").parse().expect("PID");
                let guardian: u32 = words.next().expect("Guardian PID").parse().expect("PID");
                assert!(words.next().is_none(), "unexpected gate receipt: {receipt}");
                assert!(pid > 1 && pid != nix::unistd::getpgrp().as_raw());
                assert!(guardian > 1);
                assert!(
                    Instant::now() < deadline,
                    "native pre-exec PID record parsed after original deadline"
                );
                return (stream, pid, guardian);
            }
            Err(source) if source.kind() == io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "Guardian never reached pre-exec gate"
                );
                thread::sleep(Duration::from_millis(3));
            }
            Err(source) => panic!("accept pre-exec PID: {source}"),
        }
    }
}

#[cfg(test)]
#[path = "test_support/spawned_gate_deadline_tests.rs"]
mod spawned_gate_deadline_tests;

/// Native process-image oracle, independent of JS startup and marker writes.
/// Used only with a previously observed, test-owned live child PID.
pub(super) fn kernel_process_image(pid: i32) -> String {
    let output = Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .expect("read native process image");
    assert!(
        output.status.success(),
        "kernel could not inspect owned PID {pid}"
    );
    let image = String::from_utf8(output.stdout).expect("native image path is UTF-8");
    assert!(
        !image.trim().is_empty(),
        "owned PID {pid} has no process image"
    );
    image.trim().to_owned()
}

#[test]
fn early_native_fixture_panic_reaps_original_owned_child() {
    struct Rescue(GroupIdentity);
    impl Drop for Rescue {
        fn drop(&mut self) {
            let _ = super::super::process_group::terminate_attested_group(self.0);
            let _ = nix::sys::wait::waitpid(
                Pid::from_raw(i32::try_from(self.0.pid).expect("positive original PID")),
                None,
            );
        }
    }
    // A test that panics after spawning its native group must not leave it
    // executing. The independent attested rescue below runs only AFTER this
    // absence oracle, so it cannot conceal a broken fixture Drop.
    let child = Command::new("/bin/sleep")
        .arg("30")
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("disposable independent native group");
    let original_birth = test_group_identity(child.id());
    let _failure_only_rescue = Rescue(original_birth);
    let cut = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _owned = NativeChildOwner::new(child);
        panic!("deterministic fixture failure before product ownership");
    }));
    assert!(cut.is_err(), "early native fixture cut must execute");
    assert_ne!(
        observe_group_identity(original_birth.pid).ok(),
        Some(original_birth),
        "a panic before registered custody must reap the exact original native child"
    );
}
