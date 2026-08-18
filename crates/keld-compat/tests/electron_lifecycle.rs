//! Conformance: `@keld/electron` app lifecycle over a real kipc session (KEL-72).
//!
//! Oracle (Electron docs, not a stub):
//! - [`app.whenReady()`](https://www.electronjs.org/docs/latest/api/app#appwhenready)
//! - [`app.quit()`](https://www.electronjs.org/docs/latest/api/app#appquit)
//! - [`window-all-closed`](https://www.electronjs.org/docs/latest/api/app#event-window-all-closed)
//! - [`process.type`](https://www.electronjs.org/docs/latest/api/process#processtype) (`browser` in main)
//! - [`process.versions.electron`](https://www.electronjs.org/docs/latest/api/process#processversionselectron)
//!
//! Divergence (scoreboard ▲, not a `keld.compat.ts` toggle): Electron
//! `app.quit(): void` vs Keld `Promise<void>` so a failed Quit Call is
//! visible as `KELD-IPC-*`. Keep the Promise; do not change the public
//! signature to `void`. See `docs/engineering/compat-scoreboard.md`.
//!
//! Negative control: replacing `whenReady` with `Promise.resolve()` (skipping
//! the host Ready event) makes `when_ready_does_not_resolve_before_host_ready_event`
//! fail — after draining queued stdout, `KEL72_READY` is already present
//! *before* `signal_ready`. A `has()` that only inspects the accumulator
//! (leaving READY on `rx`) is not a valid negative control.

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use keld_core::LifecycleSession;
use keld_ipc::{SessionToken, format_app_link};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

const TEST_TOKEN_BYTES: [u8; 32] = [0x72; 32];

fn test_token() -> SessionToken {
    SessionToken::from_bytes(TEST_TOKEN_BYTES)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/keld-compat -> workspace root")
        .to_path_buf()
}

fn fixtures_dir() -> PathBuf {
    workspace_root().join("packages/@keld/electron/fixtures")
}

struct LineLog {
    rx: mpsc::Receiver<String>,
    acc: String,
}

impl LineLog {
    fn wait_contains(&mut self, needle: &str) {
        while !self.acc.contains(needle) {
            let line = match self.rx.recv_timeout(Duration::from_secs(8)) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("timeout waiting for {needle}. stdout so far:\n{}", self.acc)
                }
                Err(RecvTimeoutError::Disconnected) => panic!(
                    "child stdout closed before seeing {needle}. so far:\n{}",
                    self.acc
                ),
            };
            self.acc.push_str(&line);
            self.acc.push('\n');
        }
    }

    /// Pull every line already sitting on `rx` into `acc`.
    ///
    /// `acc` is what [`Self::has`] and sequence checks inspect. After
    /// [`Self::wait_contains`] returns on `KEL72_WAITING`, an early
    /// `KEL72_READY` can still be queued on `rx`; a later
    /// `wait_contains("KEL72_READY")` after `signal_ready` would consume it
    /// and a stub `whenReady` (`Promise.resolve()`, or resolve-at-connect)
    /// would pass.
    fn drain_queued(&mut self) {
        while let Ok(line) = self.rx.try_recv() {
            self.acc.push_str(&line);
            self.acc.push('\n');
        }
    }

    fn has(&mut self, needle: &str) -> bool {
        self.drain_queued();
        self.acc.contains(needle)
    }

    /// Exact stdout line. `has("KEL72_CONNECT_CALLS=2")` also matches `=20`.
    fn has_exact_line(&mut self, line: &str) -> bool {
        self.drain_queued();
        self.acc.lines().any(|l| l == line)
    }

    fn first_marker_line(&self, needle: &str) -> Option<usize> {
        self.acc.lines().position(|line| line.contains(needle))
    }

    /// Exact line index. `contains("KEL72_READY")` also matches `KEL72_READY_SECOND`.
    fn first_exact_line(&self, line: &str) -> Option<usize> {
        self.acc.lines().position(|l| l == line)
    }

    /// Host-visible WAITING / READY order *before* `signal_ready`.
    ///
    /// Drains `rx` first so a READY line that arrived with WAITING cannot
    /// hide on the channel. Handshake completion is the barrier that the
    /// child is a live kipc peer; READY must still be absent.
    fn assert_waiting_without_ready(&mut self) {
        self.drain_queued();
        let waiting = self.first_marker_line("KEL72_WAITING");
        let ready = self.first_marker_line("KEL72_READY");
        assert!(
            waiting.is_some(),
            "KEL72_WAITING must appear before the host Ready event. stdout:\n{}",
            self.acc
        );
        assert!(
            ready.is_none(),
            "KEL72_READY before signal_ready — whenReady resolved without the host Ready event (stub Promise.resolve() / resolve-at-connect): {}",
            self.acc
        );
    }
}

