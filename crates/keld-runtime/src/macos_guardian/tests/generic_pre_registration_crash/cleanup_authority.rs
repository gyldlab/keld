//! The crash-cut observer must not signal a recycled Guardian PID after the
//! concurrent host bootstrap has already rejected and reaped that Guardian.
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::super::process_group::observe_group_identity;
use super::stop_guardian_at_pre_registration_gate;

struct OwnedNativeGuardianStandIn(Child);

impl Drop for OwnedNativeGuardianStandIn {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn old_generic_guardian_pid_cannot_kill_independent_native_group() {
    let outsider = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("test-owned native Guardian stand-in");
    let mut outsider = OwnedNativeGuardianStandIn(outsider);
    let pid = outsider.0.id();
    let raw = i32::try_from(pid).expect("valid positive test-owned PID");
    assert_ne!(getpgrp(), Pid::from_raw(raw));
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Ok(Pid::from_raw(raw)));

    // Simulate the same numeric PID with an incompatible previously recorded
    // birth. The original observer's saved-PID helper killed this outsider.
    let wrong_birth = observe_group_identity(pid)
        .expect("native outsider birth")
        .altered_birth_for_test();
    let refusal = stop_guardian_at_pre_registration_gate(wrong_birth)
        .expect_err("a mismatched native Guardian birth cannot authorize signaling");
    assert!(
        refusal.to_string().contains("identity changed"),
        "expected kernel birth mismatch denial: {refusal}"
    );

    let child = &mut outsider.0;
    child
        .stdin
        .as_mut()
        .expect("native stand-in stdin")
        .write_all(b"still-alive\n")
        .expect("native stand-in must accept fresh input");
    let stdout = child.stdout.as_mut().expect("native stand-in stdout");
    let flags = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read native pipe flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .expect("bound independent native echo");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut reply = [0_u8; 12];
    let mut filled = 0;
    while filled < reply.len() && Instant::now() < deadline {
        match stdout.read(&mut reply[filled..]) {
            Ok(0) => panic!("native outsider was killed before fresh challenge"),
            Ok(n) => filled += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::yield_now();
            }
            Err(e) => panic!("native Guardian stand-in echo: {e}"),
        }
    }
    assert_eq!(
        &reply, b"still-alive\n",
        "a stale Guardian PID must not kill the independent test-owned native group"
    );
}
