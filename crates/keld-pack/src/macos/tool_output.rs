//! Bounded native-tool I/O and lifetime. One owner polls nonblocking pipes and
//! the exact child under one monotonic deadline; no capture thread can outlive it.

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use serde::Serialize;

use super::MacosPackError;

const MAX_TOOL_OUTPUT_BYTES: usize = 1024 * 1024;
// Image creation can process the admitted 8 GiB input bound. This is a failure
// ceiling, not a delay or a retry: completed commands return immediately.
const MAX_NATIVE_TOOL_DURATION: Duration = Duration::from_secs(600);
const CHILD_CLEANUP_DURATION: Duration = Duration::from_secs(2);
const EXIT_OBSERVATION_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Serialize)]
pub(super) struct ToolOutput {
    pub stdout: String,
    pub stderr: String,
}

pub(super) fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    command
}

#[derive(Default)]
struct OutputBytes {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct CapturePipes<'a> {
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    input: &'a [u8],
    written: usize,
    retained: usize,
    output: OutputBytes,
}

fn nonblocking(fd: &impl AsFd) -> io::Result<()> {
    let flags = fcntl_getfl(fd).map_err(io::Error::from)?;
    fcntl_setfl(fd, flags | OFlags::NONBLOCK).map_err(io::Error::from)
}

fn deadline_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "native tool execution deadline exceeded",
    )
}

/// Readiness waits are capped so a child that closes both output pipes cannot
/// defer exit observation. The remaining timeout always comes from one deadline.
fn poll_until(fds: &mut [PollFd<'_>], deadline: Instant) -> io::Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(deadline_error());
    }
    let interval = remaining.min(EXIT_OBSERVATION_INTERVAL);
    let timeout = Timespec {
        tv_sec: 0,
        tv_nsec: interval.subsec_nanos().into(),
    };
    match poll(fds, Some(&timeout)) {
        Ok(_) | Err(rustix::io::Errno::INTR) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

impl<'a> CapturePipes<'a> {
    fn new(child: &mut Child, input: Option<&'a [u8]>) -> io::Result<Self> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("spawned native tool has no stdout pipe"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("spawned native tool has no stderr pipe"))?;
        let mut stdin = if input.is_some() {
            Some(
                child
                    .stdin
                    .take()
                    .ok_or_else(|| io::Error::other("spawned native tool has no stdin pipe"))?,
            )
        } else {
            None
        };
        let input = input.unwrap_or_default();
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        if let Some(pipe) = &stdin {
            nonblocking(pipe)?;
        }
        if input.is_empty() {
            stdin = None;
        }
        Ok(Self {
            stdin,
            stdout: Some(stdout),
            stderr: Some(stderr),
            input,
            written: 0,
            retained: 0,
            output: OutputBytes::default(),
        })
    }

    fn complete(&self) -> bool {
        self.stdin.is_none() && self.stdout.is_none() && self.stderr.is_none()
    }

    fn wait_for_activity(&self, deadline: Instant) -> io::Result<()> {
        let mut fds = Vec::with_capacity(3);
        if let Some(pipe) = &self.stdout {
            fds.push(PollFd::new(pipe, PollFlags::IN));
        }
        if let Some(pipe) = &self.stderr {
            fds.push(PollFd::new(pipe, PollFlags::IN));
        }
        if let Some(pipe) = &self.stdin {
            fds.push(PollFd::new(pipe, PollFlags::OUT));
        }
        poll_until(&mut fds, deadline)
    }

    fn exchange(&mut self) -> io::Result<()> {
        read_available(
            &mut self.stdout,
            &mut self.output.stdout,
            &mut self.retained,
        )?;
        read_available(
            &mut self.stderr,
            &mut self.output.stderr,
            &mut self.retained,
        )?;
        if let Some(stdin) = &mut self.stdin {
            match stdin.write(&self.input[self.written..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "native tool stdin accepted no bytes",
                    ));
                }
                Ok(written) => self.written += written,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
            if self.written == self.input.len() {
                self.stdin = None;
            }
        }
        Ok(())
    }
}

fn read_available<R: Read>(
    pipe: &mut Option<R>,
    retained: &mut Vec<u8>,
    total: &mut usize,
) -> io::Result<()> {
    let Some(source) = pipe.as_mut() else {
        return Ok(());
    };
    let mut buffer = [0_u8; 8192];
    match source.read(&mut buffer) {
        Ok(0) => *pipe = None,
        Ok(read) => {
            let next = total
                .checked_add(read)
                .filter(|next| *next <= MAX_TOOL_OUTPUT_BYTES)
                .ok_or_else(|| io::Error::other("native tool stdout+stderr exceeded 1 MiB"))?;
            retained.extend_from_slice(&buffer[..read]);
            *total = next;
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) => {}
        Err(error) => return Err(error),
    }
    Ok(())
}

enum CaptureFailure {
    Io(io::Error),
    ExitObservation(io::Error),
}

impl From<io::Error> for CaptureFailure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn capture_child(
    child: &mut Child,
    input: Option<&[u8]>,
    deadline: Instant,
) -> Result<(ExitStatus, OutputBytes), CaptureFailure> {
    let mut pipes = CapturePipes::new(child, input)?;
    let mut status = None;
    loop {
        if Instant::now() >= deadline {
            return Err(deadline_error().into());
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(observed) => status = observed,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(CaptureFailure::ExitObservation(error)),
            }
        }
        if pipes.complete()
            && let Some(status) = status
        {
            return Ok((status, pipes.output));
        }
        pipes.wait_for_activity(deadline)?;
        if Instant::now() >= deadline {
            return Err(deadline_error().into());
        }
        pipes.exchange()?;
    }
}

