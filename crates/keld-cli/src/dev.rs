//! `keld dev` — staged-host delegation plus retained hello diagnostics.

use std::fmt::Write as _;
use std::fs;
#[cfg(windows)]
use std::io::Read as _;
use std::io::{self, ErrorKind, Write};
#[cfg(windows)]
use std::io::{BufRead as _, BufReader};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::process::CommandExt as _;
#[cfg(windows)]
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::sync::mpsc;
#[cfg(windows)]
use std::thread;
use std::time::Duration;

use keld_core::{
    DEFAULT_HELLO_TITLE, DEFAULT_RENDERER, HostOwnedHelloSession, read_config_renderer,
    read_config_title,
};
use keld_runtime::RestartPolicy;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

use crate::doctor::{all_ok, renderer_load_message, renderer_path_problem, run_checks};

pub use crate::doctor::RENDERER_LOAD_CODE;

#[cfg(windows)]
const WINDOWS_STAGE_CLEANUP_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Errors starting a dev session.
#[derive(Debug)]
pub enum DevError {
    /// Environment, project, or delegated-host failure already rendered with
    /// its own registered code and fix.
    Doctor(String),
    /// Child process or thread failure.
    Io(io::Error),
    /// IPC or host failure surfaced as text.
    Runtime(String),
    /// The supervised app process died while the host owned the window
    /// (KEL-105). Rendered verbatim: `keld-core` already produced a
    /// `KELD-CORE-033` diagnostic carrying the crash and its fix, and
    /// appending `KELD-CLI-031`'s "re-run `keld doctor`" would contradict it.
    WindowPhase(String),
    /// Project renderer HTML could not be loaded.
    Renderer {
        /// Configured or defaulted relative path.
        path: PathBuf,
        /// Why loading failed.
        reason: &'static str,
    },
}

impl std::fmt::Display for DevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Both already carry a rendered, registered diagnostic with its
            // own fix; wrapping either in `KELD-CLI-031` would append a second,
            // contradicting one.
            Self::Doctor(msg) | Self::WindowPhase(msg) => write!(f, "{msg}"),
            Self::Io(e) => write!(
                f,
                "KELD-CLI-030: dev session I/O error — {e}. \
                 Check that `bun` is on PATH and the project files are readable."
            ),
            Self::Runtime(msg) => write!(
                f,
                "KELD-CLI-031: dev session failed — {msg}. \
                 Re-run `keld doctor` and fix the reported checks."
            ),
            Self::Renderer { path, reason } => {
                write!(f, "{}", renderer_load_message(path, reason))
            }
        }
    }
}

impl std::error::Error for DevError {}

impl From<io::Error> for DevError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<keld_core::HelloSessionError> for DevError {
    fn from(value: keld_core::HelloSessionError) -> Self {
        let rendered = value.to_string();
        match value {
            keld_core::HelloSessionError::WindowPhase { .. } => Self::WindowPhase(rendered),
            keld_core::HelloSessionError::Io(_)
            | keld_core::HelloSessionError::Runtime(_)
            | keld_core::HelloSessionError::Timeout { .. } => Self::Runtime(rendered),
        }
    }
}

/// Outcome of the Bun echo half of `keld dev` (window is separate).
#[derive(Debug)]
pub struct DevEchoResult {
    /// Captured Bun stdout.
    pub stdout: String,
    /// App-link endpoint used for this session.
    pub link: String,
}

