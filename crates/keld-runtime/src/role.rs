//! Authenticated Bun role coordinator shared by KEL-75 T1b/T2/T8.
//!
//! One coordinator instance is one lifecycle owner (`primary` or `app-bound`).
//! It consumes the crate-private prepared-child lease in [`Supervisor`] and
//! does not implement a second restart loop: spawn, backoff, crash-loop
//! breaking, output capture, shutdown and reap stay in the generic supervisor.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use keld_ipc::{
    AppLinkDeadlines, BootstrapAdmission, BootstrapCancellation, BootstrapListener,
    BootstrapRejection, BootstrapRejectionObserver, BootstrapStream, SessionToken, parse_app_link,
};

use crate::{
    CapturedOutput, ChildPreparer, GenerationLease, PreparedChild, RestartPolicy, RevocationCause,
    RuntimeError, Supervisor, SupervisorOutcome, lock_or_recover,
};

pub(crate) const DEFAULT_ADMISSION_TIMEOUT: Duration = Duration::from_secs(10);
const FRESH_BOOTSTRAP_ATTEMPTS: usize = 8;
const RECOVERY_PENDING: u8 = 0;
const RECOVERY_ARMED: u8 = 1;
const RECOVERY_DENIED: u8 = 2;
const RECOVERY_POLL: Duration = Duration::from_millis(10);

/// Host decision that gates crash recovery until initial application readiness.
///
/// The generic supervisor remains the sole restart owner. This handle only
/// releases or rejects its existing post-revocation/pre-provision boundary.
#[derive(Debug)]
pub struct RoleRecoveryGate {
    decision: Arc<AtomicU8>,
}

impl RoleRecoveryGate {
    /// Allows the supervisor to provision crash successors for this session.
    ///
    /// Returns `true` when recovery is armed after this call.
    #[must_use]
    pub fn arm(&self) -> bool {
        self.decision
            .compare_exchange(
                RECOVERY_PENDING,
                RECOVERY_ARMED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
            || self.decision.load(Ordering::Acquire) == RECOVERY_ARMED
    }

    /// Rejects successor provisioning before initial readiness.
    ///
    /// Returns `true` when recovery is denied after this call.
    #[must_use]
    pub fn deny(&self) -> bool {
        self.decision
            .compare_exchange(
                RECOVERY_PENDING,
                RECOVERY_DENIED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
            || self.decision.load(Ordering::Acquire) == RECOVERY_DENIED
    }
}

impl Drop for RoleRecoveryGate {
    fn drop(&mut self) {
        let _ = self.deny();
    }
}

struct BootstrapCandidate<T> {
    resource: T,
    app_link: String,
    endpoint: String,
    token: SessionToken,
}

fn select_fresh_candidate<T, E>(
    last_endpoint: Option<&str>,
    last_token: Option<SessionToken>,
    mut mint: impl FnMut() -> Result<BootstrapCandidate<T>, E>,
) -> Result<Option<BootstrapCandidate<T>>, E> {
    let mut rejected = Vec::with_capacity(FRESH_BOOTSTRAP_ATTEMPTS);
    for _ in 0..FRESH_BOOTSTRAP_ATTEMPTS {
        let candidate = mint()?;
        let endpoint_is_fresh = last_endpoint != Some(candidate.endpoint.as_str());
        let token_is_fresh = last_token != Some(candidate.token);
        if endpoint_is_fresh && token_is_fresh {
            return Ok(Some(candidate));
        }
        // Keep rejected endpoint owners live until selection completes. On
        // Windows, dropping a port-0 listener would make that same numeric
        // endpoint immediately eligible for the next retry.
        rejected.push(candidate.resource);
    }
    Ok(None)
}

/// Host-owned lifecycle category for one authenticated Bun role.
///
/// These are lifecycle owners, not package, Electron, or VS Code identities.
/// `window-bound` is T4 and is not admitted here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoleOwner {
    /// One app entry role. Stops with the host application session.
    Primary,
    /// Shared worker, PTY facade, or agent. Stops with the host application
    /// session. Independent of primary restart/crash.
    AppBound,
}

impl RoleOwner {
    /// Stable owner label for diagnostics. Never a secret.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::AppBound => "app-bound",
        }
    }

    const fn generation_phase(self) -> &'static str {
        match self {
            Self::Primary => "primary role generation",
            Self::AppBound => "app-bound role generation",
        }
    }

    const fn bootstrap_bind_phase(self) -> &'static str {
        match self {
            Self::Primary => "primary bootstrap bind",
            Self::AppBound => "app-bound bootstrap bind",
        }
    }

    const fn bootstrap_admission_phase(self) -> &'static str {
        match self {
            Self::Primary => "primary bootstrap admission",
            Self::AppBound => "app-bound bootstrap admission",
        }
    }

    const fn admission_thread_phase(self) -> &'static str {
        match self {
            Self::Primary => "primary bootstrap admission thread",
            Self::AppBound => "app-bound bootstrap admission thread",
        }
    }

    const fn admission_thread_name(self) -> &'static str {
        match self {
            Self::Primary => "keld-runtime-primary-bootstrap",
            Self::AppBound => "keld-runtime-app-bound-bootstrap",
        }
    }

    const fn admission_timeout_message(self) -> &'static str {
        match self {
            Self::Primary => "primary role did not authenticate before its generation deadline",
            Self::AppBound => "app-bound role did not authenticate before its generation deadline",
        }
    }
}

