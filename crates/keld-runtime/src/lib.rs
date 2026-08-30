//! keld-runtime — the app-process supervisor.
//!
//! Spawns the developer's JS/TS main under the caller-provided Bun, supervises
//! it (exponential backoff, crash-loop breaker), and hands it the kipc link at
//! spawn. KEL-96/T3 and T4 exercise macOS/Windows native-window survival across
//! a primary generation restart, while KEL-75/T4 still owns document-nonce plus
//! post-restart renderer-beacon continuity on every claimed backend. Normative
//! spec: `docs/architecture/06-runtime-and-tooling.md` §1.
//!
//! v0 scope (KEL-70): spawn + capture + restart-on-crash + crash-loop
//! breaker. Out of scope here: OS sandbox on the child, Bun
//! pinning/download, `--inspect` passthrough, Bun watch hot-restart, and the
//! destination `KELD_LINK`/`KELD_SHM`/`KELD_CONTRACT` env contract (v0 keeps
//! whatever env the caller's command factory sets, e.g. `KELD_APP_LINK`).

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
pub mod macos_guardian;
#[cfg(any(unix, windows))]
pub mod primary;
#[cfg(unix)]
pub mod registry;
#[cfg(any(unix, windows))]
mod role;
#[cfg(unix)]
mod virtual_port;
#[cfg(windows)]
pub mod windows_job;
#[cfg(windows)]
pub mod windows_lpac;

/// How often the supervisor thread polls a running child for exit, and the
/// granularity at which `shutdown()` is observed.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Base delay before the first restart after a crash; doubles per
/// consecutive crash inside the policy window, uncapped beyond what
/// `RestartPolicy::max_crashes` already bounds.
const BACKOFF_BASE: Duration = Duration::from_millis(20);

/// Restart policy defaults; tuned via `keld.config.ts` `runtime.supervision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPolicy {
    /// Maximum crashes tolerated inside `window_secs` before giving up.
    pub max_crashes: u8,
    /// Sliding window for the crash-loop breaker, in seconds.
    pub window_secs: u16,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_crashes: 3,
            window_secs: 30,
        }
    }
}

/// Delay before the `attempt`-th consecutive-crash restart (1-indexed:
/// `attempt=1` is the delay before retrying after the first crash).
/// Exposed so callers/tests can assert the policy without timing real
/// sleeps: `backoff_delay(1) < backoff_delay(2) < backoff_delay(3)`.
#[must_use]
pub fn backoff_delay(attempt: u8) -> Duration {
    let shift = attempt.saturating_sub(1).min(16);
    BACKOFF_BASE * (1u32 << shift)
}

/// Typed supervisor errors. `KELD-RUNTIME-*` codes.
#[derive(Debug)]
pub enum RuntimeError {
    /// The very first spawn attempt failed (e.g. `bun` is not on PATH).
    Spawn(std::io::Error),
    /// A prepared child lease could not be provisioned or revoked safely.
    Lifecycle {
        /// Stable lifecycle phase, suitable for diagnostics but never a secret.
        phase: &'static str,
        /// Underlying host I/O failure.
        source: std::io::Error,
    },
    /// The child crashed `crashes` times inside `window_secs`; the breaker
    /// stopped restarting it.
    CrashLoop {
        /// Number of crashes observed inside the window (== policy's `max_crashes`).
        crashes: u8,
        /// The policy's sliding window, in seconds.
        window_secs: u16,
        /// Exit code of the last crash, when the OS reported one.
        last_exit_code: Option<i32>,
        /// Last captured stderr, truncated to a bounded tail.
        stderr_tail: String,
    },
    /// A supervised generation terminated itself without the crash-loop
    /// breaker tripping. Recorded independently of exit status so a host that
    /// never drains events still observes a dead app process (KEL-105/KEL-116).
    ChildCrashed {
        /// OS process id of the generation that self-terminated.
        pid: u32,
        /// Exit code, when the OS reported one (`None` for a signal death).
        exit_code: Option<i32>,
        /// Last captured stderr, truncated to a bounded tail.
        stderr_tail: String,
    },
    /// The private macOS guardian exited while its host still owned the
    /// liveness writer, so the host fail-safe terminated the registered group.
    GuardianExited {
        /// Registered Bun process-group leader.
        group_pid: u32,
        /// Guardian exit code, when macOS reported one.
        exit_code: Option<i32>,
        /// Cleanup error when the fail-safe group signal itself failed.
        cleanup_error: Option<std::io::Error>,
    },
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(
                f,
                "KELD-RUNTIME-001: failed to spawn the supervised child — {e}. \
                 Check that `bun` is on PATH and re-run `keld doctor`."
            ),
            Self::Lifecycle { phase, source } => write!(
                f,
                "KELD-RUNTIME-003: prepared child lifecycle failed during {phase} — {source}. \
                 Check the role bootstrap endpoint and retry `keld dev`."
            ),
            Self::CrashLoop {
                crashes,
                window_secs,
                last_exit_code,
                stderr_tail,
            } => {
                write!(
                    f,
                    "KELD-RUNTIME-002: child crashed {crashes} times within {window_secs}s \
                     (crash-loop breaker tripped); last exit code {last_exit_code:?}. \
                     Fix the crash, then re-run `keld dev`."
                )?;
                if !stderr_tail.is_empty() {
                    write!(f, " stderr tail:\n{stderr_tail}")?;
                }
                Ok(())
            }
            Self::ChildCrashed {
                pid,
                exit_code,
                stderr_tail,
            } => {
                match exit_code {
                    Some(0) => write!(
                        f,
                        "KELD-RUNTIME-012: the supervised app process (pid {pid}) exited 0 \
                         without a host shutdown request; the crash-loop breaker did not apply. \
                         Keep the app process alive while its host-owned session is active."
                    ),
                    Some(code) => write!(
                        f,
                        "KELD-RUNTIME-012: the supervised app process (pid {pid}) exited \
                         {code}; the crash-loop breaker did not trip."
                    ),
                    None => write!(
                        f,
                        "KELD-RUNTIME-012: the supervised app process (pid {pid}) was \
                         terminated by a signal; the crash-loop breaker did not trip."
                    ),
                }?;
                if *exit_code != Some(0) {
                    write!(
                        f,
                        " Fix the crash shown in the captured stderr, then re-run `keld dev`."
                    )?;
                }
                if !stderr_tail.is_empty() {
                    write!(f, " stderr tail:\n{stderr_tail}")?;
                }
                Ok(())
            }
            Self::GuardianExited {
                group_pid,
                exit_code,
                cleanup_error,
            } => {
                write!(
                    f,
                    "KELD-RUNTIME-013: the macOS host-death guardian for process group \
                     {group_pid} exited unexpectedly with code {exit_code:?}; the host \
                     treated the session as fatal and invoked the registered-group \
                     fail-safe. Restart the host session."
                )?;
                if let Some(error) = cleanup_error {
                    write!(f, " fail-safe error: {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

impl Clone for RuntimeError {
    fn clone(&self) -> Self {
        match self {
            Self::Spawn(source) => {
                Self::Spawn(std::io::Error::new(source.kind(), source.to_string()))
            }
            Self::Lifecycle { phase, source } => Self::Lifecycle {
                phase,
                source: std::io::Error::new(source.kind(), source.to_string()),
            },
            Self::CrashLoop {
                crashes,
                window_secs,
                last_exit_code,
                stderr_tail,
            } => Self::CrashLoop {
                crashes: *crashes,
                window_secs: *window_secs,
                last_exit_code: *last_exit_code,
                stderr_tail: stderr_tail.clone(),
            },
            Self::ChildCrashed {
                pid,
                exit_code,
                stderr_tail,
            } => Self::ChildCrashed {
                pid: *pid,
                exit_code: *exit_code,
                stderr_tail: stderr_tail.clone(),
            },
            Self::GuardianExited {
                group_pid,
                exit_code,
                cleanup_error,
            } => Self::GuardianExited {
                group_pid: *group_pid,
                exit_code: *exit_code,
                cleanup_error: cleanup_error
                    .as_ref()
                    .map(|source| std::io::Error::new(source.kind(), source.to_string())),
            },
        }
    }
}

/// Stdout/stderr captured from the currently or most-recently supervised child.
#[derive(Debug, Default, Clone)]
pub struct CapturedOutput {
    /// Combined stdout across every spawn attempt so far.
    pub stdout: String,
    /// Combined stderr across every spawn attempt so far.
    pub stderr: String,
}

impl CapturedOutput {
    fn tail(s: &str, max_chars: usize) -> String {
        let char_count = s.chars().count();
        let skip = char_count.saturating_sub(max_chars);
        s.chars().skip(skip).collect()
    }

    pub(crate) fn stderr_tail(&self, max_chars: usize) -> String {
        Self::tail(&self.stderr, max_chars)
    }
}

/// One observable step in a supervised child's lifecycle. Tests await these
/// via [`Supervisor::recv_event`] instead of sleep-polling process state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorEvent {
    /// A child was spawned and is running. `attempt` is 1 for the first
    /// spawn, incrementing once per crash-triggered restart.
    Started {
        /// OS process id of the new child.
        pid: u32,
        /// 1-indexed spawn attempt count for this supervisor.
        attempt: u32,
    },
    /// The running child exited. `code` is `None` when the OS reports no
    /// exit code (e.g. killed by signal on Unix).
    Exited {
        /// OS process id of the child that exited.
        pid: u32,
        /// Process exit code, when available.
        code: Option<i32>,
    },
    /// A respawn attempt after a crash failed at the OS `spawn()` call.
    RespawnFailed,
    /// A prepared child lifecycle phase failed; inspect the terminal outcome.
    Failed {
        /// Spawn attempt that failed.
        attempt: u32,
    },
    /// The crash-loop breaker tripped; supervision has stopped permanently.
    CrashLoopTripped,
    /// The child exited zero (or `shutdown()` was called); supervision has
    /// stopped and will not restart.
    Stopped,
}

/// One unrequested child termination, independent of exit status.
///
/// Fixed-size and allocation-free so the supervisor can retain ordering for
/// status-zero exits without adding a hot-path queue. Non-zero diagnostics,
/// including their stderr tail, remain owned by [`CrashLedger::last`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelfTerminationRecord {
    /// OS process id of the generation that self-terminated.
    pub pid: u32,
    /// Exit code when the OS reported one (`None` for signal termination).
    pub exit_code: Option<i32>,
    /// Length of captured stdout when this termination was recorded.
    pub stdout_len: usize,
}

/// Crashes and all unrequested child terminations a [`Supervisor`] has
/// observed so far, as durable state rather than consumed events.
///
/// A host that blocks — `keld dev` sits in the window event loop and drains no
/// events for the whole window phase — can snapshot this before and after that
/// interval and learn whether the app process died inside it. Comparing two
/// snapshots is what separates a crash the supervisor *recovered* from
/// (KEL-70 AC1/AC3: still a success) from any self-termination after the
/// session was already live (KEL-105/KEL-116: not a window-path success).
#[derive(Debug, Clone, Default)]
pub struct CrashLedger {
    /// Crash-class terminations (non-zero exit statuses or signals) observed
    /// across every generation, never reset. Restarts do not decrement it and
    /// the crash-loop window does not evict from it: this preserves the
    /// KEL-105 crash view while the breaker's own sliding window decides
    /// restart policy.
    pub count: u32,
    /// Diagnostic for the most recent crash-class termination, carrying its
    /// `KELD-RUNTIME-012` code and a bounded stderr tail. `None` exactly when
    /// `count` is 0.
    pub last: Option<RuntimeError>,
    /// Length of captured stdout at the moment of the most recent crash-class
    /// termination, so a host can order that crash against something the app
    /// printed.
    ///
    /// The supervisor publishes stdout and its `Exited` event *before* it
    /// records the ledger fact, so a host that only compares crash counts
    /// cannot tell "crashed, then printed" from "printed, then crashed".
    /// Comparing a marker's offset against this length answers that question
    /// without any timing assumption (KEL-105).
    pub stdout_len_at_last_crash: usize,
    /// All unrequested self-terminations across every generation, including
    /// status zero. Never reset or evicted by restart policy.
    pub self_termination_count: u32,
    /// Most recent unrequested self-termination and its stdout ordering point.
    /// `None` exactly when `self_termination_count` is 0.
    pub last_self_termination: Option<SelfTerminationRecord>,
}

/// Terminal outcome of [`Supervisor::wait_for_outcome`].
#[derive(Debug)]
pub enum SupervisorOutcome {
    /// The child exited zero or the host requested shutdown; supervision ended
    /// without a restart-policy error. A generation may still have terminated
    /// itself — read [`Supervisor::crash_ledger`] for that fact
    /// (KEL-105/KEL-116).
    Stopped,
    /// The crash-loop breaker tripped.
    CrashLoop(RuntimeError),
    /// Provisioning, spawning, or revoking a prepared child failed.
    Failed(RuntimeError),
}

/// Why a prepared generation lost authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RevocationCause {
    /// The child was observed to have exited naturally.
    ChildExited,
    /// The host requested shutdown before killing/reaping the child.
    Shutdown,
    /// The authenticated app link failed while the child remained live.
    #[cfg(windows)]
    LinkFailed,
    /// The child could not be safely observed through stdout/stderr capture.
    CaptureFailed,
    /// The prepared generation failed before a live authenticated link.
    AdmissionFailed,
    /// The operating system refused to spawn the prepared command.
    SpawnFailed,
    /// The host could not observe the live child state.
    WaitFailed,
}

/// Host-owned authority that is invalidated with one prepared child attempt.
pub(crate) trait GenerationLease: Send + 'static {
    /// Records that the prepared child became observable to the supervisor.
    fn child_spawned(&mut self, _pid: u32, _attempt: u32) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Checks nonblocking lease-side state while the child is still running.
    fn poll(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Revokes every capability owned by this attempt before returning.
    fn revoke(self, cause: RevocationCause) -> Result<(), RuntimeError>;
}

/// Fresh command and host-owned authority for one supervisor attempt.
pub(crate) struct PreparedChild<L> {
    /// Command to spawn with captured stdout/stderr.
    pub(crate) command: Command,
    /// Authority lease revoked on every terminal attempt path.
    pub(crate) lease: L,
}

/// Creates a fresh prepared child before each OS spawn attempt.
pub(crate) trait ChildPreparer: Send + 'static {
    /// Lease kept beside the spawned `Child`.
    type Lease: GenerationLease;

