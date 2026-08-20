//! Unix authenticated Bun role coordinator shared by KEL-75 T1b/T2.
//!
//! One coordinator instance is one lifecycle owner (`primary` or `app-bound`).
//! It consumes the crate-private prepared-child lease in [`Supervisor`] and
//! does not implement a second restart loop: spawn, backoff, crash-loop
//! breaking, output capture, shutdown and reap stay in the generic supervisor.

use std::ffi::OsString;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use keld_ipc::{
    AppLinkDeadlines, BootstrapAdmission, BootstrapCancellation, BootstrapListener,
    BootstrapRejection, BootstrapRejectionObserver,
};

use crate::{
    CapturedOutput, ChildPreparer, GenerationLease, PreparedChild, RestartPolicy, RevocationCause,
    RuntimeError, Supervisor, SupervisorOutcome, lock_or_recover,
};

const DEFAULT_ADMISSION_TIMEOUT: Duration = Duration::from_secs(5);

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

impl std::fmt::Debug for RoleGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RoleGeneration(..)")
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

/// Configuration for one Unix authenticated Bun role coordinator.
#[derive(Debug, Clone)]
pub struct RoleConfig {
    owner: RoleOwner,
    program: OsString,
    args: Vec<OsString>,
    current_dir: Option<PathBuf>,
    restart_policy: RestartPolicy,
    admission_timeout: Duration,
    #[cfg(test)]
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
    pub fn app_bound(program: impl Into<OsString>) -> Self {
        Self::for_owner(RoleOwner::AppBound, program)
    }

    fn for_owner(owner: RoleOwner, program: impl Into<OsString>) -> Self {
        Self {
            owner,
            program: program.into(),
            args: Vec::new(),
            current_dir: None,
            restart_policy: RestartPolicy::default(),
            admission_timeout: DEFAULT_ADMISSION_TIMEOUT,
            #[cfg(test)]
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
}

impl RoleSupervisor {
    /// Starts the role under the generic supervisor.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] if the initial generation cannot be
    /// provisioned or the first child cannot be spawned.
    pub fn start(config: RoleConfig) -> Result<Self, RuntimeError> {
        let (events_tx, events_rx) = mpsc::channel();
        let policy = config.restart_policy;
        let preparer = RolePreparer {
            config,
            next_generation: 1,
            events_tx,
        };
        let supervisor = Supervisor::start_prepared(policy, preparer)?;
        Ok(Self {
            supervisor,
            events_rx,
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

    /// Stops the child, if still live.
    pub fn shutdown(&self) {
        self.supervisor.shutdown();
    }

    /// Waits for the generic supervisor's terminal outcome.
    #[must_use]
    pub fn wait_for_outcome(&self) -> SupervisorOutcome {
        self.supervisor.wait_for_outcome()
    }

    /// Snapshot of captured child stdout/stderr.
    #[must_use]
    pub fn output(&self) -> CapturedOutput {
        self.supervisor.output()
    }
}

struct RolePreparer {
    config: RoleConfig,
    next_generation: u64,
    events_tx: Sender<RoleEvent>,
}

impl ChildPreparer for RolePreparer {
    type Lease = RoleGenerationLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        let owner = self.config.owner;
        let generation = RoleGeneration(self.next_generation);
        self.next_generation =
            self.next_generation
                .checked_add(1)
                .ok_or_else(|| RuntimeError::Lifecycle {
                    phase: owner.generation_phase(),
                    source: std::io::Error::other(format!(
                        "{} role generation counter exhausted",
                        owner.as_str()
                    )),
                })?;
        let listener = BootstrapListener::bind().map_err(|source| RuntimeError::Lifecycle {
            phase: owner.bootstrap_bind_phase(),
            source,
        })?;
        let cancellation = listener.cancellation();
        let app_link = listener.app_link();
        let mut command = Command::new(&self.config.program);
        command.args(&self.config.args);
        if let Some(current_dir) = &self.config.current_dir {
            command.current_dir(current_dir);
        }
        command.env("KELD_APP_LINK", &app_link);
        let (admission_tx, admission_rx) = mpsc::channel();
        let link = Arc::new(Mutex::new(None));
        let _ = self.events_tx.send(RoleEvent::Provisioned {
            generation,
            attempt,
        });
        #[cfg(test)]
        if let Some(probe_tx) = &self.config.probe_tx {
            let _ = probe_tx.send(ProvisionedProbe {
                generation,
                app_link,
            });
        }
        Ok(PreparedChild {
            command,
            lease: RoleGenerationLease {
                owner,
                generation,
                attempt,
                admission_timeout: self.config.admission_timeout,
                listener: Some(listener),
                cancellation,
                admission_tx: Some(admission_tx),
                admission_rx,
                admission_thread: None,
                admission_done: false,
                link,
                events_tx: self.events_tx.clone(),
            },
        })
    }
}

struct RoleGenerationLease {
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
    link: Arc<Mutex<Option<UnixStream>>>,
    events_tx: Sender<RoleEvent>,
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
                *lock_or_recover(&self.link) = Some(stream);
                self.admission_done = true;
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
}

enum AdmissionResult {
    Bound(UnixStream),
    Cancelled,
    DeadlineElapsed,
    Failed(RuntimeError),
}

struct RoleBootstrapObserver {
    generation: RoleGeneration,
    attempt: u32,
    events_tx: Sender<RoleEvent>,
}

impl BootstrapRejectionObserver for RoleBootstrapObserver {
    fn rejected(&self, rejection: BootstrapRejection) {
        let _ = self.events_tx.send(RoleEvent::BootstrapRejected {
            generation: self.generation,
            attempt: self.attempt,
            code: rejection.code(),
        });
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct ProvisionedProbe {
    pub(crate) generation: RoleGeneration,
    pub(crate) app_link: String,
}

#[cfg(test)]
#[path = "unix_role_fixture.rs"]
pub(crate) mod fixture;

#[cfg(test)]
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
