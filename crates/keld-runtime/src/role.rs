//! Authenticated Bun role coordinator shared by KEL-75 T1b/T2/T8.
//!
//! One coordinator instance is one lifecycle owner (`primary` or `app-bound`).
//! It consumes the crate-private prepared-child lease in [`Supervisor`] and
//! does not implement a second restart loop: spawn, backoff, crash-loop
//! breaking, output capture, shutdown and reap stay in the generic supervisor.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use keld_ipc::{
    AppLinkDeadlines, BootstrapAdmission, BootstrapCancellation, BootstrapListener,
    BootstrapRejection, BootstrapRejectionObserver, BootstrapStream, SessionToken, parse_app_link,
};

#[cfg(target_os = "linux")]
use crate::PreparedCommand;
#[cfg(target_os = "linux")]
use crate::linux_strict::LinuxStrictProfile;
use crate::{
    CapturedOutput, ChildPreparer, CrashLedger, GenerationLease, PreparedChild, RestartPolicy,
    RevocationCause, RuntimeError, Supervisor, SupervisorOutcome,
};

pub(crate) const DEFAULT_ADMISSION_TIMEOUT: Duration = Duration::from_secs(10);
const FRESH_BOOTSTRAP_ATTEMPTS: usize = 8;
const RECOVERY_PENDING: u8 = 0;
const RECOVERY_ARMED: u8 = 1;
const RECOVERY_DENIED: u8 = 2;
const RECOVERY_POLL: Duration = Duration::from_millis(10);
const STAGED_LAUNCH_PHASE: &str = "staged Bun launch admission";

/// Bun prepends this variable's arguments to its own, so an inherited preload
/// would run before the qualified canonical capture. Every staged launch removes it.
pub const BUN_OPTIONS_ENV: &str = "BUN_OPTIONS";

fn staged_launch_refusal(detail: String) -> RuntimeError {
    RuntimeError::Lifecycle {
        phase: STAGED_LAUNCH_PHASE,
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, detail),
    }
}

/// Whether `metadata`, read without following a link, names a symbolic link or
/// any other reparse point. Junctions and non-link reparse tags count: a tag the
/// owner cannot name is not proof of a plain staged file.
fn is_link_or_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// The one staged-launch admission of every boot kind on every platform.
///
/// Refuses a stage root that carries `bunfig.toml` or `.bunfig.toml`, because Bun
/// runtime config can override the CLI define and preload, sidecar or direct entry.
/// Reads `src/kipc-transport.ts` without following a link: `Ok(None)` when it is
/// absent (a self-contained legacy entry), `Ok(Some(path))` for a regular file, and
/// a refusal for a symbolic link, junction, other reparse point or non-file, at
/// either path component, so no caller can follow it out of the stage.
///
/// # Errors
///
/// Returns [`RuntimeError::Lifecycle`] carrying the `KELD-CORE-037` refusal with its
/// restage guidance, for every refusal above and for an uninspectable path.
pub fn admit_staged_transport_sidecar(root: &Path) -> Result<Option<PathBuf>, RuntimeError> {
    for config in ["bunfig.toml", ".bunfig.toml"] {
        match std::fs::symlink_metadata(root.join(config)) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(staged_launch_refusal(format!(
                    "KELD-CORE-037: staged runtime config {config} can override canonical boot selection or preload. Restage through `keld dev`."
                )));
            }
        }
    }
    // Two components, as the boot compiler probes them: a joined `src/...` component
    // would keep its separator verbatim beneath a Windows extended-length root.
    let directory = root.join("src");
    match std::fs::symlink_metadata(&directory) {
        Ok(metadata) if !is_link_or_reparse_point(&metadata) && metadata.is_dir() => {}
        Ok(_) => {
            return Err(staged_launch_refusal(String::from(
                "KELD-CORE-037: staged src must be a real directory, not a symbolic link or reparse point. Restage through `keld dev`.",
            )));
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(staged_launch_refusal(format!(
                "KELD-CORE-037: staged src cannot be inspected: {source}. Restage through `keld dev`."
            )));
        }
    }
    let sidecar = directory.join("kipc-transport.ts");
    match std::fs::symlink_metadata(&sidecar) {
        Ok(metadata) if is_link_or_reparse_point(&metadata) => {
            Err(staged_launch_refusal(String::from(
                "KELD-CORE-037: staged kipc transport must be a regular file, not a symbolic link or reparse point. Restage through `keld dev`.",
            )))
        }
        Ok(metadata) if metadata.is_file() => Ok(Some(sidecar)),
        Ok(_) => Err(staged_launch_refusal(String::from(
            "KELD-CORE-037: staged kipc transport is not a regular file. Restage through `keld dev`.",
        ))),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(staged_launch_refusal(format!(
            "KELD-CORE-037: staged kipc transport cannot be inspected: {source}. Restage through `keld dev`."
        ))),
    }
}