    /// Prepares exactly one fresh child attempt.
    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError>;

    /// Waits at the post-revocation boundary before a crash successor may be
    /// provisioned. Ordinary supervisors permit restart immediately; a host
    /// session may require an externally observable readiness decision first.
    fn allow_restart(&mut self, _shutdown: &AtomicBool) -> Result<bool, RuntimeError> {
        Ok(true)
    }
}

struct NoLease;

impl GenerationLease for NoLease {
    fn revoke(self, _cause: RevocationCause) -> Result<(), RuntimeError> {
        Ok(())
    }
}

struct CommandPreparer<F> {
    factory: F,
}

impl<F> ChildPreparer for CommandPreparer<F>
where
    F: FnMut() -> Command + Send + 'static,
{
    type Lease = NoLease;

    fn prepare(&mut self, _attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        Ok(PreparedChild {
            command: (self.factory)(),
            lease: NoLease,
        })
    }
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(target_os = "macos")]
fn shutdown_was_accepted(state: &AtomicBool) -> bool {
    state.load(Ordering::Acquire)
}

#[cfg(not(target_os = "macos"))]
const fn shutdown_was_accepted() -> bool {
    false
}

/// Supervises one Bun (or other) child process: spawns it, captures its
/// stdout/stderr, and restarts it on crash (non-zero exit) up to a
/// [`RestartPolicy`] before giving up with a typed [`RuntimeError::CrashLoop`].
/// A zero exit is treated as graceful by restart policy — the child is not
/// restarted — while [`CrashLedger`] still records that it ended itself.
///
/// The child runs on a dedicated background thread; killing or restarting it
/// never touches the caller's thread or any window the caller owns (KEL-70
/// AC5: the caller — `keld-core`/`keld-cli` — is the window owner, not this
/// crate or the child).
#[derive(Debug)]
pub struct Supervisor {
    events_rx: Receiver<SupervisorEvent>,
    output: Arc<Mutex<CapturedOutput>>,
    current_pid: Arc<Mutex<Option<u32>>>,
    crash_loop_error: Arc<Mutex<Option<RuntimeError>>>,
    crashes: Arc<Mutex<CrashLedger>>,
    terminal_error: Arc<Mutex<Option<RuntimeError>>>,
    #[cfg(target_os = "macos")]
    accepted_shutdown: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    #[cfg(windows)]
    restart_attempt: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
}

impl Supervisor {
    /// Spawns `command_factory()` under supervision with `policy`. Blocks
    /// only long enough to perform the first `Command::spawn`; after that,
    /// the child's lifecycle (wait, capture, restart) runs on a dedicated
    /// background thread.
    ///
    /// `command_factory` is called once per spawn attempt (including
    /// restarts) and must return a fresh, unspawned [`Command`] each time —
    /// `Command` cannot be reused after `spawn()`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::Spawn`] if the very first spawn attempt fails.
    pub fn start<F>(policy: RestartPolicy, mut command_factory: F) -> Result<Self, RuntimeError>
    where
        F: FnMut() -> Command + Send + 'static,
    {
        Self::start_prepared(
            policy,
            CommandPreparer {
                factory: move || command_factory(),
            },
        )
    }

