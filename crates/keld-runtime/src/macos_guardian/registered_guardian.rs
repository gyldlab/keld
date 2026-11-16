//! Registered guardian host-side lifecycle and owned shutdown handling.
use super::process_group::{GroupIdentity, observe_group_identity, terminate_attested_group};
use super::{
    Child, ChildStdin, ChildStdout, ExitStatus, GUARDIAN_SHUTDOWN_DEADLINE, GuardianReport,
    Instant, RuntimeError, SUPERVISED_QUIT_ACCEPTED, Write, collapse_failures, io, lifecycle_error,
    read_supervised_quit_ack, reject_host_registration, require_close_on_exec,
    set_liveness_nonblocking,
};

/// Host-owned handle for one registered private guardian.
///
/// This value owns the guardian process handle and the sole liveness writer.
/// Dropping or calling [`Self::shutdown`] closes that writer so the guardian
/// follows the same EOF cleanup path used for abnormal host death.
#[derive(Debug)]
pub struct HostGuardian {
    child: Child,
    liveness_writer: Option<ChildStdin>,
    quit_ack_reader: Option<ChildStdout>,
    group: Option<GroupIdentity>,
}

impl HostGuardian {
    pub(super) fn from_registered_parts(
        mut child: Child,
        liveness_writer: ChildStdin,
        quit_ack_reader: Option<ChildStdout>,
        group: GroupIdentity,
    ) -> Result<Self, RuntimeError> {
        if let Err(error) = require_same_registered_group(group) {
            // The bootstrap already obtained the original kernel birth. If
            // the leader exited between its proof and constructing this
            // handle, close Guardian custody and still clean its surviving
            // descendants. terminate_attested_group itself refuses a live
            // mismatched/reused identity; no unverified PID may be signaled.
            return Err(reject_proven_registration(
                child,
                liveness_writer,
                group,
                error,
            ));
        }
        if let Err(error) = require_close_on_exec(&liveness_writer) {
            return Err(reject_proven_registration(
                child,
                liveness_writer,
                group,
                error,
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => return Err(unexpected_guardian_exit(group, status)),
            Ok(None) => {}
            Err(source) => {
                let error = lifecycle_error("macOS guardian process-handle inspection", source);
                return Err(reject_proven_registration(
                    child,
                    liveness_writer,
                    group,
                    error,
                ));
            }
        }
        Ok(Self {
            child,
            liveness_writer: Some(liveness_writer),
            quit_ack_reader,
            group: Some(group),
        })
    }

    /// OS process id of the private guardian.
    #[must_use]
    pub fn guardian_pid(&self) -> u32 {
        self.child.id()
    }

    /// Registered Bun process-group leader.
    #[must_use]
    pub const fn group_pid(&self) -> Option<u32> {
        match self.group {
            Some(group) => Some(group.pid),
            None => None,
        }
    }

    /// Make one already-validated live enrollment stale only for native
    /// negative-control tests. Production registration remains birth-checked.
    #[cfg(test)]
    pub(super) fn alter_group_birth_for_test(&mut self) -> GroupIdentity {
        let altered = self
            .group
            .expect("test must retain a previously registered real group")
            .altered_birth_for_test();
        self.group = Some(altered);
        altered
    }

    #[cfg(test)]
    pub(super) const fn group_identity_for_test(&self) -> Option<GroupIdentity> {
        self.group
    }

    /// Polls for an unexpected guardian exit while the host remains live.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error when process inspection fails. A
    /// guardian exit invokes the group fail-safe and returns
    /// [`RuntimeError::GuardianExited`].
    pub fn poll_fatal(&mut self) -> Result<(), RuntimeError> {
        self.require_active()?;
        let status = match self.child.try_wait() {
            Ok(status) => status,
            Err(source) => {
                return Err(
                    self.fail_observation("macOS guardian process-handle inspection", source)
                );
            }
        };
        if let Some(status) = status {
            self.liveness_writer.take();
            return Err(self.terminal_guardian_exit(status));
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
        self.require_active()?;
        let status = match self.child.wait() {
            Ok(status) => status,
            Err(source) => {
                return Err(self.fail_observation("macOS guardian process-handle wait", source));
            }
        };
        self.liveness_writer.take();
        Err(self.terminal_guardian_exit(status))
    }

    /// Records an accepted KEL-96 Quit before its correlated reply can let
    /// Bun exit cooperatively. This writes one non-authority control byte to
    /// the host-exclusive liveness pipe; only [`run_supervised`] accepts it.
    /// Group termination still begins only when [`Self::shutdown`] closes the
    /// writer, so the Quit reply can be published first.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error if the guardian is already terminal or
    /// the private control byte cannot be written and flushed.
    pub fn accept_supervised_quit(&mut self) -> Result<(), RuntimeError> {
        self.require_active()?;
        let ack_reader = self.quit_ack_reader.as_mut().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervised-Quit acknowledgment",
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "guardian was not spawned through the supervised bootstrap",
                ),
            )
        })?;
        let writer = self.liveness_writer.as_mut().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervised-Quit control",
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "guardian liveness writer is unavailable",
                ),
            )
        })?;
        set_liveness_nonblocking(writer, true)?;
        writer
            .write_all(&[SUPERVISED_QUIT_ACCEPTED])
            .and_then(|()| writer.flush())
            .map_err(|source| lifecycle_error("macOS guardian supervised-Quit control", source))?;
        read_supervised_quit_ack(ack_reader)
    }

    /// Performs orderly shutdown through the same EOF cleanup owner.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error if the guardian cannot be waited. A
    /// non-success guardian exit invokes the group fail-safe and returns
    /// [`RuntimeError::GuardianExited`].
    pub fn shutdown(&mut self) -> Result<ExitStatus, RuntimeError> {
        self.shutdown_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
    }

    pub(super) fn shutdown_until(&mut self, deadline: Instant) -> Result<ExitStatus, RuntimeError> {
        self.require_active()?;
        self.liveness_writer.take();
        let status = loop {
            match self.child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => std::thread::yield_now(),
                Ok(None) => {
                    let mut failures = vec![lifecycle_error(
                        "macOS guardian orderly-shutdown deadline",
                        io::Error::new(
                            io::ErrorKind::TimedOut,
                            "guardian did not exit before the shutdown deadline",
                        ),
                    )];
                    if let Err(error) = self.terminate_owned_group() {
                        failures.push(error);
                    }
                    if let Err(source) = self.child.kill()
                        && source.kind() != io::ErrorKind::InvalidInput
                    {
                        failures.push(lifecycle_error(
                            "macOS guardian shutdown-timeout process kill",
                            source,
                        ));
                    }
                    if let Err(source) = self.child.wait() {
                        failures.push(lifecycle_error(
                            "macOS guardian shutdown-timeout process wait",
                            source,
                        ));
                    }
                    return Err(collapse_failures(failures));
                }
                Err(source) => {
                    return Err(
                        self.fail_observation("macOS guardian orderly-shutdown wait", source)
                    );
                }
            }
        };
        if status.success() {
            // Guardian status zero is not proof of group retirement.
            // Finish the host's fail-safe before reporting clean shutdown.
            self.terminate_owned_group()?;
            Ok(status)
        } else {
            Err(self.terminal_guardian_exit(status))
        }
    }

    fn require_active(&self) -> Result<u32, RuntimeError> {
        // A terminal native error retains the kernel birth for custody, not
        // permission to reuse an already closed Guardian session. Never
        // reinterpret an earlier fatal outcome as a fresh clean shutdown.
        self.group
            .filter(|_| self.liveness_writer.is_some())
            .map(|group| group.pid)
            .ok_or_else(|| {
                lifecycle_error(
                    "macOS guardian terminal owner reuse",
                    io::Error::new(
                        io::ErrorKind::NotConnected,
                        "guardian owner is already terminal; start a fresh session",
                    ),
                )
            })
    }

    // One birth-attested fail-safe owner for all terminal paths. Retain the
    // exact enrolled witness if kernel validation or signaling fails. A
    // recycled group number alone never grants permission to signal again.
    fn terminate_owned_group(&mut self) -> Result<(), RuntimeError> {
        if let Some(group) = self.group {
            terminate_attested_group(group)?;
            self.group = None;
        }
        Ok(())
    }

    fn terminal_guardian_exit(&mut self, status: ExitStatus) -> RuntimeError {
        let group_pid = self.group_pid().unwrap_or(0);
        let cleanup_error = self
            .terminate_owned_group()
            .err()
            .map(|error| io::Error::other(error.to_string()));
        RuntimeError::GuardianExited {
            group_pid,
            exit_code: status.code(),
            cleanup_error,
        }
    }

    fn fail_observation(&mut self, phase: &'static str, source: io::Error) -> RuntimeError {
        // ECHILD means that the kernel no longer recognizes this Child as
        // waitable. Another reaper may already have released its PID, while
        // std::process::Child still has no cached exit status. In that case,
        // Child::kill would signal the numeric PID and could hit its next
        // owner. The enrolled process group has a separate birth attestation.
        let child_is_reaped = source.raw_os_error() == Some(nix::libc::ECHILD);
        self.liveness_writer.take();
        let mut failures = vec![lifecycle_error(phase, source)];
        if let Err(error) = self.terminate_owned_group() {
            failures.push(error);
        }
        if !child_is_reaped {
            if let Err(source) = self.child.kill()
                && source.kind() != io::ErrorKind::InvalidInput
            {
                failures.push(lifecycle_error(
                    "macOS guardian fail-safe process kill",
                    source,
                ));
            }
            if let Err(source) = self.child.wait() {
                failures.push(lifecycle_error(
                    "macOS guardian fail-safe process wait",
                    source,
                ));
            }
        }
        collapse_failures(failures)
    }
}

