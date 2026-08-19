//! `keld dev` — run the hello template with IPC echo + window (KEL-29).

use std::fmt::Write as _;
use std::fs;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use keld_core::{
    DEFAULT_HELLO_TITLE, DEFAULT_RENDERER, read_config_renderer, read_config_title,
    run_hello_window_html,
};
use keld_runtime::{RestartPolicy, Supervisor, SupervisorOutcome};

use crate::doctor::{all_ok, renderer_load_message, renderer_path_problem, run_checks};
use crate::echo_link::EchoServer;

pub use crate::doctor::RENDERER_LOAD_CODE;

/// Errors starting a dev session.
#[derive(Debug)]
pub enum DevError {
    /// Environment or project checks failed.
    Doctor(String),
    /// Child process or thread failure.
    Io(io::Error),
    /// IPC or host failure surfaced as text.
    Runtime(String),
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
            Self::Doctor(msg) => write!(f, "{msg}"),
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

/// Doctor checks, then one Bun IPC echo round-trip. Does not open a window.
///
/// The template speaks kipc directly (`src/kipc.ts`, KEL-30) — Bun needs only
/// `KELD_APP_LINK`, not a `keld` binary path to shell out to. Bun runs under
/// a [`keld_runtime::Supervisor`] (KEL-70): a crash (non-zero exit) is
/// retried per [`RestartPolicy::default`] before this call gives up, instead
/// of failing on the first non-zero exit.
///
/// # Errors
///
/// Returns [`DevError`] when checks fail, Bun cannot be spawned, or the
/// supervisor's crash-loop breaker trips.
pub fn run_dev_echo(project_root: &Path) -> Result<DevEchoResult, DevError> {
    let checks = run_checks(Some(project_root));
    if !all_ok(&checks) {
        let mut msg = String::from("KELD-CLI-032: environment checks failed:\n");
        for check in &checks {
            let mark = if check.ok { "ok" } else { "FAIL" };
            let _ = writeln!(msg, "  [{mark}] {} — {}", check.label, check.detail);
        }
        return Err(DevError::Doctor(msg));
    }

    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let server = EchoServer::start(&ready_tx)?;
    ready_rx
        .recv()
        .map_err(|_| DevError::Runtime("echo server failed to start".to_owned()))?;
    let link = server.link();
    let link_for_child = link.clone();

    let bun_main = project_root.join("src/main.ts");
    let project_root = project_root.to_path_buf();
    let supervisor = Supervisor::start(RestartPolicy::default(), move || {
        let mut cmd = Command::new("bun");
        cmd.arg("run")
            .arg(&bun_main)
            .current_dir(&project_root)
            .env("KELD_APP_LINK", &link_for_child);
        cmd
    })
    .map_err(|e| DevError::Runtime(e.to_string()))?;

    let outcome = supervisor.wait_for_outcome();
    let output = supervisor.output();
    io::stdout().write_all(output.stdout.as_bytes())?;
    io::stderr().write_all(output.stderr.as_bytes())?;

    match outcome {
        SupervisorOutcome::CrashLoop(err) | SupervisorOutcome::Failed(err) => {
            let _ = server.shutdown();
            Err(DevError::Runtime(err.to_string()))
        }
        SupervisorOutcome::Stopped => {
            server.shutdown()?;
            Ok(DevEchoResult {
                stdout: output.stdout,
                link,
            })
        }
    }
}

/// Runs `keld dev` in `project_root`: echo session, then the hello window.
///
/// # Errors
///
/// Returns [`DevError`] when checks fail, Bun exits non-zero, or the window fails.
pub fn run_dev(project_root: &Path) -> Result<(), DevError> {
    let _echo = run_dev_echo(project_root)?;
    open_dev_window(project_root)
}

fn open_dev_window(project_root: &Path) -> Result<(), DevError> {
    let title = hello_title_for_project(project_root);
    let html = load_dev_window_html(project_root)?;
    run_hello_window_html(&title, &html).map_err(|e| DevError::Runtime(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        #[cfg(unix)]
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
