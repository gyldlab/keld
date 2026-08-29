//! macOS host-death guardian for supervised Bun process groups.
//!
//! This is supervisor cleanup, not App Sandbox containment. The caller owns
//! the guardian process and the only liveness writer. The guardian process
//! calls [`run`] for the generic one-group API or [`run_guarded_primary`] for
//! KEL-96's persistent fresh-generation primary owner.

#![deny(unsafe_op_in_unsafe_fn)]

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
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

use crate::unix_role::{
    BoundRoleGeneration, DEFAULT_ADMISSION_TIMEOUT, RoleEvent, RoleGenerationLease,
    RoleGenerationOwner, RoleOwner,
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
const SUPERVISED_ORDERLY_SHUTDOWN: u8 = b'S';
const SUPERVISED_ORDERLY_ACK: [u8; 3] = *b"KSA";
const SUPERVISED_QUIT_ACK_DEADLINE: Duration = Duration::from_secs(5);
const GUARDIAN_SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);
const GENERATION_CONTROL_MAGIC: [u8; 4] = *b"KGC1";
const GENERATION_CONTROL_PAYLOAD_MAX: usize = 384;
const GENERATION_CONTROL_RECORD_LEN: usize = 404;
const GENERATION_CONTROL_POLL: Duration = Duration::from_millis(20);

const CONTROL_PREPARE: u8 = 1;
const CONTROL_PREPARED: u8 = 2;
const CONTROL_SPAWNED: u8 = 3;
const CONTROL_REGISTERED: u8 = 4;
const CONTROL_REVOKE: u8 = 5;
const CONTROL_REVOKED: u8 = 6;
const CONTROL_CLEAR: u8 = 7;
const CONTROL_CLEARED: u8 = 8;
const RECOVERY_PENDING: u8 = 0;
const RECOVERY_ARMED: u8 = 1;
const RECOVERY_DENIED: u8 = 2;

#[derive(Debug)]
struct GenerationControlRecord {
    kind: u8,
    cause: u8,
    attempt: u32,
    pid: u32,
    payload: Vec<u8>,
}

struct GenerationControlPeer {
    stream: std::os::unix::net::UnixStream,
    reader: GenerationControlReader,
}

impl GenerationControlRecord {
    fn new(kind: u8, attempt: u32) -> Self {
        Self {
            kind,
            cause: 0,
            attempt,
            pid: 0,
            payload: Vec::new(),
        }
    }

    fn with_pid(mut self, pid: u32) -> Self {
        self.pid = pid;
        self
    }

    fn with_cause(mut self, cause: RevocationCause) -> Self {
        self.cause = encode_revocation_cause(cause);
        self
    }

    fn with_payload(mut self, payload: impl Into<Vec<u8>>) -> Self {
        self.payload = payload.into();
        self
    }
}

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

    /// Authenticates the private guardian as a persistent primary-generation
    /// owner and starts the host half of the KEL-75 generation driver.
    ///
    /// Unlike [`Self::register_until`], the authenticated registration stream
    /// remains open as the bounded `KGC1` generation-control channel. The
    /// separate stdin pipe retains its existing host-death/accepted-Quit
    /// contract.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle failure if guardian authentication, handle
    /// extraction, control-channel setup, or host-worker creation fails.
    pub fn register_guarded_primary_until(
        mut self,
        deadline: Instant,
    ) -> Result<GuardedPrimary, RuntimeError> {
        let admission = self
            .registration
            .accept_authenticated_until(deadline, &NoopRegistrationObserver)
            .map_err(|source| lifecycle_error("macOS guardian registration admission", source));
        let control = match admission {
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
        let child = self.child.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian generation owner",
                io::Error::new(io::ErrorKind::NotConnected, "guardian child is missing"),
            )
        })?;
        let liveness_writer = self.liveness_writer.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian generation owner",
                io::Error::new(io::ErrorKind::NotConnected, "liveness writer is missing"),
            )
        })?;
        if let Err(error) = require_close_on_exec(&liveness_writer) {
            return Err(reject_host_registration(
                child,
                liveness_writer,
                None,
                error,
            ));
        }
        let Some(quit_ack_reader) = self.quit_ack_reader.take() else {
            return Err(reject_host_registration(
                child,
                liveness_writer,
                None,
                lifecycle_error(
                    "macOS guardian generation acknowledgment",
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "guarded primary requires the supervised acknowledgment pipe",
                    ),
                ),
            ));
        };
        GuardedPrimary::start(child, liveness_writer, quit_ack_reader, control)
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

/// One host-visible update from the guarded primary-generation owner.
#[derive(Debug)]
pub enum GuardedPrimaryUpdate {
    /// Existing KEL-75 lifecycle event.
    Role(RoleEvent),
    /// Newly authenticated stream for one generation.
    Bound(BoundRoleGeneration),
}

/// Host-owned handle for one persistent guardian and its primary generations.
///
/// The value owns the guardian process, sole liveness writer, current-group
/// fail-safe and host half of KEL-75 generation admission. It exposes no raw
/// control records and no child/process-group mutation API.
#[derive(Debug)]
pub struct GuardedPrimary {
    child: Child,
    liveness_writer: Option<ChildStdin>,
    quit_ack_reader: ChildStdout,
    current_generation: Arc<Mutex<Option<RegisteredPrimaryGeneration>>>,
    recovery_decision: Arc<AtomicU8>,
    updates_rx: Receiver<GuardedPrimaryUpdate>,
    worker: Option<JoinHandle<Result<(), RuntimeError>>>,
    shutdown_attributed: bool,
}

#[derive(Debug, Clone, Copy)]
struct RegisteredPrimaryGeneration {
    attempt: u32,
    group_pid: u32,
}

