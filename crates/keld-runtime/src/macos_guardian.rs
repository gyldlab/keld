//! macOS host-death guardian for one supervised Bun process group.
//!
//! This is supervisor cleanup, not App Sandbox containment. The caller owns
//! the guardian process and the only liveness writer. The guardian process
//! calls [`run`] with its reader, a fresh Bun command, and the single owning
//! revocation callback for any registered link resources.

#![deny(unsafe_op_in_unsafe_fn)]

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
use nix::sys::signal::{Signal, killpg};
use nix::sys::stat::{SFlag, fstat};
use nix::unistd::{Pid, getpgid};

use keld_ipc::link::handshake_client;
use keld_ipc::{
    BootstrapAdmission, BootstrapListener, BootstrapRejection, BootstrapRejectionObserver,
    parse_app_link,
};

use crate::{
    CapturedOutput, ChildPreparer, CrashLedger, GenerationLease, PreparedChild, RestartPolicy,
    RevocationCause, RuntimeError, Supervisor, SupervisorOutcome,
};

const REGISTRATION_ENV: &str = "KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION";
const REGISTRATION_MAGIC: [u8; 4] = *b"KGR1";
const REGISTRATION_LEN: usize = 8;
const SUPERVISED_QUIT_ACCEPTED: u8 = b'Q';
const SUPERVISED_QUIT_ACK: [u8; 3] = *b"KQA";
const SUPERVISED_QUIT_ACK_DEADLINE: Duration = Duration::from_secs(5);
const GUARDIAN_SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);

/// Private argv discriminator for the KEL-96 supervised guardian re-exec.
///
/// It carries no authority: authenticated registration and the inherited
/// liveness descriptor must still validate before Bun can spawn.
pub const SUPERVISED_GUARDIAN_ARG: &str = "--keld-internal-macos-guardian-v1";

/// Observable completion of one guardian-owned Bun process group.
#[derive(Debug)]
pub struct GuardianReport {
    leader_pid: u32,
    leader_status: ExitStatus,
}

struct SupervisedGuardianPreparer<C, W, S, F: FnOnce() -> io::Result<()>> {
    command_factory: Option<C>,
    registration: Option<W>,
    child_spawned: Option<S>,
    revoke_registered_resources: Option<F>,
    pid_tx: std::sync::mpsc::SyncSender<u32>,
}

impl<C, W, S, F> Drop for SupervisedGuardianPreparer<C, W, S, F>
where
    F: FnOnce() -> io::Result<()>,
{
    fn drop(&mut self) {
        if let Some(revoke) = self.revoke_registered_resources.take() {
            let _ = revoke();
        }
    }
}

struct SupervisedGuardianLease<W, S, F> {
    registration: W,
    child_spawned: Option<S>,
    revoke_registered_resources: Option<F>,
    pid_tx: std::sync::mpsc::SyncSender<u32>,
    group_pid: Option<u32>,
}

impl<C, W, S, F> ChildPreparer for SupervisedGuardianPreparer<C, W, S, F>
where
    C: FnOnce() -> Result<Command, RuntimeError> + Send + 'static,
    W: Write + Send + 'static,
    S: FnOnce(u32) -> io::Result<()> + Send + 'static,
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    type Lease = SupervisedGuardianLease<W, S, F>;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        if attempt != 1 {
            return Err(lifecycle_error(
                "macOS guardian supervisor policy",
                io::Error::other("KEL-96/T1b forbids a successor before KEL-96/T3"),
            ));
        }
        let factory = self.command_factory.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervisor preparation",
                io::Error::other("Bun command factory was already consumed"),
            )
        })?;
        let mut command = match factory() {
            Ok(command) => command,
            Err(error) => {
                let mut failures = vec![error];
                if let Some(revoke) = self.revoke_registered_resources.take()
                    && let Err(source) = revoke()
                {
                    failures.push(lifecycle_error(
                        "macOS guardian pre-child registered-resource revocation",
                        source,
                    ));
                }
                return Err(collapse_failures(failures));
            }
        };
        command
            .env_remove(REGISTRATION_ENV)
            .process_group(0)
            .stdin(Stdio::null());
        Ok(PreparedChild {
            command,
            lease: SupervisedGuardianLease {
                registration: self.registration.take().ok_or_else(|| {
                    lifecycle_error(
                        "macOS guardian supervisor preparation",
                        io::Error::other("authenticated registration writer is missing"),
                    )
                })?,
                child_spawned: self.child_spawned.take(),
                revoke_registered_resources: self.revoke_registered_resources.take(),
                pid_tx: self.pid_tx.clone(),
                group_pid: None,
            },
        })
    }
}

impl<W, S, F> GenerationLease for SupervisedGuardianLease<W, S, F>
where
    W: Write + Send + 'static,
    S: FnOnce(u32) -> io::Result<()> + Send + 'static,
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    fn child_spawned(&mut self, pid: u32, _attempt: u32) -> Result<(), RuntimeError> {
        validate_group_leader(pid)?;
        self.group_pid = Some(pid);
        write_group_registration(&mut self.registration, pid)?;
        if let Some(observer) = self.child_spawned.take() {
            observer(pid).map_err(|source| {
                lifecycle_error("macOS guardian supervised-child observer", source)
            })?;
        }
        self.pid_tx.send(pid).map_err(|_| {
            lifecycle_error(
                "macOS guardian supervised-child registration",
                io::Error::other("guardian owner stopped before child registration"),
            )
        })
    }

    fn revoke(mut self, _cause: RevocationCause) -> Result<(), RuntimeError> {
        let mut failures = Vec::new();
        if let Some(revoke) = self.revoke_registered_resources.take()
            && let Err(source) = revoke()
        {
            failures.push(lifecycle_error(
                "macOS guardian registered-resource revocation",
                source,
            ));
        }
        if let Some(group_pid) = self.group_pid.take()
            && let Err(error) = terminate_registered_group(group_pid)
        {
            failures.push(error);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(collapse_failures(failures))
        }
    }
}

