//! Real KGR1 host-retained-before-ACK cut through the shipping registration path.
//! A withheld host acknowledgment must never let Bun or its descendant exec.
use std::io::{self, Read as _, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::unistd::{Pid, getpgid};

use super::super::process_group::{
    GroupIdentity, observe_group_identity, terminate_attested_group, validate_guardian_child_group,
};
use super::super::{GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, run};
use super::test_support::{kernel_process_image, verified_real_bun};

mod dual_death;
mod escaped_descendant;
mod observers;
use self::observers::{group_gone, observe_pre_ack_image, read_marker, receive_event};

const ROLE_ENV: &str = "KELD_TEST_KGR1_RETAINED_ACK_ROLE";
const CUT_ENV: &str = "KELD_TEST_KGR1_RETAINED_ACK_SOCKET";
const MARKER_ENV: &str = "KELD_TEST_KGR1_RETAINED_ACK_MARKER";
const BUN_ENV: &str = "KELD_TEST_KGR1_RETAINED_ACK_BUN";
const ESCAPE_ENV: &str = "KELD_TEST_KGR1_DETACHED_DESCENDANT";
const ROLE_TEST: &str = "macos_guardian::tests::retained_ack::kgr1_retained_ack_process_role";

// Invoked ONLY in the Rust test build at bootstrap::register_until's real
// HostGuardian custody point. No alternate acknowledgment path or shipping
// environment feature exists. A disconnected controller denies the ACK.
pub(in crate::macos_guardian) fn hold_before_group_ack(
    group_pid: u32,
    guardian_pid: u32,
) -> io::Result<()> {
    let Some(socket) = std::env::var_os(CUT_ENV) else {
        return Ok(());
    };
    let mut stream = UnixStream::connect(socket)?;
    writeln!(
        stream,
        "RETAINED {group_pid} {guardian_pid} {}",
        std::process::id()
    )?;
    stream.set_nonblocking(true)?;
    await_ack_release_with_hooks(
        &mut stream,
        Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
        || {},
        |_| {},
    )
}

// A real connected and authenticated test cut peer, not a synthetic ACK. The
// native byte callbacks are for a test-owned regression gate only and do not
// exist on the shipping non-test KGR1 registration path.
fn await_ack_release_with_hooks(
    stream: &mut UnixStream,
    deadline: Instant,
    mut on_before_read: impl FnMut(),
    mut on_native_byte: impl FnMut(u8),
) -> io::Result<()> {
    loop {
        if Instant::now() >= deadline {
            return Err(ack_release_deadline_expired());
        }
        on_before_read();
        let mut release = [0_u8; 1];
        match stream.read(&mut release) {
            Ok(1) if release == *b"R" => {
                on_native_byte(release[0]);
                // A real R observed after the held ACK budget cannot turn a
                // denied generation into a successful host-retained child.
                if Instant::now() >= deadline {
                    return Err(ack_release_deadline_expired());
                }
                return Ok(());
            }
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "test custodian left",
                ));
            }
            Ok(1) => {
                on_native_byte(release[0]);
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "test denied ACK",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
            Ok(_) => unreachable!("one-byte release buffer"),
        }
        if Instant::now() >= deadline {
            return Err(ack_release_deadline_expired());
        }
        thread::yield_now();
    }
}

fn ack_release_deadline_expired() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "test ACK fence exceeded deadline")
}

#[cfg(test)]
#[path = "retained_ack/ack_deadline_tests.rs"]
mod ack_deadline_tests;