impl GuardedPrimary {
    fn start(
        mut child: Child,
        liveness_writer: ChildStdin,
        quit_ack_reader: ChildStdout,
        control: std::os::unix::net::UnixStream,
    ) -> Result<Self, RuntimeError> {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(RuntimeError::GuardianExited {
                    group_pid: 0,
                    exit_code: status.code(),
                    cleanup_error: None,
                });
            }
            Ok(None) => {}
            Err(source) => {
                return Err(reject_host_registration(
                    child,
                    liveness_writer,
                    None,
                    lifecycle_error("macOS guardian generation process inspection", source),
                ));
            }
        }
        let current_generation = Arc::new(Mutex::new(None));
        let recovery_decision = Arc::new(AtomicU8::new(RECOVERY_PENDING));
        let (updates_tx, updates_rx) = mpsc::channel();
        let generation_for_worker = Arc::clone(&current_generation);
        let recovery_for_worker = Arc::clone(&recovery_decision);
        let worker = match thread::Builder::new()
            .name("keld-runtime-guardian-generation-host".to_owned())
            .spawn(move || {
                run_generation_host(
                    control,
                    &updates_tx,
                    &generation_for_worker,
                    &recovery_for_worker,
                )
            }) {
            Ok(worker) => worker,
            Err(source) => {
                return Err(reject_host_registration(
                    child,
                    liveness_writer,
                    None,
                    lifecycle_error("macOS guardian generation host worker", source),
                ));
            }
        };
        Ok(Self {
            child,
            liveness_writer: Some(liveness_writer),
            quit_ack_reader,
            current_generation,
            recovery_decision,
            updates_rx,
            worker: Some(worker),
            shutdown_attributed: false,
        })
    }

    /// OS process id of the persistent guardian.
    #[must_use]
    pub fn guardian_pid(&self) -> u32 {
        self.child.id()
    }

    /// Current registered Bun process-group leader, when a generation is live.
    #[must_use]
    pub fn group_pid(&self) -> Option<u32> {
        lock_generation(&self.current_generation).map(|generation| generation.group_pid)
    }

    /// Arms restart only after the initial generation has reached host Ready.
    pub fn arm_recovery(&self) {
        self.recovery_decision
            .store(RECOVERY_ARMED, Ordering::Release);
    }

    /// Permanently rejects successor preparation for a failed startup.
    pub fn deny_recovery(&self) {
        self.recovery_decision
            .store(RECOVERY_DENIED, Ordering::Release);
    }

    /// Reports app-link failure for the named current generation.
    ///
    /// A stale reader is ignored. For the current attempt, runtime signals the
    /// enrolled group; the existing guardian-side Supervisor observes that
    /// process failure, performs KEL-75 revocation, and applies its one restart
    /// policy. Core never signals or restarts a process itself.
    ///
    /// # Errors
    ///
    /// Returns the owning group-signal failure.
    pub fn fail_current_generation(&self, attempt: u32) -> Result<(), RuntimeError> {
        let current = lock_generation(&self.current_generation);
        let Some(generation) = *current else {
            return Ok(());
        };
        if generation.attempt != attempt {
            return Ok(());
        }
        terminate_registered_group(generation.group_pid)
    }

    /// Waits for one generation update up to `timeout`.
    #[must_use]
    pub fn recv_update(&self, timeout: Duration) -> Option<GuardedPrimaryUpdate> {
        self.updates_rx.recv_timeout(timeout).ok()
    }

    /// Records accepted host shutdown before a correlated reply is published.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error when the guardian cannot acknowledge
    /// attribution while the host still owns the session.
    pub fn accept_shutdown(&mut self) -> Result<(), RuntimeError> {
        let ack_reader = &mut self.quit_ack_reader;
        let writer = self.liveness_writer.as_mut().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervised-Quit control",
                io::Error::new(io::ErrorKind::NotConnected, "liveness writer is missing"),
            )
        })?;
        set_liveness_nonblocking(writer)?;
        writer
            .write_all(&[SUPERVISED_QUIT_ACCEPTED])
            .and_then(|()| writer.flush())
            .map_err(|source| lifecycle_error("macOS guardian supervised-Quit control", source))?;
        read_supervised_quit_ack(ack_reader)?;
        self.shutdown_attributed = true;
        Ok(())
    }

    /// Polls guardian/control health and invokes the current-group fail-safe
    /// before returning a terminal error.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::GuardianExited`] for guardian death or the
    /// host-worker's exact typed control/admission failure.
    pub fn poll_fatal(&mut self) -> Result<(), RuntimeError> {
        match self.child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                self.liveness_writer.take();
                let group_pid = take_generation(&self.current_generation)
                    .map_or(0, |generation| generation.group_pid);
                let cleanup_error = (group_pid != 0)
                    .then(|| terminate_registered_group(group_pid).err())
                    .flatten()
                    .map(|error| io::Error::other(error.to_string()));
                Err(RuntimeError::GuardianExited {
                    group_pid,
                    exit_code: status.code(),
                    cleanup_error,
                })?;
            }
            Err(source) => {
                return Err(lifecycle_error(
                    "macOS guardian generation process inspection",
                    source,
                ));
            }
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let result = self.join_worker();
            if let Err(error) = result {
                let cleanup = self.fail_safe_current_group().err();
                return match cleanup {
                    Some(cleanup_error) => Err(collapse_failures(vec![error, cleanup_error])),
                    None => Err(error),
                };
            }
        }
        Ok(())
    }

    /// Closes the liveness writer, waits the guardian, and joins generation
    /// control after the guardian has revoked/cleared the current generation.
    ///
    /// # Errors
    ///
    /// Returns a typed failure for timeout, unexpected guardian exit, current
    /// group cleanup failure, or generation worker failure.
    pub fn shutdown(&mut self) -> Result<ExitStatus, RuntimeError> {
        let mut failures = Vec::new();
        if let Err(error) = self.prepare_orderly_shutdown() {
            failures.push(error);
        }
        self.liveness_writer.take();
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        let mut status = None;
        loop {
            match self.child.try_wait() {
                Ok(Some(observed)) => {
                    status = Some(observed);
                    break;
                }
                Ok(None) if Instant::now() < deadline => thread::yield_now(),
                Ok(None) => {
                    failures.push(lifecycle_error(
                        "macOS guardian generation shutdown deadline",
                        io::Error::new(io::ErrorKind::TimedOut, "guardian did not exit"),
                    ));
                    if let Err(error) = self.fail_safe_current_group() {
                        failures.push(error);
                    }
                    if let Err(source) = self.child.kill()
                        && source.kind() != io::ErrorKind::InvalidInput
                    {
                        failures.push(lifecycle_error(
                            "macOS guardian generation process kill",
                            source,
                        ));
                    }
                    match self.child.wait() {
                        Ok(observed) => status = Some(observed),
                        Err(source) => failures.push(lifecycle_error(
                            "macOS guardian generation process wait",
                            source,
                        )),
                    }
                    break;
                }
                Err(source) => {
                    failures.push(lifecycle_error(
                        "macOS guardian generation shutdown wait",
                        source,
                    ));
                    if let Err(error) = self.fail_safe_current_group() {
                        failures.push(error);
                    }
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        if let Err(error) = self.join_worker() {
            failures.push(error);
        }
        if let Some(observed) = status
            && !observed.success()
        {
            let group_pid = take_generation(&self.current_generation)
                .map_or(0, |generation| generation.group_pid);
            let cleanup_error = (group_pid != 0)
                .then(|| terminate_registered_group(group_pid).err())
                .flatten()
                .map(|error| io::Error::other(error.to_string()));
            failures.push(RuntimeError::GuardianExited {
                group_pid,
                exit_code: observed.code(),
                cleanup_error,
            });
        }
        if failures.is_empty() {
            status.ok_or_else(|| {
                lifecycle_error(
                    "macOS guardian generation shutdown wait",
                    io::Error::other("guardian ended without an observable status"),
                )
            })
        } else {
            Err(collapse_failures(failures))
        }
    }

    fn prepare_orderly_shutdown(&mut self) -> Result<(), RuntimeError> {
        if self.shutdown_attributed {
            return Ok(());
        }
        let writer = self.liveness_writer.as_mut().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian orderly-shutdown control",
                io::Error::new(io::ErrorKind::NotConnected, "liveness writer is missing"),
            )
        })?;
        let ack_reader = &mut self.quit_ack_reader;
        set_liveness_nonblocking(writer)?;
        writer
            .write_all(&[SUPERVISED_ORDERLY_SHUTDOWN])
            .and_then(|()| writer.flush())
            .map_err(|source| lifecycle_error("macOS guardian orderly-shutdown control", source))?;
        read_supervised_orderly_ack(ack_reader)?;
        self.shutdown_attributed = true;
        Ok(())
    }

    fn join_worker(&mut self) -> Result<(), RuntimeError> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.join().map_err(|_| {
            lifecycle_error(
                "macOS guardian generation host worker",
                io::Error::other("generation host worker panicked"),
            )
        })?
    }

    fn fail_safe_current_group(&self) -> Result<(), RuntimeError> {
        take_generation(&self.current_generation).map_or(Ok(()), |generation| {
            terminate_registered_group(generation.group_pid)
        })
    }
}