/// Finds the project root containing `keld.config.ts`, starting at `cwd`.
#[must_use]
pub fn find_project_root(cwd: &Path) -> Option<PathBuf> {
    let mut dir = cwd.to_path_buf();
    loop {
        if dir.join("keld.config.ts").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Window title for the hello slice: `keld.config.ts` `name`, else `"Keld"`.
#[must_use]
pub fn hello_title_for_project(project_root: &Path) -> String {
    read_config_title(project_root).unwrap_or_else(|| DEFAULT_HELLO_TITLE.to_owned())
}

/// HTML the hello window will render: the project's `renderer` file contents.
///
/// Defaults to [`DEFAULT_RENDERER`] (`index.html`) when the config omits `renderer`.
/// Paths must be relative to `project_root` with no `..` segment. Contents are
/// passed as inline HTML (`NavTarget::Html`); this is not a `file://` load.
///
/// # Errors
///
/// Returns [`DevError::Renderer`] (`KELD-CLI-035`) for a missing file or unsafe
/// path, or [`DevError::Io`] when the file exists but cannot be read.
pub fn load_dev_window_html(project_root: &Path) -> Result<String, DevError> {
    let renderer =
        read_config_renderer(project_root).unwrap_or_else(|| DEFAULT_RENDERER.to_owned());
    let rel = validate_renderer_relpath(&renderer)?;
    let path = project_root.join(rel);
    match fs::read_to_string(&path) {
        Ok(html) => Ok(html),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(DevError::Renderer {
            path: PathBuf::from(&renderer),
            reason: "file is missing",
        }),
        Err(e) => Err(e.into()),
    }
}

fn validate_renderer_relpath(renderer: &str) -> Result<&Path, DevError> {
    if let Some(reason) = renderer_path_problem(renderer) {
        return Err(DevError::Renderer {
            path: PathBuf::from(renderer.trim()),
            reason,
        });
    }
    Ok(Path::new(renderer.trim()))
}

fn doctor_or_err(project_root: &Path) -> Result<(), DevError> {
    let checks = run_checks(Some(project_root));
    if all_ok(&checks) {
        return Ok(());
    }
    let mut msg = String::from("KELD-CLI-032: environment checks failed:\n");
    for check in &checks {
        let mark = if check.ok { "ok" } else { "FAIL" };
        let _ = writeln!(msg, "  [{mark}] {} — {}", check.label, check.detail);
    }
    Err(DevError::Doctor(msg))
}

/// Doctor checks, then one Bun IPC echo round-trip without opening a window.
///
/// Starts the host-owned app-link + supervised Bun, awaits the ready marker,
/// then reaps the child. The hello template stays alive until reaped (KEL-30).
///
/// # Errors
///
/// Returns [`DevError`] when checks fail, Bun cannot be spawned, or the
/// ready marker never appears.
pub fn run_dev_echo(project_root: &Path) -> Result<DevEchoResult, DevError> {
    doctor_or_err(project_root)?;
    let session = HostOwnedHelloSession::start(
        project_root,
        project_root.join("src/main.ts"),
        RestartPolicy::default(),
    )?;
    // Observable wire contract: HELLO + CALL/REPLY printed by the child.
    // Stock template also prints `{name}: main process ready`; crash-recovery
    // fixtures may only print `ipc-echo ok:`, so the second wait stays optional.
    session.wait_until_output_contains("ipc-echo ok:", Duration::from_secs(30))?;
    let _ = session.wait_until_output_contains("main process ready", Duration::from_secs(5));
    let stdout = session.output().stdout;
    let stderr = session.output().stderr;
    io::stdout().write_all(stdout.as_bytes())?;
    io::stderr().write_all(stderr.as_bytes())?;
    let link = session.link().to_owned();
    // The windowless echo contract is complete once the observable reply was
    // captured. A child that then ends itself with status zero is still
    // recorded by the supervisor, but it is not a failed echo run (KEL-116).
    session.shutdown_after_completed_work()?;
    Ok(DevEchoResult { stdout, link })
}

/// Runs `keld dev` in `project_root`.
///
/// On macOS, Linux, and Windows this compiles one owner-private stage and launches its no-flag
/// host with a private stdin liveness lease. The host owns the window,
/// authenticated app link, and Bun. Other platforms fail closed until their
/// KEL-96/T4 no-flag host slice lands.
///
/// # Errors
///
/// Returns [`DevError`] when checks, staging, host launch, Bun, or the window fails.
pub fn run_dev(project_root: &Path) -> Result<(), DevError> {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        run_dev_host(project_root)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        crate::boot::stage_dev_boot(project_root, Path::new(""))
            .map(|_| ())
            .map_err(|error| DevError::Doctor(error.to_string()))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn run_dev_host(project_root: &Path) -> Result<(), DevError> {
    doctor_or_err(project_root)?;
    let cli_executable = std::env::current_exe()?;
    let developer_host = cli_executable
        .parent()
        .ok_or_else(|| {
            DevError::Runtime(String::from(
                "the keld executable has no parent directory; install keld and keld-host together",
            ))
        })?
        .join(if cfg!(windows) {
            "keld-host.exe"
        } else {
            "keld-host"
        });
    let stage = crate::boot::stage_dev_boot(project_root, &developer_host)
        .map_err(|error| DevError::Doctor(error.to_string()))?;
    #[cfg(windows)]
    let mut stage = stage;
    let stage_root = stage.root().to_owned();
    let mut command = Command::new(stage.host());
    command
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    command.process_group(0);
    let host = command.spawn();
    let mut host = match host {
        Ok(host) => host,
        Err(source) => {
            drop(stage);
            if let Err(cleanup) = fs::remove_dir_all(&stage_root) {
                return Err(DevError::Doctor(format!(
                    "KELD-CLI-047: boot staging failed during host launch cleanup — \
                     spawn failed: {source}; cleanup failed: {cleanup}. \
                     Remove that owner-private nonce directory and retry."
                )));
            }
            return Err(DevError::Io(source));
        }
    };
    let lease_writer = host.stdin.take().ok_or_else(|| {
        DevError::Runtime(String::from(
            "the staged host did not receive its private stdin lease",
        ))
    })?;
    #[cfg(windows)]
    let cleanup_sentinel = match start_windows_stage_cleanup_sentinel(
        &developer_host,
        &stage_root,
        host.id(),
    ) {
        Ok(sentinel) => sentinel,
        Err(sentinel_error) => {
            drop(lease_writer);
            let host_result = host.wait();
            stage.release_launch_guards();
            let cleanup_result = fs::remove_dir_all(&stage_root);
            return Err(DevError::Doctor(format!(
                "KELD-CLI-047: Windows dev-stage cleanup owner failed before handoff: {sentinel_error}; \
                 host cleanup={host_result:?}; stage cleanup={cleanup_result:?}. \
                 Confirm the staged host exited, remove `{}`, and retry.",
                stage_root.display()
            )));
        }
    };
    #[cfg(windows)]
    stage.release_launch_guards();
    let status = host.wait()?;
    drop(lease_writer);
    drop(stage);
    #[cfg(windows)]
    cleanup_sentinel.wait(&stage_root)?;
    if status.success() {
        Ok(())
    } else {
        Err(DevError::Doctor(format!(
            "KELD-CLI-048: staged no-flag host exited with {status}. \
             Fix the preceding host diagnostic, then re-run `keld dev`."
        )))
    }
}

#[cfg(windows)]
fn start_windows_stage_cleanup_sentinel(
    installed_host: &Path,
    stage_root: &Path,
    staged_host_pid: u32,
) -> Result<WindowsStageCleanupSentinel, DevError> {
    let mut sentinel = Command::new(installed_host);
    sentinel
        .arg("--keld-windows-dev-stage-cleanup-v1")
        .arg(stage_root)
        .arg(staged_host_pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    let mut sentinel = sentinel.spawn()?;
    let stdout = sentinel.stdout.take().ok_or_else(|| {
        DevError::Runtime(String::from(
            "Windows dev-stage cleanup sentinel has no readiness pipe",
        ))
    })?;
    let stderr = sentinel.stderr.take().ok_or_else(|| {
        DevError::Runtime(String::from(
            "Windows dev-stage cleanup sentinel has no diagnostic pipe",
        ))
    })?;
    await_windows_stage_cleanup_sentinel(
        sentinel,
        stdout,
        stderr,
        WINDOWS_STAGE_CLEANUP_READY_TIMEOUT,
    )
}

#[cfg(windows)]
fn await_windows_stage_cleanup_sentinel(
    mut sentinel: std::process::Child,
    stdout: std::process::ChildStdout,
    mut stderr: std::process::ChildStderr,
    timeout: Duration,
) -> Result<WindowsStageCleanupSentinel, DevError> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let ready_thread = thread::Builder::new()
        .name(String::from("keld-stage-cleanup-readiness"))
        .spawn(move || {
            let mut ready = String::new();
            let result = BufReader::new(stdout).read_line(&mut ready).map(|_| ready);
            let _ = ready_tx.send(result);
        });
    let ready_thread = match ready_thread {
        Ok(thread) => thread,
        Err(source) => {
            return failed_windows_stage_cleanup_readiness(
                &mut sentinel,
                &mut stderr,
                &format!("could not start readiness reader: {source}"),
            );
        }
    };
    let ready = match ready_rx.recv_timeout(timeout) {
        Ok(Ok(ready)) => ready,
        Ok(Err(source)) => {
            let _ = ready_thread.join();
            return failed_windows_stage_cleanup_readiness(
                &mut sentinel,
                &mut stderr,
                &format!("readiness pipe failed: {source}"),
            );
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = sentinel.kill();
            let _ = ready_thread.join();
            return failed_windows_stage_cleanup_readiness(
                &mut sentinel,
                &mut stderr,
                &format!("timed out after {timeout:?}"),
            );
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = sentinel.kill();
            let _ = ready_thread.join();
            return failed_windows_stage_cleanup_readiness(
                &mut sentinel,
                &mut stderr,
                "readiness reader stopped without a result",
            );
        }
    };
    if ready_thread.join().is_err() {
        return failed_windows_stage_cleanup_readiness(
            &mut sentinel,
            &mut stderr,
            "readiness reader panicked",
        );
    }
    if ready.trim_end() != "KELD_WINDOWS_DEV_STAGE_CLEANUP_READY" {
        return failed_windows_stage_cleanup_readiness(
            &mut sentinel,
            &mut stderr,
            &format!("unexpected readiness record {ready:?}"),
        );
    }
    Ok(WindowsStageCleanupSentinel {
        child: sentinel,
        stderr,
    })
}

#[cfg(windows)]
fn failed_windows_stage_cleanup_readiness(
    sentinel: &mut std::process::Child,
    stderr: &mut std::process::ChildStderr,
    reason: &str,
) -> Result<WindowsStageCleanupSentinel, DevError> {
    let _ = sentinel.kill();
    let status = sentinel.wait()?;
    let mut detail = String::new();
    stderr.read_to_string(&mut detail)?;
    Err(DevError::Runtime(format!(
        "Windows dev-stage cleanup sentinel failed readiness ({reason}) with {status}: {}",
        detail.trim()
    )))
}

#[cfg(windows)]
struct WindowsStageCleanupSentinel {
    child: std::process::Child,
    stderr: std::process::ChildStderr,
}

#[cfg(windows)]
impl WindowsStageCleanupSentinel {
    fn wait(mut self, stage_root: &Path) -> Result<(), DevError> {
        let status = self.child.wait()?;
        let mut detail = String::new();
        self.stderr.read_to_string(&mut detail)?;
        if status.success() {
            return Ok(());
        }
        Err(DevError::Doctor(format!(
            "KELD-CLI-047: Windows dev-stage cleanup sentinel exited with {status}: {}. \
             Remove `{}` after confirming the host has exited.",
            detail.trim(),
            stage_root.display()
        )))
    }
}

/// The retained CLI-owned hello path with its window phase injected.
///
/// Shipping macOS/Windows [`run_dev`] delegates to the staged host and other
/// platforms fail closed until their no-flag host slice. This retained
/// diagnostic/test seam exercises the KEL-105 supervision verdict. `window` stands where the
/// legacy `tao` `run_return` phase does: it borrows the
/// thread for the whole window phase, during which the host observes nothing
/// about the app process. Everything after it — reaping Bun, reading the
/// supervision verdict, choosing the returned status — is the KEL-105 seam,
/// and a test that cannot drive it cannot prove `keld dev` stops exiting 0.
///
/// # Errors
///
/// Returns [`DevError`] when checks fail, Bun cannot be spawned, the window
/// fails, or the supervised app process died across the window phase.
pub fn run_dev_with_window<W>(project_root: &Path, window: W) -> Result<(), DevError>
where
    W: FnOnce(&str, &str) -> Result<(), DevError>,
{
    doctor_or_err(project_root)?;
    let session = HostOwnedHelloSession::start(
        project_root,
        project_root.join("src/main.ts"),
        RestartPolicy::default(),
    )?;
    let ready = format!(
        "{}: main process ready (IPC echo ok)",
        hello_title_for_project(project_root)
    );
    session.wait_until_output_contains(&ready, Duration::from_secs(30))?;
    let stdout = session.output().stdout;
    let stderr = session.output().stderr;
    io::stdout().write_all(stdout.as_bytes())?;
    io::stderr().write_all(stderr.as_bytes())?;

    // Window phase while echo listener + Bun are still live.
    let title = hello_title_for_project(project_root);
    let html = load_dev_window_html(project_root)?;
    let window_result = window(&title, &html);
    // Window returned (tao `run_return`) — reap Bun, stop the listener, and
    // read how supervision actually ended across the window phase (KEL-105).
    window_phase_outcome(session.shutdown(), window_result)
}

/// Decides `keld dev`'s result after the window phase (KEL-105).
///
/// `supervision` is [`HostOwnedHelloSession::shutdown`]'s verdict on the app
/// process; `window` is the event loop's own result. A terminal supervision
/// failure wins: a dead app process is upstream of any window fault, and its
/// diagnostic is the one naming the crash. The window error is appended as
/// context rather than dropped.
///
/// Pure so the exit decision is testable without a GUI: every `Err` here is
/// exit 1 (`docs/architecture/07-agent-experience.md` §7) via `main`.
fn window_phase_outcome(
    supervision: Result<(), keld_core::HelloSessionError>,
    window: Result<(), DevError>,
) -> Result<(), DevError> {
    match (supervision, window) {
        (Ok(()), window) => window,
        (Err(supervision), Ok(())) => Err(supervision.into()),
        (Err(supervision), Err(window)) => Err(DevError::WindowPhase(format!(
            "{supervision} The window also failed: {window}"
        ))),
    }
}

/// Starts a host-owned hello session for tests that must observe Bun/wire
/// coexistence without entering the GUI event loop.
///
/// # Errors
///
/// Returns [`DevError`] when doctor checks fail or the session cannot start.
pub fn start_dev_session(project_root: &Path) -> Result<HostOwnedHelloSession, DevError> {
    doctor_or_err(project_root)?;
    HostOwnedHelloSession::start(
        project_root,
        project_root.join("src/main.ts"),
        RestartPolicy::default(),
    )
    .map_err(DevError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(windows)]
    use std::time::Instant;

    fn window_phase_failure() -> keld_core::HelloSessionError {
        keld_core::HelloSessionError::WindowPhase {
            cause: "KELD-RUNTIME-002: child crashed 3 times within 30s".to_owned(),
        }
    }

    #[test]
    #[cfg(windows)]
    fn cleanup_sentinel_readiness_has_a_deadline_and_reaps_the_child() {
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 60",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("spawn blocked readiness fixture");
        let stdout = child.stdout.take().expect("readiness stdout");
        let stderr = child.stderr.take().expect("diagnostic stderr");
        let started = Instant::now();
        let Err(error) =
            await_windows_stage_cleanup_sentinel(child, stdout, stderr, Duration::from_millis(100))
        else {
            panic!("silent sentinel must time out");
        };
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(error.to_string().contains("timed out"), "{error}");
    }

    #[test]
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    fn shipping_dev_fails_closed_before_resources_on_an_unsupported_platform() {
        let error = run_dev(Path::new("unused-on-an-unsupported-platform"))
            .expect_err("unsupported no-flag host must fail closed");
        let rendered = error.to_string();
        assert!(rendered.contains("KELD-CLI-047"), "{rendered}");
        assert!(rendered.contains("platform availability"), "{rendered}");
    }

    #[test]
    fn clean_window_phase_preserves_the_window_result() {
        // The shipping success path: supervision fine, window closed normally.
        // `keld dev` must still exit 0.
        assert!(window_phase_outcome(Ok(()), Ok(())).is_ok());
    }

    #[test]
    fn clean_supervision_still_surfaces_a_window_failure() {
        let err = window_phase_outcome(Ok(()), Err(DevError::Runtime("wv boom".to_owned())))
            .expect_err("a window fault must not be swallowed");
        assert!(err.to_string().contains("wv boom"), "{err}");
    }

    #[test]
    fn window_phase_app_death_fails_the_run() {
        let err = window_phase_outcome(Err(window_phase_failure()), Ok(()))
            .expect_err("a dead app process must not exit 0 (KEL-105)");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CORE-033"), "{msg}");
        assert!(msg.contains("KELD-RUNTIME-002"), "{msg}");
        // The crash diagnostic must reach the user intact. `KELD-CLI-031`'s
        // registered fix is "re-run `keld doctor`", which contradicts
        // KELD-CORE-033's "fix the crash shown in the captured stderr" —
        // wrapping the one in the other shipped both at once.
        assert!(!msg.contains("KELD-CLI-031"), "{msg}");
        assert!(!msg.contains("keld doctor"), "{msg}");
        assert!(msg.starts_with("KELD-CORE-033"), "{msg}");
    }

    #[test]
    fn app_death_wins_over_a_window_error_without_discarding_it() {
        let err = window_phase_outcome(
            Err(window_phase_failure()),
            Err(DevError::Runtime("wv boom".to_owned())),
        )
        .expect_err("either failure alone must fail the run");
        let msg = err.to_string();
        assert!(
            msg.find("KELD-CORE-033") < msg.find("wv boom"),
            "supervision failure must lead, window error must follow: {msg}"
        );
        // The window's own error keeps its own registered fix — two faults,
        // two fixes. What must not happen is the crash diagnostic being
        // wrapped so that `KELD-CLI-031`'s advice overrides its own.
        assert!(msg.starts_with("KELD-CORE-033"), "{msg}");
    }

    #[test]
    fn finds_config_from_nested_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("app");
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(root.join("keld.config.ts"), "export default {}\n").expect("config");
        assert_eq!(
            find_project_root(&root.join("src")).as_deref(),
            Some(root.as_path())
        );
    }

    #[test]
    fn missing_config_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(find_project_root(dir.path()), None);
    }

    #[test]
    fn run_dev_echo_missing_config_is_cli_032() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = run_dev_echo(dir.path()).expect_err("missing config must fail before spawn");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CLI-032"), "{msg}");
        assert!(msg.contains("keld.config.ts"), "{msg}");
        assert!(msg.contains("[FAIL] project"), "{msg}");
        assert!(
            !msg.contains("bun child"),
            "must fail at doctor, not at spawn: {msg}"
        );
    }

    #[test]
    fn run_dev_echo_missing_renderer_is_cli_032() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::write(dir.path().join("src/main.ts"), "export {}\n").expect("main");
        let err = run_dev_echo(dir.path()).expect_err("missing renderer must fail before spawn");
        let msg = err.to_string();
        assert!(msg.contains("KELD-CLI-032"), "{msg}");
        assert!(msg.contains(RENDERER_LOAD_CODE), "{msg}");
        assert!(msg.contains("[FAIL] renderer"), "{msg}");
        assert!(msg.contains("index.html"), "{msg}");
        assert!(
            !msg.contains("bun child"),
            "must fail at doctor, not at spawn: {msg}"
        );
    }

    #[test]
    fn window_title_comes_from_config_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("keld.config.ts"),
            "export default {\n  name: \"from-config\",\n} as const;\n",
        )
        .expect("config");
        assert_eq!(hello_title_for_project(dir.path()), "from-config");
    }

    #[test]
    fn window_title_defaults_when_config_has_no_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
        assert_eq!(hello_title_for_project(dir.path()), DEFAULT_HELLO_TITLE);
        assert_eq!(DEFAULT_HELLO_TITLE, "Keld");
    }

    #[test]
    fn load_dev_window_html_uses_created_index_not_hello_constant() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = crate::create::create_project(dir.path(), "demo").expect("create");
        let html = load_dev_window_html(&root).expect("renderer");
        assert!(html.contains("<h1>demo</h1>"), "{html}");
        assert!(html.contains("<title>demo</title>"), "{html}");
        assert!(html.contains("Rendered by the Keld host webview"), "{html}");
        assert!(
            !html.contains("Hello from WKWebView"),
            "must not substitute HELLO_HTML: {html}"
        );
        assert!(
            !html.contains("Phase 1 window-on-screen"),
            "must not substitute HELLO_HTML: {html}"
        );
    }

    #[test]
    fn load_dev_window_html_follows_config_renderer_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("keld.config.ts"),
            "export default {\n  name: \"custom\",\n  renderer: \"ui/app.html\",\n} as const;\n",
        )
        .expect("config");
        fs::create_dir_all(dir.path().join("ui")).expect("ui");
        fs::write(
            dir.path().join("ui/app.html"),
            "<!DOCTYPE html><h1>kel29-renderer-marker</h1>\n",
        )
        .expect("html");
        let html = load_dev_window_html(dir.path()).expect("renderer");
        assert!(html.contains("kel29-renderer-marker"), "{html}");
        assert!(!html.contains("Hello from WKWebView"), "{html}");
    }

    #[test]
    fn load_dev_window_html_defaults_to_index_when_renderer_omitted() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
        fs::write(
            dir.path().join("index.html"),
            "<!DOCTYPE html><p>default-index</p>\n",
        )
        .expect("html");
        let html = load_dev_window_html(dir.path()).expect("renderer");
        assert!(html.contains("default-index"), "{html}");
    }

    #[test]
    fn load_dev_window_html_missing_file_is_cli_035() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("keld.config.ts"),
            "export default {\n  renderer: \"index.html\",\n} as const;\n",
        )
        .expect("config");
        let err = load_dev_window_html(dir.path()).expect_err("missing renderer");
        let msg = err.to_string();
        assert!(msg.contains(RENDERER_LOAD_CODE), "{msg}");
        assert!(msg.contains("index.html"), "{msg}");
        assert!(msg.contains("missing"), "{msg}");
        assert!(msg.contains("keld.config.ts"), "{msg}");
        assert!(
            !msg.contains("Hello from WKWebView"),
            "must not fall back to HELLO_HTML: {msg}"
        );
    }

    #[test]
    fn load_dev_window_html_rejects_parent_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("keld.config.ts"),
            "export default {\n  renderer: \"../outside.html\",\n} as const;\n",
        )
        .expect("config");
        let err = load_dev_window_html(dir.path()).expect_err("traversal must fail");
        let msg = err.to_string();
        assert!(msg.contains(RENDERER_LOAD_CODE), "{msg}");
        assert!(msg.contains("relative"), "{msg}");
        assert!(msg.contains("../outside.html"), "{msg}");
    }

    #[test]
    fn load_dev_window_html_rejects_absolute_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let abs = "/tmp/keld-dev-abs.html";
        #[cfg(windows)]
        let abs = r"C:\keld-dev-abs.html";
        fs::write(
            dir.path().join("keld.config.ts"),
            format!("export default {{\n  renderer: \"{abs}\",\n}} as const;\n"),
        )
        .expect("config");
        let err = load_dev_window_html(dir.path()).expect_err("absolute must fail");
        let msg = err.to_string();
        assert!(msg.contains(RENDERER_LOAD_CODE), "{msg}");
        assert!(msg.contains("relative"), "{msg}");
        assert!(msg.contains(abs), "{msg}");
    }
}