/// Host-minted role generation.
///
/// A generation is host metadata only. It is not a PID, socket path, token,
/// environment value, or wire field. Counters are per coordinator instance,
/// so a primary generation of `1` is not the same principal as an app-bound
/// generation of `1`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoleGeneration(u64);

impl RoleGeneration {
    /// Creates a generation counter for crate-internal tests.
    #[cfg(test)]
    #[cfg(unix)]
    pub(crate) fn from_test_counter(value: u64) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for RoleGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RoleGeneration(..)")
    }
}

/// One authenticated role generation handed to its host-side stream owner.
///
/// The generation and attempt are host metadata. The stream has already
/// completed the possession-token `HELLO`; consumers must not run a second
/// handshake or derive identity from peer payload bytes.
pub struct BoundRoleGeneration {
    generation: RoleGeneration,
    attempt: u32,
    stream: BootstrapStream,
}

impl BoundRoleGeneration {
    /// Host-minted generation bound to this stream.
    #[must_use]
    pub const fn generation(&self) -> RoleGeneration {
        self.generation
    }

    /// One-indexed supervisor attempt that produced this stream.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Transfers the authenticated stream to the host router.
    #[must_use]
    pub fn into_stream(self) -> BootstrapStream {
        self.stream
    }
}

impl std::fmt::Debug for BoundRoleGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundRoleGeneration")
            .field("generation", &self.generation)
            .field("attempt", &self.attempt)
            .finish_non_exhaustive()
    }
}

/// Why a role generation was revoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleRevocationCause {
    /// The child exited naturally.
    ChildExited,
    /// The host requested shutdown.
    Shutdown,
    /// The supervisor could not safely start stdout/stderr capture.
    CaptureFailed,
    /// The generation failed before a live authenticated link was available.
    AdmissionFailed,
    /// The OS refused to spawn the prepared command.
    SpawnFailed,
    /// The supervisor could not observe the live child.
    WaitFailed,
}

impl From<RevocationCause> for RoleRevocationCause {
    fn from(cause: RevocationCause) -> Self {
        match cause {
            RevocationCause::ChildExited => Self::ChildExited,
            RevocationCause::Shutdown => Self::Shutdown,
            RevocationCause::CaptureFailed => Self::CaptureFailed,
            RevocationCause::AdmissionFailed => Self::AdmissionFailed,
            RevocationCause::SpawnFailed => Self::SpawnFailed,
            RevocationCause::WaitFailed => Self::WaitFailed,
        }
    }
}

/// Host-only authenticated-role lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleEvent {
    /// A fresh endpoint and possession token were minted for this attempt.
    Provisioned {
        /// Fresh host-minted generation.
        generation: RoleGeneration,
        /// 1-indexed supervisor attempt.
        attempt: u32,
    },
    /// The prepared child was spawned by the generic supervisor.
    Spawned {
        /// Fresh host-minted generation.
        generation: RoleGeneration,
        /// OS pid, diagnostic only.
        pid: u32,
        /// 1-indexed supervisor attempt.
        attempt: u32,
    },
    /// A peer proved possession of this generation's token.
    LinkBound {
        /// Authenticated generation.
        generation: RoleGeneration,
        /// 1-indexed supervisor attempt.
        attempt: u32,
    },
    /// A foreign peer was rejected with a redacted `KELD-IPC-*` code.
    BootstrapRejected {
        /// Generation whose endpoint rejected the peer.
        generation: RoleGeneration,
        /// 1-indexed supervisor attempt.
        attempt: u32,
        /// Stable error code only; no endpoint, token, or raw parser detail.
        code: &'static str,
    },
    /// The generation was revoked before successor provisioning.
    Revoked {
        /// Revoked generation.
        generation: RoleGeneration,
        /// 1-indexed supervisor attempt.
        attempt: u32,
        /// Revocation cause.
        cause: RoleRevocationCause,
    },
}

/// Configuration for one authenticated Bun role coordinator.
#[derive(Debug, Clone)]
pub struct RoleConfig {
    owner: RoleOwner,
    program: OsString,
    args: Vec<OsString>,
    env_remove: Vec<OsString>,
    current_dir: Option<PathBuf>,
    restart_policy: RestartPolicy,
    admission_timeout: Duration,
    #[cfg(test)]
    #[cfg(unix)]
    probe_tx: Option<Sender<ProvisionedProbe>>,
}

impl RoleConfig {
    /// Creates a primary-role command config.
    ///
    /// The coordinator injects a fresh `KELD_APP_LINK` for every spawn
    /// attempt. The program and arguments are role declaration data, not
    /// child-supplied authority.
    #[must_use]
    pub fn new(program: impl Into<OsString>) -> Self {
        Self::primary(program)
    }