fn lock_generation(
    generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
) -> std::sync::MutexGuard<'_, Option<RegisteredPrimaryGeneration>> {
    match generation.lock() {
        Ok(generation) => generation,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn take_generation(
    generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
) -> Option<RegisteredPrimaryGeneration> {
    lock_generation(generation).take()
}

impl Drop for GuardedPrimary {
    fn drop(&mut self) {
        if self.liveness_writer.is_none() {
            return;
        }
        let _ = self.shutdown();
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
/// termination is fatal; [`run_guarded_primary`] owns the KEL-96/T3
/// multi-generation path.
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

struct GuardedPrimaryPreparer<C> {
    command_factory: C,
    control: Arc<Mutex<GenerationControlPeer>>,
    host_unavailable: Arc<AtomicBool>,
}

impl<C> ChildPreparer for GuardedPrimaryPreparer<C>
where
    C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
{
    type Lease = GuardedPrimaryLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        let response = control_exchange(
            &self.control,
            &GenerationControlRecord::new(CONTROL_PREPARE, attempt),
            CONTROL_PREPARED,
            attempt,
        )?;
        let app_link = match String::from_utf8(response.payload) {
            Ok(app_link) if !app_link.is_empty() => app_link,
            Ok(_) => {
                let primary = lifecycle_error(
                    "macOS guardian generation preparation",
                    io::Error::new(io::ErrorKind::InvalidData, "prepared app link is empty"),
                );
                return Err(collapse_guarded_preparation_failure(
                    &self.control,
                    attempt,
                    primary,
                ));
            }
            Err(source) => {
                let primary = lifecycle_error(
                    "macOS guardian generation preparation",
                    io::Error::new(io::ErrorKind::InvalidData, source),
                );
                return Err(collapse_guarded_preparation_failure(
                    &self.control,
                    attempt,
                    primary,
                ));
            }
        };
        let mut command = match (self.command_factory)(&app_link) {
            Ok(command) => command,
            Err(primary) => {
                return Err(collapse_guarded_preparation_failure(
                    &self.control,
                    attempt,
                    primary,
                ));
            }
        };
        command
            .env_remove(REGISTRATION_ENV)
            .process_group(0)
            .stdin(Stdio::null());
        Ok(PreparedChild {
            command,
            lease: GuardedPrimaryLease {
                attempt,
                control: Arc::clone(&self.control),
                host_unavailable: Arc::clone(&self.host_unavailable),
                group_pid: None,
            },
        })
    }
}

fn collapse_guarded_preparation_failure(
    control: &Arc<Mutex<GenerationControlPeer>>,
    attempt: u32,
    primary: RuntimeError,
) -> RuntimeError {
    let revoke = control_exchange(
        control,
        &GenerationControlRecord::new(CONTROL_REVOKE, attempt)
            .with_cause(RevocationCause::AdmissionFailed),
        CONTROL_REVOKED,
        attempt,
    );
    let clear = revoke.and_then(|_| {
        control_exchange(
            control,
            &GenerationControlRecord::new(CONTROL_CLEAR, attempt),
            CONTROL_CLEARED,
            attempt,
        )
    });
    match clear {
        Ok(_) => primary,
        Err(cleanup) => collapse_failures(vec![primary, cleanup]),
    }
}

struct GuardedPrimaryLease {
    attempt: u32,
    control: Arc<Mutex<GenerationControlPeer>>,
    host_unavailable: Arc<AtomicBool>,
    group_pid: Option<u32>,
}

impl GenerationLease for GuardedPrimaryLease {
    fn child_spawned(&mut self, pid: u32, attempt: u32) -> Result<(), RuntimeError> {
        if attempt != self.attempt {
            return Err(lifecycle_error(
                "macOS guardian generation registration",
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "supervisor attempt differs from prepared generation",
                ),
            ));
        }
        validate_group_leader(pid)?;
        control_exchange(
            &self.control,
            &GenerationControlRecord::new(CONTROL_SPAWNED, attempt).with_pid(pid),
            CONTROL_REGISTERED,
            attempt,
        )?;
        self.group_pid = Some(pid);
        Ok(())
    }

    fn revoke(mut self, cause: RevocationCause) -> Result<(), RuntimeError> {
        let group_pid = self.group_pid.take();
        let host_available = !self.host_unavailable.load(Ordering::Acquire);
        let mut failures = Vec::new();
        if host_available
            && let Err(error) = control_exchange(
                &self.control,
                &GenerationControlRecord::new(CONTROL_REVOKE, self.attempt).with_cause(cause),
                CONTROL_REVOKED,
                self.attempt,
            )
        {
            failures.push(error);
        }
        if let Some(pid) = group_pid
            && let Err(error) = terminate_registered_group(pid)
        {
            failures.push(error);
        }
        if host_available
            && failures.is_empty()
            && let Err(error) = control_exchange(
                &self.control,
                &GenerationControlRecord::new(CONTROL_CLEAR, self.attempt)
                    .with_pid(group_pid.unwrap_or(0)),
                CONTROL_CLEARED,
                self.attempt,
            )
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

/// Runs a persistent macOS guardian whose single [`Supervisor`] requests a
/// fresh host-owned primary generation before every Bun spawn.
///
/// The authenticated guardian-registration stream becomes a bounded private
/// generation-control channel. The host remains the endpoint/token/stream
/// owner; the guardian remains the only Bun child, process-group, restart and
/// reap owner. Public kipc frame bytes are unchanged.
///
/// # Errors
///
/// Returns [`RuntimeError`] when either private bootstrap is invalid, the host
/// rejects a generation transition, the supervisor reaches a terminal
/// failure, or an unrequested status-zero termination stops supervision.
pub fn run_guarded_primary<R, C, A>(
    mut liveness: R,
    command_factory: C,
    mut quit_ack: A,
) -> Result<CapturedOutput, RuntimeError>
where
    R: Read + AsFd,
    C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
    A: Write,
{
    let control = connect_guardian_registration()?;
    control
        .set_read_timeout(Some(GENERATION_CONTROL_POLL))
        .map_err(|source| lifecycle_error("macOS guardian generation control deadline", source))?;
    control
        .set_write_timeout(Some(GENERATION_CONTROL_POLL))
        .map_err(|source| lifecycle_error("macOS guardian generation control deadline", source))?;
    validate_liveness_bootstrap(&mut liveness)?;
    let host_unavailable = Arc::new(AtomicBool::new(false));
    let supervisor = Supervisor::start_prepared(
        RestartPolicy::default(),
        GuardedPrimaryPreparer {
            command_factory,
            control: Arc::new(Mutex::new(GenerationControlPeer {
                stream: control,
                reader: GenerationControlReader::new(),
            })),
            host_unavailable: Arc::clone(&host_unavailable),
        },
    )?;

    set_liveness_nonblocking(&liveness)?;
    let mut liveness_result = Ok(());
    let mut quit_accepted = false;
    let mut orderly_requested = false;
    loop {
        match observe_guarded_liveness(
            &mut liveness,
            &supervisor,
            &mut quit_accepted,
            &mut orderly_requested,
            &mut quit_ack,
        ) {
            Ok(true) => {
                if !quit_accepted && !orderly_requested {
                    host_unavailable.store(true, Ordering::Release);
                }
                supervisor.shutdown();
                break;
            }
            Ok(false) => {}
            Err(error) => {
                liveness_result = Err(error);
                supervisor.shutdown();
                break;
            }
        }
        if let Some(event) = supervisor.recv_event(GENERATION_CONTROL_POLL)
            && matches!(
                event,
                crate::SupervisorEvent::RespawnFailed
                    | crate::SupervisorEvent::Failed { .. }
                    | crate::SupervisorEvent::CrashLoopTripped
                    | crate::SupervisorEvent::Stopped
            )
        {
            break;
        }
    }

    let outcome = supervisor.wait_for_outcome();
    let crash_ledger = supervisor.crash_ledger();
    let output = supervisor.output();
    let mut failures = Vec::new();
    if let Err(error) = liveness_result {
        failures.push(error);
    }
    match outcome {
        SupervisorOutcome::CrashLoop(error) | SupervisorOutcome::Failed(error) => {
            failures.push(error);
        }
        SupervisorOutcome::Stopped if !quit_accepted => {
            if let Some(termination) = crash_ledger.last_self_termination {
                failures.push(RuntimeError::ChildCrashed {
                    pid: termination.pid,
                    exit_code: termination.exit_code,
                    stderr_tail: output.stderr_tail(2_000),
                });
            }
        }
        SupervisorOutcome::Stopped => {}
    }
    if failures.is_empty() {
        Ok(output)
    } else {
        Err(collapse_failures(failures))
    }
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

#[allow(clippy::too_many_lines)] // one owner keeps every private protocol transition contiguous
fn run_generation_host(
    mut control: std::os::unix::net::UnixStream,
    updates_tx: &Sender<GuardedPrimaryUpdate>,
    current_generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
    recovery_decision: &AtomicU8,
) -> Result<(), RuntimeError> {
    control
        .set_read_timeout(Some(GENERATION_CONTROL_POLL))
        .map_err(|source| lifecycle_error("macOS guardian generation host deadline", source))?;
    control
        .set_write_timeout(Some(GENERATION_CONTROL_POLL))
        .map_err(|source| lifecycle_error("macOS guardian generation host deadline", source))?;
    let (events_tx, events_rx) = mpsc::channel();
    let (bound_tx, bound_rx) = mpsc::channel();
    let mut owner = RoleGenerationOwner::new(
        RoleOwner::Primary,
        DEFAULT_ADMISSION_TIMEOUT,
        events_tx,
        Some(bound_tx),
        #[cfg(test)]
        None,
    );
    let mut lease: Option<RoleGenerationLease> = None;
    let mut current_attempt = None;
    let mut revoked_attempt = None;
    let mut expected_attempt = 1_u32;
    let mut control_reader = GenerationControlReader::new();
    loop {
        if let Some(lease) = lease.as_mut() {
            lease.poll()?;
        }
        forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
        let record = match control_reader.poll(&mut control) {
            Ok(Some(record)) => record,
            Ok(None) => continue,
            Err(error)
                if runtime_io_kind(&error) == Some(io::ErrorKind::UnexpectedEof)
                    && lease.is_none()
                    && lock_generation(current_generation).is_none() =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        match record.kind {
            CONTROL_PREPARE => {
                if record.attempt > 1 {
                    await_recovery_decision(recovery_decision)?;
                }
                if record.attempt != expected_attempt
                    || current_attempt.is_some()
                    || lease.is_some()
                    || revoked_attempt.is_some()
                    || lock_generation(current_generation).is_some()
                    || record.pid != 0
                    || record.cause != 0
                    || !record.payload.is_empty()
                {
                    return Err(invalid_control_transition(
                        "prepare",
                        record.attempt,
                        expected_attempt,
                    ));
                }
                let provisioned = owner.provision(record.attempt)?;
                let response = GenerationControlRecord::new(CONTROL_PREPARED, record.attempt)
                    .with_payload(provisioned.app_link.as_bytes().to_vec());
                lease = Some(provisioned.lease);
                current_attempt = Some(record.attempt);
                forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                write_generation_control(&mut control, &response)?;
            }
            CONTROL_SPAWNED => {
                if current_attempt != Some(record.attempt)
                    || record.pid == 0
                    || record.cause != 0
                    || !record.payload.is_empty()
                    || lock_generation(current_generation).is_some()
                {
                    return Err(invalid_control_transition(
                        "spawned",
                        record.attempt,
                        expected_attempt,
                    ));
                }
                let current = lease.as_mut().ok_or_else(|| {
                    invalid_control_transition("spawned", record.attempt, expected_attempt)
                })?;
                current.child_spawned(record.pid, record.attempt)?;
                *lock_generation(current_generation) = Some(RegisteredPrimaryGeneration {
                    attempt: record.attempt,
                    group_pid: record.pid,
                });
                forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                write_generation_control(
                    &mut control,
                    &GenerationControlRecord::new(CONTROL_REGISTERED, record.attempt)
                        .with_pid(record.pid),
                )?;
            }
            CONTROL_REVOKE => {
                if current_attempt != Some(record.attempt)
                    || revoked_attempt.is_some()
                    || record.pid != 0
                    || !record.payload.is_empty()
                {
                    return Err(invalid_control_transition(
                        "revoke",
                        record.attempt,
                        expected_attempt,
                    ));
                }
                let cause = decode_revocation_cause(record.cause)?;
                let current = lease.take().ok_or_else(|| {
                    invalid_control_transition("revoke", record.attempt, expected_attempt)
                })?;
                current.revoke(cause)?;
                revoked_attempt = Some(record.attempt);
                forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                write_generation_control(
                    &mut control,
                    &GenerationControlRecord::new(CONTROL_REVOKED, record.attempt),
                )?;
            }
            CONTROL_CLEAR => {
                let registered = lock_generation(current_generation)
                    .map_or(0, |generation| generation.group_pid);
                if current_attempt != Some(record.attempt)
                    || revoked_attempt != Some(record.attempt)
                    || record.cause != 0
                    || !record.payload.is_empty()
                    || record.pid != registered
                {
                    return Err(invalid_control_transition(
                        "clear",
                        record.attempt,
                        expected_attempt,
                    ));
                }
                take_generation(current_generation);
                write_generation_control(
                    &mut control,
                    &GenerationControlRecord::new(CONTROL_CLEARED, record.attempt)
                        .with_pid(record.pid),
                )?;
                current_attempt = None;
                revoked_attempt = None;
                expected_attempt = expected_attempt.checked_add(1).ok_or_else(|| {
                    lifecycle_error(
                        "macOS guardian generation host transition",
                        io::Error::other("supervisor attempt counter exhausted"),
                    )
                })?;
            }
            _ => {
                return Err(lifecycle_error(
                    "macOS guardian generation host transition",
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("unexpected control kind {}", record.kind),
                    ),
                ));
            }
        }
    }
}

fn await_recovery_decision(decision: &AtomicU8) -> Result<(), RuntimeError> {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        match decision.load(Ordering::Acquire) {
            RECOVERY_ARMED => return Ok(()),
            RECOVERY_DENIED => {
                return Err(lifecycle_error(
                    "macOS guardian generation host transition",
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "successor preparation was rejected before initial Ready",
                    ),
                ));
            }
            RECOVERY_PENDING if Instant::now() < deadline => thread::yield_now(),
            RECOVERY_PENDING => {
                return Err(lifecycle_error(
                    "macOS guardian generation host transition",
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "initial Ready did not decide recovery before the deadline",
                    ),
                ));
            }
            _ => {
                return Err(lifecycle_error(
                    "macOS guardian generation host transition",
                    io::Error::new(io::ErrorKind::InvalidData, "invalid recovery decision"),
                ));
            }
        }
    }
}

fn forward_generation_updates(
    updates_tx: &Sender<GuardedPrimaryUpdate>,
    bound_rx: &Receiver<BoundRoleGeneration>,
    events_rx: &Receiver<RoleEvent>,
) -> Result<(), RuntimeError> {
    while let Ok(bound) = bound_rx.try_recv() {
        updates_tx
            .send(GuardedPrimaryUpdate::Bound(bound))
            .map_err(|_| generation_consumer_gone())?;
    }
    while let Ok(event) = events_rx.try_recv() {
        updates_tx
            .send(GuardedPrimaryUpdate::Role(event))
            .map_err(|_| generation_consumer_gone())?;
    }
    Ok(())
}

fn generation_consumer_gone() -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation update delivery",
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            "host generation consumer ended before shutdown",
        ),
    )
}

