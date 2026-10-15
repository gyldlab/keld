//! macOS host-death guardian for supervised Bun process groups.
//!
//! This is supervisor cleanup, not App Sandbox containment. The caller owns
//! the guardian process and the only liveness writer. The guardian process
//! calls [`run`] for the generic one-group API or [`run_guarded_primary`] for
//! KEL-96's persistent fresh-generation primary owner.

#![deny(unsafe_op_in_unsafe_fn)]

mod host_liveness;

use self::host_liveness::{
    await_host_death, observe_guarded_liveness, observe_supervised_liveness,
    set_liveness_nonblocking, validate_liveness_bootstrap,
};

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::{Pid, getpgid};

use keld_ipc::link::handshake_client;
use keld_ipc::{
    BootstrapAdmission, BootstrapListener, BootstrapRejection, BootstrapRejectionObserver,
    parse_app_link,
};

use crate::role::{
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

#[derive(Debug)]
struct RecoveryState {
    initial_ready: bool,
    denied: bool,
    retired_attempt: Option<u32>,
    cleared_attempt: Option<u32>,
    release_attempt: Option<u32>,
    expected_attempt: u32,
    shutdown_acknowledged: bool,
}

impl RecoveryState {
    const fn new() -> Self {
        Self {
            initial_ready: false,
            denied: false,
            retired_attempt: None,
            cleared_attempt: None,
            release_attempt: None,
            expected_attempt: 1,
            shutdown_acknowledged: false,
        }
    }

    fn arm(&mut self) {
        if self.denied {
            return;
        }
        if !self.initial_ready {
            self.initial_ready = true;
            return;
        }
        self.release_attempt = self
            .retired_attempt
            .and_then(|attempt| attempt.checked_add(1));
    }

    fn deny(&mut self) {
        self.denied = true;
        self.release_attempt = None;
    }

    fn retire(&mut self, attempt: u32) {
        self.retired_attempt = Some(attempt);
        self.cleared_attempt = None;
        self.release_attempt = None;
    }

    fn cleared(&mut self, attempt: u32) -> Result<(), RuntimeError> {
        self.expected_attempt = attempt.checked_add(1).ok_or_else(|| {
            lifecycle_error(
                "macOS guardian generation host transition",
                io::Error::other("supervisor attempt counter exhausted"),
            )
        })?;
        self.cleared_attempt = Some(attempt);
        Ok(())
    }

    fn permits(&self, attempt: u32) -> bool {
        !self.denied
            && self.expected_attempt == attempt
            && (attempt == 1
                || (self.initial_ready
                    && self.release_attempt == Some(attempt)
                    && self.cleared_attempt == self.retired_attempt
                    && self
                        .retired_attempt
                        .and_then(|retired| retired.checked_add(1))
                        == Some(attempt)))
    }
}

fn lock_recovery(state: &Mutex<RecoveryState>) -> std::sync::MutexGuard<'_, RecoveryState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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
            command: command.into(),
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
    recovery_decision: Arc<Mutex<RecoveryState>>,
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
        let recovery_decision = Arc::new(Mutex::new(RecoveryState::new()));
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
                    #[cfg(test)]
                    None,
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

    /// Arms initial Ready, then releases the current retired successor.
    pub fn arm_recovery(&self) {
        lock_recovery(&self.recovery_decision).arm();
    }

    /// Permanently rejects successor preparation for a failed startup.
    pub fn deny_recovery(&self) {
        lock_recovery(&self.recovery_decision).deny();
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
        // The same guard covers the actual locator/token mint. Release it
        // before Q I/O so the generation owner can finish cleanup concurrently.
        self.deny_recovery();
        let ack_reader = &mut self.quit_ack_reader;
        let writer = self.liveness_writer.as_mut().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervised-Quit control",
                io::Error::new(io::ErrorKind::NotConnected, "liveness writer is missing"),
            )
        })?;
        set_liveness_nonblocking(writer, true)?;
        writer
            .write_all(&[SUPERVISED_QUIT_ACCEPTED])
            .and_then(|()| writer.flush())
            .map_err(|source| lifecycle_error("macOS guardian supervised-Quit control", source))?;
        read_supervised_quit_ack(ack_reader)?;
        self.shutdown_attributed = true;
        lock_recovery(&self.recovery_decision).shutdown_acknowledged = true;
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
        self.deny_recovery();
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
        set_liveness_nonblocking(writer, true)?;
        writer
            .write_all(&[SUPERVISED_ORDERLY_SHUTDOWN])
            .and_then(|()| writer.flush())
            .map_err(|source| lifecycle_error("macOS guardian orderly-shutdown control", source))?;
        read_supervised_orderly_ack(ack_reader)?;
        self.shutdown_attributed = true;
        lock_recovery(&self.recovery_decision).shutdown_acknowledged = true;
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
    preparation_cancelled: Arc<AtomicBool>,
}

