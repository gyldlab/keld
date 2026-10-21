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
            let group = self.take_registered_group()?;
            return Err(unexpected_guardian_exit(group, status));
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
        let group = self.take_registered_group()?;
        Err(unexpected_guardian_exit(group, status))
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
                    if let Some(group) = self.group.take()
                        && let Err(error) = terminate_attested_group(group)
                    {
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
        let group = self.take_registered_group()?;
        if status.success() {
            // Guardian status zero is not proof of group retirement.
            // Finish the host's fail-safe before reporting clean shutdown.
            terminate_attested_group(group)?;
            Ok(status)
        } else {
            Err(unexpected_guardian_exit(group, status))
        }
    }

    fn require_active(&self) -> Result<u32, RuntimeError> {
        self.group.map(|group| group.pid).ok_or_else(|| {
            lifecycle_error(
                "macOS guardian terminal owner reuse",
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "guardian owner is already terminal; start a fresh session",
                ),
            )
        })
    }

    fn take_registered_group(&mut self) -> Result<GroupIdentity, RuntimeError> {
        self.group.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian terminal owner reuse",
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "guardian owner is already terminal; start a fresh session",
                ),
            )
        })
    }

    fn fail_observation(&mut self, phase: &'static str, source: io::Error) -> RuntimeError {
        self.liveness_writer.take();
        let mut failures = vec![lifecycle_error(phase, source)];
        if let Some(group) = self.group.take()
            && let Err(error) = terminate_attested_group(group)
        {
            failures.push(error);
        }
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