fn invalid_control_transition(phase: &str, attempt: u32, expected: u32) -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation host transition",
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid {phase} transition for attempt {attempt}; expected {expected}"),
        ),
    )
}

fn runtime_io_kind(error: &RuntimeError) -> Option<io::ErrorKind> {
    match error {
        RuntimeError::Spawn(source) | RuntimeError::Lifecycle { source, .. } => Some(source.kind()),
        RuntimeError::CrashLoop { .. }
        | RuntimeError::ChildCrashed { .. }
        | RuntimeError::GuardianExited { .. } => None,
    }
}

fn is_control_poll_timeout(kind: io::ErrorKind) -> bool {
    matches!(kind, io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

fn write_generation_control(
    writer: &mut impl Write,
    record: &GenerationControlRecord,
) -> Result<(), RuntimeError> {
    if record.payload.len() > GENERATION_CONTROL_PAYLOAD_MAX {
        return Err(lifecycle_error(
            "macOS guardian generation control encode",
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "generation-control payload exceeds its fixed bound",
            ),
        ));
    }
    let payload_len = u16::try_from(record.payload.len()).map_err(|source| {
        lifecycle_error(
            "macOS guardian generation control encode",
            io::Error::new(io::ErrorKind::InvalidInput, source),
        )
    })?;
    let mut bytes = [0_u8; GENERATION_CONTROL_RECORD_LEN];
    bytes[..4].copy_from_slice(&GENERATION_CONTROL_MAGIC);
    bytes[4] = 1;
    bytes[5] = record.kind;
    bytes[6] = record.cause;
    bytes[8..12].copy_from_slice(&record.attempt.to_be_bytes());
    bytes[12..16].copy_from_slice(&record.pid.to_be_bytes());
    bytes[16..18].copy_from_slice(&payload_len.to_be_bytes());
    bytes[20..20 + record.payload.len()].copy_from_slice(&record.payload);
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let mut written = 0;
    while written < bytes.len() {
        match writer.write(&bytes[written..]) {
            Ok(0) => {
                return Err(lifecycle_error(
                    "macOS guardian generation control write",
                    io::Error::new(
                        io::ErrorKind::WriteZero,
                        "generation-control peer accepted zero bytes",
                    ),
                ));
            }
            Ok(count) => written += count,
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) if is_control_poll_timeout(source.kind()) && Instant::now() < deadline => {
                thread::yield_now();
            }
            Err(source) => {
                return Err(lifecycle_error(
                    "macOS guardian generation control write",
                    source,
                ));
            }
        }
        if Instant::now() >= deadline && written < bytes.len() {
            return Err(lifecycle_error(
                "macOS guardian generation control write",
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "generation-control record exceeded its write deadline",
                ),
            ));
        }
    }
    writer
        .flush()
        .map_err(|source| lifecycle_error("macOS guardian generation control flush", source))
}

