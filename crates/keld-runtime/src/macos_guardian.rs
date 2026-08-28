//! macOS host-death guardian for one supervised Bun process group.
//!
//! This is supervisor cleanup, not App Sandbox containment. The caller owns
//! the guardian process and the only liveness writer. The guardian process
//! calls [`run`] with its reader, a fresh Bun command, and the single owning
//! revocation callback for any registered link resources.

#![deny(unsafe_op_in_unsafe_fn)]

use std::io::{self, Read};
use std::os::fd::AsFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
use nix::sys::signal::{Signal, killpg};
use nix::sys::stat::{SFlag, fstat};
use nix::unistd::Pid;

use crate::RuntimeError;

/// Observable completion of one guardian-owned Bun process group.
#[derive(Debug)]
pub struct GuardianReport {
    leader_pid: u32,
    leader_status: ExitStatus,
}

/// Host-owned handle for one registered private guardian.
///
/// This value owns the guardian process handle and the sole liveness writer.
/// Dropping or calling [`Self::shutdown`] closes that writer so the guardian
/// follows the same EOF cleanup path used for abnormal host death.
#[derive(Debug)]
pub struct HostGuardian {
    child: Child,
    liveness_writer: Option<ChildStdin>,
    group_pid: u32,
}

impl HostGuardian {
    /// Registers a spawned private guardian after it reports its Bun group.
    ///
    /// The writer must be the guardian child's piped stdin and must carry
    /// `FD_CLOEXEC`; this keeps the sole writer in the host. The caller must
    /// independently authenticate the reported group before registration.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error when the group is zero, the writer is
    /// inheritable, or the guardian cannot be inspected. If the guardian has
    /// already exited, its registered group is terminated and
    /// [`RuntimeError::GuardianExited`] is returned.
    pub fn register(
        mut child: Child,
        liveness_writer: ChildStdin,
        group_pid: u32,
    ) -> Result<Self, RuntimeError> {
        if group_pid == 0 {
            return Err(lifecycle_error(
                "macOS guardian host registration",
                io::Error::new(io::ErrorKind::InvalidInput, "process group is zero"),
            ));
        }
        require_close_on_exec(&liveness_writer)?;
        if let Some(status) = child
            .try_wait()
            .map_err(|source| lifecycle_error("macOS guardian process-handle inspection", source))?
        {
            return Err(unexpected_guardian_exit(group_pid, status));
        }
        Ok(Self {
            child,
            liveness_writer: Some(liveness_writer),
            group_pid,
        })
    }

    /// OS process id of the private guardian.
    #[must_use]
    pub fn guardian_pid(&self) -> u32 {
        self.child.id()
    }

    /// Registered Bun process-group leader.
    #[must_use]
    pub const fn group_pid(&self) -> u32 {
        self.group_pid
    }

    /// Polls for an unexpected guardian exit while the host remains live.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error when process inspection fails. A
    /// guardian exit invokes the group fail-safe and returns
    /// [`RuntimeError::GuardianExited`].
    pub fn poll_fatal(&mut self) -> Result<(), RuntimeError> {
        let status = self.child.try_wait().map_err(|source| {
            lifecycle_error("macOS guardian process-handle inspection", source)
        })?;
        if let Some(status) = status {
            self.liveness_writer.take();
            return Err(unexpected_guardian_exit(self.group_pid, status));
        }
        Ok(())
    }

    /// Blocks until the guardian exits while the host still owns its writer.
    ///
    /// A host watcher may use this to turn guardian death into one typed fatal
    /// session event. The registered group fail-safe runs before the event is
    /// returned.
    ///
    /// # Errors
    ///
    /// Always returns [`RuntimeError::GuardianExited`] after the process wait,
    /// or a typed lifecycle error if the wait itself fails.
    pub fn wait_fatal(&mut self) -> Result<(), RuntimeError> {
        let status = self
            .child
            .wait()
            .map_err(|source| lifecycle_error("macOS guardian process-handle wait", source))?;
        self.liveness_writer.take();
        Err(unexpected_guardian_exit(self.group_pid, status))
    }

    /// Performs orderly shutdown through the same EOF cleanup owner.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error if the guardian cannot be waited. A
    /// non-success guardian exit invokes the group fail-safe and returns
    /// [`RuntimeError::GuardianExited`].
    pub fn shutdown(&mut self) -> Result<ExitStatus, RuntimeError> {
        self.liveness_writer.take();
        let status = match self.child.wait() {
            Ok(status) => status,
            Err(source) => {
                let mut failures = vec![lifecycle_error(
                    "macOS guardian orderly-shutdown wait",
                    source,
                )];
                if let Err(error) = terminate_registered_group(self.group_pid) {
                    failures.push(error);
                }
                return Err(collapse_failures(failures));
            }
        };
        if status.success() {
            Ok(status)
        } else {
            Err(unexpected_guardian_exit(self.group_pid, status))
        }
    }
}

