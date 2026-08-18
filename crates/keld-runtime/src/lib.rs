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

/// A cheap, `Send + Sync` handle for polling a [`Supervisor`]'s captured
/// output from a thread that does not own the `Supervisor` itself.
///
/// `Supervisor` holds an `mpsc::Receiver`, which is `Send` but not `Sync`,
/// so `&Supervisor` cannot be shared across a thread boundary (e.g. from a
/// caller that wants to tail live output on a background thread while the
/// main thread blocks elsewhere, such as a windowing event loop). This
/// handle carries only the `Arc<Mutex<CapturedOutput>>` the capture threads
/// already write into, so it is safe to clone and move anywhere.
#[derive(Debug, Clone)]
pub struct OutputHandle(Arc<Mutex<CapturedOutput>>);

impl OutputHandle {
    /// Snapshot of stdout/stderr captured so far, across every spawn attempt.
    #[must_use]
    pub fn snapshot(&self) -> CapturedOutput {
        lock_or_recover(&self.0).clone()
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
        let child = spawn_piped(&mut command_factory).map_err(RuntimeError::Spawn)?;

        let (events_tx, events_rx) = mpsc::channel();
        let output = Arc::new(Mutex::new(CapturedOutput::default()));
        let current_pid = Arc::new(Mutex::new(None));
        let crash_loop_error = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread = {
            let output = Arc::clone(&output);
            let current_pid = Arc::clone(&current_pid);
            let crash_loop_error = Arc::clone(&crash_loop_error);
            let shutdown = Arc::clone(&shutdown);
            thread::Builder::new()
                .name("keld-runtime-supervisor".to_owned())
                .spawn(move || {
                    supervise(
                        policy,
                        command_factory,
                        child,
                        &events_tx,
                        &output,
                        &current_pid,
                        &crash_loop_error,
                        &shutdown,
                    );
                })
                .map_err(RuntimeError::Spawn)?
        };

        Ok(Self {
            events_rx,
            output,
            current_pid,
            crash_loop_error,
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
                    let err = lock_or_recover(&self.crash_loop_error).take();
                    return SupervisorOutcome::CrashLoop(err.unwrap_or(RuntimeError::CrashLoop {
                        crashes: 0,
                        window_secs: 0,
                        last_exit_code: None,
                        stderr_tail: String::new(),
                    }));
                }
                Ok(SupervisorEvent::Stopped) | Err(_) => return SupervisorOutcome::Stopped,
                Ok(_) => {}
            }
        }
    }

    /// Snapshot of stdout/stderr captured so far, across every spawn attempt.
    #[must_use]
    pub fn output(&self) -> CapturedOutput {
        lock_or_recover(&self.output).clone()
    }

    /// A cheap, thread-safe handle for polling captured output from a
    /// thread that does not own this `Supervisor` (see [`OutputHandle`]).
    #[must_use]
    pub fn output_handle(&self) -> OutputHandle {
        OutputHandle(Arc::clone(&self.output))
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

fn spawn_piped<F>(command_factory: &mut F) -> std::io::Result<Child>
where
    F: FnMut() -> Command,
{
    let mut cmd = command_factory();
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.spawn()
}

#[allow(clippy::too_many_arguments)] // internal worker; grouping into a struct would not reduce coupling
fn supervise<F>(
    policy: RestartPolicy,
    mut command_factory: F,
    mut child: Child,
    events_tx: &Sender<SupervisorEvent>,
    output: &Arc<Mutex<CapturedOutput>>,
    current_pid: &Arc<Mutex<Option<u32>>>,
    crash_loop_error: &Arc<Mutex<Option<RuntimeError>>>,
    shutdown: &Arc<AtomicBool>,
) where
    F: FnMut() -> Command,
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

        let killed_for_shutdown = wait_or_shutdown(&mut child, shutdown);

        if let Some(t) = stdout_thread {
            let _ = t.join();
        }
        if let Some(t) = stderr_thread {
            let _ = t.join();
        }
        *lock_or_recover(current_pid) = None;

        if killed_for_shutdown {
            let _ = events_tx.send(SupervisorEvent::Stopped);
            return;
        }

        let code = child_exit_code(&mut child);
        let _ = events_tx.send(SupervisorEvent::Exited { pid, code });

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
        if let Ok(c) = spawn_piped(&mut command_factory) {
            child = c;
        } else {
            let _ = events_tx.send(SupervisorEvent::RespawnFailed);
            return;
        }
    }
}

/// Polls `child` for exit, honoring `shutdown`. Returns `true` if it killed
/// the child because `shutdown` was requested (caller must not treat this as
/// a crash), `false` if the child exited on its own.
fn wait_or_shutdown(child: &mut Child, shutdown: &Arc<AtomicBool>) -> bool {
    loop {
        match child.try_wait() {
            Ok(None) => {
                if shutdown.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return true;
                }
                thread::sleep(POLL_INTERVAL);
            }
            Ok(Some(_)) | Err(_) => return false,
        }
    }
}

/// Exit code of a child already confirmed exited by `wait_or_shutdown`.
/// `try_wait` on an already-reaped child returns the cached status.
fn child_exit_code(child: &mut Child) -> Option<i32> {
    match child.try_wait() {
        Ok(Some(status)) => status.code(),
        _ => None,
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
    fn output_handle_reflects_captured_output_from_another_thread() {
        let sup = Supervisor::start(RestartPolicy::default(), || {
            shell_command(&joined_steps(&["echo handle-marker", "exit 0"]))
        })
        .expect("spawn must succeed");

        // Take the handle before the child even exits, proving it is a live
        // view (an Arc, not a one-time snapshot) — not just Send-able.
        let handle = sup.output_handle();

        match sup.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            SupervisorOutcome::CrashLoop(e) => panic!("must exit 0: {e}"),
        }

        // Move the handle to a fresh thread — this only compiles if
        // `OutputHandle: Send`, unlike `&Supervisor` (its `mpsc::Receiver`
        // field is `!Sync`).
        let snapshot = thread::spawn(move || handle.snapshot())
            .join()
            .expect("thread join");
        assert!(snapshot.stdout.contains("handle-marker"), "{snapshot:?}");
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
