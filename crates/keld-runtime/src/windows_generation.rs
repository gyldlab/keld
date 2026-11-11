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

/// Test-only cut points. Production builds never include this type or its branches.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestGenerationFault {
    None,
    BeforeAssign,
    BeforeResume,
    Retirement,
    ActiveZeroRefusal,
}

/// A complete, explicitly selected Windows role process input; it never executes a shell.
pub(crate) struct WindowsRoleCommand {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<OsString>,
    pub(crate) environment: Vec<(OsString, OsString)>,
    pub(crate) current_dir: PathBuf,
    #[cfg(test)]
    test_fault: TestGenerationFault,
}

impl WindowsRoleCommand {
    /// Materializes std Command's inherited environment and its explicit edits.
    /// The native process launcher consumes a complete UTF-16 environment block.
    pub(crate) fn from_command(command: &Command) -> io::Result<Self> {
        let program = PathBuf::from(command.get_program());
        if !program.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows Bun roles require an absolute executable image path",
            ));
        }
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
        Ok(Self {
            program,
            args: command.get_args().map(OsString::from).collect(),
            environment,
            current_dir,
            #[cfg(test)]
            test_fault: TestGenerationFault::None,
        })
    }
}

struct WindowsRoleChild {
    launch: WindowsLaunchedProcess,
    family: WindowsProcessJob,
    retired: bool,
    #[cfg(test)]
    test_fault: TestGenerationFault,
}