/// Host-side authenticated bootstrap for one private guardian process.
///
/// The bootstrap mints a one-use owner-only registration link, overrides the
/// private environment value, and owns the guardian child plus sole liveness
/// writer until [`Self::register_until`] consumes the guardian-emitted group
/// record. Callers never supply a numeric process-group id.
#[derive(Debug)]
pub struct GuardianBootstrap {
    child: Option<Child>,
    liveness_writer: Option<ChildStdin>,
    quit_ack_reader: Option<ChildStdout>,
    registration: BootstrapListener,
}

impl GuardianBootstrap {
    /// Spawns one private guardian command with authenticated registration and
    /// a non-inheritable host-liveness writer.
    ///
    /// # Errors
    ///
    /// Returns a typed [`RuntimeError`] if the private command is empty, the
    /// registration link cannot be provisioned, the guardian cannot spawn, or
    /// its piped stdin cannot be retained.
    pub fn spawn(mut command: Command) -> Result<Self, RuntimeError> {
        Self::spawn_inner(&mut command, false)
    }

    /// Spawns the private guardian with stdout reserved for the fixed
    /// accepted-Quit acknowledgment used by [`run_supervised`].
    ///
    /// # Errors
    ///
    /// Returns the same typed bootstrap failures as [`Self::spawn`], plus a
    /// missing acknowledgment-pipe failure.
    pub fn spawn_supervised(mut command: Command) -> Result<Self, RuntimeError> {
        Self::spawn_inner(&mut command, true)
    }

    fn spawn_inner(command: &mut Command, supervised: bool) -> Result<Self, RuntimeError> {
        if command.get_program().is_empty() {
            return Err(lifecycle_error(
                "macOS guardian private bootstrap",
                io::Error::new(io::ErrorKind::InvalidInput, "guardian program is empty"),
            ));
        }
        let registration = BootstrapListener::bind().map_err(|source| {
            lifecycle_error("macOS guardian registration-link provisioning", source)
        })?;
        command
            .env(REGISTRATION_ENV, registration.app_link())
            .stdin(Stdio::piped());
        if supervised {
            command.stdout(Stdio::piped());
        }
        let mut child = command.spawn().map_err(RuntimeError::Spawn)?;
        let Some(liveness_writer) = child.stdin.take() else {
            let mut failures = vec![lifecycle_error(
                "macOS guardian host-liveness writer provisioning",
                io::Error::other("spawned guardian has no piped stdin"),
            )];
            if let Err(source) = child.kill()
                && source.kind() != io::ErrorKind::InvalidInput
            {
                failures.push(lifecycle_error(
                    "macOS guardian bootstrap process kill",
                    source,
                ));
            }
            if let Err(source) = child.wait() {
                failures.push(lifecycle_error(
                    "macOS guardian bootstrap process wait",
                    source,
                ));
            }
            return Err(collapse_failures(failures));
        };
        let quit_ack_reader = if supervised {
            match child.stdout.take() {
                Some(reader) => Some(reader),
                None => {
                    return Err(reject_host_registration(
                        child,
                        liveness_writer,
                        None,
                        lifecycle_error(
                            "macOS guardian supervised-Quit acknowledgment",
                            io::Error::other("spawned guardian has no acknowledgment reader"),
                        ),
                    ));
                }
            }
        } else {
            None
        };
        if let Err(error) = require_close_on_exec(&liveness_writer) {
            return Err(reject_host_registration(
                child,
                liveness_writer,
                None,
                error,
            ));
        }
        Ok(Self {
            child: Some(child),
            liveness_writer: Some(liveness_writer),
            quit_ack_reader,
            registration,
        })
    }