impl Drop for HostGuardian {
    fn drop(&mut self) {
        if self.liveness_writer.take().is_none() {
            return;
        }
        match self.child.wait() {
            Ok(status) if status.success() => {}
            Ok(_) | Err(_) => {
                let _ = terminate_registered_group(self.group_pid);
            }
        }
    }
}

impl GuardianReport {
    /// OS process id of the direct Bun child and process-group leader.
    #[must_use]
    pub const fn leader_pid(&self) -> u32 {
        self.leader_pid
    }

    /// Exit status observed by waiting the direct Bun child.
    #[must_use]
    pub const fn leader_status(&self) -> ExitStatus {
        self.leader_status
    }
}

/// Runs one guardian-owned child group until the host-liveness reader reaches
/// EOF, then revokes registered link resources, signals the enrolled process
/// group, and waits the direct child.
///
/// `command` is placed in a new process group before spawn. Descendants inherit
/// that group unless they explicitly break away; a strict-profile consumer
/// must separately prove that attempted `setpgid`/`setsid` escape fails.
/// `liveness` must carry no bytes: EOF is the only accepted host-death signal.
/// The revocation callback runs before group termination, and an error from it
/// blocks successful cleanup even though the group is still terminated and
/// waited.
/// `child_spawned` runs immediately after spawn and before the liveness wait so
/// the host can retain the registered group id for fatal guardian-exit cleanup.
///
/// # Errors
///
/// Returns a typed [`RuntimeError`] when the command is empty, spawning fails,
/// the liveness reader carries bytes or errors, revocation fails, the group
/// cannot be signaled, or the direct child cannot be waited.
pub fn run<R, S, F>(
    mut command: Command,
    mut liveness: R,
    child_spawned: S,
    revoke_registered_resources: F,
) -> Result<GuardianReport, RuntimeError>
where
    R: Read + AsFd,
    S: FnOnce(u32) -> io::Result<()>,
    F: FnOnce() -> io::Result<()>,
{
    if command.get_program().is_empty() {
        return Err(lifecycle_error(
            "macOS guardian bootstrap",
            io::Error::new(io::ErrorKind::InvalidInput, "child program is empty"),
        ));
    }

    validate_liveness_bootstrap(&mut liveness)?;

    command.process_group(0).stdin(Stdio::null());
    let mut child = command.spawn().map_err(RuntimeError::Spawn)?;
    let leader_pid = child.id();

    if let Err(source) = child_spawned(leader_pid) {
        let mut failures = vec![lifecycle_error("macOS guardian child registration", source)];
        if let Err(source) = revoke_registered_resources() {
            failures.push(lifecycle_error(
                "macOS guardian registered-resource revocation",
                source,
            ));
        }
        if let Err(error) = terminate_registered_group(leader_pid) {
            failures.push(error);
        }
        if let Err(source) = child.wait() {
            failures.push(lifecycle_error("macOS guardian direct-child wait", source));
        }
        return Err(collapse_failures(failures));
    }

    let liveness_result = await_host_death(&mut liveness);
    let revocation_result = if liveness_result.is_ok() {
        revoke_registered_resources().map_err(|source| {
            lifecycle_error("macOS guardian registered-resource revocation", source)
        })
    } else {
        Ok(())
    };
    let signal_result = terminate_registered_group(leader_pid);
    let wait_result = child.wait();
    let mut failures = Vec::new();
    if let Err(error) = liveness_result {
        failures.push(error);
    }
    if let Err(error) = revocation_result {
        failures.push(error);
    }
    if let Err(error) = signal_result {
        failures.push(error);
    }
    let leader_status = match wait_result {
        Ok(status) => status,
        Err(source) => {
            failures.push(lifecycle_error("macOS guardian direct-child wait", source));
            return Err(collapse_failures(failures));
        }
    };
    if !failures.is_empty() {
        return Err(collapse_failures(failures));
    }
    Ok(GuardianReport {
        leader_pid,
        leader_status,
    })
}

