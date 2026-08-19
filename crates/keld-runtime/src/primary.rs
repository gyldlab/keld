//! Primary Bun role coordinator for the Unix KEL-75 T1b slice.
//!
//! This module consumes the crate-private prepared-child lease in
//! [`Supervisor`]. It does not implement a second restart loop: spawn,
//! backoff, crash-loop breaking, output capture, shutdown and reap stay in
//! the generic supervisor.

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

/// Host-minted primary role generation.
///
/// A generation is host metadata only. It is not a PID, socket path, token,
/// environment value, or wire field.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoleGeneration(u64);

impl std::fmt::Debug for RoleGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RoleGeneration(..)")
    }
}

/// Why a primary role generation was revoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryRoleRevocationCause {
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

impl From<RevocationCause> for PrimaryRoleRevocationCause {
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

/// Host-only primary role lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimaryRoleEvent {
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
        cause: PrimaryRoleRevocationCause,
    },
}

/// Configuration for the Unix primary Bun role coordinator.
#[derive(Debug, Clone)]
pub struct PrimaryRoleConfig {
    program: OsString,
    args: Vec<OsString>,
    current_dir: Option<PathBuf>,
    restart_policy: RestartPolicy,
    admission_timeout: Duration,
    #[cfg(test)]
    probe_tx: Option<Sender<ProvisionedProbe>>,
}

impl PrimaryRoleConfig {
    /// Creates a primary-role command config.
    ///
    /// The coordinator injects a fresh `KELD_APP_LINK` for every spawn
    /// attempt. The program and arguments are role declaration data, not
    /// child-supplied authority.
    #[must_use]
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            current_dir: None,
            restart_policy: RestartPolicy::default(),
            admission_timeout: DEFAULT_ADMISSION_TIMEOUT,
            #[cfg(test)]
            probe_tx: None,
        }
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
    fn with_probe(mut self, probe_tx: Sender<ProvisionedProbe>) -> Self {
        self.probe_tx = Some(probe_tx);
        self
    }
}

/// Running primary role supervisor.
#[derive(Debug)]
pub struct PrimaryRoleSupervisor {
    supervisor: Supervisor,
    events_rx: Receiver<PrimaryRoleEvent>,
}