    /// Starts supervision from a fresh prepared-child factory.
    ///
    /// This is crate-private because role generation is a host runtime concern.
    /// It gives KEL-75 one synchronous authority handoff point without
    /// introducing a second restart/reap loop.
    pub(crate) fn start_prepared<P>(
        policy: RestartPolicy,
        preparer: P,
    ) -> Result<Self, RuntimeError>
    where
        P: ChildPreparer,
    {
        let (events_tx, events_rx) = mpsc::channel();
        let (preparer_tx, preparer_rx) = mpsc::sync_channel::<P>(0);
        let (startup_tx, startup_rx) = mpsc::sync_channel::<Result<(), RuntimeError>>(0);
        let output = Arc::new(Mutex::new(CapturedOutput::default()));
        let current_pid = Arc::new(Mutex::new(None));
        let crash_loop_error = Arc::new(Mutex::new(None));
        let crashes = Arc::new(Mutex::new(CrashLedger::default()));
        let terminal_error = Arc::new(Mutex::new(None));
        #[cfg(target_os = "macos")]
        let accepted_shutdown = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));
        let restart_attempt = Arc::new(AtomicU32::new(0));

        let thread = {
            let output = Arc::clone(&output);
            let current_pid = Arc::clone(&current_pid);
            let crash_loop_error = Arc::clone(&crash_loop_error);
            let crashes = Arc::clone(&crashes);
            let terminal_error = Arc::clone(&terminal_error);
            #[cfg(target_os = "macos")]
            let accepted_shutdown = Arc::clone(&accepted_shutdown);
            let shutdown = Arc::clone(&shutdown);
            let restart_attempt = Arc::clone(&restart_attempt);
            thread::Builder::new()
                .name("keld-runtime-supervisor".to_owned())
                .spawn(move || {
                    let Ok(mut preparer) = preparer_rx.recv() else {
                        return;
                    };
                    let (child, lease) = match preparer.prepare(1).and_then(spawn_prepared) {
                        Ok(initial) => initial,
                        Err(error) => {
                            let _ = startup_tx.send(Err(error));
                            return;
                        }
                    };
                    if startup_tx.send(Ok(())).is_err() {
                        let _ = lease.revoke(RevocationCause::Shutdown);
                        let mut child = child;
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                    supervise(
                        policy,
                        preparer,
                        child,
                        lease,
                        &events_tx,
                        &output,
                        &current_pid,
                        &crash_loop_error,
                        &crashes,
                        &terminal_error,
                        #[cfg(target_os = "macos")]
                        &accepted_shutdown,
                        &shutdown,
                        &restart_attempt,
                    );
                })
                .map_err(RuntimeError::Spawn)?
        };

        preparer_tx
            .send(preparer)
            .map_err(|_| RuntimeError::Lifecycle {
                phase: "supervisor startup handoff",
                source: std::io::Error::other("supervisor worker exited before preparation"),
            })?;
        match startup_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(error);
            }
            Err(_) => {
                let _ = thread.join();
                return Err(RuntimeError::Lifecycle {
                    phase: "supervisor startup",
                    source: std::io::Error::other("supervisor worker ended before startup result"),
                });
            }
        }

        Ok(Self {
            events_rx,
            output,
            current_pid,
            crash_loop_error,
            crashes,
            terminal_error,
            #[cfg(target_os = "macos")]
            accepted_shutdown,
            shutdown,
            #[cfg(windows)]
            restart_attempt,
            thread: Some(thread),
        })
    }

    /// Blocks until the next lifecycle event, or `timeout` elapses.
    #[must_use]
    pub fn recv_event(&self, timeout: Duration) -> Option<SupervisorEvent> {
        self.events_rx.recv_timeout(timeout).ok()
    }

    /// Blocks until supervision reaches a terminal state: the child exited
    /// zero, or the crash-loop breaker tripped. There is no internal
    /// timeout — the crash-loop breaker itself bounds the number of
    /// spawn/wait cycles, so this call always terminates.
    #[must_use]
    pub fn wait_for_outcome(&self) -> SupervisorOutcome {
        loop {
            match self.events_rx.recv() {
                Ok(SupervisorEvent::CrashLoopTripped) => {
                    let err = lock_or_recover(&self.crash_loop_error).clone();
                    return SupervisorOutcome::CrashLoop(err.unwrap_or(RuntimeError::CrashLoop {
                        crashes: 0,
                        window_secs: 0,
                        last_exit_code: None,
                        stderr_tail: String::new(),
                    }));
                }
                Ok(SupervisorEvent::Failed { .. }) => {
                    let err = lock_or_recover(&self.terminal_error).clone();
                    return SupervisorOutcome::Failed(err.unwrap_or(RuntimeError::Lifecycle {
                        phase: "unknown",
                        source: std::io::Error::other("prepared child failed without diagnostic"),
                    }));
                }
                Ok(SupervisorEvent::Stopped) | Err(_) => {
                    if let Some(error) = lock_or_recover(&self.crash_loop_error).clone() {
                        return SupervisorOutcome::CrashLoop(error);
                    }
                    if let Some(error) = lock_or_recover(&self.terminal_error).clone() {
                        return SupervisorOutcome::Failed(error);
                    }
                    return SupervisorOutcome::Stopped;
                }
                Ok(_) => {}
            }
        }
    }

    /// Returns a terminal outcome when one is already queued, without
    /// blocking. Intermediate lifecycle events are consumed because the
    /// durable crash ledger and current-pid snapshots retain their facts.
    #[must_use]
    pub fn try_wait_for_outcome(&self) -> Option<SupervisorOutcome> {
        loop {
            match self.events_rx.try_recv() {
                Ok(SupervisorEvent::CrashLoopTripped) => {
                    let error = lock_or_recover(&self.crash_loop_error).clone().unwrap_or(
                        RuntimeError::CrashLoop {
                            crashes: 0,
                            window_secs: 0,
                            last_exit_code: None,
                            stderr_tail: String::new(),
                        },
                    );
                    return Some(SupervisorOutcome::CrashLoop(error));
                }
                Ok(SupervisorEvent::Failed { .. }) => {
                    let error = lock_or_recover(&self.terminal_error).clone().unwrap_or(
                        RuntimeError::Lifecycle {
                            phase: "unknown",
                            source: std::io::Error::other(
                                "prepared child failed without diagnostic",
                            ),
                        },
                    );
                    return Some(SupervisorOutcome::Failed(error));
                }
                Ok(SupervisorEvent::Stopped) => return Some(SupervisorOutcome::Stopped),
                Ok(_) => {}
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some(error) = lock_or_recover(&self.crash_loop_error).clone() {
                        return Some(SupervisorOutcome::CrashLoop(error));
                    }
                    if let Some(error) = lock_or_recover(&self.terminal_error).clone() {
                        return Some(SupervisorOutcome::Failed(error));
                    }
                    return Some(SupervisorOutcome::Stopped);
                }
            }
        }
    }

    /// Snapshot of unrequested self-terminations observed so far, across every
    /// spawn attempt.
    ///
    /// Readable at any time and never consumed, so a host that drains no
    /// events still sees them. Two snapshots bound an interval:
    /// `self_termination_count` grows for every unrequested exit, while
    /// `count` grows only for non-zero statuses or signals
    /// (KEL-105/KEL-116).
    #[must_use]
    pub fn crash_ledger(&self) -> CrashLedger {
        lock_or_recover(&self.crashes).clone()
    }

    /// Snapshot of stdout/stderr captured so far, across every spawn attempt.
    #[must_use]
    pub fn output(&self) -> CapturedOutput {
        lock_or_recover(&self.output).clone()
    }

    /// OS process id of the currently running child, or `None` between
    /// restart attempts or after supervision has stopped.
    #[must_use]
    pub fn current_pid(&self) -> Option<u32> {
        *lock_or_recover(&self.current_pid)
    }

    /// Marks a caller-accepted shutdown before its reply can make the child
    /// terminate cooperatively. This changes attribution only; [`Self::shutdown`]
    /// remains the sole signal that initiates kill/reap.
    #[cfg(target_os = "macos")]
    pub(crate) fn accept_shutdown(&self) {
        self.accepted_shutdown.store(true, Ordering::Release);
    }

    /// Stops supervision and kills the current child, if any. Idempotent.
    /// Does not block for the background thread to finish — call `Drop`
    /// (or drop the `Supervisor`) to join it.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Requests host-owned revoke/kill/reap/restart of the named child attempt.
    ///
    /// This does not create another restart loop; the supervisor worker
    /// consumes a matching signal at its existing wait boundary and applies
    /// the same crash-loop policy before successor preparation. A stale
    /// request cannot restart a later attempt.
    #[cfg(windows)]
    pub(crate) fn restart_generation(&self, attempt: u32) {
        self.restart_attempt.store(attempt, Ordering::Release);
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn spawn_prepared<L>(prepared: PreparedChild<L>) -> Result<(Child, L), RuntimeError>
where
    L: GenerationLease,
{
    let PreparedChild { mut command, lease } = prepared;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    match command.spawn() {
        Ok(child) => Ok((child, lease)),
        Err(source) => {
            lease.revoke(RevocationCause::SpawnFailed)?;
            Err(RuntimeError::Spawn(source))
        }
    }
}

#[allow(clippy::too_many_arguments)] // internal worker; grouping into a struct would not reduce coupling
#[allow(clippy::too_many_lines)] // one lifecycle state machine keeps lease/child ownership transitions contiguous
fn supervise<P>(
    policy: RestartPolicy,
    mut preparer: P,
    mut child: Child,
    mut lease: P::Lease,
    events_tx: &Sender<SupervisorEvent>,
    output: &Arc<Mutex<CapturedOutput>>,
    current_pid: &Arc<Mutex<Option<u32>>>,
    crash_loop_error: &Arc<Mutex<Option<RuntimeError>>>,
    crash_ledger: &Arc<Mutex<CrashLedger>>,
    terminal_error: &Arc<Mutex<Option<RuntimeError>>>,
    #[cfg(target_os = "macos")] accepted_shutdown: &Arc<AtomicBool>,
    shutdown: &Arc<AtomicBool>,
    restart_attempt: &Arc<AtomicU32>,
) where
    P: ChildPreparer,
{
    let mut crash_times: Vec<Instant> = Vec::new();
    let mut attempt: u32 = 1;
    loop {
        let pid = child.id();
        if let Err(error) = lease.child_spawned(pid, attempt) {
            let terminal = match lease.revoke(RevocationCause::AdmissionFailed) {
                Ok(()) => error,
                Err(revocation_error) => revocation_error,
            };
            let _ = child.kill();
            let _ = child.wait();
            *lock_or_recover(current_pid) = None;
            *lock_or_recover(terminal_error) = Some(terminal);
            let _ = events_tx.send(SupervisorEvent::Failed { attempt });
            return;
        }
        *lock_or_recover(current_pid) = Some(pid);
        let _ = events_tx.send(SupervisorEvent::Started { pid, attempt });
        let capture_threads = match start_capture_threads(&mut child, output) {
            Ok(threads) => threads,
            Err(error) => {
                let terminal = RuntimeError::Lifecycle {
                    phase: "capture thread",
                    source: error.source,
                };
                let terminal = match lease.revoke(RevocationCause::CaptureFailed) {
                    Ok(()) => terminal,
                    Err(revocation_error) => revocation_error,
                };
                let _ = child.kill();
                let _ = child.wait();
                join_capture_threads(error.threads);
                *lock_or_recover(current_pid) = None;
                *lock_or_recover(terminal_error) = Some(terminal);
                let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                return;
            }
        };
        let wait =
            match wait_or_shutdown(&mut child, &mut lease, shutdown, restart_attempt, attempt) {
                Ok(wait) => wait,
                Err(error) => {
                    let terminal = RuntimeError::Lifecycle {
                        phase: "child wait",
                        source: error,
                    };
                    let terminal = match lease.revoke(RevocationCause::WaitFailed) {
                        Ok(()) => terminal,
                        Err(revocation_error) => revocation_error,
                    };
                    let _ = child.kill();
                    let _ = child.wait();
                    join_capture_threads(capture_threads);
                    *lock_or_recover(current_pid) = None;
                    *lock_or_recover(terminal_error) = Some(terminal);
                    let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                    return;
                }
            };
        if let WaitResult::LeaseFailed(error) = wait {
            let terminal = match lease.revoke(RevocationCause::AdmissionFailed) {
                Ok(()) => error,
                Err(revocation_error) => revocation_error,
            };
            let _ = child.kill();
            let _ = child.wait();
            join_capture_threads(capture_threads);
            *lock_or_recover(current_pid) = None;
            *lock_or_recover(terminal_error) = Some(terminal);
            let _ = events_tx.send(SupervisorEvent::Failed { attempt });
            return;
        }
        if matches!(wait, WaitResult::ShutdownRequested) {
            // Observe the bounded ambiguity window before the host changes
            // child authority. Revoking a live generation can itself close
            // the app link and make a cooperative child exit; anything first
            // observed after that host action is host-induced, not an
            // unrequested self-termination (KEL-116). Revocation still
            // precedes close/kill as architecture 06 requires.
            let self_terminated = wait_for_self_termination(&mut child);
            if let Err(error) = lease.revoke(RevocationCause::Shutdown) {
                *lock_or_recover(terminal_error) = Some(error);
                let _ = child.kill();
                let _ = child.wait();
                finish_capture_threads_after_shutdown(capture_threads);
                #[cfg(target_os = "macos")]
                let accepted = shutdown_was_accepted(accepted_shutdown);
                #[cfg(not(target_os = "macos"))]
                let accepted = shutdown_was_accepted();
                if let Some(status) = self_terminated
                    && !accepted
                {
                    record_self_termination(crash_ledger, output, pid, status.code());
                }
                *lock_or_recover(current_pid) = None;
                let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                return;
            }
            let _ = child.kill();
            let wait_result = child.wait();
            finish_capture_threads_after_shutdown(capture_threads);
            if let Err(source) = wait_result {
                *lock_or_recover(terminal_error) = Some(RuntimeError::Lifecycle {
                    phase: "child shutdown wait",
                    source,
                });
                *lock_or_recover(current_pid) = None;
                let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                return;
            }
            #[cfg(target_os = "macos")]
            let accepted = shutdown_was_accepted(accepted_shutdown);
            #[cfg(not(target_os = "macos"))]
            let accepted = shutdown_was_accepted();
            if let Some(status) = self_terminated
                && !accepted
            {
                record_self_termination(crash_ledger, output, pid, status.code());
            }
            *lock_or_recover(current_pid) = None;
            let _ = events_tx.send(SupervisorEvent::Stopped);
            return;
        }
        let code = match wait {
            WaitResult::RestartRequested => {
                #[cfg(windows)]
                let restart_cause = RevocationCause::LinkFailed;
                #[cfg(not(windows))]
                let restart_cause = RevocationCause::AdmissionFailed;
                if let Err(error) = lease.revoke(restart_cause) {
                    *lock_or_recover(terminal_error) = Some(error);
                    let _ = child.kill();
                    let _ = child.wait();
                    drop(capture_threads);
                    *lock_or_recover(current_pid) = None;
                    let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                    return;
                }
                let _ = child.kill();
                if let Err(source) = child.wait() {
                    *lock_or_recover(terminal_error) = Some(RuntimeError::Lifecycle {
                        phase: "child restart wait",
                        source,
                    });
                    drop(capture_threads);
                    *lock_or_recover(current_pid) = None;
                    let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                    return;
                }
                // A descendant can retain Bun's inherited pipe write end.
                // Retired readers share the output mutex safely and exit when
                // that handle closes; joining here would block successor
                // provisioning on a process this supervisor does not own.
                // KEL-78 remains the descendant reaping owner.
                drop(capture_threads);
                *lock_or_recover(current_pid) = None;
                None
            }
            WaitResult::Exited(status) => {
                join_capture_threads(capture_threads);
                *lock_or_recover(current_pid) = None;
                let code = status.code();
                restart_attempt.store(0, Ordering::Release);
                let _ = events_tx.send(SupervisorEvent::Exited { pid, code });
                // The OS exit is already an observed fact. Publish it before
                // revocation so a lifecycle failure cannot erase the durable
                // ledger record (KEL-116).
                #[cfg(target_os = "macos")]
                let accepted = shutdown_was_accepted(accepted_shutdown);
                #[cfg(not(target_os = "macos"))]
                let accepted = shutdown_was_accepted();
                if !accepted {
                    record_self_termination(crash_ledger, output, pid, code);
                }
                if let Err(error) = lease.revoke(RevocationCause::ChildExited) {
                    *lock_or_recover(terminal_error) = Some(error);
                    let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                    return;
                }
                if accepted || code == Some(0) {
                    let _ = events_tx.send(SupervisorEvent::Stopped);
                    return;
                }
                code
            }
            WaitResult::ShutdownRequested | WaitResult::LeaseFailed(_) => return,
        };

        let now = Instant::now();
        let window = Duration::from_secs(u64::from(policy.window_secs));
        crash_times.retain(|t| now.duration_since(*t) <= window);
        crash_times.push(now);

        let crash_count = u8::try_from(crash_times.len()).unwrap_or(u8::MAX);
        if crash_count >= policy.max_crashes {
            let err = RuntimeError::CrashLoop {
                crashes: crash_count,
                window_secs: policy.window_secs,
                last_exit_code: code,
                stderr_tail: lock_or_recover(output).stderr_tail(2000),
            };
            *lock_or_recover(crash_loop_error) = Some(err);
            let _ = events_tx.send(SupervisorEvent::CrashLoopTripped);
            return;
        }

        if wait_backoff_or_shutdown(backoff_delay(crash_count), shutdown) {
            let _ = events_tx.send(SupervisorEvent::Stopped);
            return;
        }

        match preparer.allow_restart(shutdown) {
            Ok(true) => {}
            Ok(false) => {
                let _ = events_tx.send(SupervisorEvent::Stopped);
                return;
            }
            Err(error) => {
                *lock_or_recover(terminal_error) = Some(error);
                let _ = events_tx.send(SupervisorEvent::Failed {
                    attempt: attempt.saturating_add(1),
                });
                return;
            }
        }

        attempt += 1;
        match preparer.prepare(attempt) {
            Ok(next_prepared_child) if shutdown.load(Ordering::SeqCst) => {
                let PreparedChild { lease, .. } = next_prepared_child;
                if let Err(error) = lease.revoke(RevocationCause::Shutdown) {
                    *lock_or_recover(terminal_error) = Some(error);
                    let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                } else {
                    let _ = events_tx.send(SupervisorEvent::Stopped);
                }
                return;
            }
            Ok(next_prepared_child) => match spawn_prepared(next_prepared_child) {
                Ok((next_child, next_lease)) => {
                    if shutdown.load(Ordering::SeqCst) {
                        let mut next_child = next_child;
                        if let Err(error) = next_lease.revoke(RevocationCause::Shutdown) {
                            *lock_or_recover(terminal_error) = Some(error);
                            let _ = next_child.kill();
                            let _ = next_child.wait();
                            let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                        } else {
                            let _ = next_child.kill();
                            let _ = next_child.wait();
                            let _ = events_tx.send(SupervisorEvent::Stopped);
                        }
                        return;
                    }
                    child = next_child;
                    lease = next_lease;
                }
                Err(error) => {
                    *lock_or_recover(terminal_error) = Some(error);
                    let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                    return;
                }
            },
            Err(error) => {
                *lock_or_recover(terminal_error) = Some(error);
                let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                return;
            }
        }
    }
}

struct CaptureThreads {
    stdout: Option<JoinHandle<()>>,
    stderr: Option<JoinHandle<()>>,
}

struct CaptureStartError {
    source: std::io::Error,
    threads: CaptureThreads,
}

fn start_capture_threads(
    child: &mut Child,
    output: &Arc<Mutex<CapturedOutput>>,
) -> Result<CaptureThreads, CaptureStartError> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_thread =
        match stdout.map(|r| spawn_capture_thread(r, Arc::clone(output), "stdout", true)) {
            Some(Ok(thread)) => Some(thread),
            Some(Err(source)) => {
                return Err(CaptureStartError {
                    source,
                    threads: CaptureThreads {
                        stdout: None,
                        stderr: None,
                    },
                });
            }
            None => None,
        };
    let stderr_thread =
        match stderr.map(|r| spawn_capture_thread(r, Arc::clone(output), "stderr", false)) {
            Some(Ok(thread)) => Some(thread),
            Some(Err(source)) => {
                return Err(CaptureStartError {
                    source,
                    threads: CaptureThreads {
                        stdout: stdout_thread,
                        stderr: None,
                    },
                });
            }
            None => None,
        };
    Ok(CaptureThreads {
        stdout: stdout_thread,
        stderr: stderr_thread,
    })
}