/// The Bun arguments that fix one staged launch's boot mode before any app code.
///
/// The literal define carries `native_facade`, the validated boot kind, because the
/// transport's mode expression is that define and never the live environment. The
/// preload follows [`admit_staged_transport_sidecar`]. The caller also removes
/// [`BUN_OPTIONS_ENV`] and places these arguments before Bun's `run` subcommand.
///
/// # Errors
///
/// Returns the [`admit_staged_transport_sidecar`] refusal.
pub fn staged_bun_launch_args(
    root: &Path,
    native_facade: bool,
) -> Result<Vec<OsString>, RuntimeError> {
    let mut args = vec![OsString::from(if native_facade {
        "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\""
    } else {
        "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"0\""
    })];
    if let Some(sidecar) = admit_staged_transport_sidecar(root)? {
        let mut preload = OsString::from("--preload=");
        preload.push(strip_verbatim_prefix(&sidecar));
        args.push(preload);
    }
    Ok(args)
}

/// Strips a Windows verbatim prefix (`\\\\?\\C:\\...`, `\\\\?\\UNC\\server\\share`), which
/// `fs::canonicalize` yields. A path with no such prefix, or that is not UTF-8, is
/// returned unchanged. Bun 1.4.4 opens a verbatim path as a `run` entry but refuses it
/// as `--preload` (`Cannot find module`); the plain form also names the module the
/// entry's `./kipc-transport.ts` import resolves to, so the preload is that instance.
#[must_use]
pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let Some(s) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

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

    /// Rejects further successor provisioning for startup rollback or
    /// accepted session shutdown.
    ///
    /// Returns `true` when recovery is denied after this call.
    #[must_use]
    pub fn deny(&self) -> bool {
        self.decision.store(RECOVERY_DENIED, Ordering::Release);
        true
    }
}

