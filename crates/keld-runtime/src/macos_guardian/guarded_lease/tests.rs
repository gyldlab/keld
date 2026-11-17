//! Local lease cleanup after a failed generation-registration acknowledgment.
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, killpg};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::process_group::{LocalGroupWitness, observe_group_identity};
use super::super::{
    CONTROL_REGISTERED, CONTROL_SPAWNED, GenerationControlPeer, GenerationControlReader,
    GenerationControlRecord, GenerationLease, RevocationCause, read_generation_control,
    write_generation_control,
};
use super::GuardedPrimaryLease;

const ROLE: &str = "KELD_GUARDED_ACK_TEST_ROLE";
const ROOT: &str = "KELD_GUARDED_ACK_TEST_ROOT";
const SELECTOR: &str = "macos_guardian::guarded_lease::tests::native_role";
const BOUND: Duration = Duration::from_secs(5);

#[path = "tests/native_activity.rs"]
mod native_activity;
#[path = "tests/own_child_reap.rs"]
mod own_child_reap;

#[path = "tests/real_eperm_eof.rs"]
mod real_eperm_eof;

#[test]
fn native_role() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let root = PathBuf::from(std::env::var(ROOT).expect("fixture socket directory"));
    let _descendant = if role == "leader" {
        Some(
            role_command("descendant", &root)
                .spawn()
                .expect("native descendant"),
        )
    } else {
        assert_eq!(role, "descendant", "recognized fixture role");
        None
    };
    let listener =
        UnixListener::bind(root.join(format!("{role}.sock"))).expect("private activity observer");
    for connection in listener.incoming() {
        let mut stream = connection.expect("activity connection");
        stream
            .set_read_timeout(Some(BOUND))
            .expect("bounded activity read");
        stream
            .set_write_timeout(Some(BOUND))
            .expect("bounded activity response");
        let mut nonce = [0; 32];
        if stream.read_exact(&mut nonce).is_err() {
            continue;
        }
        if stream.write_all(&std::process::id().to_be_bytes()).is_err() {
            continue;
        }
        let _ = stream.write_all(&nonce);
    }
}

fn role_command(role: &str, root: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .args(["--exact", SELECTOR, "--nocapture"])
        .env(ROLE, role)
        .env(ROOT, root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    command
}

fn fresh_response(socket: &Path) -> io::Result<u32> {
    let mut nonce = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut nonce)?;
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.set_write_timeout(Some(Duration::from_millis(250)))?;
    stream.write_all(&nonce)?;
    let mut response = [0; 36];
    stream.read_exact(&mut response)?;
    if response[4..] != nonce {
        return Err(io::Error::other(
            "activity response did not match fresh challenge",
        ));
    }
    Ok(u32::from_be_bytes(
        response[..4].try_into().expect("fixed PID width"),
    ))
}

struct NativeGroup {
    leader: Child,
    root: tempfile::TempDir,
}

impl NativeGroup {
    fn start() -> Self {
        let root = tempfile::tempdir().expect("private fixture root");
        let leader = role_command("leader", root.path())
            .process_group(0)
            .spawn()
            .expect("isolated native leader");
        let fixture = Self { leader, root };
        let deadline = Instant::now() + BOUND;
        for role in ["leader", "descendant"] {
            let socket = fixture.socket(role);
            let pid = loop {
                if let Ok(pid) = fresh_response(&socket) {
                    break pid;
                }
                assert!(
                    Instant::now() < deadline,
                    "{role} never answered a fresh challenge"
                );
                thread::yield_now();
            };
            let actual = getpgid(Some(Pid::from_raw(
                i32::try_from(pid).expect("fixture PID"),
            )))
            .expect("native process group");
            assert_eq!(
                actual.as_raw(),
                i32::try_from(fixture.leader.id()).expect("leader PID")
            );
        }
        fixture
    }

    fn socket(&self, role: &str) -> PathBuf {
        self.root.path().join(format!("{role}.sock"))
    }

    // The retained original leader Child is deliberately unreaped until
    // NativeGroup::Drop. Query the OS process-group census, not the observer
    // sockets: a process can close its listener and remain fully executable.
    // Darwin reports SIGKILLed, unreaped group members as Z (zombies).
    fn has_executing_group_members(&self) -> io::Result<bool> {
        let group = self.leader.id().to_string();
        let output = Command::new("/bin/ps")
            .args(["-g", group.as_str(), "-o", "stat="])
            .output()?;
        Self::parse_group_census(&output)
    }