fn join_capture_threads(threads: CaptureThreads) {
    if let Some(thread) = threads.stdout {
        let _ = thread.join();
    }
    if let Some(thread) = threads.stderr {
        let _ = thread.join();
    }
}

fn finish_capture_threads_after_shutdown(threads: CaptureThreads) {
    #[cfg(windows)]
    {
        // An uncontained descendant may retain an inherited pipe write end.
        // Waiting for that unowned process would block orderly host exit;
        // KEL-78 remains the process-tree reaping owner. The readers retain
        // their shared output sink and exit when the pipe eventually closes.
        drop(threads);
    }
    #[cfg(not(windows))]
    join_capture_threads(threads);
}

/// Polls `child` for exit, honoring `shutdown`. Returns `true` if it killed
/// the child because `shutdown` was requested (caller must not treat this as
/// a crash), `false` if the child exited on its own.
enum WaitResult {
    Exited(std::process::ExitStatus),
    ShutdownRequested,
    RestartRequested,
    LeaseFailed(RuntimeError),
}

fn wait_or_shutdown<L>(
    child: &mut Child,
    lease: &mut L,
    shutdown: &Arc<AtomicBool>,
    restart_attempt: &Arc<AtomicU32>,
    attempt: u32,
) -> Result<WaitResult, std::io::Error>
where
    L: GenerationLease,
{
    loop {
        match child.try_wait() {
            Ok(None) => {
                if shutdown.load(Ordering::SeqCst) {
                    // KEL-105/KEL-116: the child can die between the
                    // `try_wait` above and this load, which would report its
                    // self-termination as a host stop and let `keld dev` exit
                    // 0 over a dead app. Look
                    // once more before conceding: an exit observed now is the
                    // child's own, whereas after the caller's kill the status
                    // is ours (a signal on unix, exit code 1 on Windows) and
                    // no longer says who died first. Routing it back to
                    // `Exited` reuses the one self-termination accounting path
                    // rather than adding a second.
                    return match child.try_wait() {
                        Ok(Some(status)) => Ok(WaitResult::Exited(status)),
                        _ => Ok(WaitResult::ShutdownRequested),
                    };
                }
                if take_restart_for_attempt(restart_attempt, attempt) {
                    return match child.try_wait() {
                        Ok(Some(status)) => Ok(WaitResult::Exited(status)),
                        Ok(None) => Ok(wait_for_self_termination(child)
                            .map_or(WaitResult::RestartRequested, WaitResult::Exited)),
                        Err(error) => Err(error),
                    };
                }
                if let Err(error) = lease.poll() {
                    return Ok(WaitResult::LeaseFailed(error));
                }
                thread::sleep(POLL_INTERVAL);
            }
            Ok(Some(status)) => return Ok(WaitResult::Exited(status)),
            Err(error) => return Err(error),
        }
    }
}