impl Drop for RoleRecoveryGate {
    fn drop(&mut self) {
        let _ = self.decision.compare_exchange(
            RECOVERY_PENDING,
            RECOVERY_DENIED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
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

/// Host-minted principal for one authenticated role generation.
///
/// A generation is not a PID, socket path, token, or wire field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RolePrincipal {
    owner: RoleOwner,
    generation: RoleGeneration,
}

impl RolePrincipal {
    /// Creates a principal for one live role generation.
    #[must_use]
    pub const fn new(owner: RoleOwner, generation: RoleGeneration) -> Self {
        Self { owner, generation }
    }

    /// Lifecycle owner category.
    #[must_use]
    pub const fn owner(self) -> RoleOwner {
        self.owner
    }

    /// Host-minted generation for this coordinator instance.
    #[must_use]
    pub const fn generation(self) -> RoleGeneration {
        self.generation
    }
}

/// One authenticated role generation handed to its host-side stream owner.
///
/// The generation and attempt are host metadata. The stream has already
/// completed the possession-token `HELLO`; consumers must not run a second
/// handshake or derive identity from peer payload bytes.
pub struct BoundRoleGeneration {
    principal: RolePrincipal,
    attempt: u32,
    stream: BootstrapStream,
}

impl BoundRoleGeneration {
    /// Host-minted generation bound to this stream.
    #[must_use]
    pub const fn generation(&self) -> RoleGeneration {
        self.principal.generation()
    }

    /// Host-minted owner and generation from the canonical accepted-role lease.
    ///
    /// This trusted metadata is never decoded from the peer's frame, locator,
    /// PID or environment. It grants no privileged capability by itself.
    #[must_use]
    pub const fn principal(&self) -> RolePrincipal {
        self.principal
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
            .field("generation", &self.generation())
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
    /// The authenticated app link failed while the child remained live.
    #[cfg(windows)]
    LinkFailed,
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
            #[cfg(windows)]
            RevocationCause::LinkFailed => Self::LinkFailed,
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
    env: Vec<(OsString, OsString)>,
    env_remove: Vec<OsString>,
    current_dir: Option<PathBuf>,
    staged_bun_launch: Option<(PathBuf, bool)>,
    restart_policy: RestartPolicy,
    admission_timeout: Duration,
    #[cfg(target_os = "linux")]
    linux_strict: Option<LinuxStrictProfile>,
    // Debug-only host observer, never an environment key for a strict child.
    #[cfg(all(target_os = "linux", debug_assertions))]
    debug_primary_attempt_witness: Option<Arc<Mutex<std::fs::File>>>,
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
            env: Vec::new(),
            env_remove: Vec::new(),
            current_dir: None,
            staged_bun_launch: None,
            restart_policy: RestartPolicy::default(),
            admission_timeout: DEFAULT_ADMISSION_TIMEOUT,
            #[cfg(target_os = "linux")]
            linux_strict: None,
            #[cfg(all(target_os = "linux", debug_assertions))]
            debug_primary_attempt_witness: None,
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

    /// Sets one explicit environment key for every child generation.
    #[must_use]
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
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

    /// Applies the one staged launch rule to every generation of this role.
    ///
    /// Each generation re-runs [`staged_bun_launch_args`] over `root` before anything
    /// is provisioned, puts its result before the configured arguments and removes
    /// [`BUN_OPTIONS_ENV`], so a stage changed between generations is refused like a
    /// stage refused at first launch. `native_facade` is the validated boot kind.
    #[must_use]
    pub fn staged_bun_launch(mut self, root: impl Into<PathBuf>, native_facade: bool) -> Self {
        self.staged_bun_launch = Some((root.into(), native_facade));
        self
    }

    /// Runs every Linux generation through the validated strict profile.
    ///
    /// Strict roles use the sandbox's fixed `/app` working directory and an
    /// exact environment, so [`Self::current_dir`] must not also be set.
    #[must_use]
    #[cfg(target_os = "linux")]
    pub fn linux_strict(mut self, profile: LinuxStrictProfile) -> Self {
        self.linux_strict = Some(profile);
        self
    }

    /// Pins an existing private witness file once, before spawning any role.
    /// The supervisor writes through this descriptor for every prepare, so
    /// later pathname replacement cannot redirect its output. Debug Linux only.
    ///
    /// # Errors
    ///
    /// Rejects unsafe filesystem objects, symlinked paths, or open failures.
    #[cfg(all(target_os = "linux", debug_assertions))]
    pub fn with_debug_primary_attempt_witness(mut self, path: &Path) -> std::io::Result<Self> {
        let file = open_debug_attempt_witness(path)?;
        self.debug_primary_attempt_witness = Some(Arc::new(Mutex::new(file)));
        Ok(self)
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
    /// Dropping an undecided gate defaults to deny; dropping an already armed
    /// gate preserves that decision. Call [`RoleRecoveryGate::arm`] only after
    /// the host has made initial readiness externally observable.
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
    /// [`Self::start_with_bound_generations`] or
    /// [`Self::start_with_bound_generations_gated`].
    #[must_use]
    pub fn recv_bound_generation(&self, timeout: Duration) -> Option<BoundRoleGeneration> {
        self.bound_rx.as_ref()?.recv_timeout(timeout).ok()
    }

    /// Returns the next already-authenticated generation without waiting.
    ///
    /// Returns `None` when no generation is queued or this supervisor was
    /// started without [`Self::start_with_bound_generations`] or
    /// [`Self::start_with_bound_generations_gated`].
    #[must_use]
    pub fn try_recv_bound_generation(&self) -> Option<BoundRoleGeneration> {
        self.bound_rx.as_ref()?.try_recv().ok()
    }

    /// Stops the child, if still live.
    pub fn shutdown(&self) {
        self.supervisor.shutdown();
    }

    /// Stops crash-successor admission without killing the current direct
    /// generation before its accepted Quit reply is written.
    #[cfg(any(target_os = "linux", windows))]
    pub fn accept_shutdown(&self) {
        self.supervisor.accept_shutdown();
    }

    /// Revokes, kills/reaps, and restart-policies the named direct generation
    /// after its authenticated app link fails while the process is still live.
    #[cfg(any(target_os = "linux", windows))]
    pub fn restart_generation(&self, attempt: u32) {
        self.supervisor.restart_generation(attempt);
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

    /// Snapshot of every unrequested child termination retained by the sole
    /// generic supervisor.
    #[must_use]
    pub fn crash_ledger(&self) -> CrashLedger {
        self.supervisor.crash_ledger()
    }
}

struct RolePreparer {
    config: RoleConfig,
    generation_owner: RoleGenerationOwner,
    recovery_decision: Option<Arc<AtomicU8>>,
}

/// Opens the parent-created attempt witness for appending under the same
/// ownership rule as the strict sandbox's paths (`linux_strict`): a regular
/// file owned by the current user and not group/other-writable, beneath
/// trusted non-symlink ancestors. The open neither follows a final symlink,
/// blocks on a FIFO, nor acquires a controlling terminal.
#[cfg(all(target_os = "linux", debug_assertions))]
fn open_debug_attempt_witness(path: &Path) -> std::io::Result<std::fs::File> {
    use crate::linux_strict::{current_effective_uid, validate_trusted_ancestors};
    use nix::fcntl::OFlag;
    use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
    const PHASE: &str = "debug primary attempt witness";
    let uid = current_effective_uid().map_err(|error| std::io::Error::other(error.to_string()))?;
    validate_trusted_ancestors(path, uid, PHASE)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let file = std::fs::OpenOptions::new()
        .append(true)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_NOCTTY).bits())
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() || metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "debug primary attempt witness must be a regular file owned by the current user and not group/other-writable",
        ));
    }
    Ok(file)
}

