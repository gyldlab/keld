//! Real-Windows KEL-75/T8 primary-generation contract.

#![cfg(windows)]
#![allow(clippy::expect_used, clippy::panic)] // Integration-test failures are assertion oracles.

use std::fs;
use std::io::{BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use keld_ipc::link::{AppLinkDeadlines, handshake_client};
use keld_ipc::{
    SessionToken, WindowsNamedPipeBootstrapStream, parse_app_link,
    serve_echo_requests_until_stopped,
};
use keld_runtime::primary::{
    BoundPrimaryGeneration, PrimaryRoleConfig, PrimaryRoleEvent, PrimaryRoleRevocationCause,
    PrimaryRoleSupervisor,
};
use keld_runtime::{RestartPolicy, SupervisorOutcome};

const EVENT_DEADLINE: Duration = Duration::from_secs(10);
const CONTROL_POLL: Duration = Duration::from_millis(200);
const CONTROL_DEADLINE: Duration = Duration::from_secs(30);

#[test]
fn windows_primary_restart_rotates_authenticated_generation_and_rejects_stale_authority() {
    let fixture = PrimaryFixture::new();
    let supervisor = start_primary(&fixture);

    let g1 = expect_provisioned(&supervisor, 1);
    expect_spawned(&supervisor, g1, 1);
    let mut g1_control = fixture.accept_control();
    let g1_link = g1_control.read_ready();
    let silent_g1 = connect_silent(&g1_link);
    g1_control.write_line("BIND");
    let g1_bound = supervisor
        .recv_bound_generation(EVENT_DEADLINE)
        .expect("g1 authenticated stream");
    assert_eq!(g1_bound.generation(), g1);
    assert_eq!(g1_bound.attempt(), 1);
    expect_bootstrap_rejected(&supervisor, g1, 1, "KELD-IPC-006");
    expect_link_bound(&supervisor, g1, 1);
    drop(silent_g1);
    assert_eq!(g1_control.read_line(), "BOUND");
    let g1_echo = EchoWorker::start(g1_bound);
    g1_control.write_line("ECHO generation-one 11");
    assert_eq!(g1_control.read_line(), "ECHO generation-one 11");

    g1_control.write_line("CRASH");
    g1_echo.finish();
    expect_revoked(&supervisor, g1, 1, PrimaryRoleRevocationCause::ChildExited);
    assert_retired_locator_closed(&g1_link);

    let g2 = expect_provisioned(&supervisor, 2);
    assert_ne!(g2, g1, "successor generation must rotate");
    expect_spawned(&supervisor, g2, 2);
    let mut g2_control = fixture.accept_control();
    let g2_link = g2_control.read_ready();
    let (g1_endpoint, g1_token) = parse_app_link(&g1_link).expect("g1 link");
    let (g2_endpoint, g2_token) = parse_app_link(&g2_link).expect("g2 link");
    assert_ne!(g2_endpoint, g1_endpoint, "successor endpoint must rotate");
    assert_ne!(g2_token, g1_token, "successor token must rotate");

    connect_with_token_from(&g1_link, &g2_link);
    expect_bootstrap_rejected(&supervisor, g2, 2, "KELD-IPC-007");

    g2_control.write_line("BIND");
    let g2_bound = supervisor
        .recv_bound_generation(EVENT_DEADLINE)
        .expect("g2 authenticated stream after hostile peer");
    assert_eq!(g2_bound.generation(), g2);
    assert_eq!(g2_bound.attempt(), 2);
    expect_link_bound(&supervisor, g2, 2);
    assert_eq!(g2_control.read_line(), "BOUND");
    let g2_echo = EchoWorker::start(g2_bound);
    g2_control.write_line("ECHO generation-two 22");
    assert_eq!(g2_control.read_line(), "ECHO generation-two 22");

    g2_echo.stop();
    supervisor.shutdown();
    expect_revoked(&supervisor, g2, 2, PrimaryRoleRevocationCause::Shutdown);
    assert!(matches!(
        supervisor.wait_for_outcome(),
        SupervisorOutcome::Stopped
    ));
    g2_control.expect_closed();
    g2_echo.join();

    run_clean_successor_cycle(&fixture);
}

fn run_clean_successor_cycle(fixture: &PrimaryFixture) {
    let supervisor = start_primary(fixture);
    let generation = expect_provisioned(&supervisor, 1);
    expect_spawned(&supervisor, generation, 1);
    let mut control = fixture.accept_control();
    let _link = control.read_ready();
    control.write_line("BIND");
    let bound = supervisor
        .recv_bound_generation(EVENT_DEADLINE)
        .expect("fresh-cycle authenticated stream");
    expect_link_bound(&supervisor, generation, 1);
    assert_eq!(control.read_line(), "BOUND");
    let echo = EchoWorker::start(bound);
    control.write_line("ECHO clean-cycle 33");
    assert_eq!(control.read_line(), "ECHO clean-cycle 33");
    echo.stop();
    supervisor.shutdown();
    expect_revoked(
        &supervisor,
        generation,
        1,
        PrimaryRoleRevocationCause::Shutdown,
    );
    assert!(matches!(
        supervisor.wait_for_outcome(),
        SupervisorOutcome::Stopped
    ));
    control.expect_closed();
    echo.join();
}

fn start_primary(fixture: &PrimaryFixture) -> PrimaryRoleSupervisor {
    PrimaryRoleSupervisor::start_with_bound_generations(
        PrimaryRoleConfig::new("bun")
            .arg(PrimaryFixture::script_path())
            .arg(fixture.control_port().to_string())
            .current_dir(fixture.dir())
            .restart_policy(RestartPolicy {
                max_crashes: 3,
                window_secs: 30,
            })
            .admission_timeout(Duration::from_secs(10)),
    )
    .expect("Windows primary role must spawn under Bun")
}

fn expect_provisioned(
    supervisor: &PrimaryRoleSupervisor,
    attempt: u32,
) -> keld_runtime::primary::RoleGeneration {
    match supervisor
        .recv_event(EVENT_DEADLINE)
        .expect("Provisioned event")
    {
        PrimaryRoleEvent::Provisioned {
            generation,
            attempt: actual,
        } => {
            assert_eq!(actual, attempt);
            generation
        }
        other => panic!("expected Provisioned({attempt}), got {other:?}"),
    }
}

fn expect_spawned(
    supervisor: &PrimaryRoleSupervisor,
    generation: keld_runtime::primary::RoleGeneration,
    attempt: u32,
) {
    match supervisor
        .recv_event(EVENT_DEADLINE)
        .expect("Spawned event")
    {
        PrimaryRoleEvent::Spawned {
            generation: actual,
            pid,
            attempt: actual_attempt,
        } => {
            assert_eq!(actual, generation);
            assert_eq!(actual_attempt, attempt);
            assert_ne!(pid, 0, "spawned child must have an OS diagnostic pid");
        }
        other => panic!("expected Spawned({attempt}), got {other:?}"),
    }
}

fn assert_retired_locator_closed(link: &str) {
    let (endpoint, _) = parse_app_link(link).expect("retired link");
    let error = WindowsNamedPipeBootstrapStream::connect(endpoint)
        .expect_err("retired generation must close its exact pipe endpoint");
    assert!(matches!(error.raw_os_error(), Some(2 | 231)));
}

fn connect_silent(link: &str) -> WindowsNamedPipeBootstrapStream {
    let (endpoint, _) = parse_app_link(link).expect("silent peer link");
    WindowsNamedPipeBootstrapStream::connect(endpoint).expect("connect silent peer")
}

fn expect_link_bound(
    supervisor: &PrimaryRoleSupervisor,
    generation: keld_runtime::primary::RoleGeneration,
    attempt: u32,
) {
    match supervisor
        .recv_event(EVENT_DEADLINE)
        .expect("LinkBound event")
    {
        PrimaryRoleEvent::LinkBound {
            generation: actual,
            attempt: actual_attempt,
        } => {
            assert_eq!(actual, generation);
            assert_eq!(actual_attempt, attempt);
        }
        other => panic!("expected LinkBound({attempt}), got {other:?}"),
    }
}

fn expect_bootstrap_rejected(
    supervisor: &PrimaryRoleSupervisor,
    generation: keld_runtime::primary::RoleGeneration,
    attempt: u32,
    code: &'static str,
) {
    match supervisor
        .recv_event(EVENT_DEADLINE)
        .expect("BootstrapRejected event")
    {
        PrimaryRoleEvent::BootstrapRejected {
            generation: actual,
            attempt: actual_attempt,
            code: actual_code,
        } => {
            assert_eq!(actual, generation);
            assert_eq!(actual_attempt, attempt);
            assert_eq!(actual_code, code);
        }
        other => panic!("expected BootstrapRejected({attempt}, {code}), got {other:?}"),
    }
}

fn expect_revoked(
    supervisor: &PrimaryRoleSupervisor,
    generation: keld_runtime::primary::RoleGeneration,
    attempt: u32,
    cause: PrimaryRoleRevocationCause,
) {
    match supervisor
        .recv_event(EVENT_DEADLINE)
        .expect("Revoked event")
    {
        PrimaryRoleEvent::Revoked {
            generation: actual,
            attempt: actual_attempt,
            cause: actual_cause,
        } => {
            assert_eq!(actual, generation);
            assert_eq!(actual_attempt, attempt);
            assert_eq!(actual_cause, cause);
        }
        other => panic!("expected Revoked({attempt}), got {other:?}"),
    }
}

fn connect_with_token_from(token_link: &str, endpoint_link: &str) {
    let (_, token) = parse_app_link(token_link).expect("g1 token link");
    let (endpoint, _) = parse_app_link(endpoint_link).expect("g2 endpoint link");
    let mut hostile =
        WindowsNamedPipeBootstrapStream::connect(endpoint).expect("connect stale client");
    hostile
        .set_app_link_deadlines(Some(Duration::from_millis(500)))
        .expect("hostile deadline");
    let stale = SessionToken::from_bytes(*token.as_bytes());
    let error = handshake_client(&mut hostile, &stale).expect_err("g1 token must fail on g2");
    assert!(
        error.to_string().contains("KELD-IPC-007") || matches!(error, keld_ipc::IpcError::Io(_)),
        "stale client must observe authentication failure or peer close: {error}"
    );
}

struct EchoWorker {
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<Result<(), keld_ipc::IpcError>>,
}

impl EchoWorker {
    fn start(bound: BoundPrimaryGeneration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_worker = Arc::clone(&stop);
        let mut stream = bound.into_stream();
        let thread = thread::spawn(move || {
            serve_echo_requests_until_stopped(&mut stream, stop_for_worker.as_ref())
        });
        Self { stop, thread }
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    fn finish(self) {
        self.stop();
        self.join();
    }

    fn join(self) {
        match self.thread.join().expect("echo worker join") {
            Ok(()) | Err(keld_ipc::IpcError::Io(_)) => {}
            Err(error) => panic!("echo worker failed: {error}"),
        }
    }
}

struct PrimaryFixture {
    dir: PathBuf,
    control: TcpListener,
    control_port: u16,
}

impl PrimaryFixture {
    fn new() -> Self {
        let dir = unique_test_dir();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../keld-cli/templates/hello/src/kipc.ts"),
            dir.join("kipc.ts"),
        )
        .expect("copy kipc.ts");
        fs::write(dir.join("role.ts"), ROLE_SCRIPT).expect("write role fixture");
        let control = TcpListener::bind(("127.0.0.1", 0)).expect("bind control listener");
        let control_port = control.local_addr().expect("control address").port();
        Self {
            dir,
            control,
            control_port,
        }
    }

    fn dir(&self) -> &Path {
        &self.dir
    }

    fn script_path() -> &'static Path {
        Path::new("role.ts")
    }

    const fn control_port(&self) -> u16 {
        self.control_port
    }

    fn accept_control(&self) -> ControlPeer {
        let listener = self.control.try_clone().expect("clone control listener");
        let wake = self.control.local_addr().expect("control wake address");
        let (tx, rx) = mpsc::channel();
        let accept = thread::spawn(move || {
            let result = listener.accept().map(|(stream, _)| stream);
            let _ = tx.send(result);
        });
        let stream = match rx.recv_timeout(EVENT_DEADLINE) {
            Ok(result) => result.expect("control accept"),
            Err(error) => {
                let _ = TcpStream::connect(wake);
                let _ = accept.join();
                panic!("timed out accepting real Bun control stream: {error}");
            }
        };
        accept.join().expect("control accept worker");
        ControlPeer::new(stream)
    }
}