fn take_restart_for_attempt(restart_attempt: &AtomicU32, attempt: u32) -> bool {
    restart_attempt.swap(0, Ordering::AcqRel) == attempt
}

/// How long the supervisor lets a child it is about to stop finish dying on its
/// own before attributing the death to the host's own kill.
///
/// This is the width of the ambiguous window, not a guess at how slow a child
/// is: `kill` returns before the process is gone, so "already terminating" and
/// "healthy, about to be killed" look identical for as long as the kernel takes
/// to reap. Paid once per session teardown and only when the child has not
/// already exited, so a healthy `keld dev` pays it once on the way out.
const SELF_TERMINATION_GRACE: Duration = Duration::from_millis(250);

/// Waits up to [`SELF_TERMINATION_GRACE`] for `child` to exit on its own.
///
/// `Some(status)` means the child ended itself and the host must account for it;
/// `None` means it was still running, so the host's own kill is what ends it.
fn wait_for_self_termination(child: &mut Child) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + SELF_TERMINATION_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    return None;
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(_) => return None,
        }
    }
}

/// Records one observed self-termination in the shared ledger. The single
/// owner of that write: both the natural-exit path and the shutdown-race check
/// go through here so the count, ordering position, and diagnostic cannot
/// disagree.
fn record_self_termination(
    crash_ledger: &Arc<Mutex<CrashLedger>>,
    output: &Arc<Mutex<CapturedOutput>>,
    pid: u32,
    exit_code: Option<i32>,
) {
    // One output snapshot: the crash tail and both ordering views must
    // describe the same point in the stream. Status zero retains no tail, so
    // its fixed-size record adds no hot-path allocation.
    let (stdout_len, stderr_tail) = {
        let captured = lock_or_recover(output);
        (
            captured.stdout.len(),
            (exit_code != Some(0)).then(|| captured.stderr_tail(2000)),
        )
    };
    let mut ledger = lock_or_recover(crash_ledger);
    ledger.self_termination_count = ledger.self_termination_count.saturating_add(1);
    ledger.last_self_termination = Some(SelfTerminationRecord {
        pid,
        exit_code,
        stdout_len,
    });
    if let Some(stderr_tail) = stderr_tail {
        ledger.count = ledger.count.saturating_add(1);
        ledger.stdout_len_at_last_crash = stdout_len;
        ledger.last = Some(RuntimeError::ChildCrashed {
            pid,
            exit_code,
            stderr_tail,
        });
    }
}

fn spawn_capture_thread(
    mut reader: impl Read + Send + 'static,
    output: Arc<Mutex<CapturedOutput>>,
    name: &'static str,
    is_stdout: bool,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(format!("keld-runtime-capture-{name}"))
        .spawn(move || {
            let mut buf = [0_u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        let mut guard = lock_or_recover(&output);
                        if is_stdout {
                            guard.stdout.push_str(&chunk);
                        } else {
                            guard.stderr.push_str(&chunk);
                        }
                    }
                }
            }
        })
}