impl PrimaryRoleSupervisor {
    /// Starts the primary role under the generic supervisor.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] if the initial generation cannot be
    /// provisioned or the first child cannot be spawned.
    pub fn start(config: PrimaryRoleConfig) -> Result<Self, RuntimeError> {
        let (events_tx, events_rx) = mpsc::channel();
        let policy = config.restart_policy;
        let preparer = PrimaryRolePreparer {
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

    /// Blocks until the next primary-role event, or `timeout` elapses.
    #[must_use]
    pub fn recv_event(&self, timeout: Duration) -> Option<PrimaryRoleEvent> {
        self.events_rx.recv_timeout(timeout).ok()
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

struct PrimaryRolePreparer {
    config: PrimaryRoleConfig,
    next_generation: u64,
    events_tx: Sender<PrimaryRoleEvent>,
}

impl ChildPreparer for PrimaryRolePreparer {
    type Lease = PrimaryGenerationLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        let generation = RoleGeneration(self.next_generation);
        self.next_generation =
            self.next_generation
                .checked_add(1)
                .ok_or_else(|| RuntimeError::Lifecycle {
                    phase: "primary role generation",
                    source: std::io::Error::other("primary role generation counter exhausted"),
                })?;
        let listener = BootstrapListener::bind().map_err(|source| RuntimeError::Lifecycle {
            phase: "primary bootstrap bind",
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
        let _ = self.events_tx.send(PrimaryRoleEvent::Provisioned {
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
            lease: PrimaryGenerationLease {
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

struct PrimaryGenerationLease {
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
    events_tx: Sender<PrimaryRoleEvent>,
}

impl GenerationLease for PrimaryGenerationLease {
    fn child_spawned(&mut self, pid: u32, attempt: u32) -> Result<(), RuntimeError> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| RuntimeError::Lifecycle {
                phase: "primary bootstrap admission",
                source: std::io::Error::other("bootstrap listener already started"),
            })?;
        let admission_tx = self
            .admission_tx
            .take()
            .ok_or_else(|| RuntimeError::Lifecycle {
                phase: "primary bootstrap admission",
                source: std::io::Error::other("admission channel already started"),
            })?;
        let deadline = Instant::now()
            .checked_add(self.admission_timeout)
            .unwrap_or_else(Instant::now);
        let observer = PrimaryBootstrapObserver {
            generation: self.generation,
            attempt: self.attempt,
            events_tx: self.events_tx.clone(),
        };
        self.admission_thread = Some(
            thread::Builder::new()
                .name("keld-runtime-primary-bootstrap".to_owned())
                .spawn(move || {
                    let result = match listener.accept_authenticated_until(deadline, &observer) {
                        Ok(BootstrapAdmission::Authenticated(stream)) => {
                            AdmissionResult::Bound(stream)
                        }
                        Ok(BootstrapAdmission::Cancelled) => AdmissionResult::Cancelled,
                        Ok(BootstrapAdmission::DeadlineElapsed) => AdmissionResult::DeadlineElapsed,
                        Err(source) => AdmissionResult::Failed(RuntimeError::Lifecycle {
                            phase: "primary bootstrap admission",
                            source,
                        }),
                    };
                    let _ = admission_tx.send(result);
                })
                .map_err(|source| RuntimeError::Lifecycle {
                    phase: "primary bootstrap admission thread",
                    source,
                })?,
        );
        let _ = self.events_tx.send(PrimaryRoleEvent::Spawned {
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
                phase: "primary bootstrap admission thread",
                source: std::io::Error::other("admission thread panicked"),
            });
        }
        if first_error.is_ok() {
            first_error = self.poll_admission();
        }
        if let Some(stream) = lock_or_recover(&self.link).take() {
            let _ = stream.shutdown_app_link();
        }
        let _ = self.events_tx.send(PrimaryRoleEvent::Revoked {
            generation: self.generation,
            attempt: self.attempt,
            cause: cause.into(),
        });
        first_error
    }
}

impl PrimaryGenerationLease {
    fn poll_admission(&mut self) -> Result<(), RuntimeError> {
        if self.admission_done {
            return Ok(());
        }
        match self.admission_rx.try_recv() {
            Ok(AdmissionResult::Bound(stream)) => {
                *lock_or_recover(&self.link) = Some(stream);
                self.admission_done = true;
                let _ = self.events_tx.send(PrimaryRoleEvent::LinkBound {
                    generation: self.generation,
                    attempt: self.attempt,
                });
                Ok(())
            }
            Ok(AdmissionResult::Cancelled) => {
                self.admission_done = true;
                Ok(())
            }
            Ok(AdmissionResult::DeadlineElapsed) => Err(RuntimeError::Lifecycle {
                phase: "primary bootstrap admission",
                source: std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "primary role did not authenticate before its generation deadline",
                ),
            }),
            Ok(AdmissionResult::Failed(error)) => Err(error),
            Err(TryRecvError::Empty) => Ok(()),
            Err(TryRecvError::Disconnected) => Err(RuntimeError::Lifecycle {
                phase: "primary bootstrap admission",
                source: std::io::Error::other("admission worker ended without a result"),
            }),
        }
    }
}

enum AdmissionResult {
    Bound(UnixStream),
    Cancelled,
    DeadlineElapsed,
    Failed(RuntimeError),
}

struct PrimaryBootstrapObserver {
    generation: RoleGeneration,
    attempt: u32,
    events_tx: Sender<PrimaryRoleEvent>,
}

impl BootstrapRejectionObserver for PrimaryBootstrapObserver {
    fn rejected(&self, rejection: BootstrapRejection) {
        let _ = self.events_tx.send(PrimaryRoleEvent::BootstrapRejected {
            generation: self.generation,
            attempt: self.attempt,
            code: rejection.code(),
        });
    }
}

#[cfg(test)]
#[derive(Debug)]
struct ProvisionedProbe {
    generation: RoleGeneration,
    app_link: String,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{BufRead, BufReader, ErrorKind, Write};
    use std::os::unix::fs::DirBuilderExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use keld_ipc::{AppLinkDeadlines, SessionToken, parse_app_link};

    use super::*;

    #[test]
    fn real_bun_primary_restart_rotates_generation_and_rejects_stale_token() {
        let fixture = PrimaryFixture::new();
        let (probe_tx, probe_rx) = mpsc::channel();
        let supervisor = PrimaryRoleSupervisor::start(
            PrimaryRoleConfig::new("bun")
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
            |event| matches!(event, PrimaryRoleEvent::Provisioned { generation, attempt: 1 } if *generation == g1_probe.generation),
            "g1 Provisioned",
        );
        assert_next_event(
            &supervisor,
            |event| matches!(event, PrimaryRoleEvent::Spawned { generation, attempt: 1, .. } if *generation == g1_probe.generation),
            "g1 Spawned",
        );
        let mut g1_control = fixture.accept_control();
        assert_ready_line(&mut g1_control, &g1_probe.app_link);
        g1_control.write_line("BIND");
        let g1 = assert_next_event(
            &supervisor,
            |event| matches!(event, PrimaryRoleEvent::LinkBound { generation, .. } if *generation == g1_probe.generation),
            "g1 LinkBound",
        );
        assert!(matches!(g1, PrimaryRoleEvent::LinkBound { .. }));
        assert_eq!(g1_control.read_line(), "BOUND");
        g1_control.write_line("CRASH");

        assert_next_event(
            &supervisor,
            |event| matches!(event, PrimaryRoleEvent::Revoked { generation, attempt: 1, cause: PrimaryRoleRevocationCause::ChildExited } if *generation == g1_probe.generation),
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
            |event| matches!(event, PrimaryRoleEvent::Provisioned { generation, attempt: 2 } if *generation == g2_probe.generation),
            "g2 Provisioned",
        );
        assert_next_event(
            &supervisor,
            |event| matches!(event, PrimaryRoleEvent::Spawned { generation, attempt: 2, .. } if *generation == g2_probe.generation),
            "g2 Spawned",
        );

        let mut g2_control = fixture.accept_control();
        assert_ready_line(&mut g2_control, &g2_probe.app_link);
        connect_with_stale_token(&g1_probe.app_link, &g2_probe.app_link);
        assert_next_event(
            &supervisor,
            |event| {
                matches!(
                    event,
                    PrimaryRoleEvent::BootstrapRejected {
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
            |event| matches!(event, PrimaryRoleEvent::LinkBound { generation, .. } if *generation == g2_probe.generation),
            "g2 LinkBound",
        );
        assert_eq!(g2_control.read_line(), "BOUND");
        supervisor.shutdown();
        assert_next_event(
            &supervisor,
            |event| matches!(event, PrimaryRoleEvent::Revoked { generation, attempt: 2, cause: PrimaryRoleRevocationCause::Shutdown } if *generation == g2_probe.generation),
            "g2 shutdown revoke",
        );
        match supervisor.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            other => panic!("shutdown should stop primary cleanly, got {other:?}"),
        }
    }

    fn assert_next_event(
        supervisor: &PrimaryRoleSupervisor,
        predicate: impl Fn(&PrimaryRoleEvent) -> bool,
        label: &str,
    ) -> PrimaryRoleEvent {
        let event = supervisor
            .recv_event(Duration::from_secs(2))
            .unwrap_or_else(|| panic!("missing event: {label}"));
        if predicate(&event) {
            event
        } else {
            panic!("expected next event {label}, got {event:?}");
        }
    }

    fn connect_with_stale_token(old_link: &str, new_link: &str) {
        let (_, old_token) = parse_app_link(old_link).expect("old link");
        let (new_endpoint, _) = parse_app_link(new_link).expect("new link");
        let stale = SessionToken::from_bytes(*old_token.as_bytes());
        let mut hostile = UnixStream::connect(new_endpoint).expect("connect stale client");
        hostile
            .set_app_link_deadlines(Some(Duration::from_millis(250)))
            .expect("deadline");
        let error = keld_ipc::link::handshake_client(&mut hostile, &stale)
            .expect_err("stale token must be rejected");
        assert!(
            error.to_string().contains("KELD-IPC-007")
                || matches!(error, keld_ipc::IpcError::Io(_)),
            "stale client must see auth failure or peer close, got {error}"
        );
    }

    fn assert_ready_line(control: &mut ControlPeer, app_link: &str) {
        assert_eq!(
            control.read_line(),
            format!("READY {app_link}"),
            "control socket is trusted test memory for app-link capture"
        );
    }

    struct PrimaryFixture {
        dir: PathBuf,
        control_path: PathBuf,
        control_listener: UnixListener,
    }

    impl PrimaryFixture {
        fn new() -> Self {
            let dir = unique_test_dir();
            fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../keld-cli/templates/hello/src/kipc.ts"),
                dir.join("kipc.ts"),
            )
            .expect("copy kipc.ts");
            fs::write(dir.join("primary-role.ts"), PRIMARY_ROLE_SCRIPT).expect("write fixture");
            let control_path = dir.join("control.sock");
            let control_listener = UnixListener::bind(&control_path).expect("bind control");
            control_listener
                .set_nonblocking(true)
                .expect("control nonblocking");
            Self {
                dir,
                control_path,
                control_listener,
            }
        }

        fn dir(&self) -> &Path {
            &self.dir
        }

        fn script_path() -> &'static Path {
            Path::new("primary-role.ts")
        }

        fn control_path(&self) -> &Path {
            &self.control_path
        }

        fn accept_control(&self) -> ControlPeer {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match self.control_listener.accept() {
                    Ok((stream, _)) => return ControlPeer::new(stream),
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "timed out accepting control socket"
                        );
                        std::thread::park_timeout(Duration::from_millis(10));
                    }
                    Err(error) => panic!("control accept failed: {error}"),
                }
            }
        }
    }

    impl Drop for PrimaryFixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.control_path);
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    struct ControlPeer {
        reader: BufReader<UnixStream>,
        writer: UnixStream,
    }

    impl ControlPeer {
        fn new(stream: UnixStream) -> Self {
            stream
                .set_app_link_deadlines(Some(Duration::from_secs(2)))
                .expect("control deadline");
            let writer = stream.try_clone().expect("clone control stream");
            Self {
                reader: BufReader::new(stream),
                writer,
            }
        }

        fn read_line(&mut self) -> String {
            let mut line = String::new();
            self.reader.read_line(&mut line).expect("read control line");
            line.trim_end_matches('\n').to_owned()
        }

        fn write_line(&mut self, line: &str) {
            self.writer
                .write_all(format!("{line}\n").as_bytes())
                .expect("write control line");
            self.writer.flush().expect("flush control line");
        }
    }

    fn unique_test_dir() -> PathBuf {
        // Keep this path short enough for macOS `sockaddr_un.sun_path`.
        // `std::env::temp_dir()` can expand to a long `/var/folders/...`
        // path, and this fixture needs room for `control.sock`.
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                duration.as_secs() ^ u64::from(duration.subsec_nanos())
            });
        let bases = [
            std::env::temp_dir(),
            PathBuf::from("/tmp"),
            PathBuf::from("/var/tmp"),
        ];
        for base in bases {
            for counter in 0..128_u32 {
                let dir = base.join(format!(
                    "kpr-{:x}-{nonce:x}-{counter:x}",
                    std::process::id()
                ));
                if dir
                    .join("control.sock")
                    .as_os_str()
                    .as_encoded_bytes()
                    .len()
                    >= 100
                {
                    continue;
                }
                match fs::DirBuilder::new().mode(0o700).create(&dir) {
                    Ok(()) => return dir,
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("create test dir {dir:?}: {error}"),
                }
            }
        }
        panic!("could not allocate primary test dir");
    }

