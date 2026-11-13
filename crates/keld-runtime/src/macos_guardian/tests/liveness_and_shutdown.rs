use std::io;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use super::super::process_group::{
    GroupIdentity, observe_group_identity, terminate_attested_group,
};
use super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, HostGuardian, RuntimeError, run_with_registration,
    terminate_registered_group,
};
use super::test_support::{assert_gone, liveness_pipe, long_running_command, test_group_identity};

mod bootstrap_admission;
mod cleanup_authority;
use self::cleanup_authority::OwnedNativeChild;

// The timeout watcher cannot take ownership of a Guardian Child held inside
// a different thread. Keep the original native identity in its rescue input;
// a delayed watcher must never adopt whichever process reuses the same PID.
fn rescue_wedged_guardian(original_birth: GroupIdentity) -> Result<(), String> {
    terminate_attested_group(original_birth).map_err(|error| error.to_string())
}

#[test]
fn registration_failure_still_reaps_the_group() {
    let (reader, writer) = liveness_pipe();
    let pid = Arc::new(Mutex::new(None));
    let pid_for_registration = Arc::clone(&pid);
    let error = run_with_registration(
        long_running_command(),
        reader,
        io::sink(),
        move |child_pid| {
            drop(writer);
            *pid_for_registration.lock().expect("pid lock") = Some(child_pid);
            Err(io::Error::other("registration rejected"))
        },
        || Ok(()),
    )
    .expect_err("registration failure must fail guardian startup");
    assert!(
        error.to_string().contains("registration rejected"),
        "{error}"
    );
    assert_gone(*pid.lock().expect("pid lock"));
}