/// On failure all capture pipes have already dropped. Kill only the exact owned
/// child and observe reaping for a bounded cleanup interval. A deadline cannot
/// turn into an unbounded wait, including when the OS refuses termination.
fn terminate_owned_child(
    child: &mut Child,
    phase: &'static str,
    detail: impl std::fmt::Display,
) -> MacosPackError {
    drop(child.stdin.take());
    drop(child.stdout.take());
    drop(child.stderr.take());
    let deadline = Instant::now() + CHILD_CLEANUP_DURATION;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return MacosPackError::new(
                    phase,
                    format!("{detail}; owned child already exited and reaped"),
                );
            }
            Ok(None) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                return MacosPackError::new(
                    phase,
                    format!("{detail}; child exit state is unknown; no signal sent: {error}"),
                );
            }
        }
        if let Err(error) = poll_until(&mut [], deadline) {
            return MacosPackError::new(
                phase,
                format!(
                    "{detail}; child exit state was not confirmed before cleanup deadline; no signal sent: {error}"
                ),
            );
        }
    }
    // Signal only after Ok(None) confirms this unreaped child is still ours.
    // A hard observation error (including an external reap) must not authorize
    // signaling a numeric PID which the OS might already have reused.
    let kill_error = child.kill().err();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return MacosPackError::new(
                    phase,
                    format!("{detail}; owned child terminated and reaped"),
                );
            }
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => break format!("exit observation failed: {error}"),
        }
        if let Err(error) = poll_until(&mut [], deadline) {
            break format!("bounded cleanup could not confirm exit: {error}");
        }
    };
    let kill_detail = kill_error.map_or_else(
        || "termination signal sent".to_owned(),
        |error| format!("termination failed: {error}"),
    );
    MacosPackError::new(
        phase,
        format!(
            "{detail}; owned child {} was not confirmed reaped: {outcome}; {kill_detail}",
            child.id()
        ),
    )
}

pub(super) fn run(
    command: &mut Command,
    phase: &'static str,
) -> Result<ToolOutput, MacosPackError> {
    capture(command, None, phase)
}

pub(super) fn run_with_timeout(
    command: &mut Command,
    phase: &'static str,
    timeout: Duration,
) -> Result<ToolOutput, MacosPackError> {
    capture_with_timeout(command, None, phase, timeout)
}

fn capture(
    command: &mut Command,
    input: Option<&[u8]>,
    phase: &'static str,
) -> Result<ToolOutput, MacosPackError> {
    capture_with_timeout(command, input, phase, MAX_NATIVE_TOOL_DURATION)
}

fn capture_with_timeout(
    command: &mut Command,
    input: Option<&[u8]>,
    phase: &'static str,
    timeout: Duration,
) -> Result<ToolOutput, MacosPackError> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| MacosPackError::new(phase, "native tool deadline is not representable"))?;
    if input.is_some_and(|bytes| bytes.len() > MAX_TOOL_OUTPUT_BYTES) {
        return Err(MacosPackError::new(
            phase,
            "native tool input exceeds the bounded plist conversion size",
        ));
    }
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| MacosPackError::new(phase, error))?;
    let (status, bytes) = match capture_child(&mut child, input, deadline) {
        Ok(result) => result,
        Err(CaptureFailure::Io(error)) => {
            return Err(terminate_owned_child(&mut child, phase, error));
        }
        Err(CaptureFailure::ExitObservation(error)) => {
            // Preserve the first hard observation failure. Re-observing cannot
            // restore authority over a numeric PID which might have been reused.
            return Err(MacosPackError::new(
                phase,
                format!("native tool exit state is unknown; no signal sent: {error}"),
            ));
        }
    };
    let stdout =
        String::from_utf8(bytes.stdout).map_err(|error| MacosPackError::new(phase, error))?;
    let stderr =
        String::from_utf8(bytes.stderr).map_err(|error| MacosPackError::new(phase, error))?;
    if !status.success() {
        return Err(MacosPackError::new(
            phase,
            format!(
                "native tool exited {status}: {}{}",
                stdout.trim(),
                stderr.trim()
            ),
        ));
    }
    Ok(ToolOutput { stdout, stderr })
}