    /// Creates a `primary` lifecycle owner config.
    #[must_use]
    pub fn primary(program: impl Into<OsString>) -> Self {
        Self::for_owner(RoleOwner::Primary, program)
    }

    /// Creates an `app-bound` lifecycle owner config.
    #[must_use]
    #[cfg(unix)]
    pub fn app_bound(program: impl Into<OsString>) -> Self {
        Self::for_owner(RoleOwner::AppBound, program)
    }

    fn for_owner(owner: RoleOwner, program: impl Into<OsString>) -> Self {
        Self {
            owner,
            program: program.into(),
            args: Vec::new(),
            env_remove: Vec::new(),
            current_dir: None,
            restart_policy: RestartPolicy::default(),
            admission_timeout: DEFAULT_ADMISSION_TIMEOUT,
            #[cfg(test)]
            #[cfg(unix)]
            probe_tx: None,
        }
    }

    /// Lifecycle owner this command will run as.
    #[must_use]
    pub const fn owner(&self) -> RoleOwner {
        self.owner
    }

    /// Adds one command argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Adds command arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Removes one inherited environment key from every child generation.
    #[must_use]
    pub fn env_remove(mut self, key: impl Into<OsString>) -> Self {
        self.env_remove.push(key.into());
        self
    }

    /// Sets the child working directory.
    #[must_use]
    pub fn current_dir(mut self, current_dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(current_dir.into());
        self
    }

    /// Sets the supervisor restart policy.
    #[must_use]
    pub fn restart_policy(mut self, restart_policy: RestartPolicy) -> Self {
        self.restart_policy = restart_policy;
        self
    }

    /// Sets the generation-wide bootstrap admission timeout.
    #[must_use]
    pub fn admission_timeout(mut self, admission_timeout: Duration) -> Self {
        self.admission_timeout = admission_timeout;
        self
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(crate) fn with_probe(mut self, probe_tx: Sender<ProvisionedProbe>) -> Self {
        self.probe_tx = Some(probe_tx);
        self
    }
}

/// Running authenticated role supervisor.
#[derive(Debug)]
pub struct RoleSupervisor {
    supervisor: Supervisor,
    events_rx: Receiver<RoleEvent>,
    bound_rx: Option<Receiver<BoundRoleGeneration>>,
}

impl RoleSupervisor {
    /// Starts the role under the generic supervisor.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] if the initial generation cannot be
    /// provisioned or the first child cannot be spawned.
    pub fn start(config: RoleConfig) -> Result<Self, RuntimeError> {
        Self::start_inner(config, None, None, None)
    }

    /// Starts the role and exposes each authenticated generation to its host
    /// stream owner.
    ///
    /// This opt-in surface is for the primary host router. Ordinary registry
    /// roles use [`Self::start`] and do not allocate an unconsumed stream feed.
    /// The received stream has already completed `HELLO`; the host must not
    /// authenticate it again or derive role identity from peer bytes.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] if the initial generation cannot be
    /// provisioned or the first child cannot be spawned.
    pub fn start_with_bound_generations(config: RoleConfig) -> Result<Self, RuntimeError> {
        let (bound_tx, bound_rx) = mpsc::channel();
        Self::start_inner(config, Some(bound_tx), Some(bound_rx), None)
    }

    /// Starts the role with authenticated-generation handoff and a one-time
    /// host readiness decision before the first crash successor is prepared.
    ///
    /// The returned gate defaults to deny when dropped. Call
    /// [`RoleRecoveryGate::arm`] only after the host has made initial readiness
    /// externally observable.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] if the initial generation cannot be
    /// provisioned or the first child cannot be spawned.
    pub fn start_with_bound_generations_gated(
        config: RoleConfig,
    ) -> Result<(Self, RoleRecoveryGate), RuntimeError> {
        let (bound_tx, bound_rx) = mpsc::channel();
        let decision = Arc::new(AtomicU8::new(RECOVERY_PENDING));
        let supervisor = Self::start_inner(
            config,
            Some(bound_tx),
            Some(bound_rx),
            Some(Arc::clone(&decision)),
        )?;
        Ok((supervisor, RoleRecoveryGate { decision }))
    }

    fn start_inner(
        config: RoleConfig,
        bound_tx: Option<Sender<BoundRoleGeneration>>,
        bound_rx: Option<Receiver<BoundRoleGeneration>>,
        recovery_decision: Option<Arc<AtomicU8>>,
    ) -> Result<Self, RuntimeError> {
        let (events_tx, events_rx) = mpsc::channel();
        let policy = config.restart_policy;
        let generation_owner = RoleGenerationOwner::new(
            config.owner,
            config.admission_timeout,
            events_tx,
            bound_tx,
            #[cfg(test)]
            #[cfg(unix)]
            config.probe_tx.clone(),
        );
        let preparer = RolePreparer {
            config,
            generation_owner,
            recovery_decision,
        };
        let supervisor = Supervisor::start_prepared(policy, preparer)?;
        Ok(Self {
            supervisor,
            events_rx,
            bound_rx,
        })
    }