    /// OS process id of the private guardian while registration is pending.
    #[must_use]
    pub fn guardian_pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// Authenticates and consumes the guardian-owned process-group record.
    ///
    /// # Errors
    ///
    /// Returns a typed [`RuntimeError`] when the deadline elapses, registration
    /// is cancelled or malformed, the group is not representable by macOS, or
    /// the guardian exits before registration completes. Every rejected path
    /// closes the liveness writer and waits the guardian before returning.
    pub fn register_until(mut self, deadline: Instant) -> Result<HostGuardian, RuntimeError> {
        let admission = self
            .registration
            .accept_authenticated_until(deadline, &NoopRegistrationObserver)
            .map_err(|source| lifecycle_error("macOS guardian registration admission", source));
        let mut stream = match admission {
            Ok(BootstrapAdmission::Authenticated(stream)) => stream,
            Ok(BootstrapAdmission::Cancelled) => {
                return Err(self.reject_registration(lifecycle_error(
                    "macOS guardian registration admission",
                    io::Error::new(io::ErrorKind::Interrupted, "registration was cancelled"),
                )));
            }
            Ok(BootstrapAdmission::DeadlineElapsed) => {
                return Err(self.reject_registration(lifecycle_error(
                    "macOS guardian registration admission",
                    io::Error::new(io::ErrorKind::TimedOut, "registration deadline elapsed"),
                )));
            }
            Err(error) => return Err(self.reject_registration(error)),
        };
        let mut record = [0_u8; REGISTRATION_LEN];
        if let Err(source) = stream.read_exact(&mut record) {
            return Err(self.reject_registration(lifecycle_error(
                "macOS guardian registration record",
                source,
            )));
        }
        if record[..4] != REGISTRATION_MAGIC {
            return Err(self.reject_registration(lifecycle_error(
                "macOS guardian registration record",
                io::Error::new(io::ErrorKind::InvalidData, "registration magic is not KGR1"),
            )));
        }
        let group_pid = u32::from_be_bytes(record[4..].try_into().map_err(|_| {
            lifecycle_error(
                "macOS guardian registration record",
                io::Error::new(io::ErrorKind::InvalidData, "registration pid is truncated"),
            )
        })?);
        validate_group_leader(group_pid).map_err(|error| self.reject_registration(error))?;

        let child = self.child.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian registration owner",
                io::Error::new(io::ErrorKind::NotConnected, "guardian child is missing"),
            )
        })?;
        let liveness_writer = self.liveness_writer.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian registration owner",
                io::Error::new(io::ErrorKind::NotConnected, "liveness writer is missing"),
            )
        })?;
        HostGuardian::from_registered_parts(
            child,
            liveness_writer,
            self.quit_ack_reader.take(),
            group_pid,
        )
    }

    fn reject_registration(&mut self, error: RuntimeError) -> RuntimeError {
        let Some(child) = self.child.take() else {
            return error;
        };
        let Some(liveness_writer) = self.liveness_writer.take() else {
            return error;
        };
        reject_host_registration(child, liveness_writer, None, error)
    }
}

impl Drop for GuardianBootstrap {
    fn drop(&mut self) {
        let (Some(mut child), Some(liveness_writer)) =
            (self.child.take(), self.liveness_writer.take())
        else {
            return;
        };
        drop(liveness_writer);
        let _ = child.wait();
    }
}

struct NoopRegistrationObserver;

impl BootstrapRejectionObserver for NoopRegistrationObserver {
    fn rejected(&self, _rejection: BootstrapRejection) {}
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
    quit_ack_reader: Option<ChildStdout>,
    group_pid: Option<u32>,
}