impl ChildPreparer for RolePreparer {
    type Lease = RoleGenerationLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        #[cfg(all(target_os = "linux", debug_assertions))]
        if let Some(witness) = &self.config.debug_primary_attempt_witness {
            use std::io::Write as _;
            // Record before provisioning, including a short-lived or failed
            // successor. Retain the original verified descriptor across attempts.
            let mut file = witness.lock().map_err(|_| RuntimeError::Lifecycle {
                phase: "debug primary attempt witness",
                source: std::io::Error::other("witness file lock poisoned"),
            })?;
            writeln!(*file, "{attempt}").map_err(|source| RuntimeError::Lifecycle {
                phase: "debug primary attempt witness",
                source,
            })?;
        }
        let staged_launch = self
            .config
            .staged_bun_launch
            .as_ref()
            .map(|(root, native_facade)| staged_bun_launch_args(root, *native_facade))
            .transpose()?;
        let provisioned = self.generation_owner.provision(attempt)?;
        #[cfg(target_os = "linux")]
        if let Some(profile) = &self.config.linux_strict {
            return prepare_linux_strict(&self.config, profile, provisioned);
        }
        let mut command = Command::new(&self.config.program);
        if let Some(args) = staged_launch {
            command.args(args).env_remove(BUN_OPTIONS_ENV);
        }
        command.args(&self.config.args);
        if let Some(current_dir) = &self.config.current_dir {
            command.current_dir(current_dir);
        }
        for key in &self.config.env_remove {
            command.env_remove(key);
        }
        for (key, value) in &self.config.env {
            command.env(key, value);
        }
        command
            .env("KELD_APP_LINK", &provisioned.app_link)
            .stdin(Stdio::null());
        Ok(PreparedChild {
            command: command.into(),
            lease: provisioned.lease,
        })
    }

    fn allow_restart(
        &mut self,
        shutdown: &AtomicBool,
        accepted_shutdown: &AtomicBool,
    ) -> Result<bool, RuntimeError> {
        let Some(decision) = &self.recovery_decision else {
            return Ok(true);
        };
        await_recovery_decision(decision, shutdown, accepted_shutdown)
    }
}