#[cfg(test)]
fn read_generation_control(
    reader: &mut impl Read,
) -> Result<GenerationControlRecord, RuntimeError> {
    let mut bytes = [0_u8; GENERATION_CONTROL_RECORD_LEN];
    reader
        .read_exact(&mut bytes)
        .map_err(|source| lifecycle_error("macOS guardian generation control read", source))?;
    decode_generation_control(&bytes)
}

struct GenerationControlReader {
    bytes: [u8; GENERATION_CONTROL_RECORD_LEN],
    filled: usize,
    started: Option<Instant>,
}

impl GenerationControlReader {
    const fn new() -> Self {
        Self {
            bytes: [0; GENERATION_CONTROL_RECORD_LEN],
            filled: 0,
            started: None,
        }
    }

    fn poll(
        &mut self,
        reader: &mut (impl Read + ?Sized),
    ) -> Result<Option<GenerationControlRecord>, RuntimeError> {
        loop {
            match reader.read(&mut self.bytes[self.filled..]) {
                Ok(0) => {
                    return Err(lifecycle_error(
                        "macOS guardian generation control read",
                        io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "generation-control peer closed mid-session",
                        ),
                    ));
                }
                Ok(read) => {
                    if self.filled == 0 {
                        self.started = Some(Instant::now());
                    }
                    self.filled += read;
                    if self.filled == GENERATION_CONTROL_RECORD_LEN {
                        self.filled = 0;
                        self.started = None;
                        let record = decode_generation_control(&self.bytes)?;
                        self.bytes.fill(0);
                        return Ok(Some(record));
                    }
                    if self
                        .started
                        .is_some_and(|started| started.elapsed() >= GUARDIAN_SHUTDOWN_DEADLINE)
                    {
                        return Err(lifecycle_error(
                            "macOS guardian generation control read",
                            io::Error::new(
                                io::ErrorKind::TimedOut,
                                "generation-control record exceeded its read deadline",
                            ),
                        ));
                    }
                }
                Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
                Err(source) if is_control_poll_timeout(source.kind()) => {
                    if self
                        .started
                        .is_some_and(|started| started.elapsed() >= GUARDIAN_SHUTDOWN_DEADLINE)
                    {
                        return Err(lifecycle_error(
                            "macOS guardian generation control read",
                            io::Error::new(
                                io::ErrorKind::TimedOut,
                                "generation-control record exceeded its read deadline",
                            ),
                        ));
                    }
                    return Ok(None);
                }
                Err(source) => {
                    return Err(lifecycle_error(
                        "macOS guardian generation control read",
                        source,
                    ));
                }
            }
        }
    }
}