    /// Blocks until the next role event, or `timeout` elapses.
    #[must_use]
    pub fn recv_event(&self, timeout: Duration) -> Option<RoleEvent> {
        self.events_rx.recv_timeout(timeout).ok()
    }

    /// Returns the next already-queued role event without waiting.
    #[must_use]
    pub fn try_recv_event(&self) -> Option<RoleEvent> {
        self.events_rx.try_recv().ok()
    }

    /// Blocks until the next authenticated generation is handed to the host,
    /// or `timeout` elapses.
    ///
    /// Returns `None` when this supervisor was started without
    /// [`Self::start_with_bound_generations`].
    #[must_use]
    pub fn recv_bound_generation(&self, timeout: Duration) -> Option<BoundRoleGeneration> {
        self.bound_rx.as_ref()?.recv_timeout(timeout).ok()
    }

    /// Returns the next already-authenticated generation without waiting.
    ///
    /// Returns `None` when no generation is queued or this supervisor was
    /// started without [`Self::start_with_bound_generations`].
    #[must_use]
    pub fn try_recv_bound_generation(&self) -> Option<BoundRoleGeneration> {
        self.bound_rx.as_ref()?.try_recv().ok()
    }

    /// Stops the child, if still live.
    pub fn shutdown(&self) {
        self.supervisor.shutdown();
    }

    /// Waits for the generic supervisor's terminal outcome.
    #[must_use]
    pub fn wait_for_outcome(&self) -> SupervisorOutcome {
        self.supervisor.wait_for_outcome()
    }

    /// Returns the terminal supervisor outcome when it is already available.
    #[must_use]
    pub fn try_wait_for_outcome(&self) -> Option<SupervisorOutcome> {
        self.supervisor.try_wait_for_outcome()
    }

    /// Snapshot of captured child stdout/stderr.
    #[must_use]
    pub fn output(&self) -> CapturedOutput {
        self.supervisor.output()
    }
}

struct RolePreparer {
    config: RoleConfig,
    generation_owner: RoleGenerationOwner,
    recovery_decision: Option<Arc<AtomicU8>>,
}

impl ChildPreparer for RolePreparer {
    type Lease = RoleGenerationLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        let provisioned = self.generation_owner.provision(attempt)?;
        let mut command = Command::new(&self.config.program);
        command.args(&self.config.args);
        if let Some(current_dir) = &self.config.current_dir {
            command.current_dir(current_dir);
        }
        for key in &self.config.env_remove {
            command.env_remove(key);
        }
        command
            .env("KELD_APP_LINK", &provisioned.app_link)
            .stdin(Stdio::null());
        Ok(PreparedChild {
            command,
            lease: provisioned.lease,
        })
    }

    fn allow_restart(&mut self, shutdown: &AtomicBool) -> Result<bool, RuntimeError> {
        let Some(decision) = &self.recovery_decision else {
            return Ok(true);
        };
        await_recovery_decision(decision, shutdown)
    }
}

fn await_recovery_decision(
    decision: &AtomicU8,
    shutdown: &AtomicBool,
) -> Result<bool, RuntimeError> {
    loop {
        if shutdown.load(Ordering::Acquire) {
            return Ok(false);
        }
        match decision.load(Ordering::Acquire) {
            RECOVERY_ARMED => return Ok(true),
            RECOVERY_DENIED => return Ok(false),
            RECOVERY_PENDING => thread::park_timeout(RECOVERY_POLL),
            _ => {
                return Err(RuntimeError::Lifecycle {
                    phase: "primary recovery decision",
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid recovery decision",
                    ),
                });
            }
        }
    }
}

pub(crate) struct RoleGenerationOwner {
    owner: RoleOwner,
    next_generation: u64,
    last_endpoint: Option<String>,
    last_token: Option<SessionToken>,
    admission_timeout: Duration,
    events_tx: Sender<RoleEvent>,
    bound_tx: Option<Sender<BoundRoleGeneration>>,
    #[cfg(test)]
    #[cfg(unix)]
    probe_tx: Option<Sender<ProvisionedProbe>>,
}

impl RoleGenerationOwner {
    pub(crate) fn new(
        owner: RoleOwner,
        admission_timeout: Duration,
        events_tx: Sender<RoleEvent>,
        bound_tx: Option<Sender<BoundRoleGeneration>>,
        #[cfg(test)]
        #[cfg(unix)]
        probe_tx: Option<Sender<ProvisionedProbe>>,
    ) -> Self {
        Self {
            owner,
            next_generation: 1,
            last_endpoint: None,
            last_token: None,
            admission_timeout,
            events_tx,
            bound_tx,
            #[cfg(test)]
            #[cfg(unix)]
            probe_tx,
        }
    }

