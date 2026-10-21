//! Test-only pre-registration crash cleanup must never adopt a foreign PGID.
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::unistd::{Pid, getpgid, getpgrp};

use super::super::super::process_group::observe_group_identity;
use super::super::super::{GuardedPrimary, RecoveryState};
use super::GuardianCrashFixture;

struct OwnedNativeGroup(Child);

impl Drop for OwnedNativeGroup {
    fn drop(&mut self) {
        // Waited children can release their numeric PID before Drop. Never
        // send a late signal after the owned Child has already been reaped.
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// Active challenge rather than PID existence: a just-signaled/zombie child
/// must never count as a surviving independent native outsider.
fn assert_native_cat_echo(child: &mut Child) {
    child
        .stdin
        .as_mut()
        .expect("live native outsider stdin")
        .write_all(b"still-alive\n")
        .expect("original native outsider must accept an independent challenge");
    let mut stdout = child.stdout.take().expect("native outsider response pipe");
    let flags = fcntl(stdout.as_fd(), FcntlArg::F_GETFL).expect("read owned pipe flags");
    fcntl(
        stdout.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .expect("bound native activity receipt without a blocking read");
    let mut reply = [0_u8; 12];
    let mut filled = 0;
    let deadline = Instant::now() + Duration::from_secs(2);
    while filled < reply.len() && Instant::now() < deadline {
        match stdout.read(&mut reply[filled..]) {
            Ok(0) => panic!("native outsider died before completing fresh challenge"),
            Ok(count) => filled += count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::yield_now();
            }
            Err(error) => panic!("native outsider fresh response: {error}"),
        }
    }
    assert_eq!(
        &reply, b"still-alive\n",
        "unverified test cleanup may not prevent independent native progress"
    );
}

fn synthetic_guardian_without_group_authority() -> GuardedPrimary {
    let mut guardian = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("test-owned stand-in, no authority over foreign groups");
    let ack = guardian
        .stdout
        .take()
        .expect("retained fake acknowledgment pipe");
    let (_, updates_rx) = mpsc::channel();
    GuardedPrimary {
        child: guardian,
        liveness_writer: None,
        quit_ack_reader: ack,
        current_generation: Arc::new(Mutex::new(None)),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: false,
        shutdown_failed: false,
        shutdown_sent: None,
        shutdown_denial_fence: None,
    }
}

#[test]
fn unverified_crash_cleanup_pid_cannot_kill_an_independent_native_group() {
    // A private test receipt may be stale. The raw numeric PGID is not the
    // original group authority even when the current PID leads its own group.
    let outsider = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("separate test-owned native outsider group");
    let mut outsider = OwnedNativeGroup(outsider);
    let outsider_pid = i32::try_from(outsider.0.id()).expect("positive outsider PGID");
    assert_ne!(getpgrp(), Pid::from_raw(outsider_pid));
    assert_eq!(
        getpgid(Some(Pid::from_raw(outsider_pid))),
        Ok(Pid::from_raw(outsider_pid)),
        "native outsider must be its own actual process-group leader"
    );

    // This synthetic stand-in is ONLY to reach the exact original fixture
    // Drop path. It is an unrelated owned Child, not the outsider's Guardian.
    let fixture = GuardianCrashFixture {
        primary: synthetic_guardian_without_group_authority(),
        group_birth: None,
    };
    drop(fixture);
    assert_native_cat_echo(&mut outsider.0);
}

#[test]
fn crash_cleanup_reaps_only_its_previously_attested_original_native_group() {
    let original = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("native original-owned test group");
    let mut original = OwnedNativeGroup(original);
    let raw = i32::try_from(original.0.id()).expect("original group PID");
    assert_ne!(Pid::from_raw(raw), getpgrp());
    let birth = observe_group_identity(original.0.id()).expect("save original live birth");
    drop(GuardianCrashFixture {
        primary: synthetic_guardian_without_group_authority(),
        group_birth: Some(birth),
    });
    let status = original
        .0
        .wait()
        .expect("reap originally attested native group leader");
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&status),
        Some(9),
        "the original attested birth retains cleanup authority"
    );
}

#[test]
fn crash_cleanup_wrong_birth_does_not_signal_a_live_native_outsider() {
    let outsider = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("test-owned wrong-birth native group");
    let mut outsider = OwnedNativeGroup(outsider);
    let birth = observe_group_identity(outsider.0.id())
        .expect("observe current genuine birth")
        .altered_birth_for_test();
    drop(GuardianCrashFixture {
        primary: synthetic_guardian_without_group_authority(),
        group_birth: Some(birth),
    });
    assert_native_cat_echo(&mut outsider.0);
}