impl Drop for HostGuardian {
    fn drop(&mut self) {
        if self.liveness_writer.is_none() {
            return;
        }
        // Use the same bounded, fail-safe cleanup as explicit shutdown.
        // An unresponsive guardian must not block its host owner indefinitely.
        let _ = self.shutdown();
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

fn unexpected_guardian_exit(group: GroupIdentity, status: ExitStatus) -> RuntimeError {
    let cleanup_error = terminate_attested_group(group)
        .err()
        .map(|error| io::Error::other(error.to_string()));
    RuntimeError::GuardianExited {
        group_pid: group.pid,
        exit_code: status.code(),
        cleanup_error,
    }
}

fn require_same_registered_group(group: GroupIdentity) -> Result<(), RuntimeError> {
    if observe_group_identity(group.pid)? == group {
        Ok(())
    } else {
        Err(lifecycle_error(
            "macOS guardian registration identity changed",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "group leader changed after authenticated host registration",
            ),
        ))
    }
}

fn reject_proven_registration(
    child: Child,
    liveness_writer: ChildStdin,
    group: GroupIdentity,
    error: RuntimeError,
) -> RuntimeError {
    // Close host authority and wait for Guardian cleanup first. A leftover
    // Bun group is then killed ONLY if its original kernel birth still matches.
    let primary = reject_host_registration(child, liveness_writer, error);
    match terminate_attested_group(group) {
        Ok(()) => primary,
        Err(cleanup) => collapse_failures(vec![primary, cleanup]),
    }
}

#[cfg(test)]
mod externally_reaped_tests {
    use super::*;
    use std::os::unix::process::CommandExt as _;
    use std::process::{Child, Command, Stdio};