    pub(crate) fn provision(
        &mut self,
        attempt: u32,
    ) -> Result<ProvisionedRoleGeneration, RuntimeError> {
        let generation = RoleGeneration(self.next_generation);
        self.next_generation =
            self.next_generation
                .checked_add(1)
                .ok_or_else(|| RuntimeError::Lifecycle {
                    phase: self.owner.generation_phase(),
                    source: std::io::Error::other(format!(
                        "{} role generation counter exhausted",
                        self.owner.as_str()
                    )),
                })?;
        let (listener, app_link) = self.bind_fresh_bootstrap()?;
        let cancellation = listener.cancellation();
        let (admission_tx, admission_rx) = mpsc::channel();
        let _ = self.events_tx.send(RoleEvent::Provisioned {
            generation,
            attempt,
        });
        #[cfg(test)]
        #[cfg(unix)]
        let probe = self.probe_tx.as_ref().map(|probe_tx| {
            (
                probe_tx.clone(),
                ProvisionedProbe {
                    generation,
                    app_link: app_link.clone(),
                },
            )
        });
        Ok(ProvisionedRoleGeneration {
            app_link,
            lease: RoleGenerationLease {
                owner: self.owner,
                generation,
                attempt,
                admission_timeout: self.admission_timeout,
                listener: Some(listener),
                cancellation,
                admission_tx: Some(admission_tx),
                admission_rx,
                admission_thread: None,
                admission_done: false,
                link: Arc::new(Mutex::new(None)),
                events_tx: self.events_tx.clone(),
                bound_tx: self.bound_tx.clone(),
                #[cfg(test)]
                #[cfg(unix)]
                probe,
            },
        })
    }

    fn bind_fresh_bootstrap(&mut self) -> Result<(BootstrapListener, String), RuntimeError> {
        let owner = self.owner;
        let candidate =
            select_fresh_candidate(self.last_endpoint.as_deref(), self.last_token, || {
                let listener =
                    BootstrapListener::bind().map_err(|source| RuntimeError::Lifecycle {
                        phase: owner.bootstrap_bind_phase(),
                        source,
                    })?;
                let app_link = listener.app_link();
                let (endpoint, token) =
                    parse_app_link(&app_link).map_err(|source| RuntimeError::Lifecycle {
                        phase: owner.bootstrap_bind_phase(),
                        source: std::io::Error::other(source.to_string()),
                    })?;
                let endpoint = endpoint.to_owned();
                Ok(BootstrapCandidate {
                    resource: listener,
                    app_link,
                    endpoint,
                    token,
                })
            })?
            .ok_or_else(|| RuntimeError::Lifecycle {
                phase: owner.bootstrap_bind_phase(),
                source: std::io::Error::other(
                    "could not mint endpoint and token distinct from the retired generation",
                ),
            })?;
        self.last_endpoint = Some(candidate.endpoint);
        self.last_token = Some(candidate.token);
        Ok((candidate.resource, candidate.app_link))
    }
}

pub(crate) struct ProvisionedRoleGeneration {
    pub(crate) app_link: String,
    pub(crate) lease: RoleGenerationLease,
}

pub(crate) struct RoleGenerationLease {
    owner: RoleOwner,
    generation: RoleGeneration,
    attempt: u32,
    admission_timeout: Duration,
    listener: Option<BootstrapListener>,
    cancellation: BootstrapCancellation,
    admission_tx: Option<Sender<AdmissionResult>>,
    admission_rx: Receiver<AdmissionResult>,
    admission_thread: Option<JoinHandle<()>>,
    admission_done: bool,
    link: Arc<Mutex<Option<BootstrapStream>>>,
    events_tx: Sender<RoleEvent>,
    bound_tx: Option<Sender<BoundRoleGeneration>>,
    #[cfg(test)]
    #[cfg(unix)]
    probe: Option<(Sender<ProvisionedProbe>, ProvisionedProbe)>,
}

impl GenerationLease for RoleGenerationLease {
    fn child_spawned(&mut self, pid: u32, attempt: u32) -> Result<(), RuntimeError> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| RuntimeError::Lifecycle {
                phase: self.owner.bootstrap_admission_phase(),
                source: std::io::Error::other("bootstrap listener already started"),
            })?;
        let admission_tx = self
            .admission_tx
            .take()
            .ok_or_else(|| RuntimeError::Lifecycle {
                phase: self.owner.bootstrap_admission_phase(),
                source: std::io::Error::other("admission channel already started"),
            })?;
        let deadline = Instant::now()
            .checked_add(self.admission_timeout)
            .unwrap_or_else(Instant::now);
        let owner = self.owner;
        let observer = RoleBootstrapObserver {
            generation: self.generation,
            attempt: self.attempt,
            events_tx: self.events_tx.clone(),
            seen_rejections: AtomicU8::new(0),
        };
        self.admission_thread = Some(
            thread::Builder::new()
                .name(owner.admission_thread_name().to_owned())
                .spawn(move || {
                    let result = match listener.accept_authenticated_until(deadline, &observer) {
                        Ok(BootstrapAdmission::Authenticated(stream)) => {
                            AdmissionResult::Bound(stream)
                        }
                        Ok(BootstrapAdmission::Cancelled) => AdmissionResult::Cancelled,
                        Ok(BootstrapAdmission::DeadlineElapsed) => AdmissionResult::DeadlineElapsed,
                        Err(source) => AdmissionResult::Failed(RuntimeError::Lifecycle {
                            phase: owner.bootstrap_admission_phase(),
                            source,
                        }),
                    };
                    let _ = admission_tx.send(result);
                })
                .map_err(|source| RuntimeError::Lifecycle {
                    phase: owner.admission_thread_phase(),
                    source,
                })?,
        );
        let _ = self.events_tx.send(RoleEvent::Spawned {
            generation: self.generation,
            pid,
            attempt,
        });
        #[cfg(test)]
        #[cfg(unix)]
        if let Some((probe_tx, probe)) = self.probe.take() {
            let _ = probe_tx.send(probe);
        }
        Ok(())
    }

    fn poll(&mut self) -> Result<(), RuntimeError> {
        self.poll_admission()
    }

    fn revoke(mut self, cause: RevocationCause) -> Result<(), RuntimeError> {
        let _ = self.cancellation.cancel();
        let mut first_error = self.poll_admission();
        if let Some(thread) = self.admission_thread.take()
            && thread.join().is_err()
            && first_error.is_ok()
        {
            first_error = Err(RuntimeError::Lifecycle {
                phase: self.owner.admission_thread_phase(),
                source: std::io::Error::other("admission thread panicked"),
            });
        }
        if first_error.is_ok() {
            first_error = self.poll_admission();
        }
        if let Some(stream) = lock_or_recover(&self.link).take() {
            let _ = stream.shutdown_app_link();
        }
        let _ = self.events_tx.send(RoleEvent::Revoked {
            generation: self.generation,
            attempt: self.attempt,
            cause: cause.into(),
        });
        first_error
    }
}