impl HostGuardian {
    fn from_registered_parts(
        mut child: Child,
        liveness_writer: ChildStdin,
        quit_ack_reader: Option<ChildStdout>,
        group_pid: u32,
    ) -> Result<Self, RuntimeError> {
        if let Err(error) = validate_group_leader(group_pid) {
            return Err(reject_host_registration(
                child,
                liveness_writer,
                (group_pid != 0).then_some(group_pid),
                error,
            ));
        }
        if let Err(error) = require_close_on_exec(&liveness_writer) {
            return Err(reject_host_registration(
                child,
                liveness_writer,
                Some(group_pid),
                error,
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => return Err(unexpected_guardian_exit(group_pid, status)),
            Ok(None) => {}
            Err(source) => {
                let error = lifecycle_error("macOS guardian process-handle inspection", source);
                return Err(reject_host_registration(
                    child,
                    liveness_writer,
                    Some(group_pid),
                    error,
                ));
            }
        }
        Ok(Self {
            child,
            liveness_writer: Some(liveness_writer),
            quit_ack_reader,
            group_pid: Some(group_pid),
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
            let group_pid = self.take_registered_group()?;
            return Err(unexpected_guardian_exit(group_pid, status));
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
        let group_pid = self.take_registered_group()?;
        Err(unexpected_guardian_exit(group_pid, status))
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
        set_liveness_nonblocking(writer)?;
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

    fn shutdown_until(&mut self, deadline: Instant) -> Result<ExitStatus, RuntimeError> {
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
                    if let Some(group_pid) = self.group_pid.take()
                        && let Err(error) = terminate_registered_group(group_pid)
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
        let group_pid = self.take_registered_group()?;
        if status.success() {
            Ok(status)
        } else {
            Err(unexpected_guardian_exit(group_pid, status))
        }
    }

    fn require_active(&self) -> Result<u32, RuntimeError> {
        self.group_pid.ok_or_else(|| {
            lifecycle_error(
                "macOS guardian terminal owner reuse",
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "guardian owner is already terminal; start a fresh session",
                ),
            )
        })
    }

    fn take_registered_group(&mut self) -> Result<u32, RuntimeError> {
        self.group_pid.take().ok_or_else(|| {
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
        if let Some(group_pid) = self.group_pid.take()
            && let Err(error) = terminate_registered_group(group_pid)
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
        if self.liveness_writer.take().is_none() {
            return;
        }
        match self.child.wait() {
            Ok(status) if status.success() => {
                self.group_pid.take();
            }
            Ok(_) | Err(_) => {
                if let Some(group_pid) = self.group_pid.take() {
                    let _ = terminate_registered_group(group_pid);
                }
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
/// The module authenticates to the host-minted private registration link before
/// child creation and emits the fixed `KGR1` group record after spawn. Callers
/// cannot substitute a numeric group id.
///
/// # Errors
///
/// Returns a typed [`RuntimeError`] when the command is empty, spawning fails,
/// the liveness reader carries bytes or errors, revocation fails, the group
/// cannot be signaled, or the direct child cannot be waited.
pub fn run<R, F>(
    command: Command,
    liveness: R,
    revoke_registered_resources: F,
) -> Result<GuardianReport, RuntimeError>
where
    R: Read + AsFd,
    F: FnOnce() -> io::Result<()>,
{
    let registration = match connect_guardian_registration() {
        Ok(registration) => registration,
        Err(error) => return Err(fail_before_child(error, revoke_registered_resources)),
    };
    run_with_registration(
        command,
        liveness,
        registration,
        |_| Ok(()),
        revoke_registered_resources,
    )
}

/// Runs one non-restarting guardian-owned [`Supervisor`] until host EOF.
///
/// The guardian authenticates its private group-registration link and validates
/// the live host-liveness pipe before starting the supervisor. The supervisor
/// remains the sole Bun spawn/capture/KEL-116-ledger/wait owner. Its prepared
/// generation lease registers the exact Bun group and revokes resources plus
/// signals that group before supervisor kill/wait. A first unrequested child
/// termination is fatal; fresh-generation recovery remains KEL-96/T3.
/// `quit_ack` is a private dedicated writer: after the guardian reads the
/// accepted-Quit control and updates Supervisor attribution, it writes fixed
/// `KQA`. The host must observe that ack before publishing the Quit reply.
///
/// # Errors
///
/// Returns [`RuntimeError`] for invalid private bootstrap, initial spawn or
/// registration failure, host-liveness failure, revocation/group cleanup
/// failure, or any unrequested Bun self-termination.
pub fn run_supervised<R, C, A, F>(
    liveness: R,
    command_factory: C,
    quit_ack: A,
    revoke_registered_resources: F,
) -> Result<CapturedOutput, RuntimeError>
where
    R: Read + AsFd,
    C: FnOnce() -> Result<Command, RuntimeError> + Send + 'static,
    A: Write,
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    let registration = match connect_guardian_registration() {
        Ok(registration) => registration,
        Err(error) => return Err(fail_before_child(error, revoke_registered_resources)),
    };
    run_supervised_with_registration(
        liveness,
        registration,
        quit_ack,
        command_factory,
        |_| Ok(()),
        revoke_registered_resources,
    )
    .map(|(_, _, output)| output)
}

fn run_supervised_with_registration<R, W, C, A, S, F>(
    mut liveness: R,
    registration: W,
    mut quit_ack: A,
    command_factory: C,
    child_spawned: S,
    revoke_registered_resources: F,
) -> Result<(u32, CrashLedger, CapturedOutput), RuntimeError>
where
    R: Read + AsFd,
    W: Write + Send + 'static,
    C: FnOnce() -> Result<Command, RuntimeError> + Send + 'static,
    A: Write,
    S: FnOnce(u32) -> io::Result<()> + Send + 'static,
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    if let Err(error) = validate_liveness_bootstrap(&mut liveness) {
        return Err(fail_before_child(error, revoke_registered_resources));
    }
    let (pid_tx, pid_rx) = std::sync::mpsc::sync_channel(1);
    let preparer = SupervisedGuardianPreparer {
        command_factory: Some(command_factory),
        registration: Some(registration),
        child_spawned: Some(child_spawned),
        revoke_registered_resources: Some(revoke_registered_resources),
        pid_tx,
    };
    let supervisor = Supervisor::start_prepared(
        RestartPolicy {
            max_crashes: 1,
            window_secs: 30,
        },
        preparer,
    )?;
    let leader_pid = pid_rx.recv().map_err(|_| {
        lifecycle_error(
            "macOS guardian supervised-child registration",
            io::Error::other("supervisor ended before registering its initial child"),
        )
    })?;

    set_liveness_nonblocking(&liveness)?;
    let mut liveness_result = Ok(());
    let mut quit_accepted = false;
    loop {
        match observe_supervised_liveness(
            &mut liveness,
            &supervisor,
            &mut quit_accepted,
            &mut quit_ack,
        ) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => {
                liveness_result = Err(error);
                supervisor.shutdown();
                break;
            }
        }
        if let Some(event) = supervisor.recv_event(Duration::from_millis(20))
            && matches!(
                event,
                crate::SupervisorEvent::Exited { .. }
                    | crate::SupervisorEvent::RespawnFailed
                    | crate::SupervisorEvent::Failed { .. }
                    | crate::SupervisorEvent::CrashLoopTripped
                    | crate::SupervisorEvent::Stopped
            )
        {
            if let Err(error) = observe_supervised_liveness(
                &mut liveness,
                &supervisor,
                &mut quit_accepted,
                &mut quit_ack,
            ) {
                liveness_result = Err(error);
                supervisor.shutdown();
            }
            break;
        }
    }
    let outcome = supervisor.wait_for_outcome();
    let crash_ledger = supervisor.crash_ledger();
    let output = supervisor.output();
    drop(supervisor);

    let mut failures = Vec::new();
    if let Err(error) = liveness_result {
        failures.push(error);
    }
    match outcome {
        SupervisorOutcome::CrashLoop(error) | SupervisorOutcome::Failed(error) => {
            failures.push(error);
        }
        SupervisorOutcome::Stopped => {
            if let Some(termination) = crash_ledger.last_self_termination {
                failures.push(RuntimeError::ChildCrashed {
                    pid: termination.pid,
                    exit_code: termination.exit_code,
                    stderr_tail: output.stderr_tail(2_000),
                });
            }
        }
    }
    if !failures.is_empty() {
        return Err(collapse_failures(failures));
    }
    Ok((leader_pid, crash_ledger, output))
}

fn run_with_registration<R, W, S, F>(
    mut command: Command,
    mut liveness: R,
    mut registration: W,
    child_spawned: S,
    revoke_registered_resources: F,
) -> Result<GuardianReport, RuntimeError>
where
    R: Read + AsFd,
    W: Write,
    S: FnOnce(u32) -> io::Result<()>,
    F: FnOnce() -> io::Result<()>,
{
    if command.get_program().is_empty() {
        let error = lifecycle_error(
            "macOS guardian bootstrap",
            io::Error::new(io::ErrorKind::InvalidInput, "child program is empty"),
        );
        return Err(fail_before_child(error, revoke_registered_resources));
    }

    if let Err(error) = validate_liveness_bootstrap(&mut liveness) {
        return Err(fail_before_child(error, revoke_registered_resources));
    }

    command
        .env_remove(REGISTRATION_ENV)
        .process_group(0)
        .stdin(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(source) => {
            return Err(fail_before_child(
                RuntimeError::Spawn(source),
                revoke_registered_resources,
            ));
        }
    };
    let leader_pid = child.id();

    let registration_result = validate_group_pid(leader_pid)
        .map(|_| ())
        .and_then(|()| write_group_registration(&mut registration, leader_pid));
    if let Err(error) = registration_result {
        let mut failures = vec![error];
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

    if let Err(source) = child_spawned(leader_pid) {
        let mut failures = vec![lifecycle_error(
            "macOS guardian local registration observer",
            source,
        )];
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
    let revocation_result = revoke_registered_resources()
        .map_err(|source| lifecycle_error("macOS guardian registered-resource revocation", source));
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

fn connect_guardian_registration() -> Result<std::os::unix::net::UnixStream, RuntimeError> {
    let link = std::env::var(REGISTRATION_ENV).map_err(|source| {
        lifecycle_error(
            "macOS guardian private registration bootstrap",
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{REGISTRATION_ENV} is unavailable: {source}"),
            ),
        )
    })?;
    let (endpoint, token) = parse_app_link(&link).map_err(|source| {
        lifecycle_error(
            "macOS guardian private registration bootstrap",
            io::Error::new(io::ErrorKind::InvalidInput, source.to_string()),
        )
    })?;
    let mut stream = std::os::unix::net::UnixStream::connect(endpoint)
        .map_err(|source| lifecycle_error("macOS guardian private registration connect", source))?;
    handshake_client(&mut stream, &token).map_err(|source| {
        lifecycle_error(
            "macOS guardian private registration authentication",
            io::Error::other(source.to_string()),
        )
    })?;
    Ok(stream)
}

fn write_group_registration(writer: &mut impl Write, group_pid: u32) -> Result<(), RuntimeError> {
    let mut record = [0_u8; REGISTRATION_LEN];
    record[..4].copy_from_slice(&REGISTRATION_MAGIC);
    record[4..].copy_from_slice(&group_pid.to_be_bytes());
    writer.write_all(&record).map_err(|source| {
        lifecycle_error("macOS guardian authenticated group registration", source)
    })?;
    writer.flush().map_err(|source| {
        lifecycle_error(
            "macOS guardian authenticated group registration flush",
            source,
        )
    })
}

fn fail_before_child(
    error: RuntimeError,
    revoke_registered_resources: impl FnOnce() -> io::Result<()>,
) -> RuntimeError {
    let mut failures = vec![error];
    if let Err(source) = revoke_registered_resources() {
        failures.push(lifecycle_error(
            "macOS guardian pre-child registered-resource revocation",
            source,
        ));
    }
    collapse_failures(failures)
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

enum HostLiveness {
    Live,
    Dead,
    QuitAccepted,
}

fn set_liveness_nonblocking(reader: &impl AsFd) -> Result<(), RuntimeError> {
    let raw_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFL).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor flags",
            nix_io_error(source),
        )
    })?;
    fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(raw_flags) | OFlag::O_NONBLOCK),
    )
    .map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor polling",
            nix_io_error(source),
        )
    })?;
    Ok(())
}

fn poll_host_liveness(reader: &mut impl Read) -> Result<HostLiveness, RuntimeError> {
    let mut byte = [0_u8; 1];
    match reader.read(&mut byte) {
        Ok(0) => Ok(HostLiveness::Dead),
        Err(source)
            if matches!(
                source.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(HostLiveness::Live)
        }
        Ok(1) if byte[0] == SUPERVISED_QUIT_ACCEPTED => Ok(HostLiveness::QuitAccepted),
        Ok(_) => Err(lifecycle_error(
            "macOS guardian host-liveness read",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "host-liveness pipe carried an unknown supervised control byte",
            ),
        )),
        Err(source) => Err(lifecycle_error("macOS guardian host-liveness read", source)),
    }
}

