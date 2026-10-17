//! Test-only bounded watchdog must never kill the new owner of a saved PID.
use std::cell::Cell;
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::super::process_group::{
    GroupIdentity, observe_group_identity, terminate_attested_group,
};
use super::rescue_wedged_guardian;

pub(super) struct OwnedNativeChild(Option<Child>);

impl OwnedNativeChild {
    pub(super) fn new(child: Child) -> Self {
        Self(Some(child))
    }

    pub(super) fn take(&mut self) -> Child {
        self.0
            .take()
            .expect("transfer exactly the original owned child")
    }
}

impl std::ops::Deref for OwnedNativeChild {
    type Target = Child;

    fn deref(&self) -> &Child {
        self.0.as_ref().expect("borrow a retained native child")
    }
}

impl std::ops::DerefMut for OwnedNativeChild {
    fn deref_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("mutably borrow a retained child")
    }
}

impl Drop for OwnedNativeChild {
    fn drop(&mut self) {
        let Some(child) = self.0.as_mut() else {
            return;
        };
        // The only emergency signal is through the exact unreaped Child.
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

fn spawn_independent_guardian_standin() -> OwnedNativeChild {
    let child = OwnedNativeChild::new(
        Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("test-owned private Guardian stand-in"),
    );
    let raw = i32::try_from(child.id()).expect("valid native group");
    assert_ne!(getpgrp(), Pid::from_raw(raw));
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Ok(Pid::from_raw(raw)));
    child
}

fn assert_native_echo(child: &mut Child) {
    child
        .stdin
        .as_mut()
        .expect("sentinel stdin")
        .write_all(b"still-alive\n")
        .expect("independent sentinel accepts fresh input");
    let stdout = child.stdout.as_mut().expect("sentinel stdout");
    let current = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read pipe flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(current) | OFlag::O_NONBLOCK),
    )
    .expect("bound native active echo");
    let mut reply = [0_u8; 12];
    let mut received = 0;
    let deadline = Instant::now() + Duration::from_secs(2);
    while received < reply.len() && Instant::now() < deadline {
        match stdout.read(&mut reply[received..]) {
            Ok(0) => panic!("independent Guardian stand-in died"),
            Ok(n) => received += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::yield_now();
            }
            Err(e) => panic!("native sentinel echo: {e}"),
        }
    }
    assert_eq!(
        &reply, b"still-alive\n",
        "watchdog may not kill foreign birth"
    );
}

#[test]
fn delayed_watchdog_cannot_signal_recycled_guardian_pid() {
    let mut outsider = spawn_independent_guardian_standin();
    let wrong_birth = observe_group_identity(outsider.id())
        .expect("original test-owned birth")
        .altered_birth_for_test();
    let refusal = rescue_wedged_guardian(wrong_birth)
        .expect_err("watchdog refuses a recycled native Guardian identity");
    assert!(
        refusal.contains("identity changed"),
        "native birth mismatch must cause specific denial: {refusal}"
    );
    assert_native_echo(&mut outsider);
}

#[test]
fn delayed_watchdog_still_reaps_original_guardian_group() {
    let mut original = spawn_independent_guardian_standin();
    let birth = observe_group_identity(original.id()).expect("actual original group birth");
    rescue_wedged_guardian(birth).expect("watchdog retains original cleanup authority");
    let status = original.wait().expect("reap original native group leader");
    assert_eq!(
        status.signal(),
        Some(9),
        "original watchdog rescue must signal"
    );
}

struct AttestedFailureCutRescue(Cell<Option<GroupIdentity>>);

impl Drop for AttestedFailureCutRescue {
    fn drop(&mut self) {
        // After the independent absence oracle, rescue an old mutant's leak
        // using only its original native birth, never a recycled numeric PID.
        if let Some(birth) = self.0.get() {
            let _ = terminate_attested_group(birth);
        }
    }
}

#[test]
fn early_observation_panic_reaps_child_before_guardian_handoff() {
    let rescue = AttestedFailureCutRescue(Cell::new(None));
    let cut = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let owned = spawn_independent_guardian_standin();
        rescue.0.set(Some(
            observe_group_identity(owned.id()).expect("save original native birth"),
        ));
        panic!("deterministic failure before HostGuardian takes the owned Child");
    }));
    assert!(
        cut.is_err(),
        "the early native observation cut must execute"
    );
    let birth = rescue.0.get().expect("original group rescue identity");
    let raw = i32::try_from(birth.pid).expect("positive original native group");
    assert_eq!(
        kill(Pid::from_raw(-raw), None),
        Err(Errno::ESRCH),
        "a panic before HostGuardian handoff must reap its original native child"
    );
}