#[cfg(target_os = "linux")]
fn prepare_linux_strict(
    config: &RoleConfig,
    profile: &LinuxStrictProfile,
    provisioned: ProvisionedRoleGeneration,
) -> Result<PreparedChild<RoleGenerationLease>, RuntimeError> {
    let ProvisionedRoleGeneration { app_link, lease } = provisioned;
    let prepared = (|| {
        if config.current_dir.is_some() {
            return Err(RuntimeError::Lifecycle {
                phase: "Linux strict role preparation",
                source: std::io::Error::other(
                    "strict roles use the fixed /app working directory; remove current_dir",
                ),
            });
        }
        let (endpoint, _) =
            parse_app_link(&app_link).map_err(|source| RuntimeError::Lifecycle {
                phase: "Linux strict app-link",
                source: std::io::Error::other(source),
            })?;
        let mut environment = config
            .env
            .iter()
            .filter(|(key, _)| !config.env_remove.contains(key))
            .cloned()
            .collect::<Vec<_>>();
        environment.push((OsString::from("KELD_APP_LINK"), OsString::from(&app_link)));
        profile
            .command_for_app_link(
                Path::new(&config.program),
                &config.args,
                &environment,
                Path::new(endpoint),
            )
            .map(PreparedCommand::LinuxStrict)
            .map_err(|source| RuntimeError::Lifecycle {
                phase: "Linux strict role preparation",
                source: std::io::Error::other(source),
            })
    })();
    match prepared {
        Ok(command) => Ok(PreparedChild { command, lease }),
        Err(error) => {
            lease.revoke(RevocationCause::SpawnFailed)?;
            Err(error)
        }
    }
}

fn await_recovery_decision(
    decision: &AtomicU8,
    shutdown: &AtomicBool,
    accepted_shutdown: &AtomicBool,
) -> Result<bool, RuntimeError> {
    loop {
        if shutdown.load(Ordering::Acquire) || accepted_shutdown.load(Ordering::Acquire) {
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
        if let Some(stream) = self
            .link
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
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
                            principal: RolePrincipal::new(self.owner, self.generation),
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
        *self.link.lock().unwrap_or_else(PoisonError::into_inner) = Some(stream);
        self.admission_done = true;
        if self.bound_tx.is_none() {
            return Ok(None);
        }
        let link = self.link.lock().unwrap_or_else(PoisonError::into_inner);
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
#[cfg(all(target_os = "linux", debug_assertions))]
mod debug_attempt_witness_tests {
    use super::open_debug_attempt_witness;
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::time::Duration;

    fn private_file(dir: &tempfile::TempDir, mode: u32) -> std::path::PathBuf {
        let path = dir.path().join("attempts.log");
        std::fs::write(&path, b"").expect("create witness");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
            .expect("set witness mode");
        path
    }

    #[test]
    fn private_regular_witness_appends() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let path = private_file(&dir, 0o600);
        writeln!(
            open_debug_attempt_witness(&path).expect("open witness"),
            "1"
        )
        .expect("record");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "1\n");
    }

    #[test]
    fn pinned_witness_stays_on_original_inode_after_same_user_replacement() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let path = private_file(&dir, 0o600);
        let config = super::RoleConfig::primary("/bin/sh")
            .with_debug_primary_attempt_witness(&path)
            .expect("pin stage-owned witness");
        let original = dir.path().join("original-inode");
        std::fs::rename(&path, &original).expect("rename original file");
        std::fs::write(&path, b"unrelated data").expect("replace with same-user file");

        let pinned = config
            .debug_primary_attempt_witness
            .as_ref()
            .expect("witness descriptor");
        writeln!(*pinned.lock().expect("pinned witness lock"), "1")
            .expect("write through original descriptor");
        assert_eq!(std::fs::read(&original).expect("read original"), b"1\n");
        assert_eq!(
            std::fs::read(&path).expect("read replacement"),
            b"unrelated data",
            "a same-user replacement must never receive supervisor output"
        );
    }

    #[test]
    fn symlinked_witness_is_refused_without_writing_its_target() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let target = private_file(&dir, 0o600);
        std::fs::write(&target, b"keep").expect("seed target");
        let link = dir.path().join("link.log");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");
        assert!(open_debug_attempt_witness(&link).is_err());
        assert_eq!(std::fs::read(&target).expect("read target"), b"keep");
    }

    #[test]
    fn witness_beneath_a_symlinked_directory_is_refused() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let real = dir.path().join("real");
        std::fs::create_dir(&real).expect("real dir");
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700))
            .expect("private dir");
        let file = real.join("attempts.log");
        std::fs::write(&file, b"").expect("create witness");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))
            .expect("private witness");
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).expect("symlink dir");
        assert!(open_debug_attempt_witness(&alias.join("attempts.log")).is_err());
    }

    #[test]
    fn fifo_witness_without_reader_is_refused_without_blocking() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let path = dir.path().join("attempts.log");
        nix::unistd::mkfifo(
            &path,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .expect("create fifo");
        // A blocking open would hang the thread, so the verdict is bounded here
        // and a regression fails instead of stalling the test binary. On such a
        // regression the spawned thread stays blocked until the process exits.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(open_debug_attempt_witness(&path).is_err());
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(true),
            "opening a FIFO witness must fail without blocking"
        );
    }

    #[test]
    fn group_writable_witness_is_refused() {
        let dir = tempfile::tempdir().expect("scratch dir");
        assert!(open_debug_attempt_witness(&private_file(&dir, 0o620)).is_err());
    }

    #[test]
    fn other_writable_witness_is_refused() {
        let dir = tempfile::tempdir().expect("scratch dir");
        assert!(open_debug_attempt_witness(&private_file(&dir, 0o602)).is_err());
    }

    #[test]
    fn device_and_directory_witnesses_are_refused() {
        assert!(open_debug_attempt_witness(std::path::Path::new("/dev/null")).is_err());
        let dir = tempfile::tempdir().expect("scratch dir");
        assert!(open_debug_attempt_witness(dir.path()).is_err());
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)] // unit-test fixture failures are assertion oracles
mod staged_launch_tests {
    use super::{admit_staged_transport_sidecar, staged_bun_launch_args};
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn staged_root(temp: &tempfile::TempDir) -> PathBuf {
        let root = temp.path().join("stage");
        fs::create_dir_all(root.join("src")).expect("stage src");
        root
    }

