//! `keld dev` — run the hello template with IPC echo + window (KEL-29).

use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

#[cfg(target_os = "macos")]
use keld_core::run_hello_window_titled;
use keld_core::{DEFAULT_HELLO_TITLE, read_config_title};

use crate::doctor::{all_ok, run_checks};
use crate::echo_link::EchoServer;

/// Errors starting a dev session.
#[derive(Debug)]
pub enum DevError {
    /// Environment or project checks failed.
    Doctor(String),
    /// Child process or thread failure.
    Io(io::Error),
    /// IPC or host failure surfaced as text.
    Runtime(String),
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

/// Resolves the `keld` binary path for child processes (`KELD_BIN`).
fn current_keld_bin() -> io::Result<PathBuf> {
    std::env::current_exe()
}

/// Doctor checks, then one Bun IPC echo round-trip. Does not open a window.
///
/// `keld_bin` is passed to the child as `KELD_BIN` so the template can spawn
/// `keld ipc-client`. Tests pass `CARGO_BIN_EXE_keld`; `keld dev` passes
/// [`std::env::current_exe`].
///
/// # Errors
///
/// Returns [`DevError`] when checks fail, Bun cannot be spawned, or echo fails.
pub fn run_dev_echo(project_root: &Path, keld_bin: &Path) -> Result<DevEchoResult, DevError> {
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
    let server = EchoServer::start(ready_tx);
    ready_rx
        .recv()
        .map_err(|_| DevError::Runtime("echo server failed to start".to_owned()))?;
    let link = server.link();

    let bun_main = project_root.join("src/main.ts");
    let child = Command::new("bun")
        .arg("run")
        .arg(&bun_main)
        .current_dir(project_root)
        .env("KELD_APP_LINK", &link)
        .env("KELD_BIN", keld_bin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let output = KillOnDrop::new(child).wait_with_output()?;
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;

    server.join()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DevError::Runtime(format!(
            "bun exited with status {}. stderr: {stderr}",
            output.status.code().unwrap_or(1)
        )));
    }

    Ok(DevEchoResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        link,
    })
}

/// Runs `keld dev` in `project_root`: echo session, then the hello window.
///
/// # Errors
///
/// Returns [`DevError`] when checks fail, Bun exits non-zero, or the window fails.
pub fn run_dev(project_root: &Path) -> Result<(), DevError> {
    let keld_bin = current_keld_bin()?;
    let _echo = run_dev_echo(project_root, &keld_bin)?;
    open_dev_window(project_root)
}

fn open_dev_window(project_root: &Path) -> Result<(), DevError> {
    let title = hello_title_for_project(project_root);
    #[cfg(target_os = "macos")]
    {
        run_hello_window_titled(&title).map_err(|e| DevError::Runtime(e.to_string()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = title;
        eprintln!(
            "keld dev: webview window not available on this OS yet; IPC echo already completed."
        );
        Ok(())
    }
}

/// Kills and reaps a child if the session is dropped before `wait`.
struct KillOnDrop {
    child: Option<Child>,
}

impl KillOnDrop {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn wait_with_output(mut self) -> io::Result<Output> {
        let child = self
            .child
            .take()
            .ok_or_else(|| io::Error::other("bun child missing"))?;
        child.wait_with_output()
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn pid_is_running(pid: u32) -> bool {
        #[cfg(unix)]
        {
            Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .status()
                .is_ok_and(|s| s.success())
        }
        #[cfg(windows)]
        {
            let output = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
                .output();
            match output {
                Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()),
                Err(_) => false,
            }
        }
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
        let err = run_dev_echo(dir.path(), Path::new("/nonexistent/keld"))
            .expect_err("missing config must fail before spawn");
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
    fn kill_on_drop_reaps_bun_child() {
        let child = Command::new("bun")
            .args(["-e", "process.stdin.resume()"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("bun must be on PATH (same contract as bun_echo)");
        let pid = child.id();
        assert!(
            pid_is_running(pid),
            "child must be alive before drop; pid={pid}"
        );
        drop(KillOnDrop::new(child));
        assert!(
            !pid_is_running(pid),
            "KillOnDrop must reap bun; pid {pid} still running"
        );
    }
}
