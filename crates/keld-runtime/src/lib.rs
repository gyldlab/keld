//! keld-runtime — the app-process supervisor.
//!
//! Spawns the developer's JS/TS main under a pinned Bun, supervises it
//! (exponential backoff, crash-loop breaker), and hands it the kipc link and
//! shared-memory handles at spawn. The renderer outlives app-process restarts
//! because the host owns all windows. Normative spec:
//! `docs/architecture/06-runtime-and-tooling.md` §1.
//!
//! v0 scope (KEL-70): spawn + capture + restart-on-crash + crash-loop
//! breaker. Out of scope here: OS sandbox on the child, Bun
//! pinning/download, `--inspect` passthrough, Bun watch hot-restart, and the
//! destination `KELD_LINK`/`KELD_SHM`/`KELD_CONTRACT` env contract (v0 keeps
//! whatever env the caller's command factory sets, e.g. `KELD_APP_LINK`).

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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

    fn stderr_tail(&self, max_chars: usize) -> String {
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

/// Terminal outcome of [`Supervisor::wait_for_outcome`].
#[derive(Debug)]
pub enum SupervisorOutcome {
    /// The child exited zero; supervision ended without error.
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
    /// The operating system refused to spawn the prepared command.
    SpawnFailed,
    /// The host could not observe the live child state.
    WaitFailed,
}

/// Host-owned authority that is invalidated with one prepared child attempt.
pub(crate) trait GenerationLease: Send + 'static {
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

/// Supervises one Bun (or other) child process: spawns it, captures its
/// stdout/stderr, and restarts it on crash (non-zero exit) up to a
/// [`RestartPolicy`] before giving up with a typed [`RuntimeError::CrashLoop`].
/// A zero exit is treated as graceful completion — the child is not
/// restarted.
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
    terminal_error: Arc<Mutex<Option<RuntimeError>>>,
    shutdown: Arc<AtomicBool>,
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
        let terminal_error = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread = {
            let output = Arc::clone(&output);
            let current_pid = Arc::clone(&current_pid);
            let crash_loop_error = Arc::clone(&crash_loop_error);
            let terminal_error = Arc::clone(&terminal_error);
            let shutdown = Arc::clone(&shutdown);
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
                        &terminal_error,
                        &shutdown,
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
            terminal_error,
            shutdown,
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

    /// Stops supervision and kills the current child, if any. Idempotent.
    /// Does not block for the background thread to finish — call `Drop`
    /// (or drop the `Supervisor`) to join it.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
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
    terminal_error: &Arc<Mutex<Option<RuntimeError>>>,
    shutdown: &Arc<AtomicBool>,
) where
    P: ChildPreparer,
{
    let mut crash_times: Vec<Instant> = Vec::new();
    let mut attempt: u32 = 1;
    loop {
        let pid = child.id();
        *lock_or_recover(current_pid) = Some(pid);
        let _ = events_tx.send(SupervisorEvent::Started { pid, attempt });
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_thread = stdout.map(|r| spawn_capture_thread(r, Arc::clone(output), true));
        let stderr_thread = stderr.map(|r| spawn_capture_thread(r, Arc::clone(output), false));
        let wait = match wait_or_shutdown(&mut child, shutdown) {
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
                join_capture_threads(stdout_thread, stderr_thread);
                *lock_or_recover(current_pid) = None;
                *lock_or_recover(terminal_error) = Some(terminal);
                let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                return;
            }
        };
        if matches!(wait, WaitResult::ShutdownRequested) {
            if let Err(error) = lease.revoke(RevocationCause::Shutdown) {
                *lock_or_recover(terminal_error) = Some(error);
                let _ = child.kill();
                let _ = child.wait();
                join_capture_threads(stdout_thread, stderr_thread);
                *lock_or_recover(current_pid) = None;
                let _ = events_tx.send(SupervisorEvent::Failed { attempt });
                return;
            }
            let _ = child.kill();
            let _ = child.wait();
            join_capture_threads(stdout_thread, stderr_thread);
            *lock_or_recover(current_pid) = None;
            let _ = events_tx.send(SupervisorEvent::Stopped);
            return;
        }
        join_capture_threads(stdout_thread, stderr_thread);
        *lock_or_recover(current_pid) = None;

        let code = match wait {
            WaitResult::Exited(status) => status.code(),
            WaitResult::ShutdownRequested => return,
        };
        let _ = events_tx.send(SupervisorEvent::Exited { pid, code });

        if let Err(error) = lease.revoke(RevocationCause::ChildExited) {
            *lock_or_recover(terminal_error) = Some(error);
            let _ = events_tx.send(SupervisorEvent::Failed { attempt });
            return;
        }

        let crashed = code != Some(0);
        if !crashed {
            let _ = events_tx.send(SupervisorEvent::Stopped);
            return;
        }

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

        thread::sleep(backoff_delay(crash_count));
        if shutdown.load(Ordering::SeqCst) {
            let _ = events_tx.send(SupervisorEvent::Stopped);
            return;
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

fn join_capture_threads(
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
) {
    if let Some(thread) = stdout_thread {
        let _ = thread.join();
    }
    if let Some(thread) = stderr_thread {
        let _ = thread.join();
    }
}

/// Polls `child` for exit, honoring `shutdown`. Returns `true` if it killed
/// the child because `shutdown` was requested (caller must not treat this as
/// a crash), `false` if the child exited on its own.
enum WaitResult {
    Exited(std::process::ExitStatus),
    ShutdownRequested,
}

fn wait_or_shutdown(
    child: &mut Child,
    shutdown: &Arc<AtomicBool>,
) -> Result<WaitResult, std::io::Error> {
    loop {
        match child.try_wait() {
            Ok(None) => {
                if shutdown.load(Ordering::SeqCst) {
                    return Ok(WaitResult::ShutdownRequested);
                }
                thread::sleep(POLL_INTERVAL);
            }
            Ok(Some(status)) => return Ok(WaitResult::Exited(status)),
            Err(error) => return Err(error),
        }
    }
}

fn spawn_capture_thread(
    mut reader: impl Read + Send + 'static,
    output: Arc<Mutex<CapturedOutput>>,
    is_stdout: bool,
) -> JoinHandle<()> {
    thread::spawn(move || {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

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
                RevocationCause::SpawnFailed => "spawn-failed",
                RevocationCause::WaitFailed => "wait-failed",
            };
            lock_or_recover(&self.record).push(format!("revoke:{}:{cause}", self.attempt));
            Ok(())
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

    #[test]
    fn backoff_delay_is_monotonically_increasing() {
        assert!(backoff_delay(1) < backoff_delay(2));
        assert!(backoff_delay(2) < backoff_delay(3));
        assert!(backoff_delay(3) < backoff_delay(4));
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
        #[cfg(unix)]
        let long_running = "sleep 30";
        #[cfg(windows)]
        let long_running = "ping -n 31 127.0.0.1 >NUL";

        let sup = Supervisor::start(RestartPolicy::default(), move || {
            shell_command(long_running)
        })
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
