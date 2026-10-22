//! Test-only rejected-bootstrap watchdog must not adopt a recycled PID.
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::super::RuntimeError;
use super::super::super::process_group::observe_group_identity;
use super::super::super::process_group::reap_pending_guardian;
use super::super::test_support::{NativeChildOwner, long_running_command};
use super::GUARDIAN_SHUTDOWN_DEADLINE;
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

// Unlike Child::try_wait, this waitid WNOWAIT probe cannot itself reap the
// original Child. ECHILD proves the product has already consumed its wait.
#[allow(unsafe_code)] // test-only, nonreaping observation of a retained Child
fn product_has_reaped_original_child(guardian: &NativeChildOwner) -> std::io::Result<bool> {
    // SAFETY: siginfo_t is output storage, and the exact Child stays owned.
    let mut info = unsafe { std::mem::zeroed::<nix::libc::siginfo_t>() };
    // SAFETY: WNOWAIT makes this a nonconsuming probe; P_PID uses the retained
    // Child handle, not a sampled PID/PGID, and grants no signaling authority.
    let result = unsafe {
        nix::libc::waitid(
            nix::libc::P_PID,
            guardian.id() as nix::libc::id_t,
            &raw mut info,
            nix::libc::WEXITED | nix::libc::WNOHANG | nix::libc::WNOWAIT,
        )
    };
    if result == 0 {
        return Ok(false);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(nix::libc::ECHILD) {
        Ok(true)
    } else {
        Err(error)
    }
}

// A borrowed Child retains its single original wait owner while shipping
// rejected-registration cleanup kills and reaps a wedged native Guardian.
#[test]
fn rejected_registration_borrowed_native_child_is_killed_and_reaped_before_return() {
    let mut command = long_running_command();
    command.process_group(0);
    let mut guardian = NativeChildOwner::new(
        command
            .spawn()
            .expect("start actual uncooperative test-owned Guardian group"),
    );
    let birth = observe_group_identity(guardian.id())
        .expect("retain original isolated Guardian group birth before cleanup");
    assert!(
        guardian
            .try_wait()
            .expect("inspect originally owned live Guardian")
            .is_none(),
        "real native Guardian must remain live before rejected cleanup"
    );

    // This product operation retains borrowed Child custody for five seconds.
    // The extra two seconds are only the existing harness observation budget.
    let started = Instant::now();
    let outcome = reap_pending_guardian(&mut guardian);
    let elapsed = started.elapsed();
    // Read-only original-parent wait proof BEFORE any test-side reap. A zombie
    // from product kill-without-wait must remain waitable and fail this oracle.
    let product_reaped = product_has_reaped_original_child(&guardian);
    let original_birth_still_visible = observe_group_identity(birth.pid).ok() == Some(birth);
    let product_status = if matches!(&product_reaped, Ok(true)) {
        guardian.try_wait()
    } else {
        Ok(None)
    };

    // The test owner must clean up the exact same Child even on a RED mutant.
    // Never authorize a signal using a newly observed numeric PID or PGID.
    if !matches!(&product_status, Ok(Some(_))) {
        let _ = guardian.kill();
        let _ = guardian.wait();
    }
    assert!(
        product_reaped.expect("nonreaping waitid of retained original Child"),
        "shipping cleanup returned without reaping the retained original Child"
    );
    assert!(
        matches!(
            &outcome,
            Err(RuntimeError::Lifecycle {
                phase: "macOS guardian rejected-registration cleanup deadline",
                source,
            }) if source.kind() == std::io::ErrorKind::TimedOut
        ),
        "native cleanup must retain the original typed deadline: {outcome:?}"
    );
    assert!(
        elapsed >= GUARDIAN_SHUTDOWN_DEADLINE
            && elapsed <= GUARDIAN_SHUTDOWN_DEADLINE + Duration::from_secs(2),
        "native cleanup exceeded the original five-second plus two-second observation: {elapsed:?}"
    );
    let native_exit = product_status
        .expect("inspect product-retired original Guardian")
        .and_then(|status| status.signal());
    assert_eq!(
        native_exit,
        Some(9),
        "shipping cleanup must SIGKILL and reap its exactly owned native Guardian before returning"
    );
    assert!(
        !original_birth_still_visible,
        "shipping cleanup returned with the original kernel group leader still alive"
    );
}
