//! Windows Bun generation owner: suspended creation, exact Job admission, and family retirement.
//!
//! This isolates a generation from the next one. The existing process-lifetime Host Job
//! remains the independent parent-death safety net. Neither Job is a substitute for LPAC.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read};
use std::os::windows::io::{AsHandle as _, AsRawHandle, RawHandle};
use std::os::windows::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus};
use std::time::Duration;

use crate::windows_job::{WindowsLaunchedProcess, WindowsProcessJob};
use crate::windows_lpac::{WindowsLpacStdio, WindowsSuspendedChild, create_private_pipe};

const FAMILY_RETIRE_DEADLINE: Duration = Duration::from_secs(10);

/// A complete, explicitly selected Windows role process input; it never executes a shell.
pub(crate) struct WindowsRoleCommand {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<OsString>,
    pub(crate) environment: Vec<(OsString, OsString)>,
    pub(crate) current_dir: PathBuf,
}

impl WindowsRoleCommand {
    /// Materializes std Command's inherited environment and its explicit edits.
    /// The native process launcher consumes a complete UTF-16 environment block.
    pub(crate) fn from_command(
        command: &Command,
        allow_dev_bun_selector: bool,
    ) -> io::Result<Self> {
        let requested_program = PathBuf::from(command.get_program());
        let current_dir = match command.get_current_dir() {
            Some(path) => path.to_path_buf(),
            None => std::env::current_dir()?,
        };
        if !current_dir.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows Bun role current directory must be absolute",
            ));
        }

        // Win32's =X: drive-cwd pseudo entries are not ordinary variables;
        // the shared native environment encoder intentionally rejects '=' keys.
        let mut environment: Vec<_> = std::env::vars_os()
            .filter(|(key, _)| !key.to_string_lossy().starts_with('='))
            .collect();
        for (key, value) in command.get_envs() {
            let folded = key.to_string_lossy().to_uppercase();
            environment.retain(|(existing, _)| existing.to_string_lossy().to_uppercase() != folded);
            if let Some(value) = value {
                environment.push((key.to_os_string(), value.to_os_string()));
            }
        }

        let program = if requested_program.is_absolute() {
            requested_program
        } else {
            // The existing developer bootstrap supplies "bun" by name. Resolve
            // only that exact executable to an absolute .exe before the
            // suspended native launch; never permit arbitrary relative images,
            // shell/batch wrappers or an alternate hidden fallback. This is
            // developer selection, NOT installed-image identity verification.
            let bare = requested_program.to_str().is_some_and(|name| {
                name.eq_ignore_ascii_case("bun") || name.eq_ignore_ascii_case("bun.exe")
            });
            if !bare || !allow_dev_bun_selector {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Windows Bun role requires an absolute image or the explicit developer Bun selector",
                ));
            }
            let path = environment
                .iter()
                .find(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case("PATH"))
                .map(|(_, value)| value)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "Windows developer Bun PATH is unavailable",
                    )
                })?;
            std::env::split_paths(path)
                .map(|directory| directory.join("bun.exe"))
                .find_map(|candidate| {
                    if candidate.is_file() {
                        std::fs::canonicalize(candidate).ok()
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "Windows developer Bun executable could not be located on PATH",
                    )
                })?
        };
        Ok(Self {
            program,
            args: command.get_args().map(OsString::from).collect(),
            environment,
            current_dir,
        })
    }
}

struct WindowsRoleChild {
    launch: WindowsLaunchedProcess,
    family: WindowsProcessJob,
    retired: bool,
}

impl WindowsRoleChild {
    fn retire(&mut self) -> io::Result<()> {
        if !self.retired {
            self.family
                .terminate_and_wait_attached(FAMILY_RETIRE_DEADLINE)
                .map_err(io::Error::other)?;
            self.retired = true;
        }
        Ok(())
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if !self.launch.has_exited().map_err(io::Error::other)? {
            return Ok(None);
        }
        // A direct Bun exit is not family retirement: children can retain IPC/stdio.
        self.retire()?;
        self.launch
            .wait(0)
            .map(ExitStatus::from_raw)
            .map(Some)
            .map_err(io::Error::other)
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        // This path follows shutdown or an observed exit. Do not return an exit
        // status that masks descendants still running in this generation.
        self.retire()?;
        self.launch
            .wait(10_000)
            .map(ExitStatus::from_raw)
            .map_err(io::Error::other)
    }

    fn kill(&mut self) -> io::Result<()> {
        self.retire()
    }
}

/// One standard output reader; a native pipe and a std child use the same capture owner.
pub(crate) enum WindowsCapturedReader {
    ChildOut(ChildStdout),
    ChildErr(ChildStderr),
    Native(File),
}

impl Read for WindowsCapturedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::ChildOut(reader) => reader.read(buf),
            Self::ChildErr(reader) => reader.read(buf),
            Self::Native(reader) => reader.read(buf),
        }
    }
}

