//! Test-only rejected-bootstrap watchdog must not adopt a recycled PID.
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::super::process_group::observe_group_identity;
use super::rescue_wedged_guardian;

struct OwnedNativeGuardian(Child);

impl OwnedNativeGuardian {
    fn spawn() -> Self {
        let child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("spawn isolated, test-owned native Guardian stand-in");
        Self(child)
    }
}

impl Drop for OwnedNativeGuardian {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn fresh_native_echo(child: &mut Child) {
    child
        .stdin
        .as_mut()
        .expect("live sentinel stdin")
        .write_all(b"still-alive\n")
        .expect("sentinel must accept a fresh challenge");
    let stdout = child.stdout.as_mut().expect("live sentinel stdout");
    let flags = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read sentinel pipe flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .expect("bound native process reply");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut response = [0_u8; 12];
    let mut read = 0;
    while read < response.len() && Instant::now() < deadline {
        match stdout.read(&mut response[read..]) {
            Ok(0) => panic!("native outsider died before replying to fresh challenge"),
            Ok(n) => read += n,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::yield_now();
            }
            Err(error) => panic!("native outsider activity: {error}"),
        }
    }
    assert_eq!(
        &response, b"still-alive\n",
        "watchdog must not kill an independently owned Guardian birth"
    );
}

#[test]
fn stale_watchdog_guardian_birth_cannot_kill_a_live_native_outsider() {
    let mut outsider = OwnedNativeGuardian::spawn();
    let raw = i32::try_from(outsider.0.id()).expect("valid owned PID");
    assert_ne!(Pid::from_raw(raw), getpgrp());
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Ok(Pid::from_raw(raw)));
    let wrong_birth = observe_group_identity(outsider.0.id())
        .expect("observe test-owned outsider's true birth")
        .altered_birth_for_test();
    let error = rescue_wedged_guardian(wrong_birth)
        .expect_err("watchdog may not adopt another live Guardian's PID");
    assert!(
        error.contains("identity changed"),
        "wrong kernel birth must fail closed: {error}"
    );
    fresh_native_echo(&mut outsider.0);
}

#[test]
fn watchdog_can_retire_only_its_attested_original_native_group() {
    let mut original = OwnedNativeGuardian::spawn();
    let birth = observe_group_identity(original.0.id())
        .expect("retain native original Guardian birth before ownership transfer");
    rescue_wedged_guardian(birth).expect("original birth authorizes cleanup");
    let status = original.0.wait().expect("reap exactly owned native group");
    assert_eq!(
        status.signal(),
        Some(9),
        "attested watchdog rescue must terminate its original native group"
    );
}