fn spawn_line_log(stdout: impl std::io::Read + Send + 'static) -> LineLog {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(s) => {
                    if tx.send(s).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    LineLog {
        rx,
        acc: String::new(),
    }
}

#[cfg(unix)]
struct Bound {
    listener: std::os::unix::net::UnixListener,
    link: String,
    session_dir: PathBuf,
}

#[cfg(windows)]
struct Bound {
    listener: std::net::TcpListener,
    link: String,
}

#[cfg(unix)]
fn bind_app_link() -> Bound {
    let session_dir = std::env::temp_dir().join(format!(
        "k7-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&session_dir)
        .expect("session dir");
    std::fs::set_permissions(&session_dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let path = session_dir.join("e.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
    let link = format_app_link(&path.display().to_string(), &test_token());
    Bound {
        listener,
        link,
        session_dir,
    }
}

#[cfg(windows)]
fn bind_app_link() -> Bound {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let link = format_app_link(&port.to_string(), &test_token());
    Bound { listener, link }
}

fn spawn_fixture(script: &str, link: Option<&str>) -> std::process::Child {
    let fixtures = fixtures_dir();
    assert!(
        fixtures.join("tsconfig.json").is_file(),
        "missing tsconfig.json paths alias at {}",
        fixtures.display()
    );
    assert!(
        fixtures.join("bunfig.toml").is_file(),
        "missing bunfig.toml (spec-named alias file) at {}",
        fixtures.display()
    );
    let mut cmd = Command::new("bun");
    cmd.arg(format!("./{script}"))
        .current_dir(&fixtures)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    match link {
        Some(link) => {
            cmd.env("KELD_APP_LINK", link);
        }
        None => {
            cmd.env_remove("KELD_APP_LINK");
        }
    }
    cmd.spawn().expect("spawn bun — bun must be on PATH")
}

#[test]
fn bunfig_alias_resolves_electron_and_shims_process_fields() {
    // Architecture 04 §3.1 / KEL-72: `import "electron"` must resolve to
    // `packages/@keld/electron`, not the npm `electron` package. Bun 1.3.14
    // honors `tsconfig.json` `compilerOptions.paths` for this (bunfig.toml
    // `[alias]` does not — removing tsconfig.json and leaving only bunfig
    // makes this fail with SyntaxError against `~/.bun/install/cache/electron`).
    let mut child = spawn_fixture("shim_env.ts", None);
    let stdout = child.stdout.take().expect("stdout");
    let mut log = spawn_line_log(stdout);
    log.wait_contains("KEL72_TYPE=");
    log.wait_contains("KEL72_ELECTRON=");
    let status = child.wait().expect("wait bun");
    assert!(status.success(), "shim_env failed: stdout={}", log.acc);
    assert!(
        log.has("KEL72_TYPE=browser"),
        "process.type must be browser in the main shim: {}",
        log.acc
    );
    assert!(
        log.has("KEL72_ELECTRON=0.0.1"),
        "process.versions.electron must be the documented shim: {}",
        log.acc
    );
}

#[test]
fn when_ready_does_not_resolve_before_host_ready_event() {
    let bound = bind_app_link();
    let (session_tx, session_rx) = mpsc::channel();
    {
        let listener = bound.listener;
        thread::spawn(move || {
            let accepted = listener.accept().map_err(|e| e.to_string());
            let result = accepted.and_then(|(server, _)| {
                server
                    .try_clone()
                    .map_err(|e| e.to_string())
                    .and_then(|writer| {
                        LifecycleSession::handshake(server, writer, &test_token())
                            .map_err(|e| e.to_string())
                    })
            });
            let _ = session_tx.send(result);
        });
    }

    let mut child = spawn_fixture("lifecycle.ts", Some(&bound.link));
    let stdout = child.stdout.take().expect("stdout");
    let mut log = spawn_line_log(stdout);

    log.wait_contains("KEL72_WAITING");
    // Drain `rx` here: WAITING and READY are consecutive writeSync lines if
    // `whenReady` is `Promise.resolve()`, and `wait_contains(WAITING)` stops
    // with READY still queued.
    log.assert_waiting_without_ready();

    // Host-visible barrier: HELLO completed. A shim that resolves at connect
    // (not at Ready) prints READY as soon as this returns.
    let mut host = session_rx
        .recv_timeout(Duration::from_secs(8))
        .expect("handshake result")
        .unwrap_or_else(|e| panic!("lifecycle handshake failed: {e}"));

    log.assert_waiting_without_ready();

    host.window_opened();
    host.signal_ready().expect("ready");
    log.wait_contains("KEL72_READY");
    let waiting_at = log
        .first_marker_line("KEL72_WAITING")
        .expect("WAITING recorded");
    let ready_at = log
        .first_marker_line("KEL72_READY")
        .expect("READY recorded");
    assert!(
        waiting_at < ready_at,
        "KEL72_WAITING must precede KEL72_READY (got waiting@{waiting_at} ready@{ready_at}): {}",
        log.acc
    );
    assert!(
        !log.has("KEL72_WINDOW_ALL_CLOSED"),
        "window-all-closed must not be emitted by the shim itself: {}",
        log.acc
    );

    host.window_closed().expect("close last window");
    log.wait_contains("KEL72_WINDOW_ALL_CLOSED");
    host.wait_for_quit().expect("quit ends the host session");

    let status = child.wait().expect("wait bun");
    assert!(
        status.success(),
        "lifecycle fixture failed: stdout={}",
        log.acc
    );
    assert!(
        log.has("KEL72_TYPE=browser") && log.has("KEL72_ELECTRON=0.0.1"),
        "shim fields missing: {}",
        log.acc
    );

    #[cfg(unix)]
    let _ = std::fs::remove_dir_all(&bound.session_dir);
}

/// Isolation / connect-retry / unhandledRejection probe (`app_ready.ts`).
///
/// The fixture stubs `LifecycleLink.connect` (no host handshake — that is
/// `lifecycle.ts`). Spawn still mints a unique unused app-link the same way
/// the READY test does (temp `0o700` dir + socket, or loopback port 0) so
/// two parallel runs do not share `/tmp/keld-kel72-unused.sock`.
///
/// Oracle lines the fixture already prints on success:
/// `KEL72_READY_SECOND`, `KEL72_CONNECT_CALLS=2`, `KEL72_UNHANDLED_COUNT=0`,
/// exit 0.
///
/// Negative control (a defect must fail this test):
/// - cached `linkPromise` after a failed connect → fixture stderr
///   `KEL72_CONNECT_NOT_RETRIED` and exit 1
/// - `emit` without per-listener try/catch → `KEL72_SECOND_LISTENER_SKIPPED`
///   and exit 1
/// - missing `ignoreIfUnawaited` on `whenReady` → `KEL72_UNHANDLED_COUNT`
///   ≠ 0 and exit 1
#[test]
fn app_ready_isolates_listeners_retries_connect_without_unhandled_rejection() {
    let bound = bind_app_link();
    // `bound` keeps the unique endpoint alive. The fixture never dials it
    // (`connect` is replaced); accepting would hang.

    let mut child = spawn_fixture("app_ready.ts", Some(&bound.link));
    let stdout = child.stdout.take().expect("stdout");
    let mut log = spawn_line_log(stdout);

    log.wait_contains("KEL72_READY_SECOND");
    log.wait_contains("KEL72_CONNECT_CALLS=");
    log.wait_contains("KEL72_UNHANDLED_COUNT=");

    let status = child.wait().expect("wait bun");
    log.drain_queued();
    assert!(
        status.success(),
        "app_ready fixture failed (exit {:?}): stdout={}",
        status.code(),
        log.acc
    );
    let second_at = log
        .first_exact_line("KEL72_READY_SECOND")
        .expect("KEL72_READY_SECOND recorded");
    let ready_at = log
        .first_exact_line("KEL72_READY")
        .expect("KEL72_READY recorded");
    assert!(
        second_at < ready_at,
        "throwing ready listener skipped the later listener (second@{second_at} ready@{ready_at}): {}",
        log.acc
    );
    assert!(
        log.has_exact_line("KEL72_CONNECT_CALLS=2"),
        "failed connect must be retried (want KEL72_CONNECT_CALLS=2): {}",
        log.acc
    );
    assert!(
        log.has_exact_line("KEL72_UNHANDLED_COUNT=0"),
        "unawaited whenReady must not become unhandledRejection: {}",
        log.acc
    );

    #[cfg(unix)]
    let _ = std::fs::remove_dir_all(&bound.session_dir);
}