fn validate_liveness_bootstrap(reader: &mut (impl Read + AsFd)) -> Result<(), RuntimeError> {
    let stat = fstat(reader.as_fd()).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor validation",
            io::Error::from_raw_os_error(source as i32),
        )
    })?;
    let kind = SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT;
    if kind != SFlag::S_IFIFO {
        return Err(lifecycle_error(
            "macOS guardian liveness descriptor validation",
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "host-liveness descriptor is not a pipe",
            ),
        ));
    }

    let descriptor_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFD).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor flags",
            nix_io_error(source),
        )
    })?;
    fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFD(FdFlag::from_bits_truncate(descriptor_flags) | FdFlag::FD_CLOEXEC),
    )
    .map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor isolation",
            nix_io_error(source),
        )
    })?;

    let raw_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFL).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor flags",
            nix_io_error(source),
        )
    })?;
    let flags = OFlag::from_bits_truncate(raw_flags);
    fcntl(reader.as_fd(), FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor probe",
            nix_io_error(source),
        )
    })?;
    let mut byte = [0_u8; 1];
    let probe = reader.read(&mut byte);
    let restore = fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFL(flags & !OFlag::O_NONBLOCK),
    )
    .map_err(nix_io_error);
    if let Err(source) = restore {
        return Err(lifecycle_error(
            "macOS guardian liveness descriptor restore",
            source,
        ));
    }
    match probe {
        Err(source) if source.kind() == io::ErrorKind::WouldBlock => Ok(()),
        Ok(0) => Err(lifecycle_error(
            "macOS guardian liveness descriptor validation",
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "host-liveness pipe has no live writer",
            ),
        )),
        Ok(_) => Err(lifecycle_error(
            "macOS guardian liveness descriptor validation",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "host-liveness pipe carried data instead of EOF",
            ),
        )),
        Err(source) => Err(lifecycle_error(
            "macOS guardian liveness descriptor validation",
            source,
        )),
    }
}

fn await_host_death(reader: &mut impl Read) -> Result<(), RuntimeError> {
    let mut byte = [0_u8; 1];
    match reader.read(&mut byte) {
        Ok(0) => Ok(()),
        Ok(_) => Err(lifecycle_error(
            "macOS guardian host-liveness read",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "host-liveness pipe carried data instead of EOF",
            ),
        )),
        Err(source) => Err(lifecycle_error("macOS guardian host-liveness read", source)),
    }
}

fn terminate_registered_group(group: u32) -> Result<(), RuntimeError> {
    let group = i32::try_from(group).map_err(|_| {
        lifecycle_error(
            "macOS guardian process-group signal",
            io::Error::new(io::ErrorKind::InvalidInput, "process id exceeds c_int"),
        )
    })?;
    match killpg(Pid::from_raw(group), Signal::SIGKILL) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(lifecycle_error(
            "macOS guardian process-group signal",
            io::Error::from_raw_os_error(error as i32),
        )),
    }
}

fn require_close_on_exec(writer: &impl AsFd) -> Result<(), RuntimeError> {
    let raw = fcntl(writer.as_fd(), FcntlArg::F_GETFD).map_err(|source| {
        lifecycle_error(
            "macOS guardian host-liveness writer flags",
            nix_io_error(source),
        )
    })?;
    let flags = FdFlag::from_bits_truncate(raw);
    if flags.contains(FdFlag::FD_CLOEXEC) {
        Ok(())
    } else {
        Err(lifecycle_error(
            "macOS guardian host-liveness writer validation",
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "host-liveness writer is inheritable; require FD_CLOEXEC",
            ),
        ))
    }
}

fn unexpected_guardian_exit(group_pid: u32, status: ExitStatus) -> RuntimeError {
    let cleanup_error = terminate_registered_group(group_pid)
        .err()
        .map(|error| io::Error::other(error.to_string()));
    RuntimeError::GuardianExited {
        group_pid,
        exit_code: status.code(),
        cleanup_error,
    }
}

fn nix_io_error(source: Errno) -> io::Error {
    io::Error::from_raw_os_error(source as i32)
}

fn lifecycle_error(phase: &'static str, source: io::Error) -> RuntimeError {
    RuntimeError::Lifecycle { phase, source }
}