    struct NativeChildOwner(Child);

    impl Drop for NativeChildOwner {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    #[test]
    fn externally_reaped_child_never_enters_direct_pid_cleanup() {
        let mut group = NativeChildOwner(
            Command::new("/bin/sleep")
                .arg("30")
                .process_group(0)
                .spawn()
                .expect("own independent native group"),
        );
        let original_birth = observe_group_identity(group.0.id()).expect("real native birth");
        let mut child = Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::piped())
            .spawn()
            .expect("own real guardian child");
        let writer = child.stdin.take().expect("host liveness writer");
        let mut guardian = HostGuardian::from_registered_parts(child, writer, None, original_birth)
            .expect("register live guardian and group");
        let stale_birth = original_birth.altered_birth_for_test();
        guardian.group = Some(stale_birth);

        let pid = i32::try_from(guardian.guardian_pid()).expect("positive guardian PID");
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGKILL,
        )
        .expect("kill exact child while owned");
        rustix::process::waitpid(
            rustix::process::Pid::from_raw(pid),
            rustix::process::WaitOptions::empty(),
        )
        .expect("externally reap exact owned guardian")
        .expect("child exit observed");

        let error = guardian.poll_fatal().expect_err("ECHILD must fail closed");
        let message = error.to_string();
        assert!(message.contains("process-handle inspection"), "{message}");
        assert!(
            message.contains("process-group leader PID was reused"),
            "{message}"
        );
        assert!(
            !message.contains("fail-safe process wait"),
            "no redundant wait: {message}"
        );
        assert!(
            !message.contains("fail-safe process kill"),
            "no numeric PID kill: {message}"
        );
        assert_eq!(
            guardian.group,
            Some(stale_birth),
            "#948 retains the attested birth witness after denied cleanup"
        );
        assert!(group.0.try_wait().expect("group is still owned").is_none());
    }
}