fn decode_generation_control(
    bytes: &[u8; GENERATION_CONTROL_RECORD_LEN],
) -> Result<GenerationControlRecord, RuntimeError> {
    if bytes[..4] != GENERATION_CONTROL_MAGIC
        || bytes[4] != 1
        || bytes[7] != 0
        || bytes[18] != 0
        || bytes[19] != 0
    {
        return Err(lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "generation-control header is malformed or unsupported",
            ),
        ));
    }
    let attempt = u32::from_be_bytes(bytes[8..12].try_into().map_err(|_| {
        lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(io::ErrorKind::InvalidData, "attempt field is truncated"),
        )
    })?);
    let pid = u32::from_be_bytes(bytes[12..16].try_into().map_err(|_| {
        lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(io::ErrorKind::InvalidData, "pid field is truncated"),
        )
    })?);
    let payload_len = usize::from(u16::from_be_bytes(bytes[16..18].try_into().map_err(
        |_| {
            lifecycle_error(
                "macOS guardian generation control decode",
                io::Error::new(io::ErrorKind::InvalidData, "payload length is truncated"),
            )
        },
    )?));
    if payload_len > GENERATION_CONTROL_PAYLOAD_MAX
        || bytes[20 + payload_len..].iter().any(|byte| *byte != 0)
    {
        return Err(lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "generation-control payload bound or padding is invalid",
            ),
        ));
    }
    Ok(GenerationControlRecord {
        kind: bytes[5],
        cause: bytes[6],
        attempt,
        pid,
        payload: bytes[20..20 + payload_len].to_vec(),
    })
}

fn control_exchange(
    control: &Arc<Mutex<GenerationControlPeer>>,
    request: &GenerationControlRecord,
    expected_kind: u8,
    expected_attempt: u32,
) -> Result<GenerationControlRecord, RuntimeError> {
    let mut control = match control.lock() {
        Ok(control) => control,
        Err(poisoned) => poisoned.into_inner(),
    };
    let GenerationControlPeer { stream, reader } = &mut *control;
    write_generation_control(stream, request)?;
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let response = loop {
        if let Some(response) = reader.poll(stream)? {
            break response;
        }
        if Instant::now() >= deadline {
            return Err(lifecycle_error(
                "macOS guardian generation control transition",
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "host did not acknowledge the generation transition before the deadline",
                ),
            ));
        }
    };
    if response.kind != expected_kind || response.attempt != expected_attempt {
        return Err(lifecycle_error(
            "macOS guardian generation control transition",
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "expected kind {expected_kind} attempt {expected_attempt}, received kind {} attempt {}",
                    response.kind, response.attempt
                ),
            ),
        ));
    }
    validate_control_response(request, &response)?;
    Ok(response)
}

fn validate_control_response(
    request: &GenerationControlRecord,
    response: &GenerationControlRecord,
) -> Result<(), RuntimeError> {
    let valid = match response.kind {
        CONTROL_PREPARED => {
            request.kind == CONTROL_PREPARE
                && response.pid == 0
                && response.cause == 0
                && !response.payload.is_empty()
        }
        CONTROL_REGISTERED => {
            request.kind == CONTROL_SPAWNED
                && response.pid == request.pid
                && response.cause == 0
                && response.payload.is_empty()
        }
        CONTROL_REVOKED => {
            request.kind == CONTROL_REVOKE
                && response.pid == 0
                && response.cause == 0
                && response.payload.is_empty()
        }
        CONTROL_CLEARED => {
            request.kind == CONTROL_CLEAR
                && response.pid == request.pid
                && response.cause == 0
                && response.payload.is_empty()
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(lifecycle_error(
            "macOS guardian generation control transition",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "generation-control response fields do not match the requested transition",
            ),
        ))
    }
}

const fn encode_revocation_cause(cause: RevocationCause) -> u8 {
    match cause {
        RevocationCause::ChildExited => 1,
        RevocationCause::Shutdown => 2,
        RevocationCause::CaptureFailed => 3,
        RevocationCause::AdmissionFailed => 4,
        RevocationCause::SpawnFailed => 5,
        RevocationCause::WaitFailed => 6,
    }
}

fn decode_revocation_cause(value: u8) -> Result<RevocationCause, RuntimeError> {
    match value {
        1 => Ok(RevocationCause::ChildExited),
        2 => Ok(RevocationCause::Shutdown),
        3 => Ok(RevocationCause::CaptureFailed),
        4 => Ok(RevocationCause::AdmissionFailed),
        5 => Ok(RevocationCause::SpawnFailed),
        6 => Ok(RevocationCause::WaitFailed),
        _ => Err(lifecycle_error(
            "macOS guardian generation control decode",
            io::Error::new(io::ErrorKind::InvalidData, "unknown revocation cause"),
        )),
    }
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
    OrderlyShutdown,
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
        Ok(1) if byte[0] == SUPERVISED_ORDERLY_SHUTDOWN => Ok(HostLiveness::OrderlyShutdown),
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
        HostLiveness::OrderlyShutdown => Err(lifecycle_error(
            "macOS guardian supervised host-liveness control",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "orderly-shutdown control is unavailable on the one-generation API",
            ),
        )),
    }
}

