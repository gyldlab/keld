//! Shared Unix role test fixtures for KEL-75 T1b/T2.
//!
//! Control sockets use a short owner-only directory so macOS `sockaddr_un`
//! paths stay under `SUN_LEN`.

use std::fs;
use std::io::{BufReader, ErrorKind, Read, Write};
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

/// How long a read blocks before the loop re-checks its deadline. This is a
/// polling granularity, not a deadline: one expiry means "nothing yet", never
/// "never" (KEL-113).
const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How long a control peer may take to produce one line before the fixture
/// gives up. Matches the 30s the rest of the repo already allows a real Bun
/// child to become ready (`keld-cli` `run_dev`), rather than the 2s that was
/// previously implied by the socket timeout alone.
const CONTROL_LINE_DEADLINE: Duration = Duration::from_secs(30);

pub(crate) struct ControlPeer {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl ControlPeer {
    fn new(stream: UnixStream) -> Self {
        // KEL-113: BSD accept() gives the new socket the listener's file
        // status flags, and these listeners are deliberately non-blocking
        // (`accept_control` polls). On macOS the accepted stream therefore
        // arrives with `O_NONBLOCK` set, which makes the `SO_RCVTIMEO` below
        // inert — a read returns `EAGAIN` at once instead of waiting, so
        // `read_line` failed the instant the peer had not written *yet*.
        // Linux's `accept4` does not inherit, which is why this only ever
        // flaked on macOS. Clear it explicitly rather than relying on the
        // platform's choice.
        stream
            .set_nonblocking(false)
            .expect("control stream must be blocking for its read deadline to apply");
        stream
            .set_app_link_deadlines(Some(CONTROL_POLL_INTERVAL))
            .expect("control deadline");
        let writer = stream.try_clone().expect("clone control stream");
        Self {
            reader: BufReader::new(stream),
            writer,
        }
    }

    /// Reads one `\n`-terminated line, waiting up to [`CONTROL_LINE_DEADLINE`].
    ///
    /// Awaits the line rather than failing on the first quiet interval: the
    /// socket timeout is a polling granularity, and a real `bun` child under
    /// load can take longer than one of them to reach its first write. Reads a
    /// byte at a time so a timeout that lands mid-line cannot lose the bytes
    /// already received — `BufRead::read_line` leaves its buffer unspecified
    /// on error, so it cannot be retried safely (KEL-113).
    pub(crate) fn read_line(&mut self) -> String {
        let deadline = Instant::now() + CONTROL_LINE_DEADLINE;
        let mut line = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            match self.reader.read_exact(&mut byte) {
                Ok(()) if byte[0] == b'\n' => {
                    return String::from_utf8(line).expect("control line is UTF-8");
                }
                Ok(()) => line.push(byte[0]),
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) =>
                {
                    assert!(
                        Instant::now() < deadline,
                        "control peer produced no complete line within \
                         {CONTROL_LINE_DEADLINE:?}; received so far: {:?}",
                        String::from_utf8_lossy(&line)
                    );
                }
                Err(error) => panic!("read control line: {error}"),
            }
        }
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

#[cfg(test)]
mod tests {
    use super::{CONTROL_POLL_INTERVAL, ControlPeer, accept_control, unique_test_dir};
    use std::io::{ErrorKind, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::thread;
    use std::time::Instant;

    /// KEL-113 regression: the accepted control stream must actually block.
    ///
    /// The deadline loop in `read_line` tolerates a quiet interval, so it hides
    /// a non-blocking socket behind a busy-spin: correct, but burning a core
    /// for the whole wait on the very machine whose load caused the wait. This
    /// binds the other half of the fix by timing a read against a peer that
    /// never writes. A blocking socket honours `SO_RCVTIMEO` and returns after
    /// roughly one poll interval; an `O_NONBLOCK` one returns `EAGAIN`
    /// immediately. The bound is deliberately loose — `SO_RCVTIMEO` never
    /// returns *early*, so only the lower bound is asserted.
    #[test]
    fn accepted_control_stream_blocks_for_its_read_deadline() {
        let dir = unique_test_dir();
        let path = dir.join("control.sock");
        let listener = UnixListener::bind(&path).expect("bind control");
        listener
            .set_nonblocking(true)
            .expect("listener is non-blocking by design");
        let silent = UnixStream::connect(&path).expect("connect control");

        let mut peer = accept_control(&listener);
        let start = Instant::now();
        let mut byte = [0_u8; 1];
        let error = peer
            .reader
            .get_mut()
            .read(&mut byte)
            .expect_err("a silent peer cannot produce a byte");
        let elapsed = start.elapsed();

        assert!(
            matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
            "expected the read deadline to expire, got {error}"
        );
        assert!(
            elapsed >= CONTROL_POLL_INTERVAL / 2,
            "the accepted stream returned in {elapsed:?}, so it never blocked — \
             it inherited O_NONBLOCK from the listener and SO_RCVTIMEO is inert"
        );
        drop(silent);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// KEL-113 regression: a peer slower than one poll interval must still be
    /// read, not reported as a failure.
    ///
    /// BSD `accept()` gives the new socket the listener's file status flags,
    /// and these listeners are non-blocking on purpose. On macOS the accepted
    /// stream therefore arrived with `O_NONBLOCK` set, which makes
    /// `SO_RCVTIMEO` inert: the read returned `EAGAIN` immediately and
    /// `read_line` panicked with `WouldBlock` the instant the peer had not
    /// written *yet*. Linux's `accept4` does not inherit, so this only ever
    /// flaked on macOS — and only when the writer happened to lose the race.
    ///
    /// The delay below is the condition under test — a writer slower than the
    /// reader — not a sleep standing in for a wait. Reverting either half of
    /// the fix fails this test on macOS.
    #[test]
    fn control_peer_waits_for_a_writer_slower_than_one_poll_interval() {
        let dir = unique_test_dir();
        let path = dir.join("control.sock");
        let listener = UnixListener::bind(&path).expect("bind control");
        listener
            .set_nonblocking(true)
            .expect("listener is non-blocking by design");

        let writer_path = path.clone();
        let writer = thread::spawn(move || {
            let mut stream = UnixStream::connect(&writer_path).expect("connect control");
            // Quiet for longer than one poll interval, so a reader that treats
            // one quiet interval as terminal cannot pass.
            thread::sleep(CONTROL_POLL_INTERVAL * 3);
            stream
                .write_all(b"READY late\n")
                .expect("write control line");
            stream.flush().expect("flush control line");
        });

        let mut peer: ControlPeer = accept_control(&listener);
        assert_eq!(
            peer.read_line(),
            "READY late",
            "a writer slower than one poll interval must still be read"
        );
        writer.join().expect("writer thread");
        std::fs::remove_dir_all(&dir).ok();
    }
}
