//! Start the native descendant only after original group birth and failure
//! cleanup are armed. A failed PID read may not orphan a background shell job.
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::sys::signal::kill;
use nix::unistd::Pid;

use super::super::super::process_group::{
    GroupIdentity, observe_group_identity, terminate_attested_group,
};
use super::TestOwnedGroupChild;

pub(super) struct TestOwnedDescendantGroup {
    // Before original birth validation, the shell is held at a pre-spawn
    // stdin gate. Its owned Child can safely be reaped without a descendant.
    pub(super) leader: Option<TestOwnedGroupChild>,
    pub(super) original_birth: Option<GroupIdentity>,
}

impl Drop for TestOwnedDescendantGroup {
    fn drop(&mut self) {
        // After the first gate is released, an attested birth is mandatory.
        // The Child field is dropped afterward, killing/reaping the leader if
        // still live. No raw descendant PID becomes signal authority.
        if let Some(birth) = self.original_birth.take() {
            let _ = terminate_attested_group(birth);
        }
    }
}

pub(super) fn spawn_held_native_descendant() -> (TestOwnedDescendantGroup, u32, Pid) {
    spawn_held_native_descendant_with_cut(|_| {})
}

fn spawn_held_native_descendant_with_cut(
    on_first_receipt_byte: impl FnOnce(GroupIdentity),
) -> (TestOwnedDescendantGroup, u32, Pid) {
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "IFS= read -r _start || exit 70; /bin/sleep 30 >/dev/null 2>&1 & echo $!; IFS= read -r _exit || exit 71; exit 0",
        ])
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut owned = TestOwnedDescendantGroup {
        leader: Some(TestOwnedGroupChild(
            command.spawn().expect("held original group leader"),
        )),
        original_birth: None,
    };
    let leader = &mut owned.leader.as_mut().expect("owned shell leader").0;
    let group = leader.id();
    // The shell cannot launch /bin/sleep until the controller sends START.
    let birth = observe_group_identity(group).expect("pre-spawn original leader birth");
    owned.original_birth = Some(birth);
    leader
        .stdin
        .as_mut()
        .expect("controlled child-start gate")
        .write_all(b"S\n")
        .expect("permit descendant after original birth is retained");

    let stdout = leader
        .stdout
        .as_mut()
        .expect("bounded native child receipt");
    let flags = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read native receipt flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .expect("bound native child receipt");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut receipt = Vec::with_capacity(16);
    let mut receipt_cut = Some(on_first_receipt_byte);
    loop {
        let mut byte = [0_u8; 1];
        match stdout.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => break,
            Ok(1) => {
                receipt.push(byte[0]);
                assert!(receipt.len() <= 16, "native descendant PID receipt bounded");
                // The shell has spawned its /bin/sleep before echoing its PID.
                // A failing observer here must already own the original
                // process-group birth, even with an incomplete receipt.
                if let Some(cut) = receipt_cut.take() {
                    cut(birth);
                }
            }
            Ok(0) => panic!("native shell exited before descendant PID receipt"),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => panic!("native descendant PID receipt: {error}"),
            Ok(_) => unreachable!("one-byte native PID read"),
        }
        assert!(
            Instant::now() < deadline,
            "native shell must publish its descendant under the original deadline"
        );
        std::thread::yield_now();
    }
    let descendant: i32 = std::str::from_utf8(&receipt)
        .expect("native descendant PID is UTF-8")
        .parse()
        .expect("native descendant PID is numeric");
    assert!(descendant > 1, "valid native child PID");
    (owned, group, Pid::from_raw(descendant))
}

struct FailedReceiptRescue(std::cell::Cell<Option<GroupIdentity>>);

impl Drop for FailedReceiptRescue {
    fn drop(&mut self) {
        // Outside the fixture under test: cleanup only after the original
        // absence oracle, including when deliberately testing a bad mutant.
        if let Some(birth) = self.0.get() {
            let _ = terminate_attested_group(birth);
        }
    }
}

#[test]
fn interrupted_first_pid_byte_reaps_the_original_native_group() {
    let rescue = FailedReceiptRescue(std::cell::Cell::new(None));
    let cut = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = spawn_held_native_descendant_with_cut(|birth| {
            rescue.0.set(Some(birth));
            panic!("controlled first-byte native PID receipt failure");
        });
    }));
    assert!(cut.is_err(), "the native first-byte observer cut must fire");
    let birth = rescue
        .0
        .get()
        .expect("first-byte observer captured the original native group birth");
    let raw = i32::try_from(birth.pid).expect("original group fits positive PID");
    let deadline = Instant::now() + Duration::from_secs(3);
    while kill(Pid::from_raw(-raw), None) != Err(Errno::ESRCH) {
        assert!(
            Instant::now() < deadline,
            "a failed first-byte receipt must not strand the original native group"
        );
        std::thread::yield_now();
    }
    // The attested rescue runs after the independent group-absence oracle.
}

#[test]
fn abandoned_native_descendant_receipt_cut_reaps_original_group() {
    let (owner, group, descendant) = spawn_held_native_descendant();
    drop(owner); // A second, post-receipt failure cut while the leader is held.
    let group_raw = i32::try_from(group).expect("positive original PGID");
    let deadline = Instant::now() + Duration::from_secs(3);
    while kill(descendant, None) != Err(Errno::ESRCH)
        || kill(Pid::from_raw(-group_raw), None) != Err(Errno::ESRCH)
    {
        assert!(
            Instant::now() < deadline,
            "test-owned original group/descendant survived a failed receipt cut"
        );
        std::thread::yield_now();
    }
}
