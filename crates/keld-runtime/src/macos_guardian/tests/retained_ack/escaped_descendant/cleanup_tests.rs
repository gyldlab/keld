//! Test-only native cleanup authority and original-birth sentinel oracles.
use super::super::RetainedCut;
use super::{
    EscapedTestGroup, RuntimeError, observe_group_identity, original_detached_child_birth,
};
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::unistd::{Pid, getpgid, getpgrp};
use std::io::{self, Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct ReapOwnedChild(Child);

impl Drop for ReapOwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn authentic_native_echo(child: &mut Child) -> bool {
    let wrote = child
        .stdin
        .as_mut()
        .expect("retained native child stdin")
        .write_all(b"still-here\n")
        .is_ok();
    let mut stdout = child.stdout.take().expect("native echo receipt");
    let flags = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read native pipe flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .expect("bounded nonblocking echo receipt");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut echoed = [0_u8; 11];
    let mut filled = 0;
    while wrote && filled < echoed.len() && Instant::now() < deadline {
        match stdout.read(&mut echoed[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::yield_now();
            }
            Err(error) => panic!("read native sentinel echo: {error}"),
        }
    }
    wrote && echoed == *b"still-here\n"
}

#[test]
fn old_marker_cannot_authorize_signaling_an_independent_native_group() {
    let temp = tempfile::tempdir().expect("private stale-marker fixture");
    let marker = temp.path().join("escaped.pid");
    let child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("test-owned, independently grouped native sentinel");
    let mut sentinel = ReapOwnedChild(child);
    let pid = i32::try_from(sentinel.0.id()).expect("native sentinel PID");
    assert_ne!(
        getpgrp(),
        Pid::from_raw(pid),
        "never target the test runner's PGID"
    );
    assert_eq!(
        getpgid(Some(Pid::from_raw(pid))),
        Ok(Pid::from_raw(pid)),
        "sentinel must lead its own native process group"
    );
    std::fs::write(&marker, format!("1 {pid}\n"))
        .expect("a stale numeric PID in the private marker");

    // No original escaped process birth was ever captured. The marker is
    // deliberately stale with respect to the independently owned child.
    drop(EscapedTestGroup {
        marker,
        observed: None,
    });

    assert!(
        authentic_native_echo(&mut sentinel.0),
        "an unverified old marker must not kill a different native group"
    );
}

#[test]
fn foreign_marker_birth_cannot_authorize_signaling_native_outsider() {
    let temp = tempfile::tempdir().expect("private wrong-child marker fixture");
    let marker = temp.path().join("foreign.pid");
    let fake_bun = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("unrelated fake Bun parent identity");
    let bun = ReapOwnedChild(fake_bun);
    let outsider = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("separately owned outsider process group");
    let mut outsider = ReapOwnedChild(outsider);
    let bun_pid = i32::try_from(bun.0.id()).expect("original owner numeric PID");
    let outsider_pid = i32::try_from(outsider.0.id()).expect("outsider numeric PID");
    assert_ne!(bun_pid, outsider_pid);
    assert_eq!(
        getpgid(Some(Pid::from_raw(outsider_pid))),
        Ok(Pid::from_raw(outsider_pid)),
        "the outsider must lead an independent native group"
    );
    std::fs::write(&marker, format!("{bun_pid} {outsider_pid}\n"))
        .expect("planted Bun-like marker with unrelated live group");
    let candidate = original_detached_child_birth(bun_pid, outsider_pid);
    let denied = matches!(&candidate, Err(RuntimeError::Lifecycle { source, .. })
        if source.kind() == io::ErrorKind::PermissionDenied);
    drop(EscapedTestGroup {
        marker,
        observed: candidate.ok(),
    });
    assert!(
        denied && authentic_native_echo(&mut outsider.0),
        "a genuine outsider kernel birth must not be accepted from a forged Bun child marker"
    );
}

#[test]
fn unverified_retained_group_cannot_kill_independent_native_sentinel() {
    let child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("test-owned, independently grouped raw-PGID sentinel");
    let mut sentinel = ReapOwnedChild(child);
    let pid = i32::try_from(sentinel.0.id()).expect("real sentinel group leader");
    assert_eq!(getpgid(Some(Pid::from_raw(pid))), Ok(Pid::from_raw(pid)));
    assert_ne!(getpgrp(), Pid::from_raw(pid));
    drop(RetainedCut {
        host: None,
        host_group: 0,
        host_birth: None,
        group: Some(pid),
        group_birth: None,
        descendant: None,
    });
    assert!(
        authentic_native_echo(&mut sentinel.0),
        "a raw test-retained PGID cannot authorize signaling another native group"
    );
}

#[test]
fn marker_cannot_attest_an_unrelated_group_without_real_bun_parent() {
    let child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("live test-owned outsider group");
    let mut outsider = ReapOwnedChild(child);
    let group = outsider.0.id();
    assert_eq!(
        getpgid(Some(Pid::from_raw(
            i32::try_from(group).expect("native PID")
        ))),
        Ok(Pid::from_raw(i32::try_from(group).expect("native PGID")))
    );
    let forged_bun = group; // A process cannot be its own kernel parent.
    let refusal = super::validate_guardian_child_group(group, forged_bun)
        .expect_err("a Bun-writable marker alone may not mint outsider group custody");
    assert!(
        matches!(refusal, super::super::super::super::RuntimeError::Lifecycle { source, .. }
        if source.kind() == io::ErrorKind::PermissionDenied)
    );
    assert!(
        authentic_native_echo(&mut outsider.0),
        "rejected forged marker must leave its native outsider alive"
    );
}

#[test]
fn original_host_group_birth_reaps_a_distinct_same_group_member() {
    let host = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("test-owned native host group leader");
    let host_group = i32::try_from(host.id()).expect("positive original host PGID");
    let mut cleanup = RetainedCut {
        host: Some(host),
        host_group,
        host_birth: None,
        group: None,
        group_birth: None,
        descendant: None,
    };
    cleanup.host_birth = Some(
        observe_group_identity(u32::try_from(host_group).expect("positive group PID"))
            .expect("capture retained original host birth"),
    );
    let member = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(host_group)
        .spawn()
        .expect("test-owned second member in the original host group");
    let mut member = ReapOwnedChild(member);
    assert_eq!(
        getpgid(Some(Pid::from_raw(
            i32::try_from(member.0.id()).expect("member PID")
        ))),
        Ok(Pid::from_raw(host_group)),
        "a second live group member must share the original host group"
    );
    assert_ne!(Pid::from_raw(host_group), getpgrp());
    drop(cleanup);
    assert!(
        !authentic_native_echo(&mut member.0),
        "host-only Child.kill is insufficient; saved host birth must retire its group members"
    );
}

#[test]
fn retained_original_group_birth_still_reaps_its_owned_native_child() {
    let child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("test-owned native group leader");
    let mut owned = ReapOwnedChild(child);
    let pid = owned.0.id();
    assert_ne!(
        getpgrp().as_raw(),
        i32::try_from(pid).expect("positive group ID")
    );
    let birth = observe_group_identity(pid).expect("retain original kernel birth");
    drop(RetainedCut {
        host: None,
        host_group: 0,
        host_birth: None,
        group: Some(i32::try_from(pid).expect("native group ID")),
        group_birth: Some(birth),
        descendant: None,
    });
    assert_eq!(
        owned
            .0
            .wait()
            .expect("reap originally enrolled group leader")
            .signal(),
        Some(9),
        "original attested group retains cleanup authority"
    );
}

#[test]
fn original_host_birth_reaps_an_independent_same_group_member() {
    // A bare Child::kill() on the original host cannot kill a second
    // member of the host PGID. This covers the separate Guardian-like
    // group-wide rescue in RetainedCut::Drop, not only Bun's group.
    let host = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("original, test-owned host group leader");
    let host_group = i32::try_from(host.id()).expect("positive host PGID");
    let mut cut = RetainedCut {
        host: Some(host),
        host_group,
        host_birth: None,
        group: None,
        group_birth: None,
        descendant: None,
    };
    cut.host_birth = Some(
        observe_group_identity(u32::try_from(host_group).expect("positive host birth PID"))
            .expect("retain original host kernel birth before second child spawn"),
    );
    assert_ne!(Pid::from_raw(host_group), getpgrp());
    let member = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(host_group)
        .spawn()
        .expect("independent live member of the original host group");
    let mut member = ReapOwnedChild(member);
    assert_eq!(
        getpgid(Some(Pid::from_raw(
            i32::try_from(member.0.id()).expect("positive member PID")
        ))),
        Ok(Pid::from_raw(host_group)),
        "the second child must really share its original host PGID"
    );
    drop(cut);

    let deadline = Instant::now() + Duration::from_secs(3);
    let ended = loop {
        if let Some(status) = member.0.try_wait().expect("poll owned group member") {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::yield_now();
    };
    assert_eq!(
        ended.and_then(|status| status.signal()),
        Some(9),
        "the host's retained original group birth must kill its second native member"
    );
}

#[test]
fn captured_original_birth_still_reaps_the_owned_native_group() {
    let temp = tempfile::tempdir().expect("private birth-attested rescue fixture");
    let child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("test-owned original group leader");
    let mut owned = ReapOwnedChild(child);
    let pid = owned.0.id();
    assert_ne!(
        getpgrp().as_raw(),
        i32::try_from(pid).expect("positive child PID")
    );
    let birth = observe_group_identity(pid).expect("retain original live kernel birth");
    drop(EscapedTestGroup {
        marker: temp.path().join("not-needed-for-rescue.pid"),
        observed: Some(birth),
    });
    assert_eq!(
        owned
            .0
            .wait()
            .expect("reap killed original native child")
            .signal(),
        Some(9),
        "the previously attested original group must still be cleaned up"
    );
}
