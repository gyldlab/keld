//! Host-owned concurrent hello app-link (KEL-30).
//!
//! Owns the authenticated echo listener and the supervised Bun child for the
//! same lifetime as the hello window. Callers open the window while this
//! session is live, then drop/shutdown after the window returns
//! ([`crate::run_hello_window_html`] uses tao `run_return` so Drop runs).

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use keld_runtime::{RestartPolicy, Supervisor, SupervisorEvent};

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
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(HelloSessionError::Timeout {
                    waiting_for: "Bun stdout ready marker",
                });
            }
            match supervisor.recv_event(remaining.min(Duration::from_millis(50))) {
                Some(SupervisorEvent::CrashLoopTripped) => {
                    return Err(HelloSessionError::Runtime(format!(
                        "KELD-RUNTIME-002: crash-loop breaker tripped before ready marker; stderr={}",
                        supervisor.output().stderr
                    )));
                }
                Some(SupervisorEvent::Failed { .. }) => {
                    return Err(HelloSessionError::Runtime(format!(
                        "supervisor failed before ready marker; stderr={}",
                        supervisor.output().stderr
                    )));
                }
                Some(SupervisorEvent::Stopped) => {
                    if supervisor.output().stdout.contains(needle) {
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

    /// Stops the listener and reaps Bun. Idempotent after the first call.
    ///
    /// # Errors
    ///
    /// Returns [`HelloSessionError`] if teardown fails fatally (currently
    /// always `Ok` after best-effort kill + listener interrupt).
    pub fn shutdown(mut self) -> Result<(), HelloSessionError> {
        self.finish();
        Ok(())
    }

    fn finish(&mut self) {
        if let Some(supervisor) = self.supervisor.take() {
            supervisor.shutdown();
            drop(supervisor);
        }
        if let Some(server) = self.server.take() {
            // Interrupt is expected when Bun was killed mid-accept or mid-session.
            let _ = server.shutdown();
        }
    }
}

impl Drop for HostOwnedHelloSession {
    fn drop(&mut self) {
        self.finish();
    }
}