fn wait_backoff_or_shutdown(delay: Duration, shutdown: &AtomicBool) -> bool {
    let start = Instant::now();
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return true;
        }
        let elapsed = start.elapsed();
        if elapsed >= delay {
            return shutdown.load(Ordering::SeqCst);
        }
        let Some(remaining) = delay.checked_sub(elapsed) else {
            return shutdown.load(Ordering::SeqCst);
        };
        thread::sleep(remaining.min(POLL_INTERVAL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[derive(Clone)]
    struct RecordingLease {
        attempt: u32,
        record: Arc<Mutex<Vec<String>>>,
    }

    impl GenerationLease for RecordingLease {
        fn revoke(self, cause: RevocationCause) -> Result<(), RuntimeError> {
            let cause = match cause {
                RevocationCause::ChildExited => "exited",
                RevocationCause::Shutdown => "shutdown",
                #[cfg(windows)]
                RevocationCause::LinkFailed => "link-failed",
                RevocationCause::CaptureFailed => "capture-failed",
                RevocationCause::AdmissionFailed => "admission-failed",
                RevocationCause::SpawnFailed => "spawn-failed",
                RevocationCause::WaitFailed => "wait-failed",
            };
            lock_or_recover(&self.record).push(format!("revoke:{}:{cause}", self.attempt));
            Ok(())
        }
    }

    struct FailingRevokeLease;

    impl GenerationLease for FailingRevokeLease {
        fn revoke(self, _cause: RevocationCause) -> Result<(), RuntimeError> {
            Err(RuntimeError::Lifecycle {
                phase: "test revoke",
                source: std::io::Error::other("intentional revoke failure"),
            })
        }
    }

    struct RecordingPreparer {
        record: Arc<Mutex<Vec<String>>>,
        commands: Vec<Command>,
    }

    struct GatedPreparer {
        inner: RecordingPreparer,
        entered_tx: mpsc::Sender<()>,
        release_rx: mpsc::Receiver<()>,
    }

    impl ChildPreparer for GatedPreparer {
        type Lease = RecordingLease;

        fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
            if attempt == 2 {
                let _ = self.entered_tx.send(());
                self.release_rx
                    .recv()
                    .map_err(|_| RuntimeError::Lifecycle {
                        phase: "test successor gate",
                        source: std::io::Error::other("test release channel closed"),
                    })?;
            }
            self.inner.prepare(attempt)
        }
    }

    impl ChildPreparer for RecordingPreparer {
        type Lease = RecordingLease;

        fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
            lock_or_recover(&self.record).push(format!("prepare:{attempt}"));
            let command = self.commands.remove(0);
            Ok(PreparedChild {
                command,
                lease: RecordingLease {
                    attempt,
                    record: Arc::clone(&self.record),
                },
            })
        }
    }

    struct FailingRevokePreparer {
        command: Option<Command>,
    }

    struct ShutdownExitPreparer {
        command: Option<Command>,
        exit_on_revoke: Option<(PathBuf, PathBuf)>,
    }

    struct ShutdownExitLease {
        exit_on_revoke: Option<(PathBuf, PathBuf)>,
        pid: Option<u32>,
    }

    impl GenerationLease for ShutdownExitLease {
        fn child_spawned(&mut self, pid: u32, _attempt: u32) -> Result<(), RuntimeError> {
            self.pid = Some(pid);
            Ok(())
        }

        fn revoke(self, cause: RevocationCause) -> Result<(), RuntimeError> {
            if cause == RevocationCause::Shutdown
                && let Some((marker, acknowledged)) = self.exit_on_revoke
            {
                std::fs::write(marker, b"exit").map_err(|source| RuntimeError::Lifecycle {
                    phase: "test shutdown exit signal",
                    source,
                })?;
                wait_for_marker(&acknowledged)?;
                let pid = self.pid.ok_or_else(|| RuntimeError::Lifecycle {
                    phase: "test shutdown exit observation",
                    source: std::io::Error::other("missing helper pid"),
                })?;
                wait_for_process_exit(pid)?;
            }
            Ok(())
        }
    }

    impl ChildPreparer for ShutdownExitPreparer {
        type Lease = ShutdownExitLease;

        fn prepare(&mut self, _attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
            let command = self.command.take().ok_or_else(|| RuntimeError::Lifecycle {
                phase: "test prepare",
                source: std::io::Error::other("missing shutdown-exit command"),
            })?;
            Ok(PreparedChild {
                command,
                lease: ShutdownExitLease {
                    exit_on_revoke: self.exit_on_revoke.clone(),
                    pid: None,
                },
            })
        }
    }

    impl ChildPreparer for FailingRevokePreparer {
        type Lease = FailingRevokeLease;

        fn prepare(&mut self, _attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
            let command = self.command.take().ok_or_else(|| RuntimeError::Lifecycle {
                phase: "test prepare",
                source: std::io::Error::other("missing test command"),
            })?;
            Ok(PreparedChild {
                command,
                lease: FailingRevokeLease,
            })
        }
    }

    /// A child that stays alive for far longer than any test needs, spawned
    /// with no shell in between so the supervisor's direct child is the only
    /// process holding its pipes.
    fn long_running_command() -> Command {
        #[cfg(unix)]
        {
            let mut cmd = Command::new("sleep");
            cmd.arg("30");
            cmd
        }
        #[cfg(windows)]
        {
            let mut cmd = Command::new("ping");
            cmd.args(["-n", "31", "127.0.0.1"]);
            cmd
        }
    }

    fn shell_command(script: &str) -> Command {
        #[cfg(unix)]
        {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", script]);
            cmd
        }
        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", script]);
            cmd
        }
    }

    #[test]
    fn stale_restart_request_cannot_restart_a_successor_generation() {
        let requested = AtomicU32::new(1);
        assert!(!take_restart_for_attempt(&requested, 2));
        assert_eq!(requested.load(Ordering::Acquire), 0);

        requested.store(2, Ordering::Release);
        assert!(take_restart_for_attempt(&requested, 2));
        assert_eq!(requested.load(Ordering::Acquire), 0);
    }

    #[cfg(windows)]
    #[test]
    fn host_requested_restart_does_not_wait_for_descendant_capture_eof() {
        let factory_attempt = Arc::new(AtomicU32::new(0));
        let factory_counter = Arc::clone(&factory_attempt);
        let supervisor = Supervisor::start(RestartPolicy::default(), move || {
            if factory_counter.fetch_add(1, Ordering::AcqRel) == 0 {
                let mut command = Command::new("cmd");
                command.args([
                    "/C",
                    "start \"\" /B ping -n 5 127.0.0.1 & ping -n 5 127.0.0.1",
                ]);
                command
            } else {
                long_running_command()
            }
        })
        .expect("first child starts");
        assert!(matches!(
            supervisor.recv_event(Duration::from_secs(2)),
            Some(SupervisorEvent::Started { attempt: 1, .. })
        ));

        supervisor.restart_generation(1);
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut successor_started = false;
        while Instant::now() < deadline {
            if matches!(
                supervisor.recv_event(Duration::from_millis(100)),
                Some(SupervisorEvent::Started { attempt: 2, .. })
            ) {
                successor_started = true;
                break;
            }
        }
        assert!(
            successor_started,
            "retired capture readers blocked successor provisioning"
        );
        supervisor.shutdown();
        assert!(matches!(
            supervisor.wait_for_outcome(),
            SupervisorOutcome::Stopped
        ));
    }

    #[cfg(windows)]
    #[test]
    fn link_failure_does_not_restart_a_child_already_exiting_zero() {
        let temp = tempfile::tempdir().expect("helper tempdir");
        let ready = temp.path().join("ready");
        let exit = temp.path().join("exit");
        let acknowledged = temp.path().join("acknowledged");
        let released = temp.path().join("released");
        let factory_attempt = Arc::new(AtomicU32::new(0));
        let factory_counter = Arc::clone(&factory_attempt);
        let command_ready = ready.clone();
        let command_exit = exit.clone();
        let command_acknowledged = acknowledged.clone();
        let command_released = released.clone();
        let supervisor = Supervisor::start(RestartPolicy::default(), move || {
            if factory_counter.fetch_add(1, Ordering::AcqRel) == 0 {
                let mut command = self_termination_helper_command(
                    &command_ready,
                    &command_exit,
                    &command_acknowledged,
                );
                command.env("KELD_RUNTIME_EXIT_RELEASED", &command_released);
                command
            } else {
                long_running_command()
            }
        })
        .expect("status-zero child starts");
        assert!(matches!(
            supervisor.recv_event(Duration::from_secs(2)),
            Some(SupervisorEvent::Started { attempt: 1, .. })
        ));
        await_marker(&ready);
        std::fs::write(&exit, b"exit").expect("request helper exit");
        await_marker(&acknowledged);
        supervisor.restart_generation(1);
        std::fs::write(&released, b"release").expect("release helper exit");
        assert!(matches!(
            supervisor.wait_for_outcome(),
            SupervisorOutcome::Stopped
        ));
        assert_eq!(
            factory_attempt.load(Ordering::Acquire),
            1,
            "a cleanly exiting child was replaced by generation 2"
        );
        assert_eq!(
            supervisor
                .crash_ledger()
                .last_self_termination
                .and_then(|record| record.exit_code),
            Some(0)
        );
    }

    #[cfg(windows)]
    #[test]
    fn orderly_shutdown_does_not_wait_for_descendant_capture_eof() {
        let supervisor = Supervisor::start(RestartPolicy::default(), || {
            let mut command = Command::new("cmd");
            command.args([
                "/C",
                "start \"\" /B ping -n 5 127.0.0.1 & ping -n 5 127.0.0.1",
            ]);
            command
        })
        .expect("child with inherited-pipe descendant starts");
        assert!(matches!(
            supervisor.recv_event(Duration::from_secs(2)),
            Some(SupervisorEvent::Started { .. })
        ));

        let started = Instant::now();
        supervisor.shutdown();
        assert!(matches!(
            supervisor.wait_for_outcome(),
            SupervisorOutcome::Stopped
        ));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "orderly shutdown waited for an unowned descendant pipe"
        );
    }

    #[test]
    fn prepared_lease_revokes_before_successor_preparation() {
        let record = Arc::new(Mutex::new(Vec::new()));
        let policy = RestartPolicy {
            max_crashes: 3,
            window_secs: 30,
        };
        let supervisor = Supervisor::start_prepared(
            policy,
            RecordingPreparer {
                record: Arc::clone(&record),
                commands: vec![shell_command("exit 1"), shell_command("exit 0")],
            },
        )
        .expect("first prepared child must spawn");
        match supervisor.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            SupervisorOutcome::CrashLoop(error) => panic!("second child exits cleanly: {error}"),
            SupervisorOutcome::Failed(error) => panic!("prepared lifecycle must not fail: {error}"),
        }
        assert_eq!(
            *lock_or_recover(&record),
            vec![
                "prepare:1".to_owned(),
                "revoke:1:exited".to_owned(),
                "prepare:2".to_owned(),
                "revoke:2:exited".to_owned(),
            ],
            "successor preparation must not run before the prior lease is revoked"
        );
    }

    #[test]
    fn prepared_spawn_failure_revokes_unstarted_lease() {
        let record = Arc::new(Mutex::new(Vec::new()));
        let result = Supervisor::start_prepared(
            RestartPolicy::default(),
            RecordingPreparer {
                record: Arc::clone(&record),
                commands: vec![Command::new("keld-runtime-definitely-not-a-real-binary")],
            },
        );
        assert!(matches!(result, Err(RuntimeError::Spawn(_))), "{result:?}");
        assert_eq!(
            *lock_or_recover(&record),
            vec!["prepare:1".to_owned(), "revoke:1:spawn-failed".to_owned()],
            "failed OS spawn must revoke its prepared lease"
        );
    }

    #[test]
    fn draining_crash_loop_event_does_not_erase_terminal_outcome() {
        let supervisor = Supervisor::start(
            RestartPolicy {
                max_crashes: 1,
                window_secs: 30,
            },
            || shell_command("exit 1"),
        )
        .expect("child must spawn");
        loop {
            if matches!(
                supervisor.recv_event(Duration::from_secs(1)),
                Some(SupervisorEvent::CrashLoopTripped)
            ) {
                break;
            }
        }
        assert!(
            matches!(
                supervisor.wait_for_outcome(),
                SupervisorOutcome::CrashLoop(_)
            ),
            "event observation must not erase the terminal crash-loop result"
        );
    }

    #[test]
    fn draining_failed_event_does_not_erase_terminal_outcome() {
        let supervisor = Supervisor::start_prepared(
            RestartPolicy::default(),
            FailingRevokePreparer {
                command: Some(shell_command("exit 0")),
            },
        )
        .expect("child must spawn");
        loop {
            if matches!(
                supervisor.recv_event(Duration::from_secs(1)),
                Some(SupervisorEvent::Failed { .. })
            ) {
                break;
            }
        }
        assert!(
            matches!(
                supervisor.wait_for_outcome(),
                SupervisorOutcome::Failed(RuntimeError::Lifecycle { .. })
            ),
            "event observation must not erase the terminal lifecycle failure"
        );
        let ledger = supervisor.crash_ledger();
        assert_eq!(
            ledger.self_termination_count, 1,
            "revocation failure must not erase the already-observed child exit: {ledger:?}"
        );
        assert!(
            ledger
                .last_self_termination
                .is_some_and(|record| record.exit_code == Some(0)),
            "{ledger:?}"
        );
    }

    #[test]
    fn draining_stopped_event_does_not_erase_terminal_outcome() {
        let supervisor = Supervisor::start(RestartPolicy::default(), || shell_command("exit 0"))
            .expect("child must spawn");
        loop {
            if matches!(
                supervisor.recv_event(Duration::from_secs(1)),
                Some(SupervisorEvent::Stopped)
            ) {
                break;
            }
        }
        assert!(
            matches!(supervisor.wait_for_outcome(), SupervisorOutcome::Stopped),
            "event observation must not erase the terminal stopped outcome"
        );
    }

    #[test]
    fn shutdown_during_successor_preparation_revokes_without_started_event() {
        let record = Arc::new(Mutex::new(Vec::new()));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let supervisor = Supervisor::start_prepared(
            RestartPolicy {
                max_crashes: 3,
                window_secs: 30,
            },
            GatedPreparer {
                inner: RecordingPreparer {
                    record: Arc::clone(&record),
                    commands: vec![shell_command("exit 1"), shell_command("sleep 1")],
                },
                entered_tx,
                release_rx,
            },
        )
        .expect("initial child must spawn");
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("g2 preparation must be gated");
        supervisor.shutdown();
        release_tx.send(()).expect("release g2 preparation");
        let mut saw_started_g2 = false;
        loop {
            match supervisor.recv_event(Duration::from_secs(1)) {
                Some(SupervisorEvent::Started { attempt: 2, .. }) => saw_started_g2 = true,
                Some(SupervisorEvent::Stopped) => break,
                Some(_) => {}
                None => panic!("supervisor did not stop after shutdown"),
            }
        }
        assert!(
            !saw_started_g2,
            "shutdown must win before g2 becomes observable"
        );
        assert!(
            lock_or_recover(&record)
                .iter()
                .any(|entry| entry == "revoke:2:shutdown"),
            "prepared g2 lease must be revoked when shutdown wins: {:?}",
            lock_or_recover(&record)
        );
    }

    #[test]
    fn shutdown_during_backoff_stops_before_successor_preparation() {
        let record = Arc::new(Mutex::new(Vec::new()));
        let supervisor = Supervisor::start_prepared(
            RestartPolicy {
                max_crashes: 3,
                window_secs: 30,
            },
            RecordingPreparer {
                record: Arc::clone(&record),
                commands: vec![shell_command("exit 1"), shell_command("exit 0")],
            },
        )
        .expect("initial child must spawn");
        loop {
            if matches!(
                supervisor.recv_event(Duration::from_secs(1)),
                Some(SupervisorEvent::Exited { code: Some(1), .. })
            ) {
                break;
            }
        }
        supervisor.shutdown();
        match supervisor.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            SupervisorOutcome::CrashLoop(error) => panic!("shutdown must not crash-loop: {error}"),
            SupervisorOutcome::Failed(error) => panic!("shutdown must not fail: {error}"),
        }
        assert_eq!(
            *lock_or_recover(&record),
            vec!["prepare:1".to_owned(), "revoke:1:exited".to_owned(),],
            "shutdown during backoff must not prepare a successor"
        );
    }

    /// Joins shell steps with the separator the OS's `shell_command` shell
    /// actually understands (`;` for `sh`, `&` for `cmd.exe` — `cmd` treats
    /// `;` as a literal character, not a statement separator).
    fn joined_steps(steps: &[&str]) -> String {
        #[cfg(unix)]
        {
            steps.join("; ")
        }
        #[cfg(windows)]
        {
            steps.join(" & ")
        }
    }

    fn recv_started(sup: &Supervisor) -> (u32, u32) {
        match sup.recv_event(Duration::from_secs(10)) {
            Some(SupervisorEvent::Started { pid, attempt }) => (pid, attempt),
            other => panic!("expected Started, got {other:?}"),
        }
    }

    fn self_termination_helper_command(ready: &Path, exit: &Path, acknowledged: &Path) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("current test binary"));
        command
            .args([
                "--exact",
                "tests::self_termination_helper_process",
                "--nocapture",
            ])
            .env("KELD_RUNTIME_HELPER_READY", ready)
            .env("KELD_RUNTIME_EXIT_AFTER", exit)
            .env("KELD_RUNTIME_EXIT_ACKNOWLEDGED", acknowledged);
        command
    }

    fn await_marker(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "helper did not create {}",
                path.display()
            );
            std::thread::yield_now();
        }
    }

    fn wait_for_marker(path: &Path) -> Result<(), RuntimeError> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !path.exists() {
            if Instant::now() >= deadline {
                return Err(RuntimeError::Lifecycle {
                    phase: "test shutdown exit acknowledgment",
                    source: std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("helper did not create {}", path.display()),
                    ),
                });
            }
            std::thread::yield_now();
        }
        Ok(())
    }

    fn wait_for_process_exit(pid: u32) -> Result<(), RuntimeError> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while process_has_not_exited(pid).map_err(|source| RuntimeError::Lifecycle {
            phase: "test shutdown exit observation",
            source,
        })? {
            if Instant::now() >= deadline {
                return Err(RuntimeError::Lifecycle {
                    phase: "test shutdown exit observation",
                    source: std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("helper process {pid} did not exit after acknowledging revocation"),
                    ),
                });
            }
            std::thread::yield_now();
        }
        Ok(())
    }

    #[cfg(unix)]
    fn process_has_not_exited(pid: u32) -> std::io::Result<bool> {
        let output = Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()?;
        let state = String::from_utf8_lossy(&output.stdout);
        let state = state.trim();
        Ok(!state.is_empty() && !state.starts_with('Z'))
    }

    #[cfg(windows)]
    fn process_has_not_exited(pid: u32) -> std::io::Result<bool> {
        use std::os::windows::process::CommandExt;
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .creation_flags(0x0800_0000)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
    }

    #[test]
    fn backoff_delay_is_monotonically_increasing() {
        assert!(backoff_delay(1) < backoff_delay(2));
        assert!(backoff_delay(2) < backoff_delay(3));
        assert!(backoff_delay(3) < backoff_delay(4));
    }

    #[test]
    fn a_host_kill_of_a_live_child_is_not_recorded_as_a_crash() {
        // KEL-105 no-false-positive guard. The shutdown branch classifies by
        // observing the child *before* killing it, so a child that was alive
        // when the host asked it to stop must leave the ledger empty — every
        // clean `keld dev` run ends exactly this way, and recording it would
        // make the command exit 1 on a healthy app.
        let sup = Supervisor::start(RestartPolicy::default(), long_running_command)
            .expect("first spawn must succeed");
        let (pid, _) = recv_started(&sup);
        assert_ne!(pid, std::process::id());
        sup.shutdown();
        let outcome = sup.wait_for_outcome();
        let ledger = sup.crash_ledger();
        assert!(
            matches!(outcome, SupervisorOutcome::Stopped),
            "a host-requested stop of a live child is not a failure: {outcome:?}"
        );
        assert_eq!(
            ledger.count, 0,
            "the host's own kill must not be recorded as the child crashing: {ledger:?}"
        );
        assert!(ledger.last.is_none(), "{ledger:?}");
        assert_eq!(ledger.self_termination_count, 0, "{ledger:?}");
        assert!(ledger.last_self_termination.is_none(), "{ledger:?}");
    }

    #[test]
    fn status_zero_self_termination_during_shutdown_grace_is_recorded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ready = dir.path().join("ready");
        let exit = dir.path().join("exit-now");
        let acknowledged = dir.path().join("exit-acknowledged");
        let sup = Supervisor::start_prepared(
            RestartPolicy::default(),
            ShutdownExitPreparer {
                command: Some(self_termination_helper_command(
                    &ready,
                    &exit,
                    &acknowledged,
                )),
                exit_on_revoke: None,
            },
        )
        .expect("helper child must spawn");
        let _ = recv_started(&sup);
        await_marker(&ready);

        std::fs::write(&exit, b"exit").expect("release helper independently of shutdown");
        sup.shutdown();
        let outcome = sup.wait_for_outcome();
        let ledger = sup.crash_ledger();
        assert!(matches!(outcome, SupervisorOutcome::Stopped), "{outcome:?}");
        assert_eq!(
            ledger.self_termination_count, 1,
            "an exit observed before the host kill must be recorded whatever its status: {ledger:?}"
        );
        assert_eq!(ledger.count, 0, "status zero is not a crash: {ledger:?}");
        assert!(
            ledger
                .last_self_termination
                .is_some_and(|record| record.exit_code == Some(0)),
            "{ledger:?}"
        );
    }

    #[test]
    fn shutdown_revocation_induced_zero_exit_is_not_self_termination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ready = dir.path().join("ready");
        let exit = dir.path().join("exit-on-revoke");
        let acknowledged = dir.path().join("exit-acknowledged");
        let sup = Supervisor::start_prepared(
            RestartPolicy::default(),
            ShutdownExitPreparer {
                command: Some(self_termination_helper_command(
                    &ready,
                    &exit,
                    &acknowledged,
                )),
                exit_on_revoke: Some((exit, acknowledged.clone())),
            },
        )
        .expect("helper child must spawn");
        let _ = recv_started(&sup);
        await_marker(&ready);

        sup.shutdown();
        let outcome = sup.wait_for_outcome();
        let ledger = sup.crash_ledger();
        assert!(matches!(outcome, SupervisorOutcome::Stopped), "{outcome:?}");
        assert_eq!(
            ledger.self_termination_count, 0,
            "the host's own revocation caused this exit and must not become an unrequested fact: {ledger:?}"
        );
        assert!(ledger.last_self_termination.is_none(), "{ledger:?}");
        assert!(
            acknowledged.exists(),
            "the child must acknowledge the revoke-triggered exit before the host kill"
        );
    }

    #[test]
    fn self_termination_helper_process() {
        let (Some(ready), Some(exit), Some(acknowledged)) = (
            std::env::var_os("KELD_RUNTIME_HELPER_READY"),
            std::env::var_os("KELD_RUNTIME_EXIT_AFTER"),
            std::env::var_os("KELD_RUNTIME_EXIT_ACKNOWLEDGED"),
        ) else {
            return;
        };
        let ready = PathBuf::from(ready);
        let exit = PathBuf::from(exit);
        let acknowledged = PathBuf::from(acknowledged);
        std::fs::write(ready, b"ready").expect("publish helper readiness");
        while !exit.exists() {
            std::thread::yield_now();
        }
        std::fs::write(acknowledged, b"acknowledged").expect("acknowledge helper exit signal");
        if let Some(released) = std::env::var_os("KELD_RUNTIME_EXIT_RELEASED") {
            let released = PathBuf::from(released);
            while !released.exists() {
                std::thread::yield_now();
            }
        }
        std::process::exit(0);
    }

    #[test]
    fn a_child_that_exits_non_zero_is_recorded_with_its_stdout_position() {
        // Binds the two facts KEL-105's verdict is built from: the crash is
        // counted, and the ledger records how far stdout had got when it
        // happened. Without the position the host cannot order a crash against
        // the app's ready marker, which is the whole mechanism.
        let sup = Supervisor::start(RestartPolicy::default(), || {
            // `&&`, not `;`: `cmd /C` does not treat `;` as a command
            // separator, so a unix-only spelling here makes the child echo one
            // literal string and exit 0 — the crash under test never happens.
            shell_command("echo alive-before-dying && exit 3")
        })
        .expect("first spawn must succeed");
        let error = match sup.wait_for_outcome() {
            SupervisorOutcome::CrashLoop(error) => error,
            other => panic!("expected the breaker to trip, got {other:?}"),
        };
        assert!(error.to_string().contains("KELD-RUNTIME-002"), "{error}");
        let ledger = sup.crash_ledger();
        assert!(
            ledger.count >= 1,
            "a non-zero self-termination must be recorded: {ledger:?}"
        );
        let rendered = ledger
            .last
            .as_ref()
            .expect("a counted crash must carry its diagnostic")
            .to_string();
        assert!(rendered.contains("KELD-RUNTIME-012"), "{rendered}");
        assert!(
            ledger.stdout_len_at_last_crash > 0,
            "the crash must record how far stdout had got, or the host cannot \
             order it against the app's ready marker: {ledger:?}"
        );
        assert!(
            ledger.stdout_len_at_last_crash <= sup.output().stdout.len(),
            "the recorded position must be a real offset into captured stdout: {ledger:?}"
        );
        assert!(
            ledger.self_termination_count >= ledger.count,
            "the all-termination view must include every non-zero crash: {ledger:?}"
        );
        let termination = ledger
            .last_self_termination
            .expect("the all-termination view must retain the latest crash");
        assert_eq!(termination.exit_code, Some(3));
        assert_eq!(termination.stdout_len, ledger.stdout_len_at_last_crash);
    }

    #[test]
    fn zero_exit_stops_without_restart() {
        let sup = Supervisor::start(RestartPolicy::default(), || shell_command("exit 0"))
            .expect("first spawn must succeed");
        let (pid, attempt) = recv_started(&sup);
        assert_eq!(attempt, 1);
        assert_ne!(
            pid,
            std::process::id(),
            "child pid must differ from host pid"
        );

        match sup.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            SupervisorOutcome::CrashLoop(e) => panic!("must not crash-loop on a clean exit: {e}"),
            SupervisorOutcome::Failed(e) => panic!("prepared lifecycle must not fail: {e}"),
        }

        let ledger = sup.crash_ledger();
        assert_eq!(
            ledger.self_termination_count, 1,
            "a child that exits itself must be recorded even when its status is zero: {ledger:?}"
        );
        assert_eq!(ledger.count, 0, "status zero is not a crash: {ledger:?}");
        let termination = ledger
            .last_self_termination
            .expect("a recorded self-termination must carry its status");
        assert_eq!(termination.exit_code, Some(0));
        assert_eq!(termination.pid, pid);
    }

    #[test]
    fn single_crash_restarts_the_child_no_sleep() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_in_closure = Arc::clone(&calls);
        let sup = Supervisor::start(RestartPolicy::default(), move || {
            let n = calls_in_closure.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                shell_command("exit 1")
            } else {
                shell_command("exit 0")
            }
        })
        .expect("first spawn must succeed");

        let (first_pid, first_attempt) = recv_started(&sup);
        assert_eq!(first_attempt, 1);
        assert_ne!(first_pid, std::process::id());

        match sup.recv_event(Duration::from_secs(10)) {
            Some(SupervisorEvent::Exited { code, .. }) => assert_eq!(code, Some(1)),
            other => panic!("expected first Exited, got {other:?}"),
        }

        // The restart, observed as a fresh Started — this is the "await the
        // next live child, no sleep" property from KEL-70 AC3.
        let (second_pid, second_attempt) = recv_started(&sup);
        assert_eq!(second_attempt, 2);
        assert_ne!(second_pid, std::process::id());

        match sup.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            SupervisorOutcome::CrashLoop(e) => panic!("second attempt exits 0: {e}"),
            SupervisorOutcome::Failed(e) => panic!("prepared lifecycle must not fail: {e}"),
        }
    }

    #[test]
    fn crash_loop_breaker_trips_after_max_crashes() {
        let policy = RestartPolicy {
            max_crashes: 2,
            window_secs: 60,
        };
        let sup = Supervisor::start(policy, || {
            shell_command(&joined_steps(&[
                "echo out-marker",
                "echo err-marker 1>&2",
                "exit 1",
            ]))
        })
        .expect("first spawn must succeed");

        match sup.wait_for_outcome() {
            SupervisorOutcome::CrashLoop(RuntimeError::CrashLoop {
                crashes,
                window_secs,
                last_exit_code,
                ..
            }) => {
                assert_eq!(crashes, 2);
                assert_eq!(window_secs, 60);
                assert_eq!(last_exit_code, Some(1));
            }
            other => panic!("expected CrashLoop, got {other:?}"),
        }

        let out = sup.output();
        assert!(out.stdout.contains("out-marker"), "{out:?}");
        assert!(out.stderr.contains("err-marker"), "{out:?}");
    }

    #[test]
    fn spawn_failure_on_missing_program_is_typed() {
        let err = Supervisor::start(RestartPolicy::default(), || {
            Command::new("keld-runtime-definitely-not-a-real-binary")
        })
        .expect_err("missing program must fail spawn");
        let msg = err.to_string();
        assert!(msg.contains("KELD-RUNTIME-001"), "{msg}");
    }

    #[test]
    fn shutdown_kills_a_long_running_child_and_does_not_restart() {
        // Spawned directly rather than through a shell. A shell wrapper is not
        // part of this contract, and on Windows `cmd /C "ping ... >NUL"` runs
        // `ping` as a *grandchild*: killing the direct child leaves it holding
        // the inherited stdout pipe, so joining the capture threads blocks
        // until it exits on its own (KEL-118). That is a real supervisor
        // defect, but it is not what this test is for — asserting it here
        // would make this test fail for a reason it does not name.
        let sup = Supervisor::start(RestartPolicy::default(), long_running_command)
            .expect("first spawn must succeed");
        let (pid, _) = recv_started(&sup);
        assert_ne!(pid, std::process::id());

        sup.shutdown();
        match sup.recv_event(Duration::from_secs(10)) {
            Some(SupervisorEvent::Stopped) => {}
            other => panic!("expected Stopped after shutdown, got {other:?}"),
        }
        assert_eq!(sup.current_pid(), None);
    }

    #[test]
    fn host_pid_is_unaffected_across_restart_cycles() {
        // KEL-70 AC5, headless form: the supervised child's pid is always
        // distinct from the host (this test process's) pid, across every
        // restart — i.e. the child never becomes/replaces the process that
        // would own a window.
        let host_pid = std::process::id();
        let calls = Arc::new(AtomicU32::new(0));
        let calls_in_closure = Arc::clone(&calls);
        let sup = Supervisor::start(RestartPolicy::default(), move || {
            let n = calls_in_closure.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                shell_command("exit 1")
            } else {
                shell_command("exit 0")
            }
        })
        .expect("first spawn must succeed");

        for expected_attempt in 1..=3u32 {
            let (pid, attempt) = recv_started(&sup);
            assert_eq!(attempt, expected_attempt);
            assert_ne!(
                pid, host_pid,
                "attempt {attempt}: child pid must not be the host pid"
            );
            let _ = sup.recv_event(Duration::from_secs(10)); // Exited
        }

        match sup.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            SupervisorOutcome::CrashLoop(e) => panic!("third attempt exits 0: {e}"),
            SupervisorOutcome::Failed(e) => panic!("prepared lifecycle must not fail: {e}"),
        }
        assert_eq!(std::process::id(), host_pid);
    }

    #[test]
    fn drop_reaps_a_long_running_child() {
        #[cfg(unix)]
        let long_running = "sleep 30";
        #[cfg(windows)]
        let long_running = "ping -n 31 127.0.0.1 >NUL";

        let sup = Supervisor::start(RestartPolicy::default(), move || {
            shell_command(long_running)
        })
        .expect("first spawn must succeed");
        let (pid, _) = recv_started(&sup);
        drop(sup);

        assert!(
            !pid_is_running(pid),
            "child pid {pid} must be reaped on Drop"
        );
    }

    fn pid_is_running(pid: u32) -> bool {
        #[cfg(unix)]
        {
            Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .is_ok_and(|s| s.success())
        }
        #[cfg(windows)]
        {
            let output = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
                .output();
            match output {
                Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()),
                Err(_) => false,
            }
        }
    }
}