    fn parse_group_census(output: &std::process::Output) -> io::Result<bool> {
        if !output.status.success() {
            // The original leader Child remains unreaped for this entire
            // observation. Even if ps exits by SIGKILL with no output, that
            // says nothing about the native group's execution. Fail closed
            // for EVERY non-success status rather than inferring absence.
            return Err(io::Error::other(format!(
                "native group census denied: status={:?}, stderr_bytes={}",
                output.status.code(),
                output.stderr.len()
            )));
        }
        let statuses = std::str::from_utf8(&output.stdout)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut observed = false;
        let mut executing = false;
        for state in statuses
            .lines()
            .map(str::trim)
            .filter(|state| !state.is_empty())
        {
            observed = true;
            executing |= !state.starts_with('Z');
        }
        if !observed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "successful native process-group census returned no process-state rows",
            ));
        }
        Ok(executing)
    }

    fn assert_stopped(&self) {
        self.assert_stopped_with_probe_until(fresh_response, Instant::now() + BOUND);
    }

    fn assert_stopped_with_probe_until(
        &self,
        mut activity: impl FnMut(&Path) -> io::Result<u32>,
        deadline: Instant,
    ) {
        for role in ["leader", "descendant"] {
            let socket = self.socket(role);
            loop {
                match activity(&socket) {
                    Ok(_) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::ConnectionRefused
                                | io::ErrorKind::ConnectionReset
                                | io::ErrorKind::UnexpectedEof
                                | io::ErrorKind::BrokenPipe
                                | io::ErrorKind::NotConnected
                        ) || error.raw_os_error() == Some(nix::libc::EINVAL) =>
                    {
                        // A Darwin peer disappearing during a connect/read
                        // can return EINVAL, not solely ConnectionRefused.
                        // ENOTCONN (NotConnected) comes only from the nonce
                        // WRITE after the listener dies mid-send: XNU
                        // uipc_send checks SS_ISCONNECTED before
                        // SS_CANTSENDMORE. A read after peer death is EOF.
                        // Accept it ONLY after the independent kernel group
                        // census proves that no original-group member runs.
                        if !self
                            .has_executing_group_members()
                            .expect("independent native process-group activity observer")
                        {
                            break;
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock
                                | io::ErrorKind::TimedOut
                                | io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => panic!("{role} activity observer failed: {error}"),
                }
                assert!(
                    Instant::now() < deadline,
                    "{role} still executes or native group has running members after product revoke, before fixture cleanup"
                );
                thread::yield_now();
            }
        }
    }
}

impl Drop for NativeGroup {
    fn drop(&mut self) {
        // The direct leader is deliberately not reaped until this owned fallback.
        let group = i32::try_from(self.leader.id()).expect("fixture group PID");
        assert!(
            group > 1 && group != getpgrp().as_raw(),
            "isolated fixture group"
        );
        let _ = killpg(Pid::from_raw(group), Signal::SIGKILL);
        let _ = self.leader.wait();
    }
}

#[derive(Clone, Copy)]
enum Reply {
    Missing,
    Truncated,
    WrongPid,
    Valid,
}

