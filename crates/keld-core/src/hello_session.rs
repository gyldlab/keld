//! Host-owned concurrent hello app-link (KEL-30).
//!
//! Owns the authenticated echo listener and the supervised Bun child for the
//! same lifetime as the hello window. Callers open the window while this
//! session is live, then drop/shutdown after the window returns
//! ([`crate::run_hello_window_html`] uses tao `run_return` so Drop runs).

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use keld_runtime::{CrashLedger, RestartPolicy, Supervisor, SupervisorEvent, SupervisorOutcome};

use crate::echo_link::EchoServer;

/// Errors starting or observing a host-owned hello app-link session.
#[derive(Debug)]
pub enum HelloSessionError {
    /// Listener bind or worker failure.
    Io(io::Error),
    /// Supervisor spawn / crash-loop / wait failure.
    Runtime(String),
    /// Timed out waiting for an observable ready condition.
    Timeout {
        /// What the caller was waiting for.
        waiting_for: &'static str,
    },
    /// Supervision reached a terminal failure while the host owned this
    /// session — the `keld dev` window phase (KEL-105). The app process is
    /// gone; any window the caller opened is still on screen, but its app
    /// link is dead.
    WindowPhase {
        /// Terminal supervisor outcome, carrying its own `KELD-RUNTIME-*`
        /// code and the captured stderr tail. Never re-derived here.
        cause: String,
    },
}

impl std::fmt::Display for HelloSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(
                f,
                "KELD-CORE-030: host-owned app-link I/O error — {e}. \
                 Check that the temp/session directory is writable."
            ),
            Self::Runtime(msg) => write!(
                f,
                "KELD-CORE-031: host-owned hello session failed — {msg}. \
                 Re-run `keld doctor` and fix the reported checks."
            ),
            Self::Timeout { waiting_for } => write!(
                f,
                "KELD-CORE-032: timed out waiting for {waiting_for}. \
                 Confirm Bun is on PATH and the project entry speaks kipc."
            ),
            Self::WindowPhase { cause } => write!(
                f,
                "KELD-CORE-033: the supervised app process stopped while the host owned \
                 the window. Fix the cause named by the supervisor outcome below, then \
                 re-run `keld dev`. Supervisor outcome: {cause}"
            ),
        }
    }
}

impl std::error::Error for HelloSessionError {}

impl From<io::Error> for HelloSessionError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Authenticated echo listener + supervised Bun child, co-owned by the host.
///
/// Drop (or [`Self::shutdown`]) stops the listener and reaps Bun. The shipping
/// `keld dev` path keeps this live across the hello window.
#[derive(Debug)]
pub struct HostOwnedHelloSession {
    server: Option<EchoServer>,
    supervisor: Option<Supervisor>,
    link: String,
    /// Whether a ready marker has been observed at all. Separate from the
    /// count below so the baseline is written exactly once.
    ready_recorded: AtomicBool,
    /// Crashes the supervisor had already recovered from when this session
    /// *first* reached its ready marker. Crashes at or below this count are the
    /// supervisor doing its job (KEL-70 AC1/AC3); crashes above it happened
    /// after the app was live, which is what `keld dev` must not call success
    /// (KEL-105). Stays 0 until a ready marker is observed, so a session that
    /// never came up treats any crash as fatal.
    recovered_crashes: AtomicU32,
}

impl HostOwnedHelloSession {
    /// Binds the echo listener, mints `KELD_APP_LINK`, and starts Bun under
    /// [`Supervisor`] with `policy`.
    ///
    /// Does not wait for echo readiness — call [`Self::wait_until_output_contains`]
    /// before opening a window when the product contract requires a live HELLO.
    ///
    /// # Errors
    ///
    /// Returns [`HelloSessionError`] if the listener cannot bind or Bun cannot
    /// be spawned.
    pub fn start(
        project_root: &Path,
        bun_main: PathBuf,
        policy: RestartPolicy,
    ) -> Result<Self, HelloSessionError> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server = EchoServer::start(&ready_tx)?;
        ready_rx
            .recv()
            .map_err(|_| HelloSessionError::Runtime("echo server failed to start".to_owned()))?;
        let link = server.link();
        let link_for_child = link.clone();
        let project_root = project_root.to_path_buf();