fn observe_guarded_liveness(
    reader: &mut impl Read,
    supervisor: &Supervisor,
    quit_accepted: &mut bool,
    orderly_requested: &mut bool,
    acknowledgment: &mut impl Write,
) -> Result<bool, RuntimeError> {
    match poll_host_liveness(reader)? {
        HostLiveness::Live => Ok(false),
        HostLiveness::Dead => {
            supervisor.shutdown();
            Ok(true)
        }
        HostLiveness::QuitAccepted if *quit_accepted || *orderly_requested => Err(lifecycle_error(
            "macOS guardian accepted-shutdown control",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "shutdown attribution was already selected",
            ),
        )),
        HostLiveness::QuitAccepted => {
            *quit_accepted = true;
            supervisor.accept_shutdown();
            acknowledgment
                .write_all(&SUPERVISED_QUIT_ACK)
                .and_then(|()| acknowledgment.flush())
                .map_err(|source| {
                    lifecycle_error("macOS guardian supervised-Quit acknowledgment", source)
                })?;
            Ok(false)
        }
        HostLiveness::OrderlyShutdown if *quit_accepted || *orderly_requested => {
            Err(lifecycle_error(
                "macOS guardian orderly-shutdown control",
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "shutdown attribution was already selected",
                ),
            ))
        }
        HostLiveness::OrderlyShutdown => {
            *orderly_requested = true;
            acknowledgment
                .write_all(&SUPERVISED_ORDERLY_ACK)
                .and_then(|()| acknowledgment.flush())
                .map_err(|source| {
                    lifecycle_error("macOS guardian orderly-shutdown acknowledgment", source)
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
        Err(Errno::EPERM) if matches!(getpgid(Some(Pid::from_raw(group))), Err(Errno::ESRCH)) => {
            // A post-Ready link failure may have already signalled the group
            // while the direct child was exiting. Darwin can report EPERM for
            // the now-retired numeric group. Require getpgid to prove the
            // leader gone; never suppress EPERM while that leader resolves.
            Ok(())
        }
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
    read_supervised_ack(
        reader,
        SUPERVISED_QUIT_ACK,
        "macOS guardian supervised-Quit acknowledgment",
        "accepted Quit",
    )
}

fn read_supervised_orderly_ack(reader: &mut ChildStdout) -> Result<(), RuntimeError> {
    read_supervised_ack(
        reader,
        SUPERVISED_ORDERLY_ACK,
        "macOS guardian orderly-shutdown acknowledgment",
        "orderly shutdown",
    )
}

fn read_supervised_ack(
    reader: &mut ChildStdout,
    expected: [u8; 3],
    phase: &'static str,
    action: &'static str,
) -> Result<(), RuntimeError> {
    let raw_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFL)
        .map_err(|source| lifecycle_error(phase, nix_io_error(source)))?;
    fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(raw_flags) | OFlag::O_NONBLOCK),
    )
    .map_err(|source| lifecycle_error(phase, nix_io_error(source)))?;
    let deadline = Instant::now() + SUPERVISED_QUIT_ACK_DEADLINE;
    let mut ack = [0_u8; 3];
    let mut filled = 0;
    while filled < ack.len() {
        match reader.read(&mut ack[filled..]) {
            Ok(0) => {
                return Err(lifecycle_error(
                    phase,
                    io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        format!("guardian exited before acknowledging {action}"),
                    ),
                ));
            }
            Ok(read) => filled += read,
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) if source.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(lifecycle_error(
                        phase,
                        io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!("guardian did not acknowledge {action} before the deadline"),
                        ),
                    ));
                }
                std::thread::yield_now();
            }
            Err(source) => {
                return Err(lifecycle_error(phase, source));
            }
        }
    }
    if ack == expected {
        Ok(())
    } else {
        Err(lifecycle_error(
            phase,
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("guardian returned an invalid {action} acknowledgment"),
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
    fn generation_control_codec_is_fixed_bounded_and_rejects_nonzero_padding() {
        let record = GenerationControlRecord::new(CONTROL_PREPARED, 7)
            .with_pid(41)
            .with_payload(b"socket#token".to_vec());
        let mut bytes = Vec::new();
        write_generation_control(&mut bytes, &record).expect("encode KGC1 record");
        assert_eq!(bytes.len(), GENERATION_CONTROL_RECORD_LEN);
        assert_eq!(&bytes[..4], b"KGC1");
        assert_eq!(bytes[4], 1);
        assert_eq!(bytes[5], CONTROL_PREPARED);
        assert_eq!(&bytes[8..12], &7_u32.to_be_bytes());
        assert_eq!(&bytes[12..16], &41_u32.to_be_bytes());
        assert_eq!(&bytes[16..18], &12_u16.to_be_bytes());
        let decoded = read_generation_control(&mut bytes.as_slice()).expect("decode KGC1 record");
        assert_eq!(decoded.kind, CONTROL_PREPARED);
        assert_eq!(decoded.attempt, 7);
        assert_eq!(decoded.pid, 41);
        assert_eq!(decoded.payload, b"socket#token");

        bytes[GENERATION_CONTROL_RECORD_LEN - 1] = 1;
        let error = read_generation_control(&mut bytes.as_slice())
            .expect_err("nonzero private-record padding must fail");
        assert!(error.to_string().contains("padding"), "{error}");
    }

    #[test]
    fn generation_control_reader_retains_split_bytes_across_would_block() {
        let record = GenerationControlRecord::new(CONTROL_REVOKE, 9)
            .with_cause(RevocationCause::ChildExited);
        let mut encoded = Vec::new();
        write_generation_control(&mut encoded, &record).expect("encode split KGC1 record");
        let mut source = SplitControlReader {
            bytes: encoded,
            offset: 0,
            block_next: false,
        };
        let mut reader = GenerationControlReader::new();
        let decoded = loop {
            if let Some(decoded) = reader.poll(&mut source).expect("poll split control") {
                break decoded;
            }
        };
        assert_eq!(decoded.kind, CONTROL_REVOKE);
        assert_eq!(decoded.attempt, 9);
        assert_eq!(
            decode_revocation_cause(decoded.cause).expect("decode split cause"),
            RevocationCause::ChildExited
        );
    }

    #[test]
    fn generation_control_partial_record_has_one_overall_deadline() {
        for source in [&mut AlwaysWouldBlock as &mut dyn Read, &mut OneByteAtATime] {
            let mut reader = GenerationControlReader::new();
            reader.filled = 1;
            reader.started = Instant::now().checked_sub(GUARDIAN_SHUTDOWN_DEADLINE);
            let error = reader
                .poll(source)
                .expect_err("expired partial record must fail");
            assert!(error.to_string().contains("read deadline"), "{error}");
        }
    }

    #[test]
    fn generation_control_response_fields_are_transition_exact() {
        let spawned = GenerationControlRecord::new(CONTROL_SPAWNED, 3).with_pid(91);
        let registered = GenerationControlRecord::new(CONTROL_REGISTERED, 3).with_pid(91);
        validate_control_response(&spawned, &registered).expect("exact registered response");

        for malformed in [
            GenerationControlRecord::new(CONTROL_REGISTERED, 3).with_pid(92),
            GenerationControlRecord::new(CONTROL_REGISTERED, 3)
                .with_pid(91)
                .with_payload(b"unexpected".to_vec()),
            GenerationControlRecord {
                cause: 1,
                ..GenerationControlRecord::new(CONTROL_REGISTERED, 3).with_pid(91)
            },
        ] {
            let error = validate_control_response(&spawned, &malformed)
                .expect_err("malformed authenticated response must fail");
            assert!(error.to_string().contains("response fields"), "{error}");
        }
    }

    #[test]
    fn guarded_primary_rejects_non_supervised_bootstrap_before_generation() {
        let registration = BootstrapListener::bind().expect("guarded registration");
        let app_link = registration.app_link();
        let peer = std::thread::spawn(move || {
            let (endpoint, token) = parse_app_link(&app_link).expect("parse registration link");
            let mut stream = std::os::unix::net::UnixStream::connect(endpoint)
                .expect("connect registration link");
            handshake_client(&mut stream, &token).expect("authenticate registration link");
        });
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "read ignored || exit 0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn().expect("spawn guardian stand-in");
        let guardian_pid = child.id();
        let writer = child.stdin.take().expect("guardian liveness writer");
        require_close_on_exec(&writer).expect("test liveness CLOEXEC");
        let bootstrap = GuardianBootstrap {
            child: Some(child),
            liveness_writer: Some(writer),
            quit_ack_reader: None,
            registration,
        };
        let error = bootstrap
            .register_guarded_primary_until(Instant::now() + Duration::from_secs(5))
            .expect_err("plain bootstrap must not become guarded primary");
        peer.join().expect("registration peer joins");
        assert!(
            error.to_string().contains("requires the supervised"),
            "{error}"
        );
        let guardian_pid = i32::try_from(guardian_pid).expect("guardian pid fits i32");
        assert_eq!(kill(Pid::from_raw(guardian_pid), None), Err(Errno::ESRCH));
    }

    #[test]
    fn generation_host_rejects_out_of_order_revoke_before_prepare() {
        let (host, mut guardian) = std::os::unix::net::UnixStream::pair().expect("KGC1 pair");
        let (updates_tx, _updates_rx) = mpsc::channel();
        let current = Mutex::new(None);
        write_generation_control(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_REVOKE, 1)
                .with_cause(RevocationCause::ChildExited),
        )
        .expect("write out-of-order revoke");
        let error =
            run_generation_host(host, &updates_tx, &current, &AtomicU8::new(RECOVERY_ARMED))
                .expect_err("revoke before prepare must fail");
        assert!(
            error.to_string().contains("invalid revoke transition"),
            "{error}"
        );
    }

    #[test]
    fn generation_host_revokes_and_clears_before_preparing_successor() {
        use std::os::unix::net::UnixStream;

        let (host, mut guardian) = UnixStream::pair().expect("KGC1 pair");
        guardian
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("guardian control deadline");
        let (updates_tx, updates_rx) = mpsc::channel();
        let current = Arc::new(Mutex::new(None));
        let recovery_armed = Arc::new(AtomicU8::new(RECOVERY_ARMED));
        let current_for_host = Arc::clone(&current);
        let recovery_for_host = Arc::clone(&recovery_armed);
        let host_thread = std::thread::spawn(move || {
            run_generation_host(host, &updates_tx, &current_for_host, &recovery_for_host)
        });

        let prepared_one = guardian_exchange(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_PREPARE, 1),
            CONTROL_PREPARED,
        );
        let app_link = String::from_utf8(prepared_one.payload).expect("g1 app link UTF-8");
        let mut child_command = long_running_command();
        child_command.process_group(0);
        let mut child = child_command.spawn().expect("g1 group leader");
        let child_pid = child.id();
        guardian_exchange(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_SPAWNED, 1).with_pid(child_pid),
            CONTROL_REGISTERED,
        );
        let (endpoint, token) = parse_app_link(&app_link).expect("parse g1 app link");
        let mut app = UnixStream::connect(endpoint).expect("connect g1 app link");
        handshake_client(&mut app, &token).expect("authenticate g1 app link");
        let bound = loop {
            let update = updates_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("g1 bound update");
            if let GuardedPrimaryUpdate::Bound(bound) = update {
                break bound;
            }
        };
        assert_eq!(bound.attempt(), 1);

        guardian_exchange(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_REVOKE, 1)
                .with_cause(RevocationCause::ChildExited),
            CONTROL_REVOKED,
        );
        terminate_registered_group(child_pid).expect("signal g1 group");
        let _ = child.wait().expect("wait g1 group leader");
        guardian_exchange(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_CLEAR, 1).with_pid(child_pid),
            CONTROL_CLEARED,
        );
        guardian_exchange(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_PREPARE, 2),
            CONTROL_PREPARED,
        );

        guardian_exchange(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_REVOKE, 2)
                .with_cause(RevocationCause::AdmissionFailed),
            CONTROL_REVOKED,
        );
        guardian_exchange(
            &mut guardian,
            &GenerationControlRecord::new(CONTROL_CLEAR, 2),
            CONTROL_CLEARED,
        );
        drop(guardian);
        host_thread
            .join()
            .expect("generation host joins")
            .expect("clean generation host EOF");

        let events: Vec<RoleEvent> = updates_rx
            .try_iter()
            .filter_map(|update| match update {
                GuardedPrimaryUpdate::Role(event) => Some(event),
                GuardedPrimaryUpdate::Bound(_) => None,
            })
            .collect();
        let revoked = events
            .iter()
            .position(|event| matches!(event, RoleEvent::Revoked { attempt: 1, .. }))
            .expect("g1 Revoked event");
        let provisioned = events
            .iter()
            .position(|event| matches!(event, RoleEvent::Provisioned { attempt: 2, .. }))
            .expect("g2 Provisioned event");
        assert!(revoked < provisioned, "g2 provisioned before g1 revocation");
    }

    fn guardian_exchange(
        guardian: &mut std::os::unix::net::UnixStream,
        request: &GenerationControlRecord,
        expected: u8,
    ) -> GenerationControlRecord {
        write_generation_control(guardian, request).expect("write guardian control request");
        let response = read_generation_control(guardian).expect("read guardian control response");
        assert_eq!(response.kind, expected);
        assert_eq!(response.attempt, request.attempt);
        response
    }

    struct AlwaysWouldBlock;

    impl Read for AlwaysWouldBlock {
        fn read(&mut self, _output: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        }
    }

    struct OneByteAtATime;

    impl Read for OneByteAtATime {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            output[0] = 0;
            Ok(1)
        }
    }

    struct SplitControlReader {
        bytes: Vec<u8>,
        offset: usize,
        block_next: bool,
    }

    impl Read for SplitControlReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self.block_next {
                self.block_next = false;
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            if self.offset == self.bytes.len() {
                return Ok(0);
            }
            self.block_next = true;
            let length = output.len().min(17).min(self.bytes.len() - self.offset);
            output[..length].copy_from_slice(&self.bytes[self.offset..self.offset + length]);
            self.offset += length;
            Ok(length)
        }
    }

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