fn observe_supervised_liveness(
    reader: &mut impl Read,
    supervisor: &Supervisor,
    quit_accepted: &mut bool,
    quit_ack: &mut impl Write,
) -> Result<bool, RuntimeError> {
    match poll_host_liveness(reader)? {
        HostLiveness::Live => Ok(false),
        HostLiveness::Dead => {
            supervisor.shutdown();
            Ok(true)
        }
        HostLiveness::QuitAccepted if *quit_accepted => Err(lifecycle_error(
            "macOS guardian supervised-Quit control",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "duplicate accepted-Quit control byte",
            ),
        )),
        HostLiveness::QuitAccepted => {
            *quit_accepted = true;
            supervisor.accept_shutdown();
            quit_ack
                .write_all(&SUPERVISED_QUIT_ACK)
                .and_then(|()| quit_ack.flush())
                .map_err(|source| {
                    lifecycle_error("macOS guardian supervised-Quit acknowledgment", source)
                })?;
            Ok(false)
        }
    }
}

fn validate_group_pid(group: u32) -> Result<i32, RuntimeError> {
    if group == 0 {
        return Err(lifecycle_error(
            "macOS guardian host registration",
            io::Error::new(io::ErrorKind::InvalidInput, "process group is zero"),
        ));
    }
    i32::try_from(group).map_err(|_| {
        lifecycle_error(
            "macOS guardian host registration",
            io::Error::new(io::ErrorKind::InvalidInput, "process group exceeds c_int"),
        )
    })
}

