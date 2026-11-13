//! A failed native CLEAR test may rescue only its original Bun-group birth.
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::{NativeBunGroup, NativeChildOwner, observe_group_identity};

fn owned_cat_group() -> NativeChildOwner {
    NativeChildOwner::new(
        Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("exact test-owned native group"),
    )
}

fn assert_fresh_native_echo(child: &mut NativeChildOwner) {
    child
        .stdin
        .as_mut()
        .expect("live outsider stdin")
        .write_all(b"still-alive\n")
        .expect("native outsider must accept fresh input");
    let stdout = child.stdout.as_mut().expect("live outsider stdout");
    let flags = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read pipe flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .expect("bound native echo receipt");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut reply = [0_u8; 12];
    let mut count = 0;
    while count < reply.len() && Instant::now() < deadline {
        match stdout.read(&mut reply[count..]) {
            Ok(0) => panic!("native outsider died during active challenge"),
            Ok(n) => count += n,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::yield_now();
            }
            Err(error) => panic!("native outsider fresh challenge: {error}"),
        }
    }
    assert_eq!(
        &reply, b"still-alive\n",
        "Bun failure cleanup cannot kill an independent original birth"
    );
}

#[test]
fn stale_clear_fixture_birth_cannot_kill_independent_native_group() {
    let mut outsider = owned_cat_group();
    let raw_outsider = i32::try_from(outsider.id()).expect("positive outsider PID");
    assert_ne!(getpgrp(), Pid::from_raw(raw_outsider));
    assert_eq!(
        getpgid(Some(Pid::from_raw(raw_outsider))),
        Ok(Pid::from_raw(raw_outsider))
    );
    let wrong_birth = observe_group_identity(outsider.id())
        .expect("observe actual outsider birth")
        .altered_birth_for_test();
    let original = owned_cat_group();
    drop(NativeBunGroup {
        leader: original,
        original_birth: Some(wrong_birth),
    });
    assert_fresh_native_echo(&mut outsider);
}

#[test]
fn clear_fixture_retirement_signals_original_native_group_member() {
    let original = owned_cat_group();
    let raw_group = i32::try_from(original.id()).expect("positive original PID");
    let original_birth = observe_group_identity(original.id())
        .expect("retain original group kernel birth before cleanup");
    let mut member = NativeChildOwner::new(
        Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(raw_group)
            .spawn()
            .expect("test owns a second original-group child"),
    );
    assert_eq!(
        getpgid(Some(Pid::from_raw(
            i32::try_from(member.id()).expect("positive group member PID")
        ))),
        Ok(Pid::from_raw(raw_group)),
        "second test-owned member must share the original Bun group"
    );
    drop(NativeBunGroup {
        leader: original,
        original_birth: Some(original_birth),
    });
    assert_eq!(
        member.wait().expect("reap original group member").signal(),
        Some(9),
        "birth-attested fixture cleanup must retire even the original group's second member"
    );
}