impl RoleGenerationLease {
    fn poll_admission(&mut self) -> Result<(), RuntimeError> {
        if self.admission_done {
            return Ok(());
        }
        match self.admission_rx.try_recv() {
            Ok(AdmissionResult::Bound(stream)) => {
                let bound =
                    self.retain_and_clone_bound_stream(stream, BootstrapStream::try_clone)?;
                if let (Some(bound_tx), Some(stream)) = (&self.bound_tx, bound) {
                    bound_tx
                        .send(BoundRoleGeneration {
                            generation: self.generation,
                            attempt: self.attempt,
                            stream,
                        })
                        .map_err(|_| RuntimeError::Lifecycle {
                            phase: self.owner.bootstrap_admission_phase(),
                            source: std::io::Error::other(
                                "host stream owner ended before generation bind",
                            ),
                        })?;
                }
                let _ = self.events_tx.send(RoleEvent::LinkBound {
                    generation: self.generation,
                    attempt: self.attempt,
                });
                Ok(())
            }
            Ok(AdmissionResult::Cancelled) => {
                self.admission_done = true;
                Ok(())
            }
            Ok(AdmissionResult::DeadlineElapsed) => {
                self.admission_done = true;
                Err(RuntimeError::Lifecycle {
                    phase: self.owner.bootstrap_admission_phase(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        self.owner.admission_timeout_message(),
                    ),
                })
            }
            Ok(AdmissionResult::Failed(error)) => {
                self.admission_done = true;
                Err(error)
            }
            Err(TryRecvError::Empty) => Ok(()),
            Err(TryRecvError::Disconnected) => {
                self.admission_done = true;
                Err(RuntimeError::Lifecycle {
                    phase: self.owner.bootstrap_admission_phase(),
                    source: std::io::Error::other("admission worker ended without a result"),
                })
            }
        }
    }

    fn retain_and_clone_bound_stream(
        &mut self,
        stream: BootstrapStream,
        clone_stream: impl FnOnce(&BootstrapStream) -> std::io::Result<BootstrapStream>,
    ) -> Result<Option<BootstrapStream>, RuntimeError> {
        *lock_or_recover(&self.link) = Some(stream);
        self.admission_done = true;
        if self.bound_tx.is_none() {
            return Ok(None);
        }
        let link = lock_or_recover(&self.link);
        let Some(stream) = link.as_ref() else {
            return Err(RuntimeError::Lifecycle {
                phase: self.owner.bootstrap_admission_phase(),
                source: std::io::Error::other(
                    "authenticated stream was not retained by its generation lease",
                ),
            });
        };
        clone_stream(stream)
            .map(Some)
            .map_err(|source| RuntimeError::Lifecycle {
                phase: self.owner.bootstrap_admission_phase(),
                source,
            })
    }
}

enum AdmissionResult {
    Bound(BootstrapStream),
    Cancelled,
    DeadlineElapsed,
    Failed(RuntimeError),
}

struct RoleBootstrapObserver {
    generation: RoleGeneration,
    attempt: u32,
    events_tx: Sender<RoleEvent>,
    seen_rejections: AtomicU8,
}

