//! Shared Unix role test fixtures for KEL-75 T1b/T2.
//!
//! Control sockets use a short owner-only directory so macOS `sockaddr_un`
//! paths stay under `SUN_LEN`.

use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use keld_ipc::{AppLinkDeadlines, SessionToken, parse_app_link};

pub(crate) struct PrimaryFixture {
    dir: PathBuf,
    control_path: PathBuf,
    control_listener: UnixListener,
}

impl PrimaryFixture {
    pub(crate) fn new() -> Self {
        let env = RoleScriptEnv::new();
        let (control_path, control_listener) = env.bind_control("control.sock");
        Self {
            dir: env.into_dir(),
            control_path,
            control_listener,
        }
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn script_path() -> &'static Path {
        Path::new("role.ts")
    }

    pub(crate) fn control_path(&self) -> &Path {
        &self.control_path
    }

    pub(crate) fn accept_control(&self) -> ControlPeer {
        accept_control(&self.control_listener)
    }
}

impl Drop for PrimaryFixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.control_path);
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// One temporary directory with two owner-only control sockets.
pub(crate) struct FamilyFixture {
    dir: PathBuf,
    primary_control_path: PathBuf,
    app_bound_control_path: PathBuf,
    primary_listener: UnixListener,
    app_bound_listener: UnixListener,
}

impl FamilyFixture {
    pub(crate) fn new() -> Self {
        let env = RoleScriptEnv::new();
        let (primary_control_path, primary_listener) = env.bind_control("p.sock");
        let (app_bound_control_path, app_bound_listener) = env.bind_control("a.sock");
        Self {
            dir: env.into_dir(),
            primary_control_path,
            app_bound_control_path,
            primary_listener,
            app_bound_listener,
        }
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn script_path() -> &'static Path {
        Path::new("role.ts")
    }

    pub(crate) fn primary_control_path(&self) -> &Path {
        &self.primary_control_path
    }

    pub(crate) fn app_bound_control_path(&self) -> &Path {
        &self.app_bound_control_path
    }

    pub(crate) fn accept_primary(&self) -> ControlPeer {
        accept_control(&self.primary_listener)
    }

    pub(crate) fn accept_app_bound(&self) -> ControlPeer {
        accept_control(&self.app_bound_listener)
    }
}

impl Drop for FamilyFixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.primary_control_path);
        let _ = fs::remove_file(&self.app_bound_control_path);
        let _ = fs::remove_dir_all(&self.dir);
    }
}

struct RoleScriptEnv {
    dir: PathBuf,
}

impl RoleScriptEnv {
    fn new() -> Self {
        let dir = unique_test_dir();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../keld-cli/templates/hello/src/kipc.ts"),
            dir.join("kipc.ts"),
        )
        .expect("copy kipc.ts");
        fs::write(dir.join("role.ts"), ROLE_SCRIPT).expect("write fixture");
        Self { dir }
    }

    fn bind_control(&self, name: &str) -> (PathBuf, UnixListener) {
        let control_path = self.dir.join(name);
        let control_listener = UnixListener::bind(&control_path).expect("bind control");
        control_listener
            .set_nonblocking(true)
            .expect("control nonblocking");
        (control_path, control_listener)
    }

    fn into_dir(self) -> PathBuf {
        self.dir
    }
}

pub(crate) struct ControlPeer {
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

    pub(crate) fn read_line(&mut self) -> String {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read control line");
        line.trim_end_matches('\n').to_owned()
    }

    pub(crate) fn write_line(&mut self, line: &str) {
        self.writer
            .write_all(format!("{line}\n").as_bytes())
            .expect("write control line");
        self.writer.flush().expect("flush control line");
    }
}

pub(crate) fn assert_ready_line(control: &mut ControlPeer, app_link: &str) {
    assert_eq!(
        control.read_line(),
        format!("READY {app_link}"),
        "control socket is trusted test memory for app-link capture"
    );
}

pub(crate) fn connect_with_foreign_token(token_link: &str, endpoint_link: &str) {
    let (_, token) = parse_app_link(token_link).expect("token link");
    let (endpoint, _) = parse_app_link(endpoint_link).expect("endpoint link");
    let foreign = SessionToken::from_bytes(*token.as_bytes());
    let mut hostile = UnixStream::connect(endpoint).expect("connect foreign client");
    hostile
        .set_app_link_deadlines(Some(Duration::from_millis(250)))
        .expect("deadline");
    let error = keld_ipc::link::handshake_client(&mut hostile, &foreign)
        .expect_err("foreign token must be rejected");
    assert!(
        error.to_string().contains("KELD-IPC-007") || matches!(error, keld_ipc::IpcError::Io(_)),
        "foreign client must see auth failure or peer close, got {error}"
    );
}

fn accept_control(listener: &UnixListener) -> ControlPeer {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match listener.accept() {
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
    panic!("could not allocate role test dir");
}

const ROLE_SCRIPT: &str = r#"
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