#[test]
fn kgr1_retained_ack_process_role() {
    let Some(role) = std::env::var_os(ROLE_ENV) else {
        return;
    };
    match role.to_str().expect("owned role is UTF-8") {
        "host" => {
            let mut command = process_role("guardian");
            command.stderr(Stdio::inherit());
            let bootstrap = GuardianBootstrap::spawn(command).expect("real private bootstrap");
            let _registered = bootstrap
                .register_until(Instant::now() + Duration::from_secs(10))
                .expect("complete real KGR1 ACK after controller release");
            thread::park(); // Only a real controller SIGKILL ends this host.
        }
        "guardian" => {
            let marker = std::env::var_os(MARKER_ENV).expect("Bun/descendant marker");
            let bun = std::env::var_os(BUN_ENV).expect("real Bun executable");
            let script = r#"
const { spawn } = require("node:child_process");
const { writeFileSync } = require("node:fs");
const descendant = spawn("/bin/sleep", ["60"], { stdio: "ignore" });
if (!descendant.pid) process.exit(70);
writeFileSync(process.env.KELD_TEST_KGR1_RETAINED_ACK_MARKER, `${process.pid} ${descendant.pid}`);
setInterval(() => {}, 1000);
"#;
            let escaped_script = r#"
const { spawn } = require("node:child_process");
const { writeFileSync } = require("node:fs");
const descendant = spawn("/bin/sleep", ["60"], { stdio: "ignore", detached: true });
if (!descendant.pid) process.exit(70);
writeFileSync(process.env.KELD_TEST_KGR1_RETAINED_ACK_MARKER, `${process.pid} ${descendant.pid}`);
setInterval(() => {}, 1000);
"#;
            let mut command = Command::new(bun);
            command
                .args([
                    "-e",
                    if std::env::var_os(ESCAPE_ENV).is_some() {
                        escaped_script
                    } else {
                        script
                    },
                ])
                .env(MARKER_ENV, marker);
            let result = run(command, io::stdin().lock(), || Ok(()));
            // A direct, native Guardian witness: success after a real ACK and
            // host EOF; rejection before ACK (no child image ever executed).
            let mut stream =
                UnixStream::connect(std::env::var_os(CUT_ENV).expect("controller event socket"))
                    .expect("notify controller of real Guardian completion");
            writeln!(
                stream,
                "GUARDIAN_DONE {} {}",
                std::process::id(),
                if result.is_ok() { "OK" } else { "ERR" }
            )
            .expect("publish Guardian completion");
        }
        other => panic!("unexpected test role {other}"),
    }
}

fn process_role(role: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("native libtest binary"));
    command
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, role)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    command
}

// Only retained original kernel births, not raw test-reported PIDs, can
// authorize rescue. Even a private marker or live getpgid membership cannot
// attest who originally owned a possibly recycled numeric process group.
struct RetainedCut {
    host: Option<Child>,
    host_group: i32,
    host_birth: Option<GroupIdentity>,
    group: Option<i32>,
    group_birth: Option<GroupIdentity>,
    descendant: Option<i32>,
}

impl Drop for RetainedCut {
    fn drop(&mut self) {
        // When possible, signal the original host group while its owned Child
        // is still retained and unreaped. A stale bare Guardian PID is never
        // a substitute for a retained original group birth.
        if let Some(birth) = self.host_birth.take()
            && i32::try_from(birth.pid).ok() == Some(self.host_group)
            && Pid::from_raw(self.host_group) != nix::unistd::getpgrp()
        {
            let _ = terminate_attested_group(birth);
        }
        if let Some(host) = self.host.as_mut() {
            let _ = host.kill();
            let _ = host.wait();
        }
        if let Some(birth) = self.group_birth.take()
            && let Ok(raw) = i32::try_from(birth.pid)
            && self.group == Some(raw)
            && Pid::from_raw(raw) != nix::unistd::getpgrp()
        {
            let _ = terminate_attested_group(birth);
        }
    }
}

/// Spawn the exact retained host and install panic cleanup before any later
/// kernel or Guardian assertions can fail. The original OS birth is retained.
fn spawn_retained_host(socket: &Path, marker: &Path, bun: &Path) -> RetainedCut {
    let mut host_command = process_role("host");
    host_command
        .env(CUT_ENV, socket)
        .env(MARKER_ENV, marker)
        .env(BUN_ENV, bun)
        .stderr(Stdio::inherit())
        .process_group(0);
    let host = host_command
        .spawn()
        .expect("start isolated real host subprocess");
    let host_group = i32::try_from(host.id()).expect("real host PGID fits");
    let mut cut = RetainedCut {
        host: Some(host),
        host_group,
        host_birth: None,
        group: None,
        group_birth: None,
        descendant: None,
    };
    cut.host_birth = Some(
        observe_group_identity(u32::try_from(host_group).expect("positive host PID"))
            .expect("retain original test host group birth"),
    );
    assert_eq!(
        getpgid(Some(Pid::from_raw(host_group))),
        Ok(Pid::from_raw(host_group)),
        "test host must own a separate cleanup process group"
    );
    cut
}