impl AsRawHandle for WindowsCapturedReader {
    fn as_raw_handle(&self) -> RawHandle {
        match self {
            Self::ChildOut(reader) => reader.as_raw_handle(),
            Self::ChildErr(reader) => reader.as_raw_handle(),
            Self::Native(reader) => reader.as_raw_handle(),
        }
    }
}

enum Kind {
    Standard(Child),
    Generation(WindowsRoleChild),
}

/// Supervisor's Windows child, including the exact family-retirement responsibility.
pub(crate) struct WindowsSupervisedChild {
    kind: Kind,
    pub(crate) stdout: Option<WindowsCapturedReader>,
    pub(crate) stderr: Option<WindowsCapturedReader>,
}

impl WindowsSupervisedChild {
    pub(crate) fn from_standard(mut child: Child) -> Self {
        let stdout = child.stdout.take().map(WindowsCapturedReader::ChildOut);
        let stderr = child.stderr.take().map(WindowsCapturedReader::ChildErr);
        Self {
            kind: Kind::Standard(child),
            stdout,
            stderr,
        }
    }

    pub(crate) fn spawn_role(command: &WindowsRoleCommand) -> io::Result<Self> {
        // Create the kill-on-close owner before constructing a process. The child
        // is suspended and cannot execute until exact Job membership is read back.
        let mut family = WindowsProcessJob::create().map_err(io::Error::other)?;
        let (stdout_read, stdout_write) = create_private_pipe()?;
        let (stderr_read, stderr_write) = create_private_pipe()?;
        let stdin_null = File::open("NUL")?;
        let child = WindowsSuspendedChild::spawn_same_token_with_stdio(
            &command.program,
            &command.args,
            &command.environment,
            &command.current_dir,
            WindowsLpacStdio {
                stdin: stdin_null.as_handle(),
                stdout: stdout_write.as_handle(),
                stderr: stderr_write.as_handle(),
            },
        )
        .map_err(io::Error::other)?;
        let mut launch = WindowsLaunchedProcess::record(child).map_err(io::Error::other)?;
        let membership = family
            .assign_child(launch.child())
            .map_err(io::Error::other)?;
        launch.resume(&membership).map_err(io::Error::other)?;
        drop(stdout_write);
        drop(stderr_write);
        drop(stdin_null);
        Ok(Self {
            kind: Kind::Generation(WindowsRoleChild {
                launch,
                family,
                retired: false,
            }),
            stdout: Some(WindowsCapturedReader::Native(stdout_read)),
            stderr: Some(WindowsCapturedReader::Native(stderr_read)),
        })
    }

    pub(crate) fn id(&self) -> u32 {
        match &self.kind {
            Kind::Standard(child) => child.id(),
            Kind::Generation(child) => child.launch.child().id(),
        }
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match &mut self.kind {
            Kind::Standard(child) => child.try_wait(),
            Kind::Generation(child) => child.try_wait(),
        }
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        match &mut self.kind {
            Kind::Standard(child) => child.wait(),
            Kind::Generation(child) => child.wait(),
        }
    }