impl Drop for PrimaryFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

struct ControlPeer {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl ControlPeer {
    fn new(stream: TcpStream) -> Self {
        stream
            .set_app_link_deadlines(Some(CONTROL_POLL))
            .expect("control deadlines");
        let writer = stream.try_clone().expect("clone control stream");
        Self {
            reader: BufReader::new(stream),
            writer,
        }
    }

    fn read_ready(&mut self) -> String {
        let line = self.read_line();
        line.strip_prefix("READY ")
            .expect("READY must carry KELD_APP_LINK")
            .to_owned()
    }

    fn read_line(&mut self) -> String {
        let deadline = Instant::now() + CONTROL_DEADLINE;
        let mut line = Vec::new();
        loop {
            assert!(
                Instant::now() < deadline,
                "control peer produced no line; partial={:?}",
                String::from_utf8_lossy(&line)
            );
            let mut byte = [0_u8; 1];
            match self.reader.read_exact(&mut byte) {
                Ok(()) if byte[0] == b'\n' => {
                    return String::from_utf8(line).expect("control line UTF-8");
                }
                Ok(()) => line.push(byte[0]),
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) => {}
                Err(error) => panic!("read control line: {error}"),
            }
        }
    }

    fn write_line(&mut self, line: &str) {
        self.writer
            .write_all(format!("{line}\n").as_bytes())
            .expect("write control line");
        self.writer.flush().expect("flush control line");
    }