pub(super) fn plist_json(
    plist: &str,
    phase: &'static str,
) -> Result<serde_json::Value, MacosPackError> {
    // A successful hdiutil attach must not depend on a staging-file write before
    // the exact observed device acquires its cleanup owner.
    let mut converter = command("/usr/bin/plutil");
    converter.args(["-convert", "json", "-o", "-", "-"]);
    let output = capture(&mut converter, Some(plist.as_bytes()), phase)?;
    serde_json::from_str(&output.stdout).map_err(|error| MacosPackError::new(phase, error))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)] // Native child OS oracle and fixture failures must fail the test.

    use std::fs;
    use std::os::unix::process::CommandExt as _;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::time::{Duration, Instant};

    use super::{MAX_TOOL_OUTPUT_BYTES, capture, capture_with_timeout, command, run};

    const DEADLINE_FIXTURE_ROOT: &str = "KELD_PACK_DEADLINE_FIXTURE_ROOT";

    #[derive(Clone, Copy, Debug)]
    enum DeadlineFixture {
        Silent,
        ClosedPipes,
        BlockedStdin,
    }

    #[test]
    fn native_tool_deadline_covers_silent_child() {
        check_deadline_fixture(
            "macos::tool_output::tests::native_tool_deadline_covers_silent_child",
            DeadlineFixture::Silent,
        );
    }

    #[test]
    fn native_tool_deadline_covers_closed_pipes_live_child() {
        check_deadline_fixture(
            "macos::tool_output::tests::native_tool_deadline_covers_closed_pipes_live_child",
            DeadlineFixture::ClosedPipes,
        );
    }

    #[test]
    fn native_tool_deadline_covers_blocked_stdin() {
        check_deadline_fixture(
            "macos::tool_output::tests::native_tool_deadline_covers_blocked_stdin",
            DeadlineFixture::BlockedStdin,
        );
    }

    fn fixture_wait(child: &mut Child, deadline: Instant) -> Option<ExitStatus> {
        loop {
            if let Some(status) = child.try_wait().expect("observe fixture child exit") {
                return Some(status);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            // The oracle is the observed child exit. This bounded empty poll only
            // limits CPU use while the outer deadline remains a kill switch.
            let interval = remaining.min(Duration::from_millis(10));
            let timeout = rustix::event::Timespec {
                tv_sec: 0,
                tv_nsec: interval.subsec_nanos().into(),
            };
            rustix::event::poll(&mut [], Some(&timeout)).expect("bounded fixture observation");
        }
    }

    fn terminate_fixture_group(child: &mut Child) {
        // This unreaped child is the group leader created by process_group(0).
        // Its PID cannot be reused while the watchdog retains this Child.
        let group = format!("-{}", child.id());
        let mut terminate = Command::new("/bin/kill")
            .args(["-KILL", "--", &group])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("terminate only the owned regression process group");
        let deadline = Instant::now() + Duration::from_secs(2);
        if fixture_wait(&mut terminate, deadline).is_none() {
            let _ = terminate.kill();
        }
        assert!(
            fixture_wait(&mut terminate, Instant::now() + Duration::from_secs(2)).is_some(),
            "watchdog signal command did not retire"
        );
        assert!(
            fixture_wait(child, Instant::now() + Duration::from_secs(2)).is_some(),
            "watchdog could not reap its exact regression child"
        );
    }

    fn check_deadline_fixture(test_name: &str, fixture: DeadlineFixture) {
        if let Some(root) = std::env::var_os(DEADLINE_FIXTURE_ROOT) {
            exercise_deadline_child(&std::path::PathBuf::from(root), fixture);
            return;
        }
        let directory = tempfile::tempdir().expect("owned isolated deadline fixture");
        let log_path = directory.path().join("watchdog.log");
        let log = fs::File::create(&log_path).expect("isolated watchdog log");
        let mut child = Command::new(std::env::current_exe().expect("libtest executable"))
            .args(["--exact", test_name, "--nocapture"])
            .env(DEADLINE_FIXTURE_ROOT, directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().expect("stdout log handle")))
            .stderr(Stdio::from(log))
            .process_group(0)
            .spawn()
            .expect("isolated native-tool deadline test");
        let status = fixture_wait(&mut child, Instant::now() + Duration::from_secs(5));
        if status.is_none() {
            terminate_fixture_group(&mut child);
        }
        let log = fs::read_to_string(log_path).expect("native deadline result log");
        assert!(
            status.is_some_and(|status| status.success()),
            "native-tool deadline watchdog fired or child failed; fixture={fixture:?}; log={log}"
        );
    }

    fn exercise_deadline_child(root: &std::path::Path, fixture: DeadlineFixture) {
        let pid_path = root.join("native-child.pid");
        let script = if matches!(fixture, DeadlineFixture::ClosedPipes) {
            "echo $$ > \"$1\"; exec 1>&- 2>&-; while :; do :; done"
        } else {
            "echo $$ > \"$1\"; while :; do :; done"
        };
        let mut native = command("/bin/sh");
        native.args(["-c", script, "sh"]).arg(&pid_path);
        let input = matches!(fixture, DeadlineFixture::BlockedStdin)
            .then(|| vec![b'x'; MAX_TOOL_OUTPUT_BYTES]);
        let error = capture_with_timeout(
            &mut native,
            input.as_deref(),
            "native completion deadline",
            Duration::from_millis(250),
        )
        .expect_err("a silent live native tool must reach the absolute deadline");
        let diagnostic = error.to_string();
        assert!(
            diagnostic.contains("execution deadline exceeded"),
            "{diagnostic}"
        );
        assert!(
            diagnostic.ends_with("; owned child terminated and reaped")
                || diagnostic.ends_with("; owned child already exited and reaped"),
            "{diagnostic}"
        );
        let pid = fs::read_to_string(pid_path).expect("observed native child PID");
        let mut probe = Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("native child existence probe");
        let status = fixture_wait(&mut probe, Instant::now() + Duration::from_secs(2));
        if status.is_none() {
            let _ = probe.kill();
            let _ = fixture_wait(&mut probe, Instant::now() + Duration::from_secs(2));
        }
        assert!(
            status.is_some_and(|status| !status.success()),
            "native child remains after the bounded failure: {diagnostic}"
        );
    }

    #[test]
    fn finite_native_output_accepts_exact_ceiling_and_rejects_combined_overflow() {
        let mut exactly = command("/usr/bin/head");
        exactly.args(["-c", "1048576", "/dev/zero"]);
        let output = run(&mut exactly, "native exact output ceiling")
            .expect("one MiB is a valid native output");
        assert_eq!(output.stdout.len(), MAX_TOOL_OUTPUT_BYTES);
        assert_eq!(output.stderr, "");

        let mut combined = command("/bin/sh");
        combined.args([
            "-c",
            "/usr/bin/head -c 600000 /dev/zero; /usr/bin/head -c 600000 /dev/zero 1>&2",
        ]);
        let error = run(&mut combined, "combined pipe ceiling")
            .expect_err("stdout and stderr must share the same one MiB cap");
        assert!(error.to_string().contains("exceeded 1 MiB"), "{error}");
    }

    #[test]
    fn infinite_native_stdout_is_rejected_terminated_and_reaped_at_real_byte_cap() {
        let directory = tempfile::tempdir().expect("owned native process fixture");
        for (name, script) in [
            (
                "stdout",
                "echo $$ > \"$1\"; exec /usr/bin/yes NATIVE-OUTPUT",
            ),
            (
                "stderr",
                "echo $$ > \"$1\"; exec /usr/bin/yes NATIVE-OUTPUT 1>&2",
            ),
        ] {
            let pid = directory.path().join(if name == "stdout" {
                "stdout.pid"
            } else {
                "stderr.pid"
            });
            let mut native = command("/bin/sh");
            native.args(["-c", script, "sh"]).arg(&pid);
            // The shell execs 'yes' with its original PID, so kill -0 after
            // the function returns is an independent OS reaping observation.
            let error = run(&mut native, "bounded output test").expect_err("infinite native tool");
            assert!(error.to_string().contains("exceeded 1 MiB"), "{error}");
            let numeric_pid = fs::read_to_string(&pid).expect("native child PID record");
            let check = Command::new("/bin/kill")
                .args(["-0", numeric_pid.trim()])
                .output()
                .expect("native process existence probe");
            assert!(!check.status.success(), "owned native child was not reaped");
        }
    }

    #[test]
    fn piped_native_input_uses_the_same_bounded_output_capture() {
        let mut native = command("/usr/bin/yes");
        let error = capture(&mut native, Some(b"valid input bytes"), "piped native tool")
            .expect_err("native tool must exceed output limit");
        assert!(error.to_string().contains("exceeded 1 MiB"), "{error}");
    }
}