fn validate_group_leader(group: u32) -> Result<i32, RuntimeError> {
    let raw = validate_group_pid(group)?;
    let observed = getpgid(Some(Pid::from_raw(raw))).map_err(|source| {
        lifecycle_error(
            "macOS guardian group-leader validation",
            nix_io_error(source),
        )
    })?;
    if observed.as_raw() == raw {
        Ok(raw)
    } else {
        Err(lifecycle_error(
            "macOS guardian group-leader validation",
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("reported pid {group} belongs to process group {observed}"),
            ),
        ))
    }
}

fn terminate_registered_group(group: u32) -> Result<(), RuntimeError> {
    let group = validate_group_pid(group)?;
    match killpg(Pid::from_raw(group), Signal::SIGKILL) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(lifecycle_error(
            "macOS guardian process-group signal",
            io::Error::from_raw_os_error(error as i32),
        )),
    }
}

fn reject_host_registration(
    mut child: Child,
    liveness_writer: ChildStdin,
    group_pid: Option<u32>,
    error: RuntimeError,
) -> RuntimeError {
    drop(liveness_writer);
    let mut failures = vec![error];
    match child.wait() {
        Ok(status) if status.success() => {}
        Ok(status) => {
            failures.push(lifecycle_error(
                "macOS guardian rejected-registration wait",
                io::Error::other(format!(
                    "guardian exited unsuccessfully during registration cleanup: {status}"
                )),
            ));
            if let Some(group_pid) = group_pid
                && let Err(error) = terminate_registered_group(group_pid)
            {
                failures.push(error);
            }
        }
        Err(source) => {
            failures.push(lifecycle_error(
                "macOS guardian rejected-registration wait",
                source,
            ));
            if let Some(group_pid) = group_pid
                && let Err(error) = terminate_registered_group(group_pid)
            {
                failures.push(error);
            }
        }
    }
    collapse_failures(failures)
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

fn read_supervised_quit_ack(reader: &mut ChildStdout) -> Result<(), RuntimeError> {
    let raw_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFL).map_err(|source| {
        lifecycle_error(
            "macOS guardian supervised-Quit acknowledgment flags",
            nix_io_error(source),
        )
    })?;
    fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(raw_flags) | OFlag::O_NONBLOCK),
    )
    .map_err(|source| {
        lifecycle_error(
            "macOS guardian supervised-Quit acknowledgment polling",
            nix_io_error(source),
        )
    })?;
    let deadline = Instant::now() + SUPERVISED_QUIT_ACK_DEADLINE;
    let mut ack = [0_u8; SUPERVISED_QUIT_ACK.len()];
    let mut filled = 0;
    while filled < ack.len() {
        match reader.read(&mut ack[filled..]) {
            Ok(0) => {
                return Err(lifecycle_error(
                    "macOS guardian supervised-Quit acknowledgment",
                    io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "guardian exited before acknowledging accepted Quit",
                    ),
                ));
            }
            Ok(read) => filled += read,
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) if source.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(lifecycle_error(
                        "macOS guardian supervised-Quit acknowledgment",
                        io::Error::new(
                            io::ErrorKind::TimedOut,
                            "guardian did not acknowledge accepted Quit before the deadline",
                        ),
                    ));
                }
                std::thread::yield_now();
            }
            Err(source) => {
                return Err(lifecycle_error(
                    "macOS guardian supervised-Quit acknowledgment",
                    source,
                ));
            }
        }
    }
    if ack == SUPERVISED_QUIT_ACK {
        Ok(())
    } else {
        Err(lifecycle_error(
            "macOS guardian supervised-Quit acknowledgment",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "guardian returned an invalid accepted-Quit acknowledgment",
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
    fn missing_private_registration_bootstrap_creates_no_child() {
        let temp = tempfile::tempdir().expect("temporary marker directory");
        let marker = temp.path().join("spawned");
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf spawned > \"$1\"", "keld-guardian-test"])
            .arg(&marker);
        let (reader, _writer) = liveness_pipe();
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_in_callback = Arc::clone(&revoked);
        let error = run(command, reader, move || {
            revoked_in_callback.store(true, Ordering::SeqCst);
            Ok(())
        })
        .expect_err("missing authenticated registration must fail before child spawn");
        assert!(error.to_string().contains(REGISTRATION_ENV), "{error}");
        assert!(!marker.exists(), "invalid bootstrap must create no child");
        assert!(revoked.load(Ordering::SeqCst));
    }

    #[test]
    fn empty_program_is_rejected_before_spawn() {
        let (reader, _writer) = liveness_pipe();
        let error =
            run_with_registration(Command::new(""), reader, io::sink(), |_| Ok(()), || Ok(()))
                .expect_err("empty guardian command must fail");
        let rendered = error.to_string();
        assert!(rendered.contains("KELD-RUNTIME-003"), "{rendered}");
        assert!(rendered.contains("child program is empty"), "{rendered}");
    }

    #[test]
    fn spawn_failure_revokes_prepared_resources() {
        let (reader, _writer) = liveness_pipe();
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_in_callback = Arc::clone(&revoked);
        let error = run_with_registration(
            Command::new("/definitely/missing/keld-bun"),
            reader,
            io::sink(),
            |_| Ok(()),
            move || {
                revoked_in_callback.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .expect_err("spawn failure must fail closed");
        assert!(matches!(error, RuntimeError::Spawn(_)), "{error}");
        assert!(
            revoked.load(Ordering::SeqCst),
            "spawn failure must revoke every prepared resource"
        );
    }

    #[test]
    fn private_registration_authority_is_not_inherited_by_the_child() {
        let (reader, writer) = liveness_pipe();
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "if [ -z \"${KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION+x}\" ]; then exec /usr/bin/tail -f /dev/null; else exit 77; fi",
            ])
            .env(REGISTRATION_ENV, "must-not-reach-child");
        let report = run_with_registration(
            command,
            reader,
            io::sink(),
            move |_| {
                drop(writer);
                Ok(())
            },
            || Ok(()),
        )
        .expect("guardian cleanup with stripped private authority");
        assert_eq!(
            report.leader_status().signal(),
            Some(9),
            "a child that inherited the private registration value exits 77"
        );
    }

    #[test]
    fn supervised_guardian_uses_one_supervisor_child_and_clean_host_shutdown() {
        let (reader, writer) = liveness_pipe();
        let registered = Arc::new(Mutex::new(None));
        let registered_in_callback = Arc::clone(&registered);
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_in_callback = Arc::clone(&revoked);
        let report = run_supervised_with_registration(
            reader,
            io::sink(),
            io::sink(),
            || Ok(long_running_command()),
            move |pid| {
                *registered_in_callback.lock().expect("pid lock") = Some(pid);
                drop(writer);
                Ok(())
            },
            move || {
                revoked_in_callback.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .expect("host-driven supervised guardian cleanup");
        assert_eq!(report.0, registered.lock().expect("pid lock").expect("pid"));
        assert_eq!(report.1.self_termination_count, 0);
        assert!(revoked.load(Ordering::SeqCst));
        assert_gone(*registered.lock().expect("pid lock"));
    }

    #[test]
    fn supervised_status_zero_is_retained_as_typed_self_termination() {
        let (reader, writer) = liveness_pipe();
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_in_callback = Arc::clone(&revoked);
        let error = run_supervised_with_registration(
            reader,
            io::sink(),
            io::sink(),
            || {
                let mut command = Command::new("/bin/sh");
                command
                    .args(["-c", "exit 0"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                Ok(command)
            },
            move |_| {
                drop(writer);
                Ok(())
            },
            move || {
                revoked_in_callback.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .expect_err("status-zero self-termination must not become host success");
        assert!(error.to_string().contains("KELD-RUNTIME-012"), "{error}");
        assert!(revoked.load(Ordering::SeqCst));
    }

    #[test]
    fn supervised_command_preparation_failure_revokes_before_child() {
        let (reader, _writer) = liveness_pipe();
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_in_callback = Arc::clone(&revoked);
        let error = run_supervised_with_registration(
            reader,
            io::sink(),
            io::sink(),
            || {
                Err(lifecycle_error(
                    "test supervised command preparation",
                    io::Error::other("intentional preparation failure"),
                ))
            },
            |_| panic!("preparation failure must not spawn a child"),
            move || {
                revoked_in_callback.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .expect_err("command preparation failure must fail closed");
        assert!(
            error
                .to_string()
                .contains("intentional preparation failure")
        );
        assert!(revoked.load(Ordering::SeqCst));
    }

    #[test]
    fn dropping_unhanded_supervised_preparer_revokes_once() {
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_in_callback = Arc::clone(&revoked);
        let (pid_tx, _pid_rx) = mpsc::sync_channel(1);
        let preparer = SupervisedGuardianPreparer {
            command_factory: Some(|| Ok::<Command, RuntimeError>(Command::new("/usr/bin/true"))),
            registration: Some(io::sink()),
            child_spawned: Some(|_: u32| Ok::<(), io::Error>(())),
            revoke_registered_resources: Some(move || {
                assert!(
                    !revoked_in_callback.swap(true, Ordering::SeqCst),
                    "unhanded resources revoked more than once"
                );
                Ok(())
            }),
            pid_tx,
        };

        drop(preparer);
        assert!(revoked.load(Ordering::SeqCst));
    }

    #[test]
    fn supervised_self_termination_returns_while_host_writer_is_still_live() {
        let (reader, writer) = liveness_pipe();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = run_supervised_with_registration(
                reader,
                io::sink(),
                io::sink(),
                || {
                    let mut command = Command::new("/bin/sh");
                    command.args(["-c", "exit 0"]);
                    Ok(command)
                },
                |_| Ok(()),
                || Ok(()),
            );
            done_tx.send(result).expect("report guardian result");
        });

        let error = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("self-termination must wake the guardian while host is live")
            .expect_err("unrequested status-zero exit is fatal");
        assert!(error.to_string().contains("KELD-RUNTIME-012"), "{error}");
        drop(writer);
        worker.join().expect("guardian worker joins");
    }

    #[test]
    fn supervised_accepted_quit_does_not_record_status_zero_as_unrequested() {
        let (reader, mut writer) = liveness_pipe();
        let (pid_tx, pid_rx) = mpsc::channel();
        let (ack_tx, ack_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = run_supervised_with_registration(
                reader,
                io::sink(),
                AckSender(ack_tx),
                || {
                    let mut command = Command::new("/bin/sh");
                    command.args(["-c", "kill -STOP $$; exit 0"]);
                    Ok(command)
                },
                move |pid| pid_tx.send(pid).map_err(io::Error::other),
                || Ok(()),
            );
            done_tx.send(result).expect("report guardian result");
        });
        let pid = pid_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("stopped child registered");

        let stopped_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let output = Command::new("/bin/ps")
                .args(["-o", "state=", "-p", &pid.to_string()])
                .output()
                .expect("inspect accepted-Quit child state");
            if String::from_utf8(output.stdout)
                .expect("process state is UTF-8")
                .trim()
                .starts_with('T')
            {
                break;
            }
            assert!(
                Instant::now() < stopped_deadline,
                "accepted-Quit child never stopped"
            );
            std::thread::yield_now();
        }

        writer.write_all(b"Q").expect("accept Quit before reply");
        assert_eq!(
            ack_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian applied accepted-Quit attribution"),
            SUPERVISED_QUIT_ACK
        );
        kill(
            Pid::from_raw(i32::try_from(pid).expect("pid fits i32")),
            Signal::SIGCONT,
        )
        .expect("let accepted-Quit child exit zero");
        let deadline = Instant::now() + Duration::from_secs(5);
        while kill(
            Pid::from_raw(i32::try_from(pid).expect("pid fits i32")),
            None,
        )
        .is_ok()
        {
            assert!(Instant::now() < deadline, "accepted-Quit child stayed live");
            std::thread::yield_now();
        }
        drop(writer);

        let report = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian finishes after host EOF")
            .expect("accepted Quit is a clean supervised shutdown");
        assert_eq!(report.1.self_termination_count, 0);
        worker.join().expect("guardian worker joins");
    }

    struct AckSender(mpsc::Sender<[u8; SUPERVISED_QUIT_ACK.len()]>);

    impl io::Write for AckSender {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let ack: [u8; SUPERVISED_QUIT_ACK.len()] = bytes.try_into().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "unexpected acknowledgment size")
            })?;
            self.0.send(ack).map_err(io::Error::other)?;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
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
    fn pre_child_liveness_bytes_fail_and_revoke_prepared_resources() {
        let (reader, mut writer) = liveness_pipe();
        std::io::Write::write_all(&mut writer, &[1_u8]).expect("write invalid liveness byte");
        let spawned = Arc::new(AtomicBool::new(false));
        let spawned_in_callback = Arc::clone(&spawned);
        let revoked = Arc::new(AtomicBool::new(false));
        let revoked_in_callback = Arc::clone(&revoked);
        let error = run_with_registration(
            long_running_command(),
            reader,
            io::sink(),
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
        assert!(revoked.load(Ordering::SeqCst));
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
    fn liveness_without_a_writer_fails_before_spawn() {
        let (reader, writer) = liveness_pipe();
        drop(writer);
        let spawned = Arc::new(AtomicBool::new(false));
        let spawned_in_callback = Arc::clone(&spawned);
        let error = run_with_registration(
            long_running_command(),
            reader,
            io::sink(),
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
        let error = run_with_registration(
            long_running_command(),
            file,
            io::sink(),
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
            let result = run_with_registration(
                long_running_command(),
                reader,
                io::sink(),
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
        let mut owner = HostGuardian::from_registered_parts(guardian, writer, None, group_pid)
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
        let mut guardian_command = Command::new("/bin/sh");
        guardian_command
            .args(["-c", "read ignored || exit 0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut guardian = guardian_command.spawn().expect("spawn guardian stand-in");
        let writer = guardian.stdin.take().expect("guardian liveness writer");
        let mut owner = HostGuardian::from_registered_parts(guardian, writer, None, group_pid)
            .expect("register host guardian owner");
        let status = owner.shutdown().expect("orderly EOF shutdown");
        assert!(status.success());
        assert_eq!(owner.group_pid(), None);
        let error = owner
            .shutdown()
            .expect_err("terminal owner must not reuse its numeric group id");
        assert!(error.to_string().contains("already terminal"), "{error}");
        terminate_registered_group(group_pid).expect("remove orderly-shutdown test group");
        let group_status = group.wait().expect("wait orderly-shutdown test group");
        assert_eq!(group_status.signal(), Some(9));
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
        let mut owner = HostGuardian::from_registered_parts(guardian, writer, None, group_pid)
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
        let error = HostGuardian::from_registered_parts(guardian, writer, None, 0)
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
        let error = HostGuardian::from_registered_parts(guardian, writer, None, u32::MAX)
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
        let error = HostGuardian::from_registered_parts(guardian, writer, None, group_pid)
            .expect_err("inheritable writer must reject registration");
        assert!(error.to_string().contains("require FD_CLOEXEC"), "{error}");
        let guardian_pid = i32::try_from(guardian_pid).expect("guardian pid fits i32");
        assert_eq!(kill(Pid::from_raw(guardian_pid), None), Err(Errno::ESRCH));
        terminate_registered_group(group_pid).expect("remove writer-rejection test group");
        let group_status = group.wait().expect("wait writer-rejection test group");
        assert_eq!(group_status.signal(), Some(9));
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
