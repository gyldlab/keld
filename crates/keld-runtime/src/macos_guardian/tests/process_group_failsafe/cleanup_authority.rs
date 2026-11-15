//! An old descendant PID/PGID pair is not a process-group birth authority.
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::super::process_group::observe_group_identity;
use super::native_descendant::TestOwnedDescendantGroup;

struct OwnedNativeGroup(Child);

impl Drop for OwnedNativeGroup {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn stale_descendant_cleanup_cannot_kill_an_independent_native_group() {
    let outsider = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("disposable native outsider group");
    let mut outsider = OwnedNativeGroup(outsider);
    let raw = i32::try_from(outsider.0.id()).expect("positive native group");
    let pid = Pid::from_raw(raw);
    assert_ne!(getpgrp(), pid);
    assert_eq!(getpgid(Some(pid)), Ok(pid));

    // The original rescue helper trusted only the group/member PID pair.
    // The new owner rejects the outsider's live PID with a mismatched birth.
    let wrong_birth = observe_group_identity(outsider.0.id())
        .expect("original live outsider birth")
        .altered_birth_for_test();
    drop(TestOwnedDescendantGroup {
        leader: None,
        original_birth: Some(wrong_birth),
    });

    let child = &mut outsider.0;
    child
        .stdin
        .as_mut()
        .expect("native outsider stdin")
        .write_all(b"still-alive\n")
        .expect("native outsider must accept fresh challenge");
    let stdout = child.stdout.as_mut().expect("native outsider stdout");
    let flags = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read owned pipe flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .expect("bound native outsider echo");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut reply = [0_u8; 12];
    let mut filled = 0;
    while filled < reply.len() && Instant::now() < deadline {
        match stdout.read(&mut reply[filled..]) {
            Ok(0) => panic!("native outsider died before fresh echo"),
            Ok(n) => filled += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::yield_now();
            }
            Err(e) => panic!("native outsider echo: {e}"),
        }
    }
    assert_eq!(
        &reply, b"still-alive\n",
        "saved descendant PGID must never kill an unrelated native group"
    );
}