        let supervisor = Supervisor::start(policy, move || {
            let mut cmd = Command::new("bun");
            cmd.arg("run")
                .arg(&bun_main)
                .current_dir(&project_root)
                .env("KELD_APP_LINK", &link_for_child);
            cmd
        })
        .map_err(|e| HelloSessionError::Runtime(e.to_string()))?;

        Ok(Self {
            server: Some(server),
            supervisor: Some(supervisor),
            link,
            ready_recorded: AtomicBool::new(false),
            recovered_crashes: AtomicU32::new(0),
        })
    }

    /// App-link value for clients (`<endpoint>#<64 hex chars>`).
    #[must_use]
    pub fn link(&self) -> &str {
        &self.link
    }

    /// OS pid of the currently supervised Bun child, if live.
    #[must_use]
    pub fn current_pid(&self) -> Option<u32> {
        self.supervisor.as_ref().and_then(Supervisor::current_pid)
    }

    /// Snapshot of captured Bun stdout/stderr so far.
    #[must_use]
    pub fn output(&self) -> keld_runtime::CapturedOutput {
        self.supervisor
            .as_ref()
            .map_or_else(keld_runtime::CapturedOutput::default, Supervisor::output)
    }

    /// Awaits Bun stdout containing `needle` without sleep-sync.
    ///
    /// Drains supervisor events so a crash-loop surfaces as
    /// [`HelloSessionError::Runtime`] instead of a hang.
    ///
    /// # Errors
    ///
    /// Returns [`HelloSessionError::Timeout`] when `timeout` elapses, or
    /// [`HelloSessionError::Runtime`] if supervision ends before the marker.
    pub fn wait_until_output_contains(
        &self,
        needle: &str,
        timeout: Duration,
    ) -> Result<(), HelloSessionError> {
        let Some(supervisor) = self.supervisor.as_ref() else {
            return Err(HelloSessionError::Runtime(
                "hello session already shut down".to_owned(),
            ));
        };
        let deadline = Instant::now() + timeout;
        loop {
            if supervisor.output().stdout.contains(needle) {
                self.mark_ready(supervisor);
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(HelloSessionError::Timeout {
                    waiting_for: "Bun stdout ready marker",
                });
            }
            match supervisor.recv_event(remaining.min(Duration::from_millis(50))) {
                Some(SupervisorEvent::CrashLoopTripped | SupervisorEvent::Failed { .. }) => {
                    // Both events mean the supervisor worker is terminating, so
                    // reading its recorded outcome here cannot block. The
                    // diagnostic and its `KELD-RUNTIME-*` code are rendered by
                    // `keld-runtime`, which owns them: re-deriving the text here
                    // is what let the two drift (KEL-105 review).
                    let cause = match supervisor.wait_for_outcome() {
                        SupervisorOutcome::CrashLoop(error) | SupervisorOutcome::Failed(error) => {
                            error.to_string()
                        }
                        SupervisorOutcome::Stopped => {
                            "supervision stopped without a recorded diagnostic".to_owned()
                        }
                    };
                    return Err(HelloSessionError::Runtime(format!(
                        "{cause} (observed before the Bun ready marker)"
                    )));
                }
                Some(SupervisorEvent::Stopped) => {
                    if supervisor.output().stdout.contains(needle) {
                        self.mark_ready(supervisor);
                        return Ok(());
                    }
                    return Err(HelloSessionError::Runtime(
                        "Bun exited before emitting the ready marker".to_owned(),
                    ));
                }
                Some(_) | None => {}
            }
        }
    }

    /// Stops the listener and reaps Bun, reporting how supervision ended.
    /// Idempotent after the first call.
    ///
    /// # Errors
    ///
    /// Returns [`HelloSessionError::WindowPhase`] (`KELD-CORE-033`) when the
    /// supervised app process died after this session was live:
    ///
    /// - a crash the supervisor did not recover from before teardown — the
    ///   dominant `keld dev` case, and the one that never trips the breaker:
    ///   the default policy needs three crashes in 30s, while the restarted
    ///   generation cannot re-enter the one-session listener to produce them;
    /// - a tripped crash-loop breaker;
    /// - a generation that failed to provision.
    ///
    /// A crash the supervisor recovered from *before* the ready marker is not
    /// a failure (KEL-70 AC1/AC3) and is not reported here.
    ///
    /// `keld dev` blocks in the window event loop across exactly this interval
    /// and observes nothing until here, so discarding this verdict is what
    /// made a dead app process exit 0 (KEL-105).
    pub fn shutdown(mut self) -> Result<(), HelloSessionError> {
        match self.finish() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Single owned teardown: stop supervision, read the verdict the
    /// supervisor already recorded, then stop the listener.
    ///
    /// Both halves of the verdict are drain-independent by construction.
    /// `keld-runtime` stores the terminal [`keld_runtime::RuntimeError`]
    /// *before* it sends the matching event, and
    /// [`Supervisor::wait_for_outcome`] falls back to that record when the
    /// event channel is empty or already disconnected; the crash ledger is
    /// durable state that is never consumed. A caller that drained events
    /// earlier — [`Self::wait_until_output_contains`], which `run_dev` runs
    /// before the window — therefore cannot erase the failure.
    ///
    /// Calling [`Supervisor::shutdown`] *first* is what bounds the wait: it
    /// makes a healthy child be killed and reported `Stopped` instead of the
    /// wait blocking for a process that never exits. The wait then returns
    /// just before the supervisor thread ends, so the join in `Supervisor`'s
    /// own `Drop` — which this path already performed — stays the only
    /// blocking step.
    fn finish(&mut self) -> Option<HelloSessionError> {
        let verdict = self.supervisor.take().map(|supervisor| {
            supervisor.shutdown();
            let outcome = supervisor.wait_for_outcome();
            // Read after the wait: the worker records the crash before it
            // reports the exit, so this snapshot cannot miss one it caused.
            let ledger = supervisor.crash_ledger();
            drop(supervisor);
            (outcome, ledger)
        });
        if let Some(server) = self.server.take() {
            // Interrupt is expected when Bun was killed mid-accept or mid-session.
            let _ = server.shutdown();
        }
        let (outcome, ledger) = verdict?;
        window_phase_error(
            outcome,
            ledger,
            self.recovered_crashes.load(Ordering::SeqCst),
        )
    }

    /// Records the crashes already recovered from at the moment this session
    /// *first* reached its ready marker, so [`Self::finish`] can tell a
    /// recovered crash (KEL-70 AC1/AC3) from one that killed a live app
    /// (KEL-105).
    ///
    /// First transition only. [`Self::wait_until_output_contains`] matches
    /// against cumulative stdout, so a later call for a marker the app already
    /// printed returns immediately — re-baselining there would absorb a crash
    /// that happened *after* the app was live and report it as recovered.
    fn mark_ready(&self, supervisor: &Supervisor) {
        if !self.ready_recorded.swap(true, Ordering::SeqCst) {
            self.recovered_crashes
                .store(supervisor.crash_ledger().count, Ordering::SeqCst);
        }
    }
}