impl BootstrapRejectionObserver for RoleBootstrapObserver {
    fn rejected(&self, rejection: BootstrapRejection) {
        let bit = match rejection {
            BootstrapRejection::Io => 1 << 0,
            BootstrapRejection::Header => 1 << 1,
            BootstrapRejection::PayloadTooLarge => 1 << 2,
            BootstrapRejection::Protocol => 1 << 3,
            BootstrapRejection::Timeout => 1 << 4,
            BootstrapRejection::HelloAuth => 1 << 5,
        };
        if self.seen_rejections.fetch_or(bit, Ordering::AcqRel) & bit != 0 {
            return;
        }
        let _ = self.events_tx.send(RoleEvent::BootstrapRejected {
            generation: self.generation,
            attempt: self.attempt,
            code: rejection.code(),
        });
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn armed_recovery_gate_never_overrides_shutdown() {
        let decision = AtomicU8::new(RECOVERY_ARMED);
        let shutdown = AtomicBool::new(true);
        assert!(matches!(
            await_recovery_decision(&decision, &shutdown),
            Ok(false)
        ));
    }

    #[test]
    fn repeated_rejections_are_coalesced_to_one_event_per_class() {
        let (events_tx, events_rx) = mpsc::channel();
        let observer = RoleBootstrapObserver {
            generation: RoleGeneration(1),
            attempt: 1,
            events_tx,
            seen_rejections: AtomicU8::new(0),
        };

        observer.rejected(BootstrapRejection::HelloAuth);
        observer.rejected(BootstrapRejection::HelloAuth);
        observer.rejected(BootstrapRejection::Timeout);
        observer.rejected(BootstrapRejection::Timeout);

        assert!(matches!(
            events_rx.try_recv(),
            Ok(RoleEvent::BootstrapRejected {
                code: "KELD-IPC-007",
                ..
            })
        ));
        assert!(matches!(
            events_rx.try_recv(),
            Ok(RoleEvent::BootstrapRejected {
                code: "KELD-IPC-006",
                ..
            })
        ));
        assert!(matches!(events_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn freshness_selection_retains_endpoint_and_token_collisions_until_success() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct DropProbe(Arc<AtomicUsize>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let drops = Arc::new(AtomicUsize::new(0));
        let mut call = 0_u8;
        let retired_token = SessionToken::from_bytes([1_u8; keld_ipc::SESSION_TOKEN_LEN]);
        let selected = select_fresh_candidate(
            Some("retired"),
            Some(retired_token),
            || -> Result<BootstrapCandidate<DropProbe>, std::convert::Infallible> {
                assert_eq!(
                    drops.load(Ordering::SeqCst),
                    0,
                    "rejected endpoint owners must stay live while the allocator retries"
                );
                call += 1;
                let (endpoint, token) = match call {
                    1 => (
                        "retired",
                        SessionToken::from_bytes([2_u8; keld_ipc::SESSION_TOKEN_LEN]),
                    ),
                    2 => ("fresh", retired_token),
                    _ => (
                        "fresh",
                        SessionToken::from_bytes([3_u8; keld_ipc::SESSION_TOKEN_LEN]),
                    ),
                };
                Ok(BootstrapCandidate {
                    resource: DropProbe(Arc::clone(&drops)),
                    app_link: format!("{endpoint}#candidate"),
                    endpoint: endpoint.to_owned(),
                    token,
                })
            },
        )
        .expect("infallible candidate allocator")
        .expect("third candidate must satisfy endpoint and token freshness");

        assert_eq!(call, 3);
        assert_eq!(selected.endpoint, "fresh");
        assert_eq!(
            drops.load(Ordering::SeqCst),
            2,
            "rejected owners drop only after selection completes"
        );
        drop(selected);
        assert_eq!(drops.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn bound_stream_clone_failure_is_latched_and_not_masked_during_revoke() {
        let listener = BootstrapListener::bind().expect("bootstrap listener");
        let cancellation = listener.cancellation();
        let (_admission_tx, admission_rx) = mpsc::channel();
        let (events_tx, _events_rx) = mpsc::channel();
        let (bound_tx, _bound_rx) = mpsc::channel();
        let mut lease = RoleGenerationLease {
            owner: RoleOwner::Primary,
            generation: RoleGeneration(1),
            attempt: 1,
            admission_timeout: DEFAULT_ADMISSION_TIMEOUT,
            listener: Some(listener),
            cancellation,
            admission_tx: None,
            admission_rx,
            admission_thread: None,
            admission_done: false,
            link: Arc::new(Mutex::new(None)),
            events_tx,
            bound_tx: Some(bound_tx),
            #[cfg(test)]
            #[cfg(unix)]
            probe: None,
        };
        let (stream, peer) = connected_stream_pair();

        let error = lease
            .retain_and_clone_bound_stream(stream, |_| Err(std::io::Error::other("clone sentinel")))
            .expect_err("injected clone failure");
        assert!(error.to_string().contains("clone sentinel"), "{error}");
        assert!(lease.admission_done, "consumed admission must stay latched");
        assert!(
            lock_or_recover(&lease.link).is_some(),
            "the generation lease must retain the authenticated stream for revoke"
        );

        drop(peer);
        lease
            .revoke(RevocationCause::AdmissionFailed)
            .expect("revoke must not replace the clone failure with channel disconnect");
    }

    #[cfg(unix)]
    fn connected_stream_pair() -> (BootstrapStream, BootstrapStream) {
        std::os::unix::net::UnixStream::pair().expect("bound stream pair")
    }

    #[cfg(windows)]
    fn connected_stream_pair() -> (BootstrapStream, BootstrapStream) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("pair listener");
        let address = listener.local_addr().expect("pair address");
        let client = std::net::TcpStream::connect(address).expect("pair client");
        let (server, _) = listener.accept().expect("pair server");
        (server, client)
    }
}

#[cfg(test)]
#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct ProvisionedProbe {
    pub(crate) generation: RoleGeneration,
    pub(crate) app_link: String,
}

#[cfg(test)]
#[cfg(unix)]
#[path = "unix_role_fixture.rs"]
pub(crate) mod fixture;

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::fixture::{PrimaryFixture, assert_ready_line, connect_with_foreign_token};
    use std::process::Command;
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn real_bun_primary_restart_rotates_generation_and_rejects_stale_token() {
        let fixture = PrimaryFixture::new();
        let (probe_tx, probe_rx) = mpsc::channel();
        let supervisor = RoleSupervisor::start(
            RoleConfig::new("bun")
                .arg(PrimaryFixture::script_path())
                .arg(fixture.control_path())
                .current_dir(fixture.dir())
                .restart_policy(RestartPolicy {
                    max_crashes: 3,
                    window_secs: 30,
                })
                .admission_timeout(Duration::from_secs(5))
                .with_probe(probe_tx),
        )
        .expect("primary role must spawn under Bun");

        let g1_probe = probe_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("g1 app link");
        assert_next_event(
            &supervisor,
            |event| matches!(event, RoleEvent::Provisioned { generation, attempt: 1 } if *generation == g1_probe.generation),
            "g1 Provisioned",
        );
        assert_next_event(
            &supervisor,
            |event| matches!(event, RoleEvent::Spawned { generation, attempt: 1, .. } if *generation == g1_probe.generation),
            "g1 Spawned",
        );
        let mut g1_control = fixture.accept_control();
        assert_ready_line(&mut g1_control, &g1_probe.app_link);
        g1_control.write_line("BIND");
        let g1 = assert_next_event(
            &supervisor,
            |event| matches!(event, RoleEvent::LinkBound { generation, .. } if *generation == g1_probe.generation),
            "g1 LinkBound",
        );
        assert!(matches!(g1, RoleEvent::LinkBound { .. }));
        assert_eq!(g1_control.read_line(), "BOUND");
        g1_control.write_line("CRASH");

        assert_next_event(
            &supervisor,
            |event| matches!(event, RoleEvent::Revoked { generation, attempt: 1, cause: RoleRevocationCause::ChildExited } if *generation == g1_probe.generation),
            "g1 Revoked",
        );
        let g2_probe = probe_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("g2 app link");
        assert_ne!(
            g1_probe.generation, g2_probe.generation,
            "restart must mint a fresh host generation"
        );
        assert_ne!(
            g1_probe.app_link, g2_probe.app_link,
            "restart must mint a fresh endpoint/token"
        );
        assert_next_event(
            &supervisor,
            |event| matches!(event, RoleEvent::Provisioned { generation, attempt: 2 } if *generation == g2_probe.generation),
            "g2 Provisioned",
        );
        assert_next_event(
            &supervisor,
            |event| matches!(event, RoleEvent::Spawned { generation, attempt: 2, .. } if *generation == g2_probe.generation),
            "g2 Spawned",
        );

        let mut g2_control = fixture.accept_control();
        assert_ready_line(&mut g2_control, &g2_probe.app_link);
        connect_with_foreign_token(&g1_probe.app_link, &g2_probe.app_link);
        assert_next_event(
            &supervisor,
            |event| {
                matches!(
                    event,
                    RoleEvent::BootstrapRejected {
                        generation,
                        attempt: 2,
                        code: "KELD-IPC-007"
                    } if *generation == g2_probe.generation
                )
            },
            "g2 stale-token rejection",
        );

        g2_control.write_line("BIND");
        assert_next_event(
            &supervisor,
            |event| matches!(event, RoleEvent::LinkBound { generation, .. } if *generation == g2_probe.generation),
            "g2 LinkBound",
        );
        assert_eq!(g2_control.read_line(), "BOUND");
        supervisor.shutdown();
        assert_next_event(
            &supervisor,
            |event| matches!(event, RoleEvent::Revoked { generation, attempt: 2, cause: RoleRevocationCause::Shutdown } if *generation == g2_probe.generation),
            "g2 shutdown revoke",
        );
        match supervisor.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            other => panic!("shutdown should stop primary cleanly, got {other:?}"),
        }
    }

    fn assert_next_event(
        supervisor: &RoleSupervisor,
        predicate: impl Fn(&RoleEvent) -> bool,
        label: &str,
    ) -> RoleEvent {
        let event = supervisor
            .recv_event(Duration::from_secs(2))
            .unwrap_or_else(|| panic!("missing event: {label}"));
        if predicate(&event) {
            event
        } else {
            panic!("expected next event {label}, got {event:?}");
        }
    }

    #[test]
    fn bun_is_available_for_primary_fixture() {
        let output = Command::new("bun")
            .arg("--version")
            .output()
            .expect("spawn bun --version");
        assert!(output.status.success(), "bun --version must succeed");
    }
}