impl<C> ChildPreparer for GuardedPrimaryPreparer<C>
where
    C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
{
    type Lease = GuardedPrimaryLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        let response = control_exchange_with_cancellation(
            &self.control,
            &GenerationControlRecord::new(CONTROL_PREPARE, attempt),
            CONTROL_PREPARED,
            attempt,
            Some((&self.preparation_cancelled, &self.host_unavailable)),
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
            command: command.into(),
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
    let preparation_cancelled = Arc::new(AtomicBool::new(false));
    let supervisor = Supervisor::start_prepared(
        RestartPolicy::default(),
        GuardedPrimaryPreparer {
            command_factory,
            control: Arc::new(Mutex::new(GenerationControlPeer {
                stream: control,
                reader: GenerationControlReader::new(),
            })),
            host_unavailable: Arc::clone(&host_unavailable),
            preparation_cancelled: Arc::clone(&preparation_cancelled),
        },
    )?;

    set_liveness_nonblocking(&liveness, true)?;
    let (liveness_result, quit_accepted, host_dead) = await_guarded_primary_liveness(
        &mut liveness,
        &supervisor,
        &mut quit_ack,
        &host_unavailable,
        &preparation_cancelled,
    );

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
        SupervisorOutcome::Stopped if !quit_accepted && !host_dead => {
            if let Some(termination) = crash_ledger.last_self_termination {
                failures.push(RuntimeError::ChildCrashed {
                    pid: termination.pid,
                    exit_code: termination.exit_code,
                    stderr_tail: output.stderr_tail(2_000),
                });
            } else {
                failures.push(lifecycle_error(
                    "macOS guardian generation supervisor stop",
                    io::Error::other("supervisor stopped without owned shutdown attribution"),
                ));
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

fn await_guarded_primary_liveness(
    liveness: &mut (impl Read + AsFd),
    supervisor: &Supervisor,
    quit_ack: &mut impl Write,
    host_unavailable: &AtomicBool,
    preparation_cancelled: &AtomicBool,
) -> (Result<(), RuntimeError>, bool, bool) {
    let mut liveness_result = Ok(());
    let mut quit_accepted = false;
    let mut orderly_requested = false;
    let mut host_dead = false;
    loop {
        match observe_guarded_liveness(
            liveness,
            supervisor,
            &mut quit_accepted,
            &mut orderly_requested,
            quit_ack,
            preparation_cancelled,
        ) {
            Ok(true) => {
                host_dead = true;
                if !quit_accepted && !orderly_requested {
                    host_unavailable.store(true, Ordering::Release);
                }
                supervisor.shutdown();
                break;
            }
            Ok(false) => {}
            Err(error) => {
                liveness_result = Err(error);
                // A liveness fault interrupts a pending read as a fatal error,
                // never as the privately attributed stop marker.
                host_unavailable.store(true, Ordering::Release);
                supervisor.shutdown();
                break;
            }
        }
        match supervisor.recv_event(GENERATION_CONTROL_POLL) {
            Some(crate::SupervisorEvent::Stopped) if quit_accepted => {
                // Supervisor stop does not terminate this persistent guardian.
                // Core may still own real native retirement before host EOF.
                if let Err(error) = set_liveness_nonblocking(liveness, false) {
                    liveness_result = Err(error);
                    break;
                }
            }
            Some(
                crate::SupervisorEvent::RespawnFailed
                | crate::SupervisorEvent::Failed { .. }
                | crate::SupervisorEvent::CrashLoopTripped
                | crate::SupervisorEvent::Stopped,
            ) => break,
            None => thread::yield_now(),
            Some(_) => {}
        }
    }

    (liveness_result, quit_accepted, host_dead)
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

    set_liveness_nonblocking(&liveness, true)?;
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

#[cfg(test)]
struct GenerationProvisionFence {
    attempt: u32,
    entered: mpsc::SyncSender<()>,
    release: Receiver<()>,
}

#[derive(Default)]
struct HostGenerationState {
    lease: Option<RoleGenerationLease>,
    current_attempt: Option<u32>,
    revoked_attempt: Option<u32>,
    pending_prepare: Option<GenerationControlRecord>,
    initial_decision_deadline: Option<Instant>,
}

#[allow(clippy::too_many_lines)] // one owner keeps every private protocol transition contiguous
fn run_generation_host(
    mut control: std::os::unix::net::UnixStream,
    updates_tx: &Sender<GuardedPrimaryUpdate>,
    current_generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
    recovery_decision: &Mutex<RecoveryState>,
    #[cfg(test)] provision_fence: Option<&GenerationProvisionFence>,
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
        #[cfg(unix)]
        None,
    );
    let mut state = HostGenerationState::default();
    let mut control_reader = GenerationControlReader::new();
    loop {
        if let Some(lease) = state.lease.as_mut() {
            lease.poll()?;
        }
        forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
        let record = match control_reader.poll(&mut control) {
            Ok(record) => record,
            Err(error) if runtime_io_kind(&error) == Some(io::ErrorKind::UnexpectedEof) => {
                let result = finish_generation_control_eof(
                    error,
                    control_reader.filled,
                    &mut state,
                    current_generation,
                    recovery_decision,
                );
                forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                return result;
            }
            Err(error) => return Err(error),
        };
        let expected_attempt = lock_recovery(recovery_decision).expected_attempt;
        if let Some(record) = record {
            if state.pending_prepare.is_some() {
                return Err(invalid_control_transition(
                    "pending prepare",
                    record.attempt,
                    expected_attempt,
                ));
            }
            match record.kind {
                CONTROL_PREPARE => {
                    if record.attempt != expected_attempt
                        || state.current_attempt.is_some()
                        || state.lease.is_some()
                        || state.revoked_attempt.is_some()
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
                    if record.attempt > 1 && !lock_recovery(recovery_decision).initial_ready {
                        state.initial_decision_deadline =
                            Some(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE);
                    }
                    state.pending_prepare = Some(record);
                }
                CONTROL_SPAWNED => {
                    if state.current_attempt != Some(record.attempt)
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
                    let current = state.lease.as_mut().ok_or_else(|| {
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
                    if state.current_attempt != Some(record.attempt)
                        || state.revoked_attempt.is_some()
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
                    let current = state.lease.take().ok_or_else(|| {
                        invalid_control_transition("revoke", record.attempt, expected_attempt)
                    })?;
                    // Close native release before the lease emits Revoked.
                    lock_recovery(recovery_decision).retire(record.attempt);
                    current.revoke(cause)?;
                    state.revoked_attempt = Some(record.attempt);
                    forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                    write_generation_control(
                        &mut control,
                        &GenerationControlRecord::new(CONTROL_REVOKED, record.attempt),
                    )?;
                }
                CONTROL_CLEAR => {
                    let registered = lock_generation(current_generation)
                        .map_or(0, |generation| generation.group_pid);
                    if state.current_attempt != Some(record.attempt)
                        || state.revoked_attempt != Some(record.attempt)
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
                    state.current_attempt = None;
                    state.revoked_attempt = None;
                    // None alone is not Clear: only the completed acknowledgment
                    // advances the attempt and permits a pending release request.
                    lock_recovery(recovery_decision).cleared(record.attempt)?;
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
        if control_reader.filled != 0 {
            continue;
        }
        let Some(pending) = state.pending_prepare.as_ref() else {
            continue;
        };
        let provisioned = {
            let mut recovery = lock_recovery(recovery_decision);
            check_initial_recovery_decision(&recovery, state.initial_decision_deadline)?;
            if pending.attempt == 1 && recovery.denied {
                return Err(rejected_recovery());
            }
            if !recovery.permits(pending.attempt) {
                continue;
            }
            recovery.release_attempt = None;
            // This actual bind/token mint is ordered against permanent denial.
            // Do not release the recovery guard between the check and provision.
            #[cfg(test)]
            if let Some(fence) = provision_fence.filter(|fence| fence.attempt == pending.attempt) {
                // Scoped unit-test custody only: shipping code has no fence,
                // callback, global, extra owner, or branch here. The real host
                // owner retains its recovery guard through the actual mint.
                fence.entered.send(()).map_err(|_| {
                    lifecycle_error(
                        "test generation pre-mint receipt",
                        io::Error::other("test custodian ended"),
                    )
                })?;
                fence
                    .release
                    .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
                    .map_err(|source| {
                        lifecycle_error(
                            "test generation pre-mint release",
                            io::Error::other(source),
                        )
                    })?;
            }
            owner.provision(pending.attempt)?
        };
        let response = GenerationControlRecord::new(CONTROL_PREPARED, pending.attempt)
            .with_payload(provisioned.app_link.as_bytes().to_vec());
        state.current_attempt = Some(pending.attempt);
        state.lease = Some(provisioned.lease);
        state.pending_prepare = None;
        state.initial_decision_deadline = None;
        forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
        if let Err(error) = write_generation_control(&mut control, &response) {
            if matches!(
                runtime_io_kind(&error),
                Some(io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset)
            ) {
                let result = finish_generation_control_eof(
                    error,
                    control_reader.filled,
                    &mut state,
                    current_generation,
                    recovery_decision,
                );
                forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                return result;
            }
            return Err(error);
        }
    }
}

fn rejected_recovery() -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation host transition",
        io::Error::new(
            io::ErrorKind::InvalidData,
            "successor preparation was rejected before initial Ready",
        ),
    )
}

fn check_initial_recovery_decision(
    recovery: &RecoveryState,
    deadline: Option<Instant>,
) -> Result<(), RuntimeError> {
    if recovery.initial_ready || deadline.is_none() {
        return Ok(());
    }
    if recovery.denied {
        return Err(rejected_recovery());
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(lifecycle_error(
            "macOS guardian generation host transition",
            io::Error::new(
                io::ErrorKind::TimedOut,
                "initial Ready did not decide recovery before the deadline",
            ),
        ));
    }
    Ok(())
}

fn finish_generation_control_eof(
    primary: RuntimeError,
    partial_bytes: usize,
    state: &mut HostGenerationState,
    current_generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
    recovery: &Mutex<RecoveryState>,
) -> Result<(), RuntimeError> {
    if partial_bytes != 0 || lock_generation(current_generation).is_some() {
        return Err(primary);
    }
    if let Some(lease) = state.lease.take() {
        let Some(attempt) = state.current_attempt.take() else {
            return Err(primary);
        };
        if state.revoked_attempt.is_some() {
            return Err(primary);
        }
        lock_recovery(recovery).retire(attempt);
        if let Err(cleanup) = lease.revoke(RevocationCause::Shutdown) {
            return Err(collapse_failures(vec![primary, cleanup]));
        }
    }
    if state.current_attempt.is_some() || state.revoked_attempt.is_some() {
        return Err(primary);
    }
    // EOF can precede the host's Q/S ACK read by one scheduling turn. Retain
    // the EOF fact and its one existing attribution budget; never reread EOF.
    let deadline = Instant::now() + SUPERVISED_QUIT_ACK_DEADLINE;
    loop {
        let recovery = lock_recovery(recovery);
        if !recovery.denied {
            return Err(primary);
        }
        if recovery.shutdown_acknowledged {
            return Ok(());
        }
        drop(recovery);
        if Instant::now() >= deadline {
            return Err(primary);
        }
        thread::yield_now();
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
    control_exchange_with_cancellation(control, request, expected_kind, expected_attempt, None)
}

fn control_exchange_with_cancellation(
    control: &Arc<Mutex<GenerationControlPeer>>,
    request: &GenerationControlRecord,
    expected_kind: u8,
    expected_attempt: u32,
    cancellation: Option<(&AtomicBool, &AtomicBool)>,
) -> Result<GenerationControlRecord, RuntimeError> {
    let mut control = match control.lock() {
        Ok(control) => control,
        Err(poisoned) => poisoned.into_inner(),
    };
    let GenerationControlPeer { stream, reader } = &mut *control;
    write_generation_control(stream, request)?;
    let native_wait = request.kind == CONTROL_PREPARE && request.attempt > 1;
    let deadline = (!native_wait).then(|| Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE);
    let response = loop {
        if native_wait
            && reader.filled == 0
            && let Some((cancelled, host_unavailable)) = cancellation
        {
            if cancelled.load(Ordering::Acquire) {
                return Err(interrupted_successor_preparation(expected_attempt));
            }
            if host_unavailable.load(Ordering::Acquire) {
                return Err(lifecycle_error(
                    "macOS guardian generation control transition",
                    io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "host-liveness observation failed",
                    ),
                ));
            }
        }
        if let Some(response) = reader.poll(stream)? {
            break response;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
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

#[derive(Debug)]
struct OwnedPreparationInterruption {
    attempt: u32,
}

impl std::fmt::Display for OwnedPreparationInterruption {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "owned shutdown interrupted successor attempt {}",
            self.attempt
        )
    }
}

impl std::error::Error for OwnedPreparationInterruption {}

fn interrupted_successor_preparation(attempt: u32) -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation control transition",
        io::Error::new(
            io::ErrorKind::Interrupted,
            OwnedPreparationInterruption { attempt },
        ),
    )
}

/// Recognizes only this preparer's private interruption for the exact attempt.
pub(crate) fn is_owned_preparation_interruption(error: &RuntimeError, attempt: u32) -> bool {
    matches!(error, RuntimeError::Lifecycle { source, .. }
        if source.get_ref()
            .and_then(|source| source.downcast_ref::<OwnedPreparationInterruption>())
            .is_some_and(|marker| marker.attempt == attempt && attempt > 1))
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

    mod generation_control_wire;

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
    fn recovery_release_requires_retired_clear_and_initial_arm_is_not_release() {
        let mut recovery = RecoveryState::new();
        assert!(recovery.permits(1));
        recovery.retire(1);
        recovery.arm();
        assert!(recovery.initial_ready);
        assert!(
            !recovery.permits(2),
            "late initial Ready released retirement"
        );
        recovery.arm();
        assert_eq!(recovery.release_attempt, Some(2));
        assert!(!recovery.permits(2), "None before Cleared released G2");
        recovery.cleared(1).expect("record completed G1 Clear");
        assert!(recovery.permits(2));
        assert!(
            !recovery.permits(3),
            "G1 release authorized a wrong attempt"
        );
        recovery.retire(2);
        assert!(!recovery.permits(3), "revocation retained native release");
        recovery.cleared(2).expect("record completed G2 Clear");
        assert!(!recovery.permits(3), "new Clear reused old release");
        recovery.arm();
        assert!(recovery.permits(3));
        recovery.deny();
        recovery.arm();
        assert!(!recovery.permits(3));
        assert!(recovery.release_attempt.is_none());
    }

    #[test]
    fn unresolved_initial_ready_retains_its_deadline_after_native_wait_is_excluded() {
        let mut recovery = RecoveryState::new();
        let expired = Some(
            Instant::now()
                .checked_sub(GUARDIAN_SHUTDOWN_DEADLINE)
                .expect("test clock can represent one expired initial decision budget"),
        );
        let error = check_initial_recovery_decision(&recovery, expired)
            .expect_err("unresolved initial Ready remains bounded");
        assert_eq!(runtime_io_kind(&error), Some(io::ErrorKind::TimedOut));
        recovery.arm();
        check_initial_recovery_decision(&recovery, expired)
            .expect("decided eligibility does not time out legal native waiting");
        assert!(
            !recovery.permits(2),
            "eligibility alone released native waiting"
        );
    }

    #[test]
    fn unclaimed_control_eof_explicitly_revokes_exact_lease_and_requires_attribution() {
        for acknowledged in [false, true] {
            let (events_tx, events_rx) = mpsc::channel();
            let mut owner = RoleGenerationOwner::new(
                RoleOwner::Primary,
                DEFAULT_ADMISSION_TIMEOUT,
                events_tx,
                None,
                None,
            );
            let provisioned = owner.provision(7).expect("own one unclaimed attempt");
            let (endpoint, _) = parse_app_link(&provisioned.app_link).expect("actual locator");
            let mut state = HostGenerationState {
                lease: Some(provisioned.lease),
                current_attempt: Some(7),
                ..HostGenerationState::default()
            };
            let recovery = Mutex::new(RecoveryState::new());
            if acknowledged {
                let mut recovery = lock_recovery(&recovery);
                recovery.deny();
                recovery.shutdown_acknowledged = true;
            }
            let result = finish_generation_control_eof(
                lifecycle_error("test EOF", io::Error::from(io::ErrorKind::UnexpectedEof)),
                0,
                &mut state,
                &Mutex::new(None),
                &recovery,
            );
            let events: Vec<_> = events_rx.try_iter().collect();
            let endpoint_closed = std::os::unix::net::UnixStream::connect(endpoint).is_err();
            assert!(state.lease.is_none() && state.current_attempt.is_none());
            assert!(
                endpoint_closed,
                "unclaimed listener survived explicit revoke"
            );
            assert!(
                events.iter().any(|event| matches!(
                    event,
                    RoleEvent::Revoked {
                        attempt: 7,
                        cause: crate::role::RoleRevocationCause::Shutdown,
                        ..
                    }
                )),
                "exact owner emitted no Shutdown revocation: {events:?}"
            );
            assert_eq!(
                result.is_ok(),
                acknowledged,
                "denial/EOF fabricated attribution"
            );
        }
    }

    #[test]
    fn attributed_control_eof_keeps_partial_and_registered_failures_fatal() {
        for (partial, registered) in [
            (1, None),
            (
                0,
                Some(RegisteredPrimaryGeneration {
                    attempt: 2,
                    group_pid: 42,
                }),
            ),
        ] {
            let recovery = Mutex::new(RecoveryState::new());
            {
                let mut recovery = lock_recovery(&recovery);
                recovery.deny();
                recovery.shutdown_acknowledged = true;
            }
            let mut state = HostGenerationState::default();
            let result = finish_generation_control_eof(
                lifecycle_error("test EOF", io::Error::from(io::ErrorKind::UnexpectedEof)),
                partial,
                &mut state,
                &Mutex::new(registered),
                &recovery,
            );
            assert_eq!(
                runtime_io_kind(&result.expect_err("partial/active EOF stays fatal")),
                Some(io::ErrorKind::UnexpectedEof)
            );
        }
    }

    #[test]
    fn denied_recovery_cannot_be_rearmed_by_late_ready() {
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("owned liveness peer");
        let liveness_writer = child.stdin.take().expect("liveness writer");
        let quit_ack_reader = child.stdout.take().expect("ack reader");
        let (_updates_tx, updates_rx) = mpsc::channel();
        let mut primary = GuardedPrimary {
            child,
            liveness_writer: Some(liveness_writer),
            quit_ack_reader,
            current_generation: Arc::new(Mutex::new(None)),
            recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
            updates_rx,
            worker: None,
            shutdown_attributed: false,
        };
        primary.deny_recovery();
        primary.arm_recovery();
        primary.arm_recovery();
        let finally_denied = lock_recovery(&primary.recovery_decision).denied;
        primary.liveness_writer.take();
        primary.child.wait().expect("wait owned liveness peer");
        assert!(finally_denied, "late Ready reopened denial");
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
        let error = run_generation_host(
            host,
            &updates_tx,
            &current,
            &Mutex::new(RecoveryState::new()),
            None,
        )
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
        let recovery_armed = Arc::new(Mutex::new(RecoveryState::new()));
        lock_recovery(&recovery_armed).arm();
        let host_current = Arc::clone(&current);
        let host_recovery = Arc::clone(&recovery_armed);
        let host_thread = std::thread::spawn(move || {
            run_generation_host(host, &updates_tx, &host_current, &host_recovery, None)
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
        lock_recovery(&recovery_armed).arm();
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
        {
            let mut recovery = lock_recovery(&recovery_armed);
            recovery.deny();
            recovery.shutdown_acknowledged = true;
        }
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

    #[test]
    fn unmarked_successor_preparation_errors_preserve_failed_outcome() {
        use crate::SupervisorEvent;
        use PreparationAttribution::{Accepted, Requested, Unattributed};
        use io::ErrorKind::{Interrupted, InvalidData};

        const DIAGNOSTIC_PHASE: &str = "macOS guardian generation control transition";
        // Ordinary I/O kinds and public diagnostic phases do not mark an
        // intentional runtime cancellation, even after a stop fact is set.
        for attribution in [Accepted, Requested, Unattributed] {
            for (phase, kind) in [
                (DIAGNOSTIC_PHASE, Interrupted),
                (DIAGNOSTIC_PHASE, InvalidData),
                ("test unrelated preparation I/O", Interrupted),
                ("test unrelated preparation I/O", InvalidData),
            ] {
                let (entered_tx, entered_rx) = mpsc::channel();
                let (release_tx, release_rx) = mpsc::channel();
                let mut command = Command::new("/bin/sh");
                command.args([
                    "-c",
                    "printf KEL143_UNMARKED_G1_STDOUT; \
                 printf KEL143_UNMARKED_G1_STDERR >&2; exit 1",
                ]);
                let supervisor = Supervisor::start_prepared(
                    RestartPolicy::default(),
                    GatedUnmarkedFailurePreparer {
                        initial: Some(command),
                        entered_tx,
                        release_rx,
                        phase,
                        kind,
                        owned: false,
                    },
                )
                .expect("initial real child must spawn");
                // Only attempt 2 reaches this fence, after actual G1 retirement.
                let entered = entered_rx.recv_timeout(Duration::from_secs(5));
                match attribution {
                    Accepted => supervisor.accept_shutdown(),
                    Requested => supervisor.shutdown(),
                    Unattributed => {}
                }
                if entered.is_err() {
                    supervisor.shutdown();
                }
                // Release before asserting so Drop cannot wait on our own gate.
                let released = release_tx.send(());
                drop(release_tx);
                entered.expect("successor preparation must reach the existing trait fence");
                released.expect("release the controlled preparation error");

                let deadline = Instant::now() + Duration::from_secs(5);
                let terminal = loop {
                    let remaining = deadline
                        .checked_duration_since(Instant::now())
                        .expect("successor preparation terminal event deadline");
                    match supervisor.recv_event(remaining) {
                        Some(SupervisorEvent::Started { attempt: 2, .. }) => {
                            panic!("failed successor preparation spawned G2");
                        }
                        Some(
                            event @ (SupervisorEvent::Stopped | SupervisorEvent::Failed { .. }),
                        ) => {
                            break event;
                        }
                        Some(_) => {}
                        None => panic!("no terminal successor event for {attribution:?}/{kind:?}"),
                    }
                };
                let outcome = supervisor.wait_for_outcome();
                assert!(
                    supervisor.current_pid().is_none(),
                    "G2 must not remain live"
                );
                let ledger = supervisor.crash_ledger();
                assert_eq!(ledger.self_termination_count, 1);
                assert!(
                    ledger
                        .last_self_termination
                        .is_some_and(|exit| exit.exit_code == Some(1)),
                    "shutdown must preserve the already-observed G1 exit"
                );
                let output = supervisor.output();
                assert!(output.stdout.contains("KEL143_UNMARKED_G1_STDOUT"));
                assert!(output.stderr.contains("KEL143_UNMARKED_G1_STDERR"));
                assert!(
                    matches!(terminal, SupervisorEvent::Failed { attempt: 2 }),
                    "unmarked error became a stop: {attribution:?}/{phase}/{kind:?}: {terminal:?}"
                );
                match outcome {
                    SupervisorOutcome::Failed(RuntimeError::Lifecycle {
                        phase: actual,
                        source,
                    }) => {
                        assert_eq!(actual, phase);
                        assert_eq!(source.kind(), kind);
                        assert_eq!(
                            source.to_string(),
                            "controlled successor preparation failure"
                        );
                    }
                    other => panic!("unmarked preparation failure was hidden: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn owned_successor_interruption_requires_supervisor_stop_and_exact_attempt() {
        use crate::SupervisorEvent;
        for attribution in [
            PreparationAttribution::Accepted,
            PreparationAttribution::Requested,
            PreparationAttribution::Unattributed,
        ] {
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "exit 1"]);
            let supervisor = Supervisor::start_prepared(
                RestartPolicy::default(),
                GatedUnmarkedFailurePreparer {
                    initial: Some(command),
                    entered_tx,
                    release_rx,
                    phase: "unused marked phase",
                    kind: io::ErrorKind::Interrupted,
                    owned: true,
                },
            )
            .expect("initial real child");
            let entered = entered_rx.recv_timeout(Duration::from_secs(5));
            match attribution {
                PreparationAttribution::Accepted => supervisor.accept_shutdown(),
                PreparationAttribution::Requested => supervisor.shutdown(),
                PreparationAttribution::Unattributed => {}
            }
            if entered.is_err() {
                supervisor.shutdown();
            }
            let released = release_tx.send(());
            drop(release_tx);
            entered.expect("actual successor preparation fence");
            released.expect("release owned interruption");
            let deadline = Instant::now() + Duration::from_secs(5);
            let terminal = loop {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .expect("terminal event bound");
                match supervisor.recv_event(remaining) {
                    Some(event @ (SupervisorEvent::Stopped | SupervisorEvent::Failed { .. })) => {
                        break event;
                    }
                    Some(SupervisorEvent::Started { attempt: 2, .. }) => {
                        panic!("cancelled G2 spawned")
                    }
                    Some(_) => {}
                    None => panic!("missing owned preparation terminal"),
                }
            };
            let outcome = supervisor.wait_for_outcome();
            assert!(supervisor.current_pid().is_none());
            assert_eq!(supervisor.crash_ledger().self_termination_count, 1);
            match attribution {
                PreparationAttribution::Accepted | PreparationAttribution::Requested => {
                    assert!(matches!(terminal, SupervisorEvent::Stopped));
                    assert!(matches!(outcome, SupervisorOutcome::Stopped));
                }
                PreparationAttribution::Unattributed => {
                    assert!(matches!(terminal, SupervisorEvent::Failed { attempt: 2 }));
                    assert!(matches!(outcome, SupervisorOutcome::Failed(_)));
                }
            }
        }
        let marker = interrupted_successor_preparation(2);
        assert!(is_owned_preparation_interruption(&marker, 2));
        assert!(!is_owned_preparation_interruption(&marker, 1));
        assert!(!is_owned_preparation_interruption(&marker, 3));
    }

    #[derive(Debug, Clone, Copy)]
    enum PreparationAttribution {
        Accepted,
        Requested,
        Unattributed,
    }

    struct GatedUnmarkedFailurePreparer {
        initial: Option<Command>,
        entered_tx: mpsc::Sender<()>,
        release_rx: mpsc::Receiver<()>,
        phase: &'static str,
        kind: io::ErrorKind,
        owned: bool,
    }

    impl ChildPreparer for GatedUnmarkedFailurePreparer {
        type Lease = crate::NoLease;

        fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
            if attempt == 1 {
                return Ok(PreparedChild {
                    command: self.initial.take().expect("one initial command").into(),
                    lease: crate::NoLease,
                });
            }
            assert_eq!(attempt, 2, "the failed preparation must not be retried");
            self.entered_tx.send(()).expect("observe successor prepare");
            self.release_rx
                .recv()
                .expect("controlled preparation release");
            if self.owned {
                return Err(interrupted_successor_preparation(attempt));
            }
            Err(RuntimeError::Lifecycle {
                phase: self.phase,
                source: io::Error::new(self.kind, "controlled successor preparation failure"),
            })
        }
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
        let (pid_tx, pid_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let deadline = Instant::now() + Duration::from_secs(5);
        let worker = std::thread::spawn(move || {
            let result = run_supervised_with_registration(
                reader,
                io::sink(),
                io::sink(),
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
        // Enrollment must precede the exit under test. Confirm the shell has
        // actually stopped so an early SIGCONT cannot race its SIGSTOP.
        let pid = pid_rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("self-terminating child registered");
        await_stopped_child(pid, deadline);
        kill(
            Pid::from_raw(i32::try_from(pid).expect("pid fits i32")),
            Signal::SIGCONT,
        )
        .expect("let registered child exit zero");
        let result = done_rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        // The result must arrive with the writer live; close it before asserting
        // so a failed observation still lets the guardian clean up its child.
        drop(writer);
        worker.join().expect("guardian worker joins");
        let error = result
            .expect("self-termination must wake the guardian while host is live")
            .expect_err("unrequested status-zero exit is fatal");
        assert!(error.to_string().contains("KELD-RUNTIME-012"), "{error}");
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
        await_stopped_child(pid, stopped_deadline);

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

    fn await_stopped_child(pid: u32, deadline: Instant) {
        loop {
            let output = Command::new("/bin/ps")
                .args(["-o", "state=", "-p", &pid.to_string()])
                .output()
                .expect("inspect stopped child state");
            if String::from_utf8(output.stdout)
                .expect("process state is UTF-8")
                .trim()
                .starts_with('T')
            {
                break;
            }
            assert!(Instant::now() < deadline, "child never stopped");
            std::thread::yield_now();
        }
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

    // Root supplies an already-built Host artifact. Guardian stdout remains
    // exclusively the existing Q/S ACK pipe, never a libtest output stream.
    struct PersistentPrimaryFixture {
        primary: GuardedPrimary,
        stderr: File,
        stage_root: std::path::PathBuf,
    }

    impl PersistentPrimaryFixture {
        fn start() -> Self {
            use std::os::unix::fs::MetadataExt as _;
            // Host's existing fixture owner stages the public application and
            // keeps its ordinary HTTP observer alive. Runtime never stages it.
            let receipt = std::env::var_os("KELD_KEL143_FIXTURE_RECEIPT")
                .expect("Root supplies the existing Host fixture artifact JSON");
            let bytes = std::fs::read(receipt).expect("owned staged fixture receipt");
            assert!(
                !bytes.is_empty() && bytes.len() <= 8192,
                "bounded fixture data"
            );
            let data: serde_json::Value = serde_json::from_slice(&bytes).expect("fixture JSON");
            let host = data["stagedHost"]
                .as_str()
                .expect("exact staged Host artifact");
            let stage = std::path::PathBuf::from(data["stageRoot"].as_str().expect("stage root"));
            let entry = data["entryRelative"]
                .as_str()
                .expect("existing public app entry");
            let retained = File::open(stage.join(entry)).expect("retain exact entry identity");
            let identity = retained.metadata().expect("retained entry metadata");
            let stderr = tempfile::tempfile().expect("owned guardian diagnostics");
            let mut command = Command::new(host);
            command
                .arg(SUPERVISED_GUARDIAN_ARG)
                .arg(&stage)
                .arg(entry)
                .arg(identity.dev().to_string())
                .arg(identity.ino().to_string())
                .env_remove("KELD_DEV_LEASE")
                .process_group(0)
                .stderr(Stdio::from(
                    stderr.try_clone().expect("diagnostic capture clone"),
                ));
            let bootstrap = GuardianBootstrap::spawn_supervised(command)
                .expect("existing supervised bootstrap");
            drop(retained);
            let primary = bootstrap
                .register_guarded_primary_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
                .expect("real authenticated guardian registration");
            Self {
                primary,
                stderr,
                stage_root: stage,
            }
        }

        fn ready(&mut self) -> u32 {
            let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
            let bound = loop {
                self.primary
                    .poll_fatal()
                    .expect("healthy real guardian before Bound");
                if let Some(GuardedPrimaryUpdate::Bound(bound)) =
                    self.primary.recv_update(GENERATION_CONTROL_POLL)
                {
                    break bound;
                }
                assert!(Instant::now() < deadline, "actual G1 Bound deadline");
            };
            assert_eq!(bound.attempt(), 1);
            let pid = self
                .primary
                .group_pid()
                .expect("actual registered G1 group");
            let mut app = bound.into_stream();
            let ready = keld_ipc::codec::encode(&keld_ipc::lifecycle::LifecycleEvent::Ready)
                .expect("canonical lifecycle Ready payload");
            self.primary.arm_recovery();
            keld_ipc::link::write_frame(
                &mut app,
                keld_ipc::FrameKind::Event,
                0,
                keld_ipc::lifecycle::LIFECYCLE_CHANNEL,
                keld_ipc::CorrelationId(0),
                &ready,
            )
            .expect("existing actual Ready event");
            // The host lease also retains its stream; this alias mints no
            // process authority and no independent child/wait owner.
            pid
        }

        fn await_cleared(&mut self) {
            let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
            let mut revoked = false;
            loop {
                self.primary
                    .poll_fatal()
                    .expect("healthy guardian during real retirement");
                if let Some(update) = self.primary.recv_update(GENERATION_CONTROL_POLL) {
                    match update {
                        GuardedPrimaryUpdate::Role(RoleEvent::Revoked { attempt: 1, .. }) => {
                            revoked = true;
                        }
                        GuardedPrimaryUpdate::Bound(bound) => {
                            panic!("closed permit bound G{}", bound.attempt())
                        }
                        GuardedPrimaryUpdate::Role(_) => {}
                    }
                }
                let recovery = lock_recovery(&self.primary.recovery_decision);
                if revoked && recovery.cleared_attempt == Some(1) && recovery.expected_attempt == 2
                {
                    assert!(recovery.release_attempt.is_none());
                    break;
                }
                drop(recovery);
                assert!(
                    Instant::now() < deadline,
                    "actual G1 revoke/Cleared deadline"
                );
            }
            assert!(self.primary.group_pid().is_none());
        }

        fn assert_live_for_budget(&mut self, phase: &str) {
            let guardian = self.primary.guardian_pid();
            let began = Instant::now();
            let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE + GENERATION_CONTROL_POLL;
            let mut observations = 0_u64;
            // This measures survival while an owned release/EOF is withheld;
            // it is the contract under test, not a synchronization sleep.
            while Instant::now() < deadline {
                self.primary
                    .poll_fatal()
                    .expect("persistent guardian remains healthy before host EOF");
                assert!(
                    self.primary
                        .child
                        .try_wait()
                        .expect("owned guardian status")
                        .is_none()
                );
                assert_eq!(self.primary.guardian_pid(), guardian);
                assert!(self.primary.group_pid().is_none());
                observations += 1;
                if let Some(GuardedPrimaryUpdate::Bound(bound)) =
                    self.primary.recv_update(Duration::ZERO)
                {
                    panic!("closed native permit bound G{}", bound.attempt());
                }
                std::thread::yield_now();
            }
            println!(
                "KEL143_RUNTIME_LIVE phase={phase} guardian={guardian} observations={observations} elapsed_ns={} group=None",
                began.elapsed().as_nanos()
            );
        }

        fn diagnostics(&mut self) -> String {
            use std::io::Seek as _;
            self.stderr.rewind().expect("rewind owned diagnostics");
            let mut text = String::new();
            (&self.stderr)
                .take(65_536)
                .read_to_string(&mut text)
                .expect("bounded diagnostics");
            text
        }
    }

    #[test]
    #[ignore = "explicit real macOS staged Host guardian shutdown proof"]
    fn guarded_primary_attributed_stop_keeps_guardian_live_until_host_eof() {
        let mut fixture = PersistentPrimaryFixture::start();
        let group = fixture.ready();
        println!(
            "KEL143_RUNTIME_OWNED host={} guardian={} g1_group={group}",
            std::process::id(),
            fixture.primary.guardian_pid()
        );
        fixture
            .primary
            .fail_current_generation(1)
            .expect("only actual registered live G1");
        fixture.await_cleared();
        fixture.assert_live_for_budget("runtime-permit-held");
        fixture
            .primary
            .accept_shutdown()
            .expect("actual Q ACK before host tail");
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        while fixture
            .primary
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            fixture
                .primary
                .poll_fatal()
                .expect("Q-attributed guardian health");
            assert!(
                Instant::now() < deadline,
                "actual cancelled control EOF deadline"
            );
            std::thread::yield_now();
        }
        fixture
            .primary
            .join_worker()
            .expect("attributed resource-free control EOF");
        println!(
            "KEL143_RUNTIME_CONTROL_EOF guardian={} ack={}",
            fixture.primary.guardian_pid(),
            lock_recovery(&fixture.primary.recovery_decision).shutdown_acknowledged
        );
        fixture.assert_live_for_budget("q-stopped-before-host-eof");
        let status = fixture
            .primary
            .shutdown()
            .expect("host EOF and exact guardian reap");
        let diagnostics = fixture.diagnostics();
        println!(
            "KEL143_RUNTIME_REAP guardian={} raw_status={} group={:?}",
            fixture.primary.guardian_pid(),
            status.into_raw(),
            fixture.primary.group_pid()
        );
        assert!(
            status.success(),
            "attributed guardian final status: {status:?}; {diagnostics}"
        );
        assert!(fixture.primary.group_pid().is_none());
        assert!(
            fixture
                .primary
                .child
                .try_wait()
                .expect("retained terminal status")
                .is_some()
        );
        assert!(group > 1);
        assert!(
            !fixture.stage_root.exists(),
            "existing guardian completed its owned stage cleanup"
        );
    }

    #[test]
    fn poll_fatal_rejects_owned_guardian_zero_even_after_shutdown_attribution() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("owned zero-status health fixture");
        let writer = child.stdin.take().expect("owned liveness writer");
        let ack = child.stdout.take().expect("owned acknowledgment reader");
        let (_tx, updates_rx) = mpsc::channel();
        let mut primary = GuardedPrimary {
            child,
            liveness_writer: Some(writer),
            quit_ack_reader: ack,
            current_generation: Arc::new(Mutex::new(None)),
            recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
            updates_rx,
            worker: None,
            shutdown_attributed: true,
        };
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        let error = loop {
            if let Err(error) = primary.poll_fatal() {
                break error;
            }
            assert!(
                Instant::now() < deadline,
                "owned zero-status inspection deadline"
            );
            std::thread::yield_now();
        };
        assert!(
            matches!(
                error,
                RuntimeError::GuardianExited {
                    exit_code: Some(0),
                    ..
                }
            ),
            "attribution hid unexpected guardian zero: {error}"
        );
        assert!(primary.liveness_writer.is_none());
        assert!(
            primary
                .child
                .try_wait()
                .expect("already-reaped status retained")
                .is_some()
        );
    }

    fn owned_acknowledgment_primary() -> GuardedPrimary {
        // This owns a real ACK pipe peer, not a guardian/native mock claim.
        // The staged-Host case above separately proves the actual guardian.
        let script = r#"
const input = Bun.stdin.stream().getReader();
const first = await input.read();
if (first.done || first.value.byteLength !== 1 || first.value[0] !== 81) process.exit(2);
process.stdout.write("KQA");
while (!(await input.read()).done) {}
"#;
        let mut child = Command::new("bun")
            .args(["-e", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("owned Q ACK producer");
        let writer = child.stdin.take().expect("sole Q/liveness writer");
        let ack = child.stdout.take().expect("sole actual ACK reader");
        let (_tx, updates_rx) = mpsc::channel();
        GuardedPrimary {
            child,
            liveness_writer: Some(writer),
            quit_ack_reader: ack,
            current_generation: Arc::new(Mutex::new(None)),
            recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
            updates_rx,
            worker: None,
            shutdown_attributed: false,
        }
    }

    #[test]
    fn control_eof_acknowledgment_is_order_independent_and_attributed() {
        for ack_first in [true, false] {
            let (mut host, peer) = std::os::unix::net::UnixStream::pair().expect("real EOF pair");
            let mut primary = owned_acknowledgment_primary();
            primary.deny_recovery();
            if ack_first {
                primary.accept_shutdown().expect("actual Q ACK before EOF");
            }
            let state_for_reader = Arc::clone(&primary.recovery_decision);
            let (observed_tx, observed_rx) = mpsc::sync_channel(1);
            let reader = std::thread::spawn(move || {
                let mut reader = GenerationControlReader::new();
                let eof = reader.poll(&mut host).expect_err("actual peer EOF");
                let ack_at_eof = lock_recovery(&state_for_reader).shutdown_acknowledged;
                observed_tx
                    .send(ack_at_eof)
                    .expect("record EOF before later ACK");
                finish_generation_control_eof(
                    eof,
                    reader.filled,
                    &mut HostGenerationState::default(),
                    &Mutex::new(None),
                    &state_for_reader,
                )
            });
            drop(peer);
            let observed = observed_rx
                .recv_timeout(SUPERVISED_QUIT_ACK_DEADLINE)
                .expect("real EOF receipt");
            if !ack_first {
                primary
                    .accept_shutdown()
                    .expect("actual Q ACK after observed EOF");
            }
            let result = reader.join().expect("one owned EOF reader joins");
            let status = primary.shutdown().expect("ACK peer EOF and owned reap");
            println!(
                "KEL143_RUNTIME_ACK_EOF ack_first={ack_first} ack_at_eof={observed} peer_status={status:?}"
            );
            assert_eq!(observed, ack_first);
            result.expect("either ordered actual EOF/ACK completes cleanly");
            assert!(status.success());
        }
    }

    #[test]
    fn control_eof_without_ack_cannot_succeed_from_denial() {
        let (mut host, peer) = std::os::unix::net::UnixStream::pair().expect("real EOF pair");
        drop(peer);
        let mut reader = GenerationControlReader::new();
        let eof = reader.poll(&mut host).expect_err("real peer EOF");
        let recovery = Mutex::new(RecoveryState::new());
        lock_recovery(&recovery).deny();
        let began = Instant::now();
        let error = finish_generation_control_eof(
            eof,
            reader.filled,
            &mut HostGenerationState::default(),
            &Mutex::new(None),
            &recovery,
        )
        .expect_err("denial alone cannot fabricate Q ACK");
        assert!(
            began.elapsed() >= SUPERVISED_QUIT_ACK_DEADLINE,
            "existing attribution budget was lost"
        );
        assert_eq!(runtime_io_kind(&error), Some(io::ErrorKind::UnexpectedEof));
    }

    #[test]
    fn real_control_eof_preserves_partial_and_unattributed_failures() {
        for partial in [false, true] {
            let (mut host, mut peer) =
                std::os::unix::net::UnixStream::pair().expect("real EOF pair");
            if partial {
                let mut encoded = Vec::new();
                write_generation_control(
                    &mut encoded,
                    &GenerationControlRecord::new(CONTROL_CLEAR, 1),
                )
                .expect("canonical fixed record");
                peer.write_all(&encoded[..1]).expect("actual partial input");
            }
            drop(peer);
            let mut reader = GenerationControlReader::new();
            let eof = reader
                .poll(&mut host)
                .expect_err("actual partial/unattributed EOF");
            assert_eq!(reader.filled, usize::from(partial));
            let recovery = Mutex::new(RecoveryState::new());
            if partial {
                let mut recovery = lock_recovery(&recovery);
                recovery.deny();
                recovery.shutdown_acknowledged = true;
            }
            let error = finish_generation_control_eof(
                eof,
                reader.filled,
                &mut HostGenerationState::default(),
                &Mutex::new(None),
                &recovery,
            )
            .expect_err("unowned/partial closure remains fatal");
            assert_eq!(runtime_io_kind(&error), Some(io::ErrorKind::UnexpectedEof));
        }
    }

    #[test]
    fn successor_prepare_zero_byte_reply_wait_outlives_transition_budget() {
        let (client, mut peer) = std::os::unix::net::UnixStream::pair().expect("real control pair");
        client
            .set_read_timeout(Some(GENERATION_CONTROL_POLL))
            .expect("existing read poll");
        client
            .set_write_timeout(Some(GENERATION_CONTROL_POLL))
            .expect("existing write poll");
        peer.set_read_timeout(Some(GUARDIAN_SHUTDOWN_DEADLINE))
            .expect("bounded request receipt");
        peer.set_write_timeout(Some(GENERATION_CONTROL_POLL))
            .expect("existing reply write poll");
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let client_thread = std::thread::spawn(move || {
            let control = Arc::new(Mutex::new(GenerationControlPeer {
                stream: client,
                reader: GenerationControlReader::new(),
            }));
            let result = control_exchange(
                &control,
                &GenerationControlRecord::new(CONTROL_PREPARE, 2),
                CONTROL_PREPARED,
                2,
            );
            let _ = result_tx.send(result);
        });
        let request = read_generation_control(&mut peer);
        let began = Instant::now();
        let release_at = began + GUARDIAN_SHUTDOWN_DEADLINE + Duration::from_secs(1);
        let mut early = None;
        let mut disconnected = false;
        if request.is_ok() {
            // The request receipt, not a guessed startup delay, starts this
            // independent measured interval. No reply bytes exist before release.
            while Instant::now() < release_at {
                match result_rx.try_recv() {
                    Ok(result) => {
                        early = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let held = began.elapsed();
        let reply = GenerationControlRecord::new(CONTROL_PREPARED, 2)
            .with_payload(b"test-prepared-reply".to_vec());
        let written = if request.is_ok() && early.is_none() && !disconnected {
            Some(write_generation_control(&mut peer, &reply))
        } else {
            None
        };
        let terminal = if written.as_ref().is_some_and(Result::is_ok) {
            Some(result_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE))
        } else {
            None
        };
        // Closing the retained peer makes every unfinished read observe real
        // EOF. Join before the first assertion, including early-error schedules.
        drop(peer);
        let joined = client_thread.join();
        println!(
            "KEL143_ZERO_BYTE held_ns={} early={early:?} disconnected={disconnected} written={written:?} terminal={terminal:?}",
            held.as_nanos()
        );
        joined.expect("sole control reader joined");
        let request = request.expect("actual complete Prepare2 receipt");
        assert_eq!(request.kind, CONTROL_PREPARE);
        assert_eq!(request.attempt, 2);
        assert_eq!(request.pid, 0);
        assert_eq!(request.cause, 0);
        assert_eq!(request.payload, Vec::<u8>::new());
        assert!(
            early.is_none() && !disconnected,
            "zero-byte successor completed before owned reply release: {early:?}"
        );
        assert!(held >= GUARDIAN_SHUTDOWN_DEADLINE + Duration::from_secs(1));
        written
            .expect("one owned response release")
            .expect("complete canonical Prepared2 write");
        let response = terminal
            .expect("post-release result")
            .expect("bounded result collection")
            .expect("actual successor exchange succeeded");
        assert_eq!(response.kind, CONTROL_PREPARED);
        assert_eq!(response.attempt, 2);
        assert_eq!(response.pid, 0);
        assert_eq!(response.cause, 0);
        assert_eq!(response.payload, reply.payload);
    }

    struct TerminalLivenessReadProbe<'a> {
        reader: File,
        supervisor: &'a Supervisor,
        observed: mpsc::SyncSender<(bool, u32)>,
        terminal_reads: u32,
        reported: bool,
    }

    impl Read for TerminalLivenessReadProbe<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let flags = fcntl(self.reader.as_fd(), FcntlArg::F_GETFL).map_err(nix_io_error)?;
            let nonblocking = OFlag::from_bits_truncate(flags).contains(OFlag::O_NONBLOCK);
            if self
                .supervisor
                .thread
                .as_ref()
                .is_some_and(JoinHandle::is_finished)
            {
                self.terminal_reads = self.terminal_reads.saturating_add(1);
            }
            if !self.reported && (!nonblocking || self.terminal_reads >= 64) {
                let _ = self.observed.send((nonblocking, self.terminal_reads));
                self.reported = true;
            }
            self.reader.read(bytes)
        }
    }

    impl AsFd for TerminalLivenessReadProbe<'_> {
        fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
            self.reader.as_fd()
        }
    }

    fn run_terminal_liveness_probe(
        mut reader: File,
        mut acknowledgment: std::os::unix::net::UnixStream,
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        observed: mpsc::SyncSender<(bool, u32)>,
        initialized: &mpsc::SyncSender<()>,
    ) -> (Result<(), RuntimeError>, bool, bool, SupervisorOutcome, u32) {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exit 1"]);
        let supervisor = Supervisor::start_prepared(
            RestartPolicy::default(),
            GatedUnmarkedFailurePreparer {
                initial: Some(command),
                entered_tx: entered,
                release_rx: release,
                phase: "unused marked phase",
                kind: io::ErrorKind::Interrupted,
                owned: true,
            },
        )
        .expect("real controlled Supervisor child");
        validate_liveness_bootstrap(&mut reader).expect("actual live owned pipe");
        set_liveness_nonblocking(&reader, true).expect("existing active-loop descriptor mode");
        initialized
            .send(())
            .expect("descriptor validation precedes actual Q");
        let mut probe = TerminalLivenessReadProbe {
            reader,
            supervisor: &supervisor,
            observed,
            terminal_reads: 0,
            reported: false,
        };
        let (result, accepted, dead) = await_guarded_primary_liveness(
            &mut probe,
            &supervisor,
            &mut acknowledgment,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
        );
        drop(probe);
        let outcome = supervisor.wait_for_outcome();
        let terminations = supervisor.crash_ledger().self_termination_count;
        (result, accepted, dead, outcome, terminations)
    }

    #[test]
    fn attributed_stopped_liveness_wait_blocks_on_owned_descriptor() {
        let (reader, mut writer) = liveness_pipe();
        let (acknowledgment, mut ack_reader) =
            std::os::unix::net::UnixStream::pair().expect("actual Q ACK pair");
        ack_reader
            .set_read_timeout(Some(SUPERVISED_QUIT_ACK_DEADLINE))
            .expect("existing acknowledgment kill switch");
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (observed_tx, observed_rx) = mpsc::sync_channel(1);
        let (initialized_tx, initialized_rx) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            run_terminal_liveness_probe(
                reader,
                acknowledgment,
                entered_tx,
                release_rx,
                observed_tx,
                &initialized_tx,
            )
        });
        let initialized = initialized_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE);
        let entered = entered_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE);
        let write = if initialized.is_ok() && entered.is_ok() {
            writer.write_all(&[SUPERVISED_QUIT_ACCEPTED])
        } else {
            Err(io::Error::other(
                "controlled preparation did not reach attempt2",
            ))
        };
        let mut ack = [0_u8; 3];
        let acknowledged = if write.is_ok() {
            ack_reader.read_exact(&mut ack)
        } else {
            Err(io::Error::other("Q was not written"))
        };
        // Release only after the actual ACK cut; always release during failed
        // collection so no assertion strands the owned preparer or pipe read.
        let released = release_tx.send(());
        drop(release_tx);
        let observation = if acknowledged.is_ok() {
            observed_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
        } else {
            Err(mpsc::RecvTimeoutError::Disconnected)
        };
        drop(writer);
        let joined = worker.join();
        println!(
            "KEL143_TERMINAL_WAIT acknowledged={acknowledged:?} ack={ack:?} observation={observation:?}"
        );
        initialized.expect("actual descriptor validation/mode setup");
        entered.expect("actual retired G1/attempt2 fence");
        write.expect("actual Q byte");
        acknowledged.expect("actual Q ACK bytes");
        released.expect("owned interruption release");
        let (nonblocking, terminal_reads) = observation.expect("bounded terminal wait witness");
        let (result, accepted, dead, outcome, terminations) = joined.expect("all owners joined");
        result.expect("actual host EOF completes liveness wait");
        assert_eq!(ack, SUPERVISED_QUIT_ACK);
        assert!(accepted && dead);
        assert!(matches!(outcome, SupervisorOutcome::Stopped));
        assert_eq!(terminations, 1, "retained actual G1 exit fact");
        assert!(
            !nonblocking,
            "terminal loop remained nonblocking after sender exit; reads={terminal_reads}"
        );
        assert!(
            terminal_reads < 64,
            "terminal disconnected-event loop repeated reads"
        );
    }

    // Existing approved KEL-143 §7 schedules. This test relay copies exact
    // private records; run_generation_host remains the actual mint/lease owner.
    mod runtime_race_validation {
        use super::*;
        use std::net::Shutdown;
        use std::os::unix::net::UnixStream;
        use std::sync::TryLockError;

        const COLLECTION: Duration = Duration::from_secs(7);

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DeliveryCut {
            Zero,
            Partial,
            FullBeforeLease,
            LeaseBeforeSpawn,
        }

        enum RelayCommand {
            Full,
            Prefix,
            AwaitClosure,
        }

        #[derive(Debug, PartialEq, Eq)]
        struct ControlFields {
            kind: u8,
            cause: u8,
            attempt: u32,
            pid: u32,
            payload_len: usize,
        }

        impl ControlFields {
            fn capture(record: &GenerationControlRecord) -> Self {
                Self {
                    kind: record.kind,
                    cause: record.cause,
                    attempt: record.attempt,
                    pid: record.pid,
                    payload_len: record.payload.len(),
                }
            }
        }

        enum RelayReceipt {
            ClearedOne,
            PreparedTwo(GenerationControlRecord),
            PrefixSent(Instant),
            NormalRevokeTwo {
                request: ControlFields,
                response: ControlFields,
            },
            NormalClearTwo {
                request: ControlFields,
                response: ControlFields,
            },
            GuardianControlEof,
        }

        struct PhaseGate {
            entered: Sender<()>,
            release: Receiver<()>,
        }

        impl PhaseGate {
            fn wait(&self) -> Result<(), RuntimeError> {
                self.entered.send(()).map_err(|_| {
                    lifecycle_error(
                        "test Runtime phase receipt",
                        io::Error::other("custodian ended"),
                    )
                })?;
                self.release.recv_timeout(COLLECTION).map_err(|source| {
                    lifecycle_error("test Runtime phase release", io::Error::other(source))
                })
            }
        }

        struct ReturnedLeasePreparer<C> {
            inner: GuardedPrimaryPreparer<C>,
            gate: Option<PhaseGate>,
        }

        impl<C> ChildPreparer for ReturnedLeasePreparer<C>
        where
            C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
        {
            type Lease = GuardedPrimaryLease;

            fn prepare(
                &mut self,
                attempt: u32,
            ) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
                let prepared = self.inner.prepare(attempt)?;
                // This receipt follows actual GuardedPrimaryLease construction.
                // It does not replace that lease or call another spawn owner.
                if attempt == 2
                    && let Some(gate) = &self.gate
                {
                    gate.wait()?;
                }
                Ok(prepared)
            }
        }

        fn timed_pair() -> (UnixStream, UnixStream) {
            let (one, two) = UnixStream::pair().expect("existing private Unix pair");
            for stream in [&one, &two] {
                stream
                    .set_read_timeout(Some(GENERATION_CONTROL_POLL))
                    .expect("read poll");
                stream
                    .set_write_timeout(Some(GENERATION_CONTROL_POLL))
                    .expect("write poll");
            }
            (one, two)
        }

        fn receive_record(
            stream: &mut UnixStream,
        ) -> Result<GenerationControlRecord, RuntimeError> {
            let mut reader = GenerationControlReader::new();
            let deadline = Instant::now() + COLLECTION;
            loop {
                if let Some(record) = reader.poll(stream)? {
                    return Ok(record);
                }
                if Instant::now() >= deadline {
                    return Err(lifecycle_error(
                        "test Runtime record collection",
                        io::Error::from(io::ErrorKind::TimedOut),
                    ));
                }
            }
        }

        fn private_record_relay(
            mut host: UnixStream,
            mut guardian: UnixStream,
            commands: &Receiver<RelayCommand>,
            receipts: &Sender<RelayReceipt>,
        ) -> Result<(), RuntimeError> {
            loop {
                let request = match receive_record(&mut guardian) {
                    Ok(record) => record,
                    Err(error) if runtime_io_kind(&error) == Some(io::ErrorKind::UnexpectedEof) => {
                        // Actual preparer/Supervisor control closure drives the
                        // real host owner EOF; the relay fabricates no record.
                        receipts
                            .send(RelayReceipt::GuardianControlEof)
                            .map_err(|_| {
                                lifecycle_error(
                                    "test relay actual EOF receipt",
                                    io::Error::other("custodian ended"),
                                )
                            })?;
                        return host.shutdown(Shutdown::Both).map_err(|source| {
                            lifecycle_error("test relay observed control closure", source)
                        });
                    }
                    Err(error) => return Err(error),
                };
                write_generation_control(&mut host, &request)?;
                let response = receive_record(&mut host)?;
                if response.kind == CONTROL_PREPARED && response.attempt == 2 {
                    let mut encoded = Vec::new();
                    write_generation_control(&mut encoded, &response)?;
                    receipts
                        .send(RelayReceipt::PreparedTwo(response))
                        .map_err(|_| {
                            lifecycle_error(
                                "test relay Prepared receipt",
                                io::Error::other("custodian ended"),
                            )
                        })?;
                    match commands.recv_timeout(COLLECTION).map_err(|source| {
                        lifecycle_error("test relay delivery release", io::Error::other(source))
                    })? {
                        RelayCommand::Full => guardian.write_all(&encoded).map_err(|source| {
                            lifecycle_error("test relay complete Prepared write", source)
                        })?,
                        RelayCommand::Prefix => {
                            // One canonical byte is a valid prefix, not a
                            // malformed substitute response or model timeout.
                            let began = Instant::now();
                            guardian.write_all(&encoded[..1]).map_err(|source| {
                                lifecycle_error("test relay Prepared prefix write", source)
                            })?;
                            receipts
                                .send(RelayReceipt::PrefixSent(began))
                                .map_err(|_| {
                                    lifecycle_error(
                                        "test relay prefix receipt",
                                        io::Error::other("custodian ended"),
                                    )
                                })?;
                            match commands.recv_timeout(COLLECTION).map_err(|source| {
                                lifecycle_error(
                                    "test relay partial outcome collection",
                                    io::Error::other(source),
                                )
                            })? {
                                RelayCommand::AwaitClosure => {}
                                RelayCommand::Full | RelayCommand::Prefix => {
                                    return Err(lifecycle_error(
                                        "test relay prefix command",
                                        io::Error::other("unexpected duplicate delivery"),
                                    ));
                                }
                            }
                        }
                        RelayCommand::AwaitClosure => {}
                    }
                } else {
                    write_generation_control(&mut guardian, &response)?;
                    // These facts come from the actual requests and actual
                    // host replies, not the shared Revoked event that EOF can
                    // also emit. Normal lease cleanup must earn its own oracle.
                    let receipt = match (request.kind, request.attempt) {
                        (CONTROL_REVOKE, 2) => Some(RelayReceipt::NormalRevokeTwo {
                            request: ControlFields::capture(&request),
                            response: ControlFields::capture(&response),
                        }),
                        (CONTROL_CLEAR, 2) => Some(RelayReceipt::NormalClearTwo {
                            request: ControlFields::capture(&request),
                            response: ControlFields::capture(&response),
                        }),
                        _ => None,
                    };
                    if let Some(receipt) = receipt {
                        receipts.send(receipt).map_err(|_| {
                            lifecycle_error(
                                "test relay normal cleanup receipt",
                                io::Error::other("custodian ended"),
                            )
                        })?;
                    }
                    if response.kind == CONTROL_CLEARED && response.attempt == 1 {
                        receipts.send(RelayReceipt::ClearedOne).map_err(|_| {
                            lifecycle_error(
                                "test relay Clear receipt",
                                io::Error::other("custodian ended"),
                            )
                        })?;
                    }
                }
            }
        }

        // Test control custody only. The actual production host owner remains
        // run_generation_host; the relay never mints or chooses authority.
        struct ControlPlane {
            host: Option<JoinHandle<Result<(), RuntimeError>>>,
            relay: Option<JoinHandle<Result<(), RuntimeError>>>,
            commands: Sender<RelayCommand>,
            receipts: Receiver<RelayReceipt>,
            recovery: Arc<Mutex<RecoveryState>>,
            updates: Receiver<GuardedPrimaryUpdate>,
            consumption_probe: Option<UnixStream>,
            current: Arc<Mutex<Option<RegisteredPrimaryGeneration>>>,
            guardian_control: Option<UnixStream>,
        }

        impl ControlPlane {
            fn start(cut: DeliveryCut) -> Self {
                let (host_control, relay_host) = timed_pair();
                let (guardian_control, relay_guardian) = timed_pair();
                // This clone observes FIONREAD only, never consumes bytes.
                // It is dropped before cancellation/EOF to preserve custody.
                let consumption_probe = (cut == DeliveryCut::Partial).then(|| {
                    guardian_control
                        .try_clone()
                        .expect("sole-consumer observation alias")
                });
                let (updates_tx, updates) = mpsc::channel();
                let current = Arc::new(Mutex::new(None));
                let recovery = Arc::new(Mutex::new(RecoveryState::new()));
                lock_recovery(&recovery).arm();
                let host_current = Arc::clone(&current);
                let host_recovery = Arc::clone(&recovery);
                let host = thread::spawn(move || {
                    run_generation_host(
                        host_control,
                        &updates_tx,
                        &host_current,
                        &host_recovery,
                        None,
                    )
                });
                let (commands, commands_rx) = mpsc::channel();
                let (receipts_tx, receipts) = mpsc::channel();
                let relay = thread::spawn(move || {
                    private_record_relay(relay_host, relay_guardian, &commands_rx, &receipts_tx)
                });
                Self {
                    host: Some(host),
                    relay: Some(relay),
                    commands,
                    receipts,
                    recovery,
                    updates,
                    consumption_probe,
                    current,
                    guardian_control: Some(guardian_control),
                }
            }
        }

        impl Drop for ControlPlane {
            fn drop(&mut self) {
                lock_recovery(&self.recovery).deny();
                self.consumption_probe.take();
                if let Some(stream) = self.guardian_control.take() {
                    let _ = stream.shutdown(Shutdown::Both);
                }
                for worker in [self.relay.take(), self.host.take()].into_iter().flatten() {
                    match worker.join() {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => eprintln!("KEL143_RACE_RESCUE owner error: {error}"),
                        Err(error) => eprintln!("KEL143_RACE_RESCUE thread panic: {error:?}"),
                    }
                }
            }
        }

        fn await_initial_registration(
            supervisor: &Supervisor,
            current: &Mutex<Option<RegisteredPrimaryGeneration>>,
        ) -> u32 {
            let pid = match supervisor.recv_event(COLLECTION) {
                Some(crate::SupervisorEvent::Started { pid, attempt: 1 }) => pid,
                Some(event) => panic!("unexpected initial enrollment event: {event:?}"),
                None => panic!("actual initial group registration absent"),
            };
            assert_eq!(
                lock_generation(current).map(|generation| generation.group_pid),
                Some(pid)
            );
            pid
        }

        struct CutFixture {
            supervisor: Option<Supervisor>,
            plane: ControlPlane,
            phase_entered: Receiver<()>,
            phase_release: Sender<()>,
            cancelled: Arc<AtomicBool>,
            no_effect: tempfile::TempDir,
            initial_pid: u32,
        }

        impl CutFixture {
            fn start(cut: DeliveryCut) -> Self {
                let mut plane = ControlPlane::start(cut);
                let guardian_control = plane
                    .guardian_control
                    .take()
                    .expect("one guardian private stream");
                let (phase_tx, phase_entered) = mpsc::channel();
                let (phase_release, phase_rx) = mpsc::channel();
                let mut factory_gate = None;
                let mut lease_gate = None;
                match cut {
                    DeliveryCut::FullBeforeLease => {
                        factory_gate = Some(PhaseGate {
                            entered: phase_tx,
                            release: phase_rx,
                        });
                    }
                    DeliveryCut::LeaseBeforeSpawn => {
                        lease_gate = Some(PhaseGate {
                            entered: phase_tx,
                            release: phase_rx,
                        });
                    }
                    DeliveryCut::Zero | DeliveryCut::Partial => {}
                }
                let no_effect = tempfile::tempdir().expect("owned successor no-effect sentinel");
                let successor_effect = no_effect.path().join("unexpected-g2-effect");
                let initial_release = no_effect.path().join("g1-release");
                let initial_release_for_child = initial_release.clone();
                let mut calls = 0;
                let command_factory = move |_app_link: &str| {
                    calls += 1;
                    let mut command = Command::new("/bin/sh");
                    if calls == 1 {
                        command.args([
                            "-c",
                            "printf KEL143_RACE_G1; while [ ! -e \"$1\" ]; do :; done; exit 1",
                            "keld-race",
                        ]);
                        command.arg(&initial_release_for_child);
                    } else {
                        if let Some(gate) = &factory_gate {
                            gate.wait()?;
                        }
                        command.args([
                            "-c",
                            "printf KEL143_RACE_BAD_G2; printf bad > \"$1\"; exit 0",
                            "keld-race",
                        ]);
                        command.arg(&successor_effect);
                    }
                    Ok(command)
                };
                let cancelled = Arc::new(AtomicBool::new(false));
                let supervisor = Supervisor::start_prepared(
                    RestartPolicy::default(),
                    ReturnedLeasePreparer {
                        inner: GuardedPrimaryPreparer {
                            command_factory,
                            control: Arc::new(Mutex::new(GenerationControlPeer {
                                stream: guardian_control,
                                reader: GenerationControlReader::new(),
                            })),
                            host_unavailable: Arc::new(AtomicBool::new(false)),
                            preparation_cancelled: Arc::clone(&cancelled),
                        },
                        gate: lease_gate,
                    },
                )
                .expect("real initial Supervisor child with actual guarded preparation");
                // Startup spawn precedes group enrollment. The production
                // Started receipt follows actual child_spawned/host enrollment.
                let initial_pid = await_initial_registration(&supervisor, &plane.current);
                std::fs::write(initial_release, b"release-g1")
                    .expect("release actual registered G1");
                Self {
                    supervisor: Some(supervisor),
                    plane,
                    phase_entered,
                    phase_release,
                    cancelled,
                    no_effect,
                    initial_pid,
                }
            }

            fn release_after_clear(&self) -> GenerationControlRecord {
                assert!(matches!(
                    self.plane
                        .receipts
                        .recv_timeout(COLLECTION)
                        .expect("actual Clear1"),
                    RelayReceipt::ClearedOne
                ));
                assert!(lock_generation(&self.plane.current).is_none());
                lock_recovery(&self.plane.recovery).arm();
                match self
                    .plane
                    .receipts
                    .recv_timeout(COLLECTION)
                    .expect("actual host Prepared2 emission")
                {
                    RelayReceipt::PreparedTwo(record) => record,
                    _ => panic!("unexpected protocol receipt before Prepared2"),
                }
            }

            fn accept_actual_q(&self) {
                lock_recovery(&self.plane.recovery).deny();
                let (mut reader, mut writer) = liveness_pipe();
                validate_liveness_bootstrap(&mut reader).expect("owned validated liveness pipe");
                set_liveness_nonblocking(&reader, true).expect("active liveness poll");
                let (mut ack_writer, mut ack_reader) = timed_pair();
                writer
                    .write_all(&[SUPERVISED_QUIT_ACCEPTED])
                    .expect("actual Q byte");
                let mut quit = false;
                let mut orderly = false;
                assert!(
                    !observe_guarded_liveness(
                        &mut reader,
                        self.supervisor.as_ref().expect("Supervisor owner"),
                        &mut quit,
                        &mut orderly,
                        &mut ack_writer,
                        &self.cancelled
                    )
                    .expect("existing guardian Q handling")
                );
                let mut bytes = [0; 3];
                ack_reader
                    .read_exact(&mut bytes)
                    .expect("actual KQA receipt");
                assert_eq!(bytes, *b"KQA");
                assert!(quit && !orderly);
                lock_recovery(&self.plane.recovery).shutdown_acknowledged = true;
            }

            fn collect(
                mut self,
                cut: DeliveryCut,
                response: &GenerationControlRecord,
                began: Option<Instant>,
            ) {
                // All releases/joins precede assertions. The fixture Drop also
                // releases every fence on failed collection or an unwind.
                let supervisor = self.supervisor.as_ref().expect("Supervisor owner");
                let (terminal, started_two) = collect_terminal_event(supervisor);
                let outcome = supervisor.wait_for_outcome();
                let current_pid = supervisor.current_pid();
                let ledger = supervisor.crash_ledger();
                let output = supervisor.output();
                // wait_for_outcome observes completion; dropping this sole
                // Supervisor joins its actual direct child/capture owner.
                drop(self.supervisor.take());
                let relay = self
                    .plane
                    .relay
                    .take()
                    .expect("one relay")
                    .join()
                    .expect("relay thread joins");
                let host = self
                    .plane
                    .host
                    .take()
                    .expect("one actual host owner")
                    .join()
                    .expect("host thread joins");
                let updates: Vec<_> = self.plane.updates.try_iter().collect();
                let cleanup_receipts: Vec<_> = self.plane.receipts.try_iter().collect();
                let (endpoint, _) = parse_app_link(
                    std::str::from_utf8(&response.payload).expect("actual app link UTF8"),
                )
                .expect("actual minted locator");
                let endpoint_gone = UnixStream::connect(endpoint).is_err();
                let no_effect = !self.no_effect.path().join("unexpected-g2-effect").exists();
                relay.expect("real control EOF forwarded after guardian-side owner drop");
                host.expect("actual exact host lease cleanup completed before EOF success");
                assert!(!started_two && current_pid.is_none());
                assert!(lock_generation(&self.plane.current).is_none());
                assert_eq!(ledger.self_termination_count, 1);
                assert_eq!(
                    ledger.last_self_termination.map(|entry| entry.pid),
                    Some(self.initial_pid)
                );
                assert_gone(Some(self.initial_pid));
                assert!(
                    ledger
                        .last_self_termination
                        .is_some_and(|entry| entry.exit_code == Some(1))
                );
                assert!(output.stdout.contains("KEL143_RACE_G1"));
                assert!(!output.stdout.contains("KEL143_RACE_BAD_G2") && no_effect);
                assert!(updates.iter().any(|update| matches!(
                    update,
                    GuardedPrimaryUpdate::Role(RoleEvent::Provisioned { attempt: 2, .. })
                )));
                assert!(updates.iter().any(|update| matches!(
                    update,
                    GuardedPrimaryUpdate::Role(RoleEvent::Revoked {
                        attempt: 2,
                        cause: crate::role::RoleRevocationCause::Shutdown,
                        ..
                    })
                )));
                assert!(!updates.iter().any(|update| matches!(update, GuardedPrimaryUpdate::Bound(bound) if bound.attempt() == 2)));
                assert!(!updates.iter().any(|update| matches!(
                    update,
                    GuardedPrimaryUpdate::Role(
                        RoleEvent::Spawned { attempt: 2, .. }
                            | RoleEvent::LinkBound { attempt: 2, .. }
                    )
                )));
                assert!(endpoint_gone, "exact minted successor listener survived");
                assert_cleanup_route(cut, &cleanup_receipts);
                assert_cut_outcome(cut, &terminal, &outcome, began);
                println!(
                    "KEL143_PREPARED_CUT cut={cut:?} actual_mint=true q_ack=true exact_revoked=2 listener_absent={endpoint_gone} g2_effect_absent={no_effect} direct_child_reaped=true"
                );
            }
        }

        fn assert_cleanup_route(cut: DeliveryCut, receipts: &[RelayReceipt]) {
            let revokes: Vec<_> = receipts
                .iter()
                .enumerate()
                .filter_map(|(index, receipt)| match receipt {
                    RelayReceipt::NormalRevokeTwo { request, response } => {
                        Some((index, request, response))
                    }
                    _ => None,
                })
                .collect();
            let clears: Vec<_> = receipts
                .iter()
                .enumerate()
                .filter_map(|(index, receipt)| match receipt {
                    RelayReceipt::NormalClearTwo { request, response } => {
                        Some((index, request, response))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                receipts
                    .iter()
                    .filter(|receipt| matches!(receipt, RelayReceipt::GuardianControlEof))
                    .count(),
                1,
                "actual guardian control EOF absent/duplicated"
            );
            if matches!(
                cut,
                DeliveryCut::FullBeforeLease | DeliveryCut::LeaseBeforeSpawn
            ) {
                assert_eq!(
                    revokes.len(),
                    1,
                    "normal lease Revoke2 request/ack missing or duplicated"
                );
                assert_eq!(
                    clears.len(),
                    1,
                    "normal lease Clear2 request/ack missing or duplicated"
                );
                // Literal approved private wire fields are an independent
                // oracle; changing production constants cannot change these.
                let expected = [
                    ControlFields {
                        kind: 5,
                        cause: 2,
                        attempt: 2,
                        pid: 0,
                        payload_len: 0,
                    },
                    ControlFields {
                        kind: 6,
                        cause: 0,
                        attempt: 2,
                        pid: 0,
                        payload_len: 0,
                    },
                    ControlFields {
                        kind: 7,
                        cause: 0,
                        attempt: 2,
                        pid: 0,
                        payload_len: 0,
                    },
                    ControlFields {
                        kind: 8,
                        cause: 0,
                        attempt: 2,
                        pid: 0,
                        payload_len: 0,
                    },
                ];
                let actual = [revokes[0].1, revokes[0].2, clears[0].1, clears[0].2];
                assert_eq!(
                    actual,
                    [&expected[0], &expected[1], &expected[2], &expected[3]]
                );
                assert!(
                    revokes[0].0 < clears[0].0,
                    "normal Clear2 acknowledgment preceded Revoke2 acknowledgment"
                );
            } else {
                assert!(
                    revokes.is_empty(),
                    "unclaimed zero/partial cut acquired normal Revoke2 path"
                );
                assert!(
                    clears.is_empty(),
                    "unclaimed zero/partial cut acquired normal Clear2 path"
                );
                // Together with the separately asserted exact host-owned
                // Revoked2/Shutdown and successful host join, actual EOF plus
                // absent normal exchanges identifies the host EOF cleanup.
            }
        }

        fn collect_terminal_event(supervisor: &Supervisor) -> (crate::SupervisorEvent, bool) {
            let deadline = Instant::now() + COLLECTION;
            let mut started_two = false;
            let terminal = loop {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .expect("bounded terminal event collection");
                match supervisor.recv_event(remaining) {
                    Some(crate::SupervisorEvent::Started { attempt: 2, .. }) => started_two = true,
                    Some(
                        event @ (crate::SupervisorEvent::Stopped
                        | crate::SupervisorEvent::Failed { .. }),
                    ) => break event,
                    Some(_) => {}
                    None => panic!("actual Supervisor terminal event absent"),
                }
            };
            (terminal, started_two)
        }

        fn assert_cut_outcome(
            cut: DeliveryCut,
            terminal: &crate::SupervisorEvent,
            outcome: &SupervisorOutcome,
            began: Option<Instant>,
        ) {
            if cut == DeliveryCut::Partial {
                assert!(
                    began.expect("actual prefix write clock").elapsed()
                        >= GUARDIAN_SHUTDOWN_DEADLINE
                );
                assert!(matches!(
                    terminal,
                    crate::SupervisorEvent::Failed { attempt: 2 }
                ));
                match outcome {
                    SupervisorOutcome::Failed(error) => {
                        assert_eq!(runtime_io_kind(error), Some(io::ErrorKind::TimedOut));
                        assert!(!is_owned_preparation_interruption(error, 2));
                        assert!(error.to_string().contains("read deadline"));
                    }
                    other => panic!("partial Prepared timeout was hidden: {other:?}"),
                }
            } else {
                assert!(matches!(terminal, crate::SupervisorEvent::Stopped));
                assert!(matches!(outcome, SupervisorOutcome::Stopped));
            }
        }

        impl Drop for CutFixture {
            fn drop(&mut self) {
                lock_recovery(&self.plane.recovery).deny();
                self.plane.consumption_probe.take();
                if let Some(supervisor) = &self.supervisor {
                    supervisor.shutdown();
                }
                self.cancelled.store(true, Ordering::Release);
                let _ = self.phase_release.send(());
                let _ = self.plane.commands.send(RelayCommand::AwaitClosure);
                drop(self.supervisor.take());
            }
        }

        fn run_delivery_cut(cut: DeliveryCut) {
            let mut fixture = CutFixture::start(cut);
            let response = fixture.release_after_clear();
            let mut began = None;
            match cut {
                DeliveryCut::Zero => {
                    fixture.accept_actual_q();
                    fixture
                        .plane
                        .commands
                        .send(RelayCommand::AwaitClosure)
                        .expect("zero-byte closure collection");
                }
                DeliveryCut::Partial => {
                    fixture
                        .plane
                        .commands
                        .send(RelayCommand::Prefix)
                        .expect("one canonical prefix");
                    began = match fixture
                        .plane
                        .receipts
                        .recv_timeout(COLLECTION)
                        .expect("actual prefix write receipt")
                    {
                        RelayReceipt::PrefixSent(time) => Some(time),
                        _ => panic!("unexpected prefix receipt"),
                    };
                    let deadline = Instant::now() + COLLECTION;
                    loop {
                        if rustix::io::ioctl_fionread(
                            fixture
                                .plane
                                .consumption_probe
                                .as_ref()
                                .expect("observation alias"),
                        )
                        .expect("existing safe available-byte primitive")
                            == 0
                        {
                            break;
                        }
                        assert!(
                            Instant::now() < deadline,
                            "valid prefix not consumed by sole control reader"
                        );
                        thread::yield_now();
                    }
                    drop(fixture.plane.consumption_probe.take());
                    fixture.accept_actual_q();
                    fixture
                        .plane
                        .commands
                        .send(RelayCommand::AwaitClosure)
                        .expect("partial suffix remains withheld until actual failure/EOF");
                }
                DeliveryCut::FullBeforeLease | DeliveryCut::LeaseBeforeSpawn => {
                    fixture
                        .plane
                        .commands
                        .send(RelayCommand::Full)
                        .expect("full canonical Prepared2 release");
                    fixture
                        .phase_entered
                        .recv_timeout(COLLECTION)
                        .expect("exact factory/returned-lease boundary");
                    fixture.accept_actual_q();
                    fixture
                        .phase_release
                        .send(())
                        .expect("release only after actual Q ACK");
                }
            }
            fixture.collect(cut, &response, began);
        }

        #[test]
        fn prepared_delivery_zero_bytes_retires_exact_unclaimed_host_lease() {
            run_delivery_cut(DeliveryCut::Zero);
        }

        #[test]
        fn prepared_delivery_partial_prefix_keeps_framing_clock_and_typed_timeout() {
            run_delivery_cut(DeliveryCut::Partial);
        }

        #[test]
        fn prepared_delivery_full_before_lease_uses_normal_supervisor_revoke() {
            run_delivery_cut(DeliveryCut::FullBeforeLease);
        }

        #[test]
        fn prepared_delivery_post_lease_pre_spawn_uses_normal_supervisor_revoke() {
            run_delivery_cut(DeliveryCut::LeaseBeforeSpawn);
        }

        struct RetiredOwner {
            guardian: Option<UnixStream>,
            recovery: Arc<Mutex<RecoveryState>>,
            updates: Receiver<GuardedPrimaryUpdate>,
            worker: Option<JoinHandle<Result<(), RuntimeError>>>,
        }

        impl RetiredOwner {
            fn stream(&mut self) -> &mut UnixStream {
                self.guardian.as_mut().expect("retained private peer")
            }

            fn join(&mut self) -> Result<Vec<GuardedPrimaryUpdate>, RuntimeError> {
                self.worker
                    .take()
                    .expect("one actual host owner")
                    .join()
                    .map_err(|_| {
                        lifecycle_error(
                            "test actual mint owner join",
                            io::Error::other("owner panicked"),
                        )
                    })??;
                Ok(self.updates.try_iter().collect())
            }
        }

        impl Drop for RetiredOwner {
            fn drop(&mut self) {
                lock_recovery(&self.recovery).deny();
                self.guardian.take();
                if let Some(worker) = self.worker.take() {
                    match worker.join() {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => eprintln!("KEL143_MINT_RESCUE owner error: {error}"),
                        Err(error) => eprintln!("KEL143_MINT_RESCUE panic: {error:?}"),
                    }
                }
            }
        }

        fn prepare_retired_owner(fence: Option<GenerationProvisionFence>) -> RetiredOwner {
            let (host, guardian) = timed_pair();
            let (updates_tx, updates_rx) = mpsc::channel();
            let current = Arc::new(Mutex::new(None));
            let recovery = Arc::new(Mutex::new(RecoveryState::new()));
            lock_recovery(&recovery).arm();
            let state = Arc::clone(&recovery);
            let worker = thread::spawn(move || {
                run_generation_host(host, &updates_tx, &current, &state, fence.as_ref())
            });
            let mut owner = RetiredOwner {
                guardian: Some(guardian),
                recovery,
                updates: updates_rx,
                worker: Some(worker),
            };
            write_generation_control(
                owner.stream(),
                &GenerationControlRecord::new(CONTROL_PREPARE, 1),
            )
            .expect("actual Prepare1");
            assert_eq!(
                receive_record(owner.stream())
                    .expect("actual Prepared1")
                    .kind,
                CONTROL_PREPARED
            );
            for (kind, expected) in [
                (CONTROL_REVOKE, CONTROL_REVOKED),
                (CONTROL_CLEAR, CONTROL_CLEARED),
            ] {
                let record = if kind == CONTROL_REVOKE {
                    GenerationControlRecord::new(kind, 1)
                        .with_cause(RevocationCause::AdmissionFailed)
                } else {
                    GenerationControlRecord::new(kind, 1)
                };
                write_generation_control(owner.stream(), &record)
                    .expect("actual retired predecessor transition");
                assert_eq!(
                    receive_record(owner.stream())
                        .expect("actual predecessor acknowledgment")
                        .kind,
                    expected
                );
            }
            owner
        }

        #[test]
        fn generation_host_provision_mutex_orders_actual_mint_and_denial() {
            let (entered_tx, entered_rx) = mpsc::sync_channel(1);
            let (release_tx, release_rx) = mpsc::channel();
            let mut owner = prepare_retired_owner(Some(GenerationProvisionFence {
                attempt: 2,
                entered: entered_tx,
                release: release_rx,
            }));
            let recovery = Arc::clone(&owner.recovery);
            lock_recovery(&recovery).arm();
            write_generation_control(
                owner.stream(),
                &GenerationControlRecord::new(CONTROL_PREPARE, 2),
            )
            .expect("actual Prepare2");
            let entered = entered_rx.recv_timeout(COLLECTION);
            // This is an independent lock observable while actual host mint is
            // fenced. The fault releases the guard before this same fence.
            let denial_won_before_mint = match recovery.try_lock() {
                Ok(mut state) => {
                    state.deny();
                    true
                }
                Err(TryLockError::WouldBlock) => false,
                Err(TryLockError::Poisoned(error)) => {
                    error.into_inner().deny();
                    true
                }
            };
            let released = release_tx.send(());
            let response = receive_record(owner.stream());
            let events_at_mint: Vec<_> = owner.updates.try_iter().collect();
            lock_recovery(&recovery).deny();
            let mut listener_was_live = false;
            let mut listener_gone = false;
            if let Ok(response) = &response {
                let (endpoint, _) = parse_app_link(
                    std::str::from_utf8(&response.payload).expect("actual app link UTF8"),
                )
                .expect("actual minted locator");
                listener_was_live = UnixStream::connect(endpoint).is_ok();
                for (kind, expected) in [
                    (CONTROL_REVOKE, CONTROL_REVOKED),
                    (CONTROL_CLEAR, CONTROL_CLEARED),
                ] {
                    let record = if kind == CONTROL_REVOKE {
                        GenerationControlRecord::new(kind, 2).with_cause(RevocationCause::Shutdown)
                    } else {
                        GenerationControlRecord::new(kind, 2)
                    };
                    write_generation_control(owner.stream(), &record)
                        .expect("actual successor cleanup transition");
                    assert_eq!(
                        receive_record(owner.stream())
                            .expect("actual successor cleanup acknowledgment")
                            .kind,
                        expected
                    );
                }
                listener_gone = UnixStream::connect(endpoint).is_err();
            }
            lock_recovery(&recovery).shutdown_acknowledged = true;
            owner.guardian.take();
            let joined = owner.join();
            entered.expect("actual pre-mint owner fence reached");
            released.expect("release actual mint fence before assertions");
            let response = response.expect("actual owner minted matching Prepared2");
            assert_eq!((response.kind, response.attempt), (CONTROL_PREPARED, 2));
            let after_cleanup =
                joined.expect("actual host owner joins after exact cleanup and attributed EOF");
            assert!(after_cleanup.iter().any(|update| matches!(
                update,
                GuardedPrimaryUpdate::Role(RoleEvent::Revoked {
                    attempt: 2,
                    cause: crate::role::RoleRevocationCause::Shutdown,
                    ..
                })
            )));
            assert!(events_at_mint.iter().any(|update| matches!(
                update,
                GuardedPrimaryUpdate::Role(RoleEvent::Provisioned { attempt: 2, .. })
            )));
            assert!(listener_was_live && listener_gone);
            println!(
                "KEL143_MINT_FENCE denied_before_actual_mint={denial_won_before_mint} actual_provisioned=2 listener_was_live={listener_was_live} listener_absent={listener_gone}"
            );
            assert!(
                !denial_won_before_mint,
                "actual mint followed observed denial at the shared pre-mint fence"
            );
        }

        #[test]
        fn generation_host_denial_winner_has_no_successor_locator() {
            let mut owner = prepare_retired_owner(None);
            let recovery = Arc::clone(&owner.recovery);
            {
                let mut state = lock_recovery(&recovery);
                state.deny();
                state.arm();
                state.shutdown_acknowledged = true;
            }
            write_generation_control(
                owner.stream(),
                &GenerationControlRecord::new(CONTROL_PREPARE, 2),
            )
            .expect("actual denied Prepare2");
            // Draining EOF is the actual owner's final processing witness;
            // absence during a guessed timed interval would not establish it.
            owner
                .stream()
                .shutdown(Shutdown::Write)
                .expect("real private request EOF after complete Prepare2");
            let updates = owner
                .join()
                .expect("actual denied host owner joins after resource-free attributed completion");
            assert!(!updates.iter().any(|update| matches!(
                update,
                GuardedPrimaryUpdate::Role(RoleEvent::Provisioned { attempt: 2, .. })
            )));
            let closure =
                receive_record(owner.stream()).expect_err("denial fabricated Prepared2 locator");
            assert_eq!(
                runtime_io_kind(&closure),
                Some(io::ErrorKind::UnexpectedEof)
            );
            println!(
                "KEL143_MINT_FENCE denial_winner=true successor_locator=false actual_owner_eof=true"
            );
        }
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