fn native_retained_ack_cut(release_ack: bool) {
    let bun = verified_real_bun();
    let temp = tempfile::tempdir().expect("private retained-ACK fixture");
    let socket: PathBuf = temp.path().join("retained.sock");
    let marker: PathBuf = temp.path().join("bun-and-descendant.pid");
    let listener = UnixListener::bind(&socket).expect("owned test control socket");
    listener.set_nonblocking(true).expect("bounded accept loop");
    let mut cut = spawn_retained_host(&socket, &marker, &bun);
    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut ack_fence, receipt) = receive_event(&listener, deadline);
    let parts: Vec<&str> = receipt.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("RETAINED"), "{receipt}");
    assert_eq!(parts.len(), 4, "{receipt}");
    let group: u32 = parts[1].parse().expect("registered group");
    let guardian: u32 = parts[2].parse().expect("Guardian PID");
    let host: u32 = parts[3].parse().expect("host PID");
    assert_eq!(cut.host.as_ref().expect("host owner").id(), host);
    let group_i32 = i32::try_from(group).expect("native group PID");
    let guardian_i32 = i32::try_from(guardian).expect("native Guardian PID");
    cut.group = Some(group_i32);
    assert_ne!(Pid::from_raw(group_i32), nix::unistd::getpgrp());
    cut.group_birth = Some(
        validate_guardian_child_group(group, guardian)
            .expect("host-retained group is truly Guardian's live direct child"),
    );
    let original_image = observe_pre_ack_image(group_i32, &marker);

    if release_ack {
        ack_fence
            .write_all(b"R")
            .expect("release host's actual KGR1 ACK");
        let (bun_pid, descendant) = read_marker(&marker, deadline);
        cut.descendant = Some(descendant);
        assert_eq!(bun_pid, group_i32, "Bun is the registered group leader");
        assert_ne!(
            kernel_process_image(bun_pid),
            original_image,
            "the real ACK must permit the Bun executable image"
        );
        assert_eq!(
            getpgid(Some(Pid::from_raw(descendant))),
            Ok(Pid::from_raw(group_i32)),
            "real Bun descendant inherited the guarded native process group"
        );
    }
    let killed_at = Instant::now();
    // Keep the original owned Child and its unreaped PID (the host PGID
    // leader) in the guard until Guardian completion and group retirement.
    // This also keeps the exact original PGID reserved across failure cuts.
    let host = cut.host.as_mut().expect("sole host subprocess owner");
    host.kill().expect("SIGKILL real host during held-ACK cut");
    // Host death closes both its liveness writer and KGR1 stream. Before
    // ACK, the pre-exec child must deny and exit without any Bun code.
    let (_finished, witness) = receive_event(&listener, Instant::now() + Duration::from_secs(10));
    let expected = format!(
        "GUARDIAN_DONE {guardian} {}",
        if release_ack { "OK" } else { "ERR" }
    );
    assert_eq!(
        witness, expected,
        "real Guardian must finish its own shutdown"
    );
    group_gone(group_i32, cut.descendant, guardian_i32);
    if !release_ack {
        assert!(
            killed_at.elapsed() < Duration::from_secs(3),
            "host-death pre-exec denial waited for timeout, not KGR1 EOF"
        );
        assert!(!marker.exists(), "host died before ACK yet Bun executed");
    }
    assert_eq!(
        cut.host
            .as_mut()
            .expect("original retained host")
            .wait()
            .expect("reap host child after Guardian/group retirement")
            .signal(),
        Some(9)
    );
    cut.host_birth = None;
    let _ = cut.host.take(); // original live birth is disarmed only after reap
    cut.group = None;
    cut.group_birth = None;
    cut.descendant = None;
}

#[test]
fn native_image_oracle_detects_bun_before_any_js_marker() {
    let bun = verified_real_bun();
    let temp = tempfile::tempdir().expect("owned empty marker fixture");
    let marker = temp.path().join("absent");
    let mut child = Command::new(bun)
        .args(["-e", "setInterval(() => {}, 1000)"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("actual unmarked Bun image");
    let pid = i32::try_from(child.id()).expect("Bun pid");
    let original_image = kernel_process_image(i32::try_from(std::process::id()).expect("PID"));
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let mut changed_image = false;
    while Instant::now() < deadline {
        if kernel_process_image(pid) != original_image {
            changed_image = true;
            break;
        }
        thread::yield_now();
    }
    let _ = child.kill();
    child.wait().expect("reap native Bun image probe");
    assert!(
        !marker.exists(),
        "image oracle test never asks Bun to write a JS marker"
    );
    assert!(
        changed_image,
        "native image oracle must catch exec without JS marker"
    );
}

#[test]
fn real_kgr1_host_sigkill_after_retention_before_ack_denies_bun_exec() {
    native_retained_ack_cut(false);
}

#[test]
fn real_kgr1_ack_release_permits_bun_descendant_then_host_death_reaps_group() {
    native_retained_ack_cut(true);
}