fn failed_exchange_keeps_cleanup(reply: Reply) {
    let group = NativeGroup::start();
    let pid = group.leader.id();
    let (guardian, mut host) = UnixStream::pair().expect("generation-control pair");
    guardian
        .set_read_timeout(Some(Duration::from_millis(20)))
        .expect("control read poll");
    guardian
        .set_write_timeout(Some(Duration::from_millis(20)))
        .expect("control write poll");
    host.set_read_timeout(Some(BOUND))
        .expect("fault peer bound");
    host.set_write_timeout(Some(BOUND))
        .expect("fault reply bound");
    let peer = thread::spawn(move || {
        let request = read_generation_control(&mut host).expect("actual SPAWNED request");
        assert_eq!(
            (request.kind, request.attempt, request.pid),
            (CONTROL_SPAWNED, 1, pid)
        );
        match reply {
            Reply::Missing => {}
            Reply::Truncated => host.write_all(b"KGC1").expect("partial ACK"),
            Reply::WrongPid | Reply::Valid => {
                let ack_pid = if matches!(reply, Reply::WrongPid) {
                    pid + 1
                } else {
                    pid
                };
                write_generation_control(
                    &mut host,
                    &GenerationControlRecord::new(CONTROL_REGISTERED, 1).with_pid(ack_pid),
                )
                .expect("fault-controlled ACK");
            }
        }
        // Close before revocation: cleanup must still run when its peer is unavailable.
    });
    let mut lease = GuardedPrimaryLease {
        attempt: 1,
        control: Arc::new(Mutex::new(GenerationControlPeer {
            stream: guardian,
            reader: GenerationControlReader::new(),
        })),
        host_unavailable: Arc::new(AtomicBool::new(false)),
        group_pid: None,
        group_witness: LocalGroupWitness::default(),
        admission: None,
        exec_withheld: false,
        host_closed_registration: false,
        refused_registration_retired: Arc::new(AtomicBool::new(false)),
    };
    let admission = lease.child_spawned(pid, 1);
    peer.join().expect("fault peer joins");
    assert_eq!(
        admission.is_ok(),
        matches!(reply, Reply::Valid),
        "ACK validation remains enforced"
    );
    let cleanup = lease.revoke(RevocationCause::AdmissionFailed);
    assert!(
        cleanup.is_err(),
        "closed control peer must remain an observable error"
    );
    group.assert_stopped();
}

#[test]
fn missing_registration_ack_still_retires_native_group() {
    failed_exchange_keeps_cleanup(Reply::Missing);
}

#[test]
fn partial_registration_ack_still_retires_native_group() {
    failed_exchange_keeps_cleanup(Reply::Truncated);
}

#[test]
fn invalid_registration_ack_still_retires_native_group() {
    failed_exchange_keeps_cleanup(Reply::WrongPid);
}

#[test]
fn valid_registration_ack_retirement_control() {
    failed_exchange_keeps_cleanup(Reply::Valid);
}

/// Local Guardian cleanup uses the reporter's original kernel birth, even
/// with no usable host-control peer. The fallback may not trust `group_pid`.
fn locally_enrolled_lease(pid: u32, witness: LocalGroupWitness) -> GuardedPrimaryLease {
    let (stream, host) = UnixStream::pair().expect("private offline control pair");
    drop(host);
    GuardedPrimaryLease {
        attempt: 1,
        control: Arc::new(Mutex::new(GenerationControlPeer {
            stream,
            reader: GenerationControlReader::new(),
        })),
        host_unavailable: Arc::new(AtomicBool::new(true)),
        group_pid: Some(pid),
        group_witness: witness,
        admission: None,
        exec_withheld: false,
        host_closed_registration: false,
        refused_registration_retired: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn local_guardian_revocation_reaps_attested_native_descendant_group() {
    let group = NativeGroup::start();
    let pid = group.leader.id();
    let witness = LocalGroupWitness::default();
    witness
        .register_child(pid)
        .expect("kernel proves owned direct leader before registration");
    locally_enrolled_lease(pid, witness)
        .revoke(RevocationCause::Shutdown)
        .expect("Guardian-side birth-attested revocation succeeds without host");
    group.assert_stopped(); // Product gone before fixture Drop rescue.
}

#[test]
fn local_guardian_revocation_cannot_kill_foreign_group_after_birth_change() {
    let group = NativeGroup::start();
    let pid = group.leader.id();
    let witness = LocalGroupWitness::default();
    let true_birth = observe_group_identity(pid).expect("test-owned group leader live");
    witness.seed_for_test(true_birth.altered_birth_for_test());
    let error = locally_enrolled_lease(pid, witness)
        .revoke(RevocationCause::AdmissionFailed)
        .expect_err("different-birth Guardian group may not be signaled");
    // Independent native activity from both original leader and its child,
    // observed BEFORE the owned fixture's Drop invokes emergency cleanup.
    let leader_alive = fresh_response(&group.socket("leader")).is_ok();
    let descendant_alive = fresh_response(&group.socket("descendant")).is_ok();
    assert!(
        error.to_string().contains("identity changed"),
        "wrong reason for refusal: {error}"
    );
    assert!(
        leader_alive && descendant_alive,
        "refused Guardian revocation signaled an unrelated live process group"
    );
}