    pub(crate) fn kill(&mut self) -> io::Result<()> {
        match &mut self.kind {
            Kind::Standard(child) => child.kill(),
            Kind::Generation(child) => child.kill(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt as _;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    const FIXTURE: &str = "KELD_TEST_GENERATION_CHILD";
    const MARKER: &str = "KELD_TEST_GENERATION_PID_MARKER";
    const EXIT_MODE: &str = "KELD_TEST_GENERATION_EXIT_MODE";

    #[test]
    #[ignore = "private subprocess fixture"]
    fn generation_descendant_fixture() {
        assert_eq!(std::env::var(FIXTURE).as_deref(), Ok("1"));
        let marker = std::env::var_os(MARKER).expect("private PID marker path");
        let mut descendant = Command::new("cmd.exe")
            .args(["/C", "ping -n 60 127.0.0.1 >NUL"])
            .creation_flags(0x0000_0008) // DETACHED_PROCESS: no console host.
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn descendant");
        std::fs::write(marker, descendant.id().to_string())
            .expect("report exact direct descendant PID");
        if std::env::var(EXIT_MODE).as_deref() == Ok("natural") {
            // Abnormal child-lifecycle cell: the parent exits and its live
            // descendant must be retired by the generation Job, not by wait().
            std::process::exit(0);
        }
        // The parent remains alive until the generation owner terminates its Job.
        let _ = descendant.wait().expect("fixture descendant wait");
    }

    fn spawn_fixture(marker: &Path, natural_exit: bool) -> WindowsSupervisedChild {
        let mut command = Command::new(std::env::current_exe().expect("test fixture image"));
        command
            .args([
                "--exact",
                "windows_generation::tests::generation_descendant_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env(FIXTURE, "1")
            .env(MARKER, marker)
            .current_dir(std::env::current_dir().expect("test fixture directory"));
        if natural_exit {
            command.env(EXIT_MODE, "natural");
        } else {
            command.env_remove(EXIT_MODE);
        }
        WindowsSupervisedChild::spawn_role(
            &WindowsRoleCommand::from_command(&command, false).expect("absolute native inputs"),
        )
        .expect("suspended role launch and exact Job membership")
    }

    fn descendant_pid(marker: &Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(contents) = std::fs::read_to_string(marker) {
                return contents.parse().expect("numeric descendant PID");
            }
            assert!(
                Instant::now() < deadline,
                "role did not report its spawned descendant"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn generation_shutdown_retires_exact_live_descendant() {
        let directory = tempfile::tempdir().expect("test marker directory");
        let marker = directory.path().join("descendant.pid");
        let mut child = spawn_fixture(&marker, false);
        let pid = descendant_pid(&marker);
        let Kind::Generation(ref generation) = child.kind else {
            panic!("must use native generation Job");
        };
        let witness = generation
            .family
            .pin_test_live_member(pid)
            .expect("reported descendant is alive inside the exact Job");
        assert!(!witness.has_exited().expect("before-shutdown liveness"));
        child
            .kill()
            .expect("terminate entire generation Job and observe zero");
        let retired_at = Instant::now();
        assert!(
            witness
                .wait_until_exited(Duration::from_secs(5))
                .expect("retired descendant handle"),
            "reported descendant did not finish OS termination after Job retirement"
        );
        eprintln!(
            "descendant_exit_observation_delay_ms={}",
            retired_at.elapsed().as_millis()
        );
        let Kind::Generation(ref generation) = child.kind else {
            panic!("must use native generation Job");
        };
        assert_eq!(
            generation
                .family
                .active_processes()
                .expect("Job accounting"),
            0
        );
        let _ = child
            .wait()
            .expect("direct child reaped after family retirement");
    }

    #[test]
    fn natural_bun_exit_retires_descendant_before_returning_status() {
        let directory = tempfile::tempdir().expect("test marker directory");
        let marker = directory.path().join("descendant.pid");
        let mut child = spawn_fixture(&marker, true);
        let pid = descendant_pid(&marker);
        let Kind::Generation(ref generation) = child.kind else {
            panic!("must use native generation Job");
        };
        let witness = generation
            .family
            .pin_test_live_member(pid)
            .expect("live grandson of exiting Bun generation");
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll and fully retire generation") {
                break status;
            }
            assert!(Instant::now() < deadline, "Bun fixture did not exit");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.code(), Some(0), "parent's natural exit is preserved");
        let retired_at = Instant::now();
        assert!(
            witness
                .wait_until_exited(Duration::from_secs(5))
                .expect("retired descendant handle"),
            "a successor must not start over a live old-generation descendant"
        );
        eprintln!(
            "natural_descendant_exit_observation_delay_ms={}",
            retired_at.elapsed().as_millis()
        );
        let Kind::Generation(ref generation) = child.kind else {
            panic!("must use native generation Job");
        };
        assert_eq!(
            generation
                .family
                .active_processes()
                .expect("Job accounting"),
            0
        );
    }

    #[test]
    fn role_environment_overrides_and_path_admission_are_explicit() {
        let mut command = Command::new(std::env::current_exe().expect("absolute image"));
        command
            .env("Path", "first")
            .env("PATH", "second")
            .env("BUN_OPTIONS", "--preload=untrusted")
            .env_remove("bun_options");
        let inputs = WindowsRoleCommand::from_command(&command, false).expect("native role input");
        let path_values = inputs
            .environment
            .iter()
            .filter(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case("PATH"))
            .collect::<Vec<_>>();
        assert_eq!(path_values.len(), 1, "only one case-folded PATH survives");
        assert_eq!(path_values[0].1, OsString::from("second"));
        assert!(
            inputs
                .environment
                .iter()
                .all(|(key, _)| !key.to_string_lossy().eq_ignore_ascii_case("BUN_OPTIONS")),
            "the explicitly removed Bun preload must stay removed"
        );

        let developer_bun = Command::new("bun");
        assert!(
            WindowsRoleCommand::from_command(&developer_bun, false).is_err(),
            "only explicit developer staging may select Bun from PATH"
        );
        let resolved = WindowsRoleCommand::from_command(&developer_bun, true)
            .expect("the development Bun selector resolves to an absolute executable");
        assert!(resolved.program.is_absolute());
        assert_eq!(
            resolved
                .program
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_lowercase(),
            "bun.exe",
        );
        let unrelated_relative = Command::new("cmd.exe");
        assert!(
            WindowsRoleCommand::from_command(&unrelated_relative, true).is_err(),
            "native roles must not resolve arbitrary executables from PATH"
        );
    }
}