/// Decides whether a finished session died on the host's watch.
///
/// Pure over the two facts `keld-runtime` publishes — how supervision ended,
/// and how many crashes it observed — so every arm is falsifiable without a
/// process fixture. `recovered` is the crash count at the last ready marker.
///
/// A terminal supervision failure is always fatal. `Stopped` is not decided by
/// the outcome at all: supervision stops cleanly whether or not the app
/// survived, so the ledger is what separates the two (KEL-105).
fn window_phase_error(
    outcome: SupervisorOutcome,
    ledger: CrashLedger,
    recovered: u32,
) -> Option<HelloSessionError> {
    let cause = match outcome {
        SupervisorOutcome::CrashLoop(cause) | SupervisorOutcome::Failed(cause) => cause,
        SupervisorOutcome::Stopped => ledger.last.filter(|_| ledger.count > recovered)?,
    };
    Some(HelloSessionError::WindowPhase {
        cause: cause.to_string(),
    })
}

impl Drop for HostOwnedHelloSession {
    fn drop(&mut self) {
        // Drop has no channel to report on; `shutdown()` is the surfacing
        // path. Teardown itself is identical either way (KEL-105).
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::window_phase_error;
    use keld_runtime::{CrashLedger, RuntimeError, SupervisorOutcome};

    fn crash(pid: u32, exit_code: Option<i32>) -> RuntimeError {
        RuntimeError::ChildCrashed {
            pid,
            exit_code,
            stderr_tail: "boom".to_owned(),
        }
    }

    fn ledger(count: u32) -> CrashLedger {
        CrashLedger {
            count,
            last: (count > 0).then(|| crash(4242, Some(3))),
        }
    }

    #[test]
    fn clean_stop_with_no_crash_is_success() {
        assert!(
            window_phase_error(SupervisorOutcome::Stopped, ledger(0), 0).is_none(),
            "the shipping success path must stay exit 0"
        );
    }

    #[test]
    fn crash_after_the_ready_marker_fails_the_session() {
        // KEL-105's dominant path: one crash, breaker never tripped, so the
        // outcome is a clean `Stopped` and only the ledger dissents.
        let err = window_phase_error(SupervisorOutcome::Stopped, ledger(1), 0)
            .expect("a crash after the app was live must not report success");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CORE-033"), "{msg}");
        assert!(msg.contains("KELD-RUNTIME-012"), "{msg}");
        assert!(
            msg.contains("boom"),
            "the captured stderr must reach the user: {msg}"
        );
    }

    #[test]
    fn crash_the_supervisor_recovered_from_is_not_a_failure() {
        // KEL-70 AC1/AC3: a crash before the ready marker was recovered from,
        // and `run_dev_echo` must still succeed.
        assert!(
            window_phase_error(SupervisorOutcome::Stopped, ledger(1), 1).is_none(),
            "a recovered crash is the supervisor working, not a failure"
        );
    }

    #[test]
    fn a_further_crash_after_recovery_still_fails() {
        assert!(
            window_phase_error(SupervisorOutcome::Stopped, ledger(2), 1).is_some(),
            "only crashes up to the ready marker are forgiven"
        );
    }

    #[test]
    fn a_tripped_breaker_is_fatal_whatever_the_ledger_says() {
        let err = window_phase_error(
            SupervisorOutcome::CrashLoop(RuntimeError::CrashLoop {
                crashes: 3,
                window_secs: 30,
                last_exit_code: Some(3),
                stderr_tail: String::new(),
            }),
            ledger(3),
            3,
        )
        .expect("a tripped breaker is fatal even if every crash predates ready");
        assert!(err.to_string().contains("KELD-RUNTIME-002"), "{err}");
    }

    #[test]
    fn a_generation_that_failed_to_provision_is_fatal() {
        let err = window_phase_error(
            SupervisorOutcome::Failed(RuntimeError::Lifecycle {
                phase: "provision",
                source: std::io::Error::other("no bootstrap"),
            }),
            ledger(0),
            0,
        )
        .expect("a generation that never provisioned is not a success");
        assert!(err.to_string().contains("KELD-RUNTIME-003"), "{err}");
    }
}