    fn refusal(root: &Path) -> String {
        let error = admit_staged_transport_sidecar(root).expect_err("admission must refuse");
        let message = error.to_string();
        assert!(message.contains("KELD-CORE-037"), "{message}");
        message
    }

    /// `New-Item -ItemType Junction`: a reparse point needs no privilege, unlike a
    /// Windows symbolic link, so this row runs on every Windows session.
    #[cfg(windows)]
    fn junction(link: &Path, target: &Path) {
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:KELD_TEST_LINK -Target $env:KELD_TEST_TARGET | Out-Null",
            ])
            .env("KELD_TEST_LINK", link)
            .env("KELD_TEST_TARGET", target)
            .output()
            .expect("junction fixture command");
        assert!(
            output.status.success(),
            "junction setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn staged_sidecar_admission_skips_absent_and_accepts_regular_file() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let root = staged_root(&temp);
        assert_eq!(
            admit_staged_transport_sidecar(&root).expect("absent sidecar"),
            None,
            "NotFound must allow self-contained entries"
        );
        fs::remove_dir(root.join("src")).expect("no src directory");
        assert_eq!(
            admit_staged_transport_sidecar(&root).expect("absent src"),
            None,
            "a stage whose entry sits at its root has no sidecar"
        );
        fs::create_dir(root.join("src")).expect("stage src");
        fs::write(root.join("src").join("kipc-transport.ts"), "export {}\n")
            .expect("regular sidecar");
        assert_eq!(
            admit_staged_transport_sidecar(&root).expect("regular sidecar"),
            Some(root.join("src").join("kipc-transport.ts")),
            "regular file must be eligible to preload"
        );
    }

    #[test]
    fn staged_sidecar_admission_rejects_directory() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let root = staged_root(&temp);
        fs::create_dir(root.join("src").join("kipc-transport.ts")).expect("directory sidecar");
        assert!(refusal(&root).contains("not a regular file"));
    }

    #[test]
    fn staged_sidecar_admission_rejects_a_src_that_is_not_a_directory() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let root = staged_root(&temp);
        fs::remove_dir(root.join("src")).expect("no src directory");
        fs::write(root.join("src"), "not a directory\n").expect("src file");
        assert!(refusal(&root).contains("staged src must be a real directory"));
    }

    #[test]
    fn staged_runtime_config_is_refused_with_and_without_a_sidecar() {
        for config in ["bunfig.toml", ".bunfig.toml"] {
            for sidecar in [false, true] {
                let temp = tempfile::tempdir().expect("config root");
                let root = staged_root(&temp);
                if sidecar {
                    fs::write(root.join("src").join("kipc-transport.ts"), "export {}\n")
                        .expect("regular sidecar");
                }
                fs::write(root.join(config), "preload = []\n").expect("runtime config");
                assert!(
                    refusal(&root).contains(&format!("staged runtime config {config}")),
                    "{config} sidecar={sidecar}"
                );
            }
        }
    }

    #[test]
    fn staged_launch_args_carry_the_boot_mode_and_only_an_admitted_preload() {
        let temp = tempfile::tempdir().expect("launch root");
        let root = staged_root(&temp);
        assert_eq!(
            staged_bun_launch_args(&root, false).expect("direct renderer entry"),
            [OsString::from(
                "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"0\""
            )]
        );
        fs::write(root.join("src").join("kipc-transport.ts"), "export {}\n")
            .expect("regular sidecar");
        let mut preload = OsString::from("--preload=");
        preload.push(root.join("src").join("kipc-transport.ts"));
        assert_eq!(
            staged_bun_launch_args(&root, true).expect("sidecar facade entry"),
            [
                OsString::from("--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\""),
                preload
            ]
        );
        fs::write(root.join("bunfig.toml"), "preload = []\n").expect("runtime config");
        assert!(
            staged_bun_launch_args(&root, true).is_err(),
            "no launch argument survives a refused stage"
        );
    }

    /// A canonical Windows root is verbatim; its preload argument never is.
    #[test]
    #[cfg(windows)]
    fn staged_launch_preload_loses_the_canonical_verbatim_prefix() {
        let temp = tempfile::tempdir().expect("launch root");
        let root = staged_root(&temp);
        fs::write(root.join("src").join("kipc-transport.ts"), "export {}\n")
            .expect("regular sidecar");
        let canonical = root.canonicalize().expect("canonical stage root");
        assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
        let args = staged_bun_launch_args(&canonical, true).expect("verbatim root");
        let preload = args[1].to_string_lossy();
        assert!(preload.starts_with("--preload="), "{preload}");
        assert!(!preload.contains(r"\\?\"), "{preload}");
    }

    #[test]
    #[cfg(unix)]
    fn staged_sidecar_admission_rejects_symlink_to_regular_file() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let root = staged_root(&temp);
        let outside = temp.path().join("outside.ts");
        fs::write(&outside, "export const steal = 1;\n").expect("external target");
        let sidecar = root.join("src").join("kipc-transport.ts");
        std::os::unix::fs::symlink(&outside, &sidecar).expect("sidecar symlink");
        // `metadata()` follows and would admit this as a regular file: CWE-59.
        assert!(
            fs::metadata(&sidecar).expect("follow").is_file(),
            "negative control: following the symlink sees a regular file"
        );
        assert!(refusal(&root).contains("symbolic link"));
    }

    #[test]
    #[cfg(unix)]
    fn staged_sidecar_admission_rejects_a_src_symlink() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let root = staged_root(&temp);
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).expect("external directory");
        fs::write(outside.join("kipc-transport.ts"), "export {}\n").expect("external file");
        fs::remove_dir(root.join("src")).expect("no src directory");
        std::os::unix::fs::symlink(&outside, root.join("src")).expect("src symlink");
        assert!(refusal(&root).contains("symbolic link"));
    }

    #[test]
    #[cfg(windows)]
    fn staged_sidecar_admission_rejects_junction() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let root = staged_root(&temp);
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).expect("external directory");
        junction(&root.join("src").join("kipc-transport.ts"), &outside);
        assert!(refusal(&root).contains("reparse point"));
    }

    #[test]
    #[cfg(windows)]
    fn staged_sidecar_admission_rejects_a_src_junction() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let root = staged_root(&temp);
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).expect("external directory");
        fs::write(outside.join("kipc-transport.ts"), "export {}\n").expect("external file");
        fs::remove_dir(root.join("src")).expect("no src directory");
        junction(&root.join("src"), &outside);
        assert!(
            fs::metadata(root.join("src").join("kipc-transport.ts"))
                .expect("follow")
                .is_file(),
            "negative control: following the junction sees a regular file"
        );
        assert!(refusal(&root).contains("reparse point"));
    }

    #[test]
    fn staged_sidecar_admission_uses_lstat_not_follow() {
        let source = include_str!("role.rs");
        let start = source
            .find("pub fn admit_staged_transport_sidecar")
            .expect("admission helper");
        let end = source
            .find("/// The Bun arguments that fix one staged launch")
            .expect("next item");
        let body = source.get(start..end).expect("admission helper body");
        assert!(
            body.contains("symlink_metadata"),
            "admission must lstat so a sidecar symlink is not followed into a preload or mount"
        );
        assert!(
            !body.contains(".metadata()") && !body.contains("fs::metadata"),
            "Path::metadata follows symlinks and would reintroduce CWE-59"
        );
        assert!(
            body.contains("is_link_or_reparse_point"),
            "admission must reject a link before any launch uses the path"
        );
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
            await_recovery_decision(&decision, &shutdown, &AtomicBool::new(false)),
            Ok(false)
        ));
    }

    #[test]
    fn accepted_shutdown_interrupts_a_pending_recovery_gate() {
        let decision = AtomicU8::new(RECOVERY_PENDING);
        assert!(matches!(
            await_recovery_decision(&decision, &AtomicBool::new(false), &AtomicBool::new(true),),
            Ok(false)
        ));
    }

    #[test]
    fn dropping_an_armed_recovery_gate_preserves_the_decision() {
        let decision = Arc::new(AtomicU8::new(RECOVERY_PENDING));
        let gate = RoleRecoveryGate {
            decision: Arc::clone(&decision),
        };
        assert!(gate.arm());
        drop(gate);
        assert_eq!(decision.load(Ordering::Acquire), RECOVERY_ARMED);
    }

    #[test]
    fn accepted_shutdown_disarms_an_armed_recovery_gate() {
        let gate = RoleRecoveryGate {
            decision: Arc::new(AtomicU8::new(RECOVERY_PENDING)),
        };
        assert!(gate.arm());
        assert!(gate.deny());
        assert!(matches!(
            await_recovery_decision(
                &gate.decision,
                &AtomicBool::new(false),
                &AtomicBool::new(false),
            ),
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
            lease
                .link
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some(),
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
        let listener = Arc::new(BootstrapListener::bind().expect("pair listener"));
        let app_link = listener.app_link();
        let (endpoint, token) = parse_app_link(&app_link).expect("pair app link");
        let endpoint = endpoint.to_owned();
        let acceptor = Arc::clone(&listener);
        let worker = std::thread::spawn(move || acceptor.accept_authenticated());
        let mut client = BootstrapStream::connect(&endpoint).expect("pair client");
        client
            .set_app_link_deadlines(Some(keld_ipc::APP_LINK_IO_DEADLINE))
            .expect("pair deadlines");
        keld_ipc::link::handshake_client(&mut client, &token).expect("pair HELLO");
        let server = worker
            .join()
            .expect("pair accept join")
            .expect("pair accept")
            .expect("authenticated pair");
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
    fn authenticated_bound_roles_preserve_owner_and_rotate_principal() {
        for owner in [RoleOwner::Primary, RoleOwner::AppBound] {
            let fixture = PrimaryFixture::new();
            let (probe_tx, probe_rx) = mpsc::channel();
            let config = match owner {
                RoleOwner::Primary => RoleConfig::primary("bun"),
                RoleOwner::AppBound => RoleConfig::app_bound("bun"),
            };
            let supervisor = RoleSupervisor::start_with_bound_generations(
                config
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
            .expect("one actual role coordinator");
            let g1 = probe_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("actual g1 mint");
            let mut control = fixture.accept_control();
            assert_ready_line(&mut control, &g1.app_link);
            control.write_line("BIND");
            let bound1 = supervisor
                .recv_bound_generation(Duration::from_secs(5))
                .expect("actual authenticated g1");
            assert_eq!(control.read_line(), "BOUND");
            assert_eq!(bound1.principal().owner(), owner);
            assert_eq!(bound1.principal().generation(), g1.generation);
            assert_eq!(bound1.generation(), bound1.principal().generation());
            let p1 = bound1.principal();
            control.write_line("CRASH");
            let g2 = probe_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("actual fresh g2 mint");
            let mut control = fixture.accept_control();
            assert_ready_line(&mut control, &g2.app_link);
            control.write_line("BIND");
            let bound2 = supervisor
                .recv_bound_generation(Duration::from_secs(5))
                .expect("actual authenticated g2");
            assert_eq!(control.read_line(), "BOUND");
            assert_eq!(bound2.principal().owner(), owner);
            assert_eq!(bound2.principal().generation(), g2.generation);
            assert_ne!(p1, bound2.principal());
            supervisor.shutdown();
            assert!(matches!(
                supervisor.wait_for_outcome(),
                SupervisorOutcome::Stopped
            ));
        }
    }

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
