//! Failure cleanup for the real pre-registration host-death test must not
//! signal an unrelated native process from a saved, possibly recycled PID.
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::super::process_group::observe_group_identity;
use super::PreRegistrationProcessCleanup;

struct OwnedCat(Child);

impl Drop for OwnedCat {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn native_cat(isolated_group: bool) -> OwnedCat {
    let mut command = Command::new("/bin/cat");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if isolated_group {
        command.process_group(0);
    }
    OwnedCat(
        command
            .spawn()
            .expect("test owns disposable native sentinel"),
    )
}

// PID existence alone could count a dead/zombie child as a false success.
// Make the independent sentinel complete a fresh, bounded IO challenge.
fn assert_fresh_native_echo(child: &mut Child) {
    child
        .stdin
        .as_mut()
        .expect("sentinel stdin")
        .write_all(b"still-alive\n")
        .expect("native sentinel must accept fresh input");
    let stdout = child.stdout.as_mut().expect("sentinel stdout");
    let current = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read sentinel pipe flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(current) | OFlag::O_NONBLOCK),
    )
    .expect("bound native sentinel read");
    let mut reply = [0_u8; 12];
    let mut received = 0;
    let deadline = Instant::now() + Duration::from_secs(2);
    while received < reply.len() && Instant::now() < deadline {
        match stdout.read(&mut reply[received..]) {
            Ok(0) => panic!("native sentinel exited before a fresh echo"),
            Ok(count) => received += count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::yield_now();
            }
            Err(error) => panic!("native sentinel failed fresh echo: {error}"),
        }
    }
    assert_eq!(
        &reply, b"still-alive\n",
        "cleanup must not kill an unrelated test-owned native sentinel"
    );
}

#[test]
fn unverified_pre_registration_group_cannot_kill_an_independent_native_group() {
    let mut outsider = native_cat(true);
    let pid = outsider.0.id();
    let raw = i32::try_from(pid).expect("positive test-owned group");
    assert_ne!(getpgrp(), Pid::from_raw(raw));
    assert_eq!(getpgid(Some(Pid::from_raw(raw))), Ok(Pid::from_raw(raw)));

    // The original cleanup killed this unrelated group from its numeric PID.
    // Even a saved identity for the same number cannot adopt a wrong birth.
    let wrong_birth = observe_group_identity(pid)
        .expect("observe live independent native group")
        .altered_birth_for_test();
    drop(PreRegistrationProcessCleanup {
        host: None,
        group_birth: Some(wrong_birth),
    });
    assert_fresh_native_echo(&mut outsider.0);
}

#[test]
fn attested_pre_registration_cleanup_preserves_unrelated_native_children() {
    let mut fake_guardian = native_cat(false);
    let mut fake_descendant = native_cat(false);
    let mut original = native_cat(true);
    // The former fixture stored numeric Guardian/descendant PID kill targets.
    // Those fields are structurally absent now: the real group must retire
    // while unrelated independent children keep completing fresh IO.
    let actual_birth =
        observe_group_identity(original.0.id()).expect("retain exact original native group birth");
    drop(PreRegistrationProcessCleanup {
        host: None,
        group_birth: Some(actual_birth),
    });
    assert_eq!(
        original
            .0
            .wait()
            .expect("reap the owned original group")
            .signal(),
        Some(9),
        "attested cleanup must actually retire its original native group"
    );
    assert_fresh_native_echo(&mut fake_guardian.0);
    assert_fresh_native_echo(&mut fake_descendant.0);
}