impl WindowsRoleChild {
    fn retire(&mut self) -> io::Result<()> {
        #[cfg(test)]
        if self.test_fault == TestGenerationFault::Retirement {
            return Err(io::Error::other(
                "injected generation Job retirement refusal",
            ));
        }
        #[cfg(test)]
        if self.test_fault == TestGenerationFault::ActiveZeroRefusal {
            return self
                .family
                .terminate_then_refuse_active_zero_for_test(FAMILY_RETIRE_DEADLINE)
                .map_err(io::Error::other);
        }
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
        #[cfg(test)]
        if command.test_fault == TestGenerationFault::BeforeAssign {
            // Both native owners unwind without any first instruction.
            return Err(io::Error::other(
                "injected before generation Job assignment",
            ));
        }
        let membership = family
            .assign_child(launch.child())
            .map_err(io::Error::other)?;
        #[cfg(test)]
        if command.test_fault == TestGenerationFault::BeforeResume {
            // The exact child remains suspended, and the Job still kills on close.
            return Err(io::Error::other("injected before generation thread resume"));
        }
        launch.resume(&membership).map_err(io::Error::other)?;
        drop(stdout_write);
        drop(stderr_write);
        drop(stdin_null);
        Ok(Self {
            kind: Kind::Generation(WindowsRoleChild {
                launch,
                family,
                retired: false,
                #[cfg(test)]
                test_fault: command.test_fault,
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
    const CAPTURE_MODE: &str = "KELD_TEST_GENERATION_CAPTURE_MODE";

    #[test]
    #[ignore = "private subprocess fixture"]
    fn generation_descendant_fixture() {
        assert_eq!(std::env::var(FIXTURE).as_deref(), Ok("1"));
        let marker =
            std::path::PathBuf::from(std::env::var_os(MARKER).expect("private PID marker path"));
        let capture_mode = std::env::var(CAPTURE_MODE).as_deref() == Ok("1");
        let ready_marker = marker.with_extension("ready");
        let shell_command = if capture_mode {
            String::from(
                "echo KELD_GRANDCHILD_STDOUT & echo KELD_GRANDCHILD_STDERR 1>&2 & echo ready > descendant.ready & ping -n 60 127.0.0.1 >NUL",
            )
        } else {
            String::from("ping -n 60 127.0.0.1 >NUL")
        };
        let mut command = Command::new("cmd.exe");
        command
            .args(["/C", &shell_command])
            .creation_flags(0x0000_0008) // DETACHED_PROCESS: no console host.
            .stdin(Stdio::null());
        if capture_mode {
            // Grandchild retains the actual anonymous stdio pipes inherited
            // from the suspended native role. Closing only the parent is insufficient.
            command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        } else {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let mut descendant = command.spawn().expect("spawn descendant");
        std::fs::write(&marker, descendant.id().to_string())
            .expect("report exact direct descendant PID");
        if capture_mode {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ready_marker.is_file() {
                assert!(
                    Instant::now() < deadline,
                    "grandchild did not report its pipe writes"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        if std::env::var(EXIT_MODE).as_deref() == Ok("natural") {
            // Abnormal child-lifecycle cell: the parent exits and its live
            // descendant must be retired by the generation Job, not by wait().
            std::process::exit(0);
        }
        // The parent remains alive until the generation owner terminates its Job.
        let _ = descendant.wait().expect("fixture descendant wait");
    }

    fn fixture_command(marker: &Path, natural_exit: bool, capture_mode: bool) -> Command {
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
            .current_dir(marker.parent().expect("test fixture directory"));
        if natural_exit {
            command.env(EXIT_MODE, "natural");
        } else {
            command.env_remove(EXIT_MODE);
        }
        if capture_mode {
            command.env(CAPTURE_MODE, "1");
        } else {
            command.env_remove(CAPTURE_MODE);
        }
        command
    }

    fn spawn_fixture(
        marker: &Path,
        natural_exit: bool,
        capture_mode: bool,
    ) -> WindowsSupervisedChild {
        let command = fixture_command(marker, natural_exit, capture_mode);
        WindowsSupervisedChild::spawn_role(
            &WindowsRoleCommand::from_command(&command).expect("absolute native inputs"),
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
        let mut child = spawn_fixture(&marker, false, false);
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
        let mut child = spawn_fixture(&marker, true, false);
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

    fn bounded_capture_read(mut reader: WindowsCapturedReader) -> Vec<u8> {
        use std::io::Read as _;
        let (tx, rx) = std::sync::mpsc::channel();
        let owner = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let read = reader.read_to_end(&mut bytes).map(|_| bytes);
            let _ = tx.send(read);
        });
        let bytes = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Job retirement must release inherited stdio writers")
            .expect("captured pipe read");
        owner.join().expect("bounded capture owner");
        bytes
    }

    #[test]
    fn natural_exit_retires_grandchild_inherited_capture_before_successor() {
        let directory = tempfile::tempdir().expect("test marker directory");
        let marker = directory.path().join("descendant.pid");
        let mut child = spawn_fixture(&marker, true, true);
        let pid = descendant_pid(&marker);
        let Kind::Generation(ref generation) = child.kind else {
            panic!("must use native generation Job");
        };
        let witness = generation
            .family
            .pin_test_live_member(pid)
            .expect("live descendant retained native standard handles");
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().expect("retire generation before result") {
                break status;
            }
            assert!(Instant::now() < deadline, "capture fixture did not exit");
            std::thread::sleep(Duration::from_millis(20));
        };
        if status.code() != Some(0) {
            let stderr = bounded_capture_read(child.stderr.take().expect("fixture stderr"));
            panic!(
                "fixture returned {:?}: {}",
                status.code(),
                String::from_utf8_lossy(&stderr)
            );
        }
        assert!(
            witness
                .wait_until_exited(Duration::from_secs(5))
                .expect("exact descendant process exit")
        );
        let Kind::Generation(ref generation) = child.kind else {
            panic!("must use native generation Job");
        };
        assert_eq!(
            generation
                .family
                .active_processes()
                .expect("authoritative Job-zero"),
            0,
        );
        let stdout = bounded_capture_read(child.stdout.take().expect("stdout reader"));
        let stderr = bounded_capture_read(child.stderr.take().expect("stderr reader"));
        assert!(
            String::from_utf8_lossy(&stdout).contains("KELD_GRANDCHILD_STDOUT"),
            "grandchild must actually inherit and write the native stdout pipe"
        );
        assert!(
            String::from_utf8_lossy(&stderr).contains("KELD_GRANDCHILD_STDERR"),
            "grandchild must actually inherit and write the native stderr pipe"
        );
    }

    #[test]
    fn independent_writer_prevents_capture_eof_until_released() {
        // Negative control: a surviving writer must prevent EOF. Otherwise the
        // inherited-grandchild test could pass without checking the real race.
        use std::io::Read as _;
        let (mut read, write) = create_private_pipe().expect("private pipe");
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = read.read_to_end(&mut bytes).map(|_| bytes);
            let _ = tx.send(result);
        });
        assert!(
            matches!(
                rx.recv_timeout(Duration::from_millis(100)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ),
            "an unretired writer must keep the reader open"
        );
        drop(write);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2))
                .expect("EOF observed after writer release")
                .expect("pipe bytes"),
            Vec::<u8>::new()
        );
        reader.join().expect("test capture worker");
    }

    #[test]
    fn refused_suspended_admission_never_executes_bun_marker() {
        let dir = tempfile::tempdir_in(std::env::current_dir().expect("D: working dir"))
            .expect("private D: fixture");
        for fault in [
            TestGenerationFault::BeforeAssign,
            TestGenerationFault::BeforeResume,
        ] {
            let marker = dir.path().join(format!("{fault:?}.pid"));
            let command = fixture_command(&marker, false, false);
            let mut role =
                WindowsRoleCommand::from_command(&command).expect("absolute native inputs");
            role.test_fault = fault;
            let refusal = WindowsSupervisedChild::spawn_role(&role)
                .err()
                .expect("injected refused admission");
            assert!(
                refusal.to_string().contains("injected"),
                "refusal must preserve the expected phase: {refusal}"
            );
            assert!(
                !marker.exists(),
                "the test executable must never reach its first marker write before Job admission"
            );
        }
    }

    #[test]
    fn refused_retirement_keeps_job_kill_on_close_fallback() {
        let dir = tempfile::tempdir_in(std::env::current_dir().expect("D: working dir"))
            .expect("private D: fixture");
        let marker = dir.path().join("failure-descendant.pid");
        let mut child = spawn_fixture(&marker, true, false);
        let pid = descendant_pid(&marker);
        let Kind::Generation(ref mut generation) = child.kind else {
            panic!("must own generation Job");
        };
        let witness = generation
            .family
            .pin_test_live_member(pid)
            .expect("live exact nested descendant");
        generation.test_fault = TestGenerationFault::Retirement;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !generation.launch.has_exited().expect("direct child exit") {
            assert!(Instant::now() < deadline, "direct fixture did not exit");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            child.try_wait().is_err(),
            "failed Job-zero proof must not become a successful exit status"
        );
        drop(child);
        assert!(
            witness
                .wait_until_exited(Duration::from_secs(5))
                .expect("retained descendant handle"),
            "dropped faulted generation must still kill its descendants via KILL_ON_JOB_CLOSE"
        );
    }

    struct FailingRetirementPreparer {
        marker: PathBuf,
        prepare_calls: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    impl crate::ChildPreparer for FailingRetirementPreparer {
        type Lease = crate::NoLease;

        fn prepare(
            &mut self,
            _attempt: u32,
        ) -> Result<crate::PreparedChild<Self::Lease>, crate::RuntimeError> {
            self.prepare_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let command = fixture_command(&self.marker, true, false);
            let mut native =
                WindowsRoleCommand::from_command(&command).expect("exact absolute fixture image");
            native.test_fault = TestGenerationFault::ActiveZeroRefusal;
            Ok(crate::PreparedChild {
                command: crate::PreparedCommand::WindowsRole(native),
                lease: crate::NoLease,
            })
        }
    }

    #[test]
    fn failed_generation_retirement_is_terminal_without_successor() {
        let dir = tempfile::tempdir_in(std::env::current_dir().expect("D: working dir"))
            .expect("private D: fixture");
        let marker = dir.path().join("supervisor-descendant.pid");
        let prepare_calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let supervisor = crate::Supervisor::start_prepared(
            crate::RestartPolicy::default(),
            FailingRetirementPreparer {
                marker: marker.clone(),
                prepare_calls: std::sync::Arc::clone(&prepare_calls),
            },
        )
        .expect("native first generation admitted");
        let outcome = supervisor.wait_for_outcome();
        let crate::SupervisorOutcome::Failed(ref error) = outcome else {
            panic!("failed Job retirement must report Failed, got {outcome:?}");
        };
        assert!(
            error
                .to_string()
                .contains("injected generation Job active-zero accounting refusal"),
            "the original retirement failure must remain observable: {error}"
        );
        assert!(
            marker.exists(),
            "fixture must spawn a real child and descendant before retirement fails"
        );
        assert_eq!(
            prepare_calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "a failed Job-zero witness must never admit a second generation",
        );
    }

    const NESTED_HOST_FIXTURE: &str = "KELD_TEST_GENERATION_NESTED_HOST";

    #[test]
    #[ignore = "private native nested-Host Job subprocess"]
    fn nested_outer_host_job_fixture() {
        assert_eq!(
            std::env::var(NESTED_HOST_FIXTURE).as_deref(),
            Ok("1"),
            "only the parent test may launch this fixture"
        );
        let _host_death =
            crate::windows_job::install_host_death_job().expect("host joins nested Job");
        let dir = tempfile::tempdir_in(std::env::current_dir().expect("D: working dir"))
            .expect("private D: fixture");
        let marker = dir.path().join("nested-descendant.pid");
        let mut child = spawn_fixture(&marker, true, false);
        let pid = descendant_pid(&marker);
        let Kind::Generation(ref generation) = child.kind else {
            panic!("new child requires generation Job");
        };
        let witness = generation
            .family
            .pin_test_live_member(pid)
            .expect("actual nested Job must retain exact grandchild");
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().expect("nested Job retirement") {
                break status;
            }
            assert!(Instant::now() < deadline, "nested fixture did not retire");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.code(), Some(0));
        assert!(
            witness
                .wait_until_exited(Duration::from_secs(5))
                .expect("retained grandchild handle")
        );
        let Kind::Generation(ref generation) = child.kind else {
            panic!("native generation Job must survive until end of proof");
        };
        assert_eq!(
            generation
                .family
                .active_processes()
                .expect("nested Job accounting"),
            0
        );
        println!("KELD_NESTED_HOST_AND_GENERATION_ZERO");
    }

    #[test]
    fn nested_outer_host_and_generation_job_complete_retirement() {
        // The test runner itself is admitted suspended to a non-breakaway outer
        // Job, modeling the ordinary launcher. Inside it the real Host installs
        // its separate death Job, then Bun joins its per-generation Job. This
        // exercises three nested Jobs plus the real grandchild.
        let mut command = Command::new(std::env::current_exe().expect("test binary"));
        command
            .args([
                "--exact",
                "windows_generation::tests::nested_outer_host_job_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env(NESTED_HOST_FIXTURE, "1")
            .current_dir(std::env::current_dir().expect("D: working dir"));
        let mut child = WindowsSupervisedChild::spawn_role(
            &WindowsRoleCommand::from_command(&command).expect("absolute native launcher input"),
        )
        .expect("outer launcher Job must admit suspended Host");
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().expect("outer launcher Job retirement") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "three-level nested Job fixture did not exit"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let stdout = bounded_capture_read(child.stdout.take().expect("launcher stdout"));
        let stderr = bounded_capture_read(child.stderr.take().expect("launcher stderr"));
        assert!(
            status.success()
                && String::from_utf8_lossy(&stdout)
                    .contains("KELD_NESTED_HOST_AND_GENERATION_ZERO"),
            "three-level nested launcher/Host/Generation failed: stdout={} stderr={}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
        );
        let Kind::Generation(ref launcher) = child.kind else {
            panic!("native outer launcher requires its own Job");
        };
        assert_eq!(
            launcher
                .family
                .active_processes()
                .expect("outer launcher Job accounting"),
            0,
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
        let inputs = WindowsRoleCommand::from_command(&command).expect("native role input");
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
            WindowsRoleCommand::from_command(&developer_bun).is_err(),
            "native supervisor never searches PATH for a Bun executable"
        );
        let unrelated_relative = Command::new("cmd.exe");
        assert!(
            WindowsRoleCommand::from_command(&unrelated_relative).is_err(),
            "native roles must not resolve arbitrary executables from PATH"
        );
    }
}