    fn expect_closed(&mut self) {
        let deadline = Instant::now() + EVENT_DEADLINE;
        let mut byte = [0_u8; 1];
        loop {
            assert!(
                Instant::now() < deadline,
                "the enrolled Bun control connection remained live after handle-owned shutdown"
            );
            match self.reader.read(&mut byte) {
                Ok(0) => return,
                Ok(_) => panic!("the stopped Bun emitted unexpected control bytes"),
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::ConnectionReset
                            | ErrorKind::ConnectionAborted
                            | ErrorKind::BrokenPipe
                            | ErrorKind::UnexpectedEof
                    ) =>
                {
                    return;
                }
                Err(error) => panic!("observe stopped Bun control connection: {error}"),
            }
        }
    }
}

fn unique_test_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    for counter in 0..128_u32 {
        let dir = std::env::temp_dir().join(format!(
            "keld-t8-{}-{nonce:x}-{counter:x}",
            std::process::id()
        ));
        match fs::create_dir(&dir) {
            Ok(()) => return dir,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => panic!("create fixture directory: {error}"),
        }
    }
    panic!("could not allocate T8 fixture directory");
}

const ROLE_SCRIPT: &str = r#"
import { AppLinkSession } from "./kipc";

const controlPort = Number.parseInt(process.argv[2] ?? "", 10);
const appLink = process.env.KELD_APP_LINK;
if (!Number.isInteger(controlPort) || !appLink) {
  console.error("missing control port or KELD_APP_LINK");
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
  hostname: "127.0.0.1",
  port: controlPort,
  socket: {
    binaryType: "uint8array",
    data(_socket, data) {
      buffer += decoder.decode(data, { stream: true });
      drainLines();
    },
    close() { process.exit(0); },
    error(_socket, err) { console.error(err.message); process.exit(3); },
    connectError(_socket, err) { console.error(err.message); process.exit(3); },
  },
});

async function writeLine(line) {
  const payload = encoder.encode(`${line}\n`);
  let offset = 0;
  while (offset < payload.length) {
    const written = control.write(payload.subarray(offset));
    if (written <= 0) throw new Error("control socket write failed");
    offset += written;
  }
}

await writeLine(`READY ${appLink}`);
if ((await readLine()) !== "BIND") process.exit(4);
const session = await AppLinkSession.connect(appLink);
await writeLine("BOUND");

while (true) {
  const command = await readLine();
  if (command === "CRASH") {
    session.close();
    process.exit(17);
  }
  if (command === "STOP") {
    session.close();
    process.exit(0);
  }
  if (command.startsWith("ECHO ")) {
    const [, message, rawCount] = command.split(" ");
    const count = Number.parseInt(rawCount, 10);
    const reply = await session.echo({ message, count });
    await writeLine(`ECHO ${reply.message} ${reply.count}`);
    continue;
  }
  console.error(`unexpected control command: ${command}`);
  process.exit(5);
}
"#;