#[test]
fn post_registration_liveness_bytes_still_revoke_and_reap() {
    let (reader, mut writer) = liveness_pipe();
    let pid = Arc::new(Mutex::new(None));
    let pid_for_registration = Arc::clone(&pid);
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_in_callback = Arc::clone(&revoked);
    let error = run_with_registration(
        long_running_command(),
        reader,
        io::sink(),
        move |child_pid| {
            *pid_for_registration.lock().expect("pid lock") = Some(child_pid);
            std::io::Write::write_all(&mut writer, &[1_u8])?;
            Ok(())
        },
        move || {
            revoked_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .expect_err("post-registration liveness bytes must fail closed");
    assert!(
        error.to_string().contains("carried data instead of EOF"),
        "{error}"
    );
    assert!(
        revoked.load(Ordering::SeqCst),
        "registered resources must be revoked on every fatal liveness result"
    );
    assert_gone(*pid.lock().expect("pid lock"));
}

#[test]
fn revocation_failure_still_reaps_the_group() {
    let (reader, writer) = liveness_pipe();
    let pid = Arc::new(Mutex::new(None));
    let pid_for_registration = Arc::clone(&pid);
    let error = run_with_registration(
        long_running_command(),
        reader,
        io::sink(),
        move |child_pid| {
            *pid_for_registration.lock().expect("pid lock") = Some(child_pid);
            drop(writer);
            Ok(())
        },
        || Err(io::Error::other("revocation rejected")),
    )
    .expect_err("revocation failure must block cleanup success");
    assert!(error.to_string().contains("revocation rejected"), "{error}");
    assert_gone(*pid.lock().expect("pid lock"));
}

#[test]
fn registered_group_fallback_terminates_a_live_leader() {
    let mut command = long_running_command();
    command.process_group(0);
    let mut child = command.spawn().expect("spawn fallback group leader");
    let pid = child.id();
    terminate_registered_group(pid).expect("terminate registered group");
    let status = child.wait().expect("wait fallback group leader");
    assert_eq!(status.signal(), Some(9));
}

#[test]
fn guardian_death_is_typed_and_cannot_leave_the_group() {
    let mut group_command = long_running_command();
    group_command.process_group(0);
    let mut group = group_command.spawn().expect("spawn registered group");
    let group_pid = group.id();

    let mut guardian_command = Command::new("/usr/bin/tail");
    guardian_command
        .args(["-f", "/dev/null"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut guardian = guardian_command.spawn().expect("spawn guardian stand-in");
    let writer = guardian.stdin.take().expect("guardian liveness writer");
    let mut owner =
        HostGuardian::from_registered_parts(guardian, writer, None, test_group_identity(group_pid))
            .expect("register host guardian owner");
    kill(
        Pid::from_raw(i32::try_from(owner.guardian_pid()).expect("guardian pid fits i32")),
        Signal::SIGKILL,
    )
    .expect("kill guardian while host remains live");

    let error = owner
        .wait_fatal()
        .expect_err("guardian death must be a fatal session event");
    assert!(matches!(
        error,
        RuntimeError::GuardianExited {
            group_pid: observed,
            cleanup_error: None,
            ..
        } if observed == group_pid
    ));
    let status = group.wait().expect("wait fail-safe-killed group");
    assert_eq!(status.signal(), Some(9));
    assert_eq!(owner.group_pid(), None);
}

#[test]
fn orderly_shutdown_closes_the_same_liveness_writer() {
    let mut group_command = long_running_command();
    group_command.process_group(0);
    let mut group = group_command.spawn().expect("spawn registered group");
    let group_pid = group.id();
    let group_birth = test_group_identity(group_pid);
    let mut guardian_command = Command::new("/bin/sh");
    guardian_command
        .args(["-c", "read ignored || exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut guardian = guardian_command.spawn().expect("spawn guardian stand-in");
    let writer = guardian.stdin.take().expect("guardian liveness writer");
    let mut owner = HostGuardian::from_registered_parts(guardian, writer, None, group_birth)
        .expect("register host guardian owner");
    let status = owner.shutdown().expect("orderly EOF shutdown");
    assert!(status.success());
    assert_eq!(owner.group_pid(), None);
    let error = owner
        .shutdown()
        .expect_err("terminal owner must not reuse its numeric group id");
    assert!(error.to_string().contains("already terminal"), "{error}");
    let group_status = group.wait().expect("wait orderly-shutdown test group");
    assert_eq!(group_status.signal(), Some(9));
    // Darwin may report EPERM for a killed but unreaped group leader.
    terminate_attested_group(group_birth).expect("confirm exact original group is gone");
}

// A zero-status guardian is not proof that its registered Bun group retired.
#[test]
fn zero_exit_guardian_shutdown_reaps_still_registered_group() {
    let mut cmd = long_running_command();
    cmd.process_group(0);
    let mut group = cmd.spawn().expect("independent registered group");
    let group_pid = group.id();
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "read ignored || exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut guardian = cmd.spawn().expect("successful EOF guardian");
    let writer = guardian.stdin.take().expect("guardian liveness writer");
    let mut owner =
        HostGuardian::from_registered_parts(guardian, writer, None, test_group_identity(group_pid))
            .expect("registered owner");

    let status = owner.shutdown().expect("successful guardian shutdown");
    assert!(status.success());
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut retired = group.try_wait().expect("inspect registered group");
    while retired.is_none() && Instant::now() < deadline {
        thread::yield_now();
        retired = group.try_wait().expect("observe group retirement");
    }
    let group_reaped = retired.is_some();
    if !group_reaped {
        terminate_registered_group(group_pid).expect("test fallback signal");
        group.wait().expect("test fallback reap");
    }
    assert!(
        group_reaped,
        "zero-status guardian stranded its registered group"
    );
    assert_eq!(owner.group_pid(), None);
}

// Drop must not block forever if an otherwise owned guardian ignores host EOF.
// The regression has an independent kill switch and always reaps its test group.
#[test]
fn dropping_wedged_registered_guardian_enforces_shutdown_deadline() {
    let mut group_command = long_running_command();
    group_command.process_group(0);
    let mut group = OwnedNativeChild::new(
        group_command
            .spawn()
            .expect("spawn isolated registered group"),
    );
    let group_pid = group.id();

    let mut guardian_command = long_running_command();
    guardian_command.stdin(Stdio::piped()).process_group(0);
    let mut guardian =
        OwnedNativeChild::new(guardian_command.spawn().expect("spawn wedged guardian"));
    let guardian_birth = observe_group_identity(guardian.id())
        .expect("pin exact native Guardian group while owned Child remains live");
    // All fallible prerequisites must finish while both native Child
    // emergency owners remain armed, before transferring the Guardian Child.
    let group_birth = test_group_identity(group_pid);
    let writer = guardian.stdin.take().expect("sole liveness writer");
    let owner = HostGuardian::from_registered_parts(guardian.take(), writer, None, group_birth)
        .expect("register owned guardian");

    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let drop_worker = thread::spawn(move || {
        drop(owner);
        let _ = done_tx.send(());
    });
    let dropped_before_deadline = done_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE + Duration::from_secs(2))
        .is_ok();
    if !dropped_before_deadline {
        // Failure-first watchdog rescue: never signal a PID that the Drop
        // thread may have concurrently reaped and released for reuse.
        let _ = rescue_wedged_guardian(guardian_birth);
    }
    drop_worker.join().expect("owned shutdown worker joins");

    let deadline = Instant::now() + Duration::from_secs(1);
    let mut group_status = group.try_wait().expect("inspect registered group");
    while group_status.is_none() && Instant::now() < deadline {
        thread::yield_now();
        group_status = group
            .try_wait()
            .expect("observe registered-group retirement");
    }
    let group_reaped_by_drop = group_status.is_some();
    if !group_reaped_by_drop {
        terminate_registered_group(group_pid).expect("test fallback group signal");
        group.wait().expect("test fallback group reap");
    }
    assert!(
        dropped_before_deadline,
        "Drop blocked beyond the guardian shutdown deadline on an unresponsive child"
    );
    assert!(
        group_reaped_by_drop,
        "bounded Drop must terminate and reap the still-registered group"
    );
}

#[test]
fn orderly_shutdown_deadline_kills_guardian_and_registered_group() {
    let mut group_command = long_running_command();
    group_command.process_group(0);
    let mut group = group_command.spawn().expect("spawn timeout group");
    let group_pid = group.id();
    let mut guardian_command = long_running_command();
    guardian_command.stdin(Stdio::piped());
    let mut guardian = guardian_command.spawn().expect("spawn wedged guardian");
    let guardian_pid = guardian.id();
    let writer = guardian.stdin.take().expect("guardian liveness writer");
    let mut owner =
        HostGuardian::from_registered_parts(guardian, writer, None, test_group_identity(group_pid))
            .expect("register timeout owner");

    let error = owner
        .shutdown_until(Instant::now())
        .expect_err("wedged guardian must hit the shutdown deadline");
    assert!(error.to_string().contains("shutdown deadline"), "{error}");
    assert_eq!(owner.group_pid(), None);
    assert_eq!(
        kill(
            Pid::from_raw(i32::try_from(guardian_pid).expect("guardian pid fits i32")),
            None,
        ),
        Err(Errno::ESRCH)
    );
    let group_status = group.wait().expect("wait timeout-killed group");
    assert_eq!(group_status.signal(), Some(9));
}

#[test]
fn rejected_host_registration_waits_the_guardian() {
    let mut guardian_command = Command::new("/bin/sh");
    guardian_command
        .args(["-c", "read ignored || exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut guardian = guardian_command.spawn().expect("spawn guardian stand-in");
    let guardian_pid = guardian.id();
    let writer = guardian.stdin.take().expect("guardian liveness writer");
    let error = HostGuardian::from_registered_parts(
        guardian,
        writer,
        None,
        GroupIdentity::unverified_for_test(0),
    )
    .expect_err("zero group must reject registration");
    assert!(
        error.to_string().contains("process group is zero"),
        "{error}"
    );
    let guardian_pid = i32::try_from(guardian_pid).expect("guardian pid fits i32");
    assert_eq!(kill(Pid::from_raw(guardian_pid), None), Err(Errno::ESRCH));
}

#[test]
fn unrepresentable_group_is_rejected_and_guardian_is_waited() {
    let mut guardian_command = Command::new("/bin/sh");
    guardian_command
        .args(["-c", "read ignored || exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut guardian = guardian_command.spawn().expect("spawn guardian stand-in");
    let guardian_pid = guardian.id();
    let writer = guardian.stdin.take().expect("guardian liveness writer");
    let error = HostGuardian::from_registered_parts(
        guardian,
        writer,
        None,
        GroupIdentity::unverified_for_test(u32::MAX),
    )
    .expect_err("unrepresentable group must reject registration");
    assert!(error.to_string().contains("exceeds c_int"), "{error}");
    let guardian_pid = i32::try_from(guardian_pid).expect("guardian pid fits i32");
    assert_eq!(kill(Pid::from_raw(guardian_pid), None), Err(Errno::ESRCH));
}

#[test]
fn inheritable_writer_rejection_waits_the_guardian() {
    let mut group_command = long_running_command();
    group_command.process_group(0);
    let mut group = group_command.spawn().expect("spawn registered group");
    let group_pid = group.id();
    let group_birth = test_group_identity(group_pid);
    let mut guardian_command = Command::new("/bin/sh");
    guardian_command
        .args(["-c", "read ignored || exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut guardian = guardian_command.spawn().expect("spawn guardian stand-in");
    let guardian_pid = guardian.id();
    let writer = guardian.stdin.take().expect("guardian liveness writer");
    fcntl(&writer, FcntlArg::F_SETFD(FdFlag::empty())).expect("make test writer inheritable");
    let error = HostGuardian::from_registered_parts(guardian, writer, None, group_birth)
        .expect_err("inheritable writer must reject registration");
    assert!(error.to_string().contains("require FD_CLOEXEC"), "{error}");
    let guardian_pid = i32::try_from(guardian_pid).expect("guardian pid fits i32");
    assert_eq!(kill(Pid::from_raw(guardian_pid), None), Err(Errno::ESRCH));
    let group_status = group.wait().expect("wait writer-rejection test group");
    assert_eq!(group_status.signal(), Some(9));
    // Idempotence must be checked after reaping the zombie leader.
    terminate_attested_group(group_birth).expect("confirm exact original group is gone");
}