    #[test]
    fn bun_is_available_for_primary_fixture() {
        let output = Command::new("bun")
            .arg("--version")
            .output()
            .expect("spawn bun --version");
        assert!(output.status.success(), "bun --version must succeed");
    }

    const PRIMARY_ROLE_SCRIPT: &str = r#"
import { AppLinkSession } from "./kipc";

const controlPath = process.argv[2];
const appLink = process.env.KELD_APP_LINK;
if (!controlPath || !appLink) {
  console.error("missing control path or KELD_APP_LINK");
  process.exit(2);
}

const decoder = new TextDecoder();
const encoder = new TextEncoder();
let buffer = "";
const waiters = [];

function drainLines() {
  while (true) {
    const index = buffer.indexOf("\n");
    if (index < 0 || waiters.length === 0) return;
    const line = buffer.slice(0, index);
    buffer = buffer.slice(index + 1);
    waiters.shift()(line);
  }
}

function readLine() {
  return new Promise((resolve) => {
    const index = buffer.indexOf("\n");
    if (index >= 0) {
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      resolve(line);
      return;
    }
    waiters.push(resolve);
  });
}

const control = await Bun.connect({
  unix: controlPath,
  socket: {
    binaryType: "uint8array",
    data(_socket, data) {
      buffer += decoder.decode(data, { stream: true });
      drainLines();
    },
    close() {
      process.exit(0);
    },
    error(_socket, err) {
      console.error(err.message);
      process.exit(3);
    },
    connectError(_socket, err) {
      console.error(err.message);
      process.exit(3);
    },
  },
});

async function writeLine(line) {
  const payload = encoder.encode(`${line}\n`);
  let offset = 0;
  while (offset < payload.length) {
    const written = control.write(payload.subarray(offset));
    if (written < 0) throw new Error("control socket closed");
    if (written === 0) throw new Error("control socket backpressure");
    offset += written;
  }
}

await writeLine(`READY ${appLink}`);
const bind = await readLine();
if (bind !== "BIND") {
  console.error(`unexpected command before bind: ${bind}`);
  process.exit(4);
}
const session = await AppLinkSession.connect(appLink);
await writeLine("BOUND");
const command = await readLine();
if (command === "CRASH") {
  session.close();
  process.exit(17);
}
if (command === "STOP") {
  session.close();
  process.exit(0);
}
await new Promise(() => {});
"#;
}