fn collapse_failures(mut failures: Vec<RuntimeError>) -> RuntimeError {
    if failures.len() == 1
        && let Some(error) = failures.pop()
    {
        return error;
    }
    let details = failures
        .into_iter()
        .map(|error| error.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    lifecycle_error("macOS guardian cleanup", io::Error::other(details))
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, TryRecvError};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use nix::sys::signal::{Signal, kill};
    use nix::unistd::pipe;

    use super::*;

    #[test]
    fn empty_program_is_rejected_before_spawn() {
        let (reader, _writer) = liveness_pipe();
        let error = run(Command::new(""), reader, |_| Ok(()), || Ok(()))
            .expect_err("empty guardian command must fail");
        let rendered = error.to_string();
        assert!(rendered.contains("KELD-RUNTIME-003"), "{rendered}");
        assert!(rendered.contains("child program is empty"), "{rendered}");
    }

    #[test]
    fn registration_failure_still_reaps_the_group() {
        let (reader, writer) = liveness_pipe();
        let pid = Arc::new(Mutex::new(None));
        let pid_for_registration = Arc::clone(&pid);
        let error = run(
            long_running_command(),
            reader,
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
    fn liveness_bytes_fail_and_do_not_run_revocation() {
        let (reader, mut writer) = liveness_pipe();
        std::io::Write::write_all(&mut writer, &[1_u8]).expect("write invalid liveness byte");
        let spawned = Arc::new(AtomicBool::new(false));
        let spawned_in_callback = Arc::clone(&spawned);
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_in_callback = Arc::clone(&revoked);
        let error = run(
            long_running_command(),
            reader,
            move |_| {
                spawned_in_callback.store(true, Ordering::SeqCst);
                Ok(())
            },
            move || {
                revoked_in_callback.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .expect_err("liveness bytes must fail closed");
        assert!(
            error.to_string().contains("carried data instead of EOF"),
            "{error}"
        );
        assert!(!spawned.load(Ordering::SeqCst));
        assert!(!revoked.load(Ordering::SeqCst));
    }

    #[test]
    fn liveness_without_a_writer_fails_before_spawn() {
        let (reader, writer) = liveness_pipe();
        drop(writer);
        let spawned = Arc::new(AtomicBool::new(false));
        let spawned_in_callback = Arc::clone(&spawned);
        let error = run(
            long_running_command(),
            reader,
            move |_| {
                spawned_in_callback.store(true, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
        )
        .expect_err("a forged bootstrap without a host writer must fail");
        assert!(error.to_string().contains("no live writer"), "{error}");
        assert!(!spawned.load(Ordering::SeqCst));
    }

    #[test]
    fn non_pipe_liveness_fails_before_spawn() {
        let file = tempfile::tempfile().expect("temporary regular file");
        let spawned = Arc::new(AtomicBool::new(false));
        let spawned_in_callback = Arc::clone(&spawned);
        let error = run(
            long_running_command(),
            file,
            move |_| {
                spawned_in_callback.store(true, Ordering::SeqCst);
                Ok(())
            },
            || Ok(()),
        )
        .expect_err("a forged regular-file bootstrap must fail");
        assert!(error.to_string().contains("is not a pipe"), "{error}");
        assert!(!spawned.load(Ordering::SeqCst));
    }

    #[test]
    fn initially_nonblocking_liveness_is_normalized_to_wait_for_eof() {
        let (reader, writer) = liveness_pipe();
        let flags = fcntl(&reader, FcntlArg::F_GETFL).expect("read liveness flags");
        fcntl(
            &reader,
            FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
        )
        .expect("make liveness reader initially nonblocking");
        let (spawned_tx, spawned_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = run(
                long_running_command(),
                reader,
                move |_| {
                    spawned_tx.send(()).map_err(io::Error::other)?;
                    Ok(())
                },
                || Ok(()),
            );
            done_tx.send(result).expect("report guardian result");
        });
        spawned_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian child registered");
        assert!(
            matches!(done_rx.try_recv(), Err(TryRecvError::Empty)),
            "guardian must still own the child while a live writer exists"
        );

        drop(writer);
        let report = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian finishes after EOF")
            .expect("guardian EOF cleanup succeeds");
        assert_eq!(report.leader_status().signal(), Some(9));
        worker.join().expect("guardian worker joins");
    }

    #[test]
    fn revocation_failure_still_reaps_the_group() {
        let (reader, writer) = liveness_pipe();
        let pid = Arc::new(Mutex::new(None));
        let pid_for_registration = Arc::clone(&pid);
        let error = run(
            long_running_command(),
            reader,
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
        let mut owner = HostGuardian::register(guardian, writer, group_pid)
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
    }

    #[test]
    fn orderly_shutdown_closes_the_same_liveness_writer() {
        let mut guardian_command = Command::new("/bin/sh");
        guardian_command
            .args(["-c", "read ignored || exit 0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut guardian = guardian_command.spawn().expect("spawn guardian stand-in");
        let writer = guardian.stdin.take().expect("guardian liveness writer");
        let mut owner = HostGuardian::register(guardian, writer, u32::MAX)
            .expect("register host guardian owner");
        let status = owner.shutdown().expect("orderly EOF shutdown");
        assert!(status.success());
    }

    fn long_running_command() -> Command {
        let mut command = Command::new("/usr/bin/tail");
        command
            .args(["-f", "/dev/null"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        command
    }

    fn liveness_pipe() -> (File, File) {
        let (reader, writer) = pipe().expect("create host-liveness pipe");
        fcntl(&reader, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))
            .expect("make liveness reader non-inheritable");
        fcntl(&writer, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))
            .expect("make liveness writer non-inheritable");
        (File::from(reader), File::from(writer))
    }

    fn assert_gone(pid: Option<u32>) {
        let pid = pid.expect("child registered");
        let pid = i32::try_from(pid).expect("pid fits i32");
        assert_eq!(kill(Pid::from_raw(pid), None), Err(Errno::ESRCH));
    }
}
