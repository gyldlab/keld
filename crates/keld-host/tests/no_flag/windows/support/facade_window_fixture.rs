//! Host/Bun/control/pipe owner for real Windows facade acceptance.

#[path = "facade_project_fixture.rs"]
mod project;
pub(crate) use project::{FacadeFixture, LastWindowAction};

#[path = "facade_native_observation.rs"]
mod native;

#[path = "facade_release_observation.rs"]
mod release_observation;
pub(crate) use native::{
    assert_retired, capture, census, ephemeral_profile_dir, retained_descendants, same_tree,
};

use crate::support::control::{
    accept_control_or_host_failure, read_control_line_or_host_failure, try_read_control_line,
};
use crate::support::process::{
    PIPE_DRAIN_KILL_SWITCH, PipeText, assert_process_signaled, pipe_line_after, pipe_text_by,
    read_pipe_on_thread,
};
use crate::support::signed_process::SignedStateProcessGuard;
use crate::{PRODUCT_DEADLINE, startup_kill_switch, ui_end_kill_switch};
use serde_json::Value;
use std::io::{BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::OwnedHandle;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const BUILD_FAILURE: &str = "KELD_TEST_WINDOWS_FAIL_BUILD";
pub(crate) const NATIVE_HOLD: &str = "KELD_TEST_WINDOWS_NATIVE_HOLD";

pub(crate) struct FacadeRun {
    pub(crate) host: SignedStateProcessGuard,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    stdout: PipeText,
    stderr: PipeText,
    pub(crate) bun_pid: u32,
    pub(crate) link: String,
    retained: Vec<(u32, OwnedHandle)>,
    control_capture: Option<PipeText>,
}

impl FacadeRun {
    pub(crate) fn spawn(mut command: Command, control: &TcpListener) -> (Self, Value) {
        let mut host = SignedStateProcessGuard::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("real facade Host"),
        );
        let stdout = read_pipe_on_thread(host.child_mut().stdout.take().expect("Host stdout"));
        let stderr = read_pipe_on_thread(host.child_mut().stderr.take().expect("Host stderr"));
        let (reader, writer, bun_pid, link, ready) = generation(control, &mut host);
        (
            Self {
                host,
                reader,
                writer,
                stdout,
                stderr,
                bun_pid,
                link,
                retained: Vec::new(),
                control_capture: None,
            },
            ready,
        )
    }

    pub(crate) fn reconnect(&mut self, control: &TcpListener) -> Value {
        assert!(
            self.control_capture.is_none(),
            "continuous control capture is terminal"
        );
        let (reader, writer, bun_pid, link, ready) = generation(control, &mut self.host);
        self.reader = reader;
        self.writer = writer;
        self.bun_pid = bun_pid;
        self.link = link;
        ready
    }

    /// Transfer the original buffered reader, not a fresh socket clone, before
    /// an intentional crash/long hold. Winsock reset can discard unread bytes;
    /// all later observations use this live capture, never a second reader.
    pub(crate) fn capture_control(&mut self) {
        assert!(
            self.control_capture.is_none(),
            "control already continuously captured"
        );
        let unused = self
            .writer
            .try_clone()
            .expect("retained unused control read handle");
        let reader = std::mem::replace(&mut self.reader, BufReader::new(unused));
        self.control_capture = Some(read_pipe_on_thread(reader));
    }

    /// Read-only access to the existing single continuous capture. Callers own
    /// byte cursors, never another socket reader, and cannot reconnect afterward.
    pub(crate) fn captured_control(&self) -> &PipeText {
        self.control_capture
            .as_ref()
            .expect("continuous capture owns control reads")
    }

    pub(crate) fn pid(&self) -> u32 {
        self.host.host_pid()
    }

    pub(crate) fn try_send(&mut self, command: &str) -> std::io::Result<()> {
        writeln!(self.writer, "{command}")?;
        self.writer.flush()
    }

    pub(crate) fn send(&mut self, command: &str) {
        self.try_send(command).expect("facade control command");
    }

    pub(crate) fn read(&mut self, expected: &str) -> Value {
        let (kind, value) = self.next();
        assert_eq!(kind, expected, "actual facade response: {kind} {value}");
        value
    }

    pub(crate) fn next(&mut self) -> (String, Value) {
        assert!(
            self.control_capture.is_none(),
            "continuous capture owns control reads"
        );
        let line = read_control_line_or_host_failure(
            &mut self.reader,
            self.host.child_mut(),
            "facade record",
        );
        let (kind, json) = line
            .split_once(' ')
            .unwrap_or_else(|| panic!("malformed facade record: {line}"));
        (
            kind.to_owned(),
            serde_json::from_str(json).expect("facade record JSON"),
        )
    }

    pub(crate) fn trace(&self) -> String {
        pipe_text_by(&self.stderr, Instant::now())
    }

    pub(crate) fn stderr(&self) -> &PipeText {
        &self.stderr
    }

    pub(crate) fn try_next_by(&mut self, deadline: Instant) -> Result<(String, Value), String> {
        if self.control_capture.is_some() {
            return Err("continuous capture owns control reads".to_owned());
        }
        let line = try_read_control_line(&mut self.reader, deadline)?;
        let (kind, json) = line
            .split_once(' ')
            .ok_or_else(|| format!("malformed facade record: {line}"))?;
        let value =
            serde_json::from_str(json).map_err(|error| format!("facade JSON: {error}; {line}"))?;
        Ok((kind.to_owned(), value))
    }

    pub(crate) fn try_marker_by(
        &mut self,
        prefix: &str,
        after: usize,
        deadline: Instant,
    ) -> Result<(String, usize), String> {
        loop {
            if let Some(line) = pipe_line_after(&self.stderr, prefix, after) {
                return Ok(line);
            }
            if let Some(status) = self.host.child_mut().try_wait().expect("observe Host") {
                // A process handle can signal before the capture thread reads its
                // final write. Recheck after bounded pipe drain before reporting absence.
                let trace = pipe_text_by(&self.stderr, Instant::now() + PIPE_DRAIN_KILL_SWITCH);
                if let Some(line) = pipe_line_after(&self.stderr, prefix, after) {
                    return Ok(line);
                }
                return Err(format!("Host exited before {prefix}: {status}\n{trace}"));
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "missing {prefix} at its observation bound\n{}",
                    self.trace()
                ));
            }
            thread::park_timeout(Duration::from_millis(10));
        }
    }

    pub(crate) fn marker_or_exit(&mut self, prefix: &str, after: usize) -> Option<(String, usize)> {
        match self.try_marker_by(prefix, after, Instant::now() + PRODUCT_DEADLINE) {
            Ok(value) => Some(value),
            Err(_)
                if self
                    .host
                    .child_mut()
                    .try_wait()
                    .expect("observe Host")
                    .is_some() =>
            {
                None
            }
            Err(error) => panic!("{error}"),
        }
    }

    pub(crate) fn marker(&mut self, prefix: &str, after: usize) -> (String, usize) {
        self.try_marker_by(prefix, after, Instant::now() + PRODUCT_DEADLINE)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Keep exact kernel process identities before the terminal. The normal exit
    /// assertions run before the guard's emergency cleanup can terminate anything.
    pub(crate) fn retain_descendants(&mut self) {
        self.retained
            .extend(retained_descendants(self.host.child_mut()));
        assert!(
            self.retained.iter().any(|(pid, _)| *pid == self.bun_pid),
            "live Bun missing from independent native census"
        );
    }

    pub(crate) fn finish(self, deadline: Instant) -> (ExitStatus, String) {
        let (status, trace, _) = self.finish_observed(deadline);
        (status, trace)
    }

    pub(crate) fn finish_observed(self, deadline: Instant) -> (ExitStatus, String, String) {
        let Self {
            host,
            stdout,
            stderr,
            retained,
            mut reader,
            writer,
            control_capture,
            ..
        } = self;
        let status = host.wait(deadline);
        // Keep the control connection until Host/Bun retirement; then retain every
        // unread fixture response, including raw-call timeouts whose results are
        // explicitly not AC25(b)'s native oracle.
        drop(writer);
        let control = if let Some(capture) = control_capture {
            pipe_text_by(&capture, Instant::now() + PIPE_DRAIN_KILL_SWITCH)
        } else {
            reader
                .get_mut()
                .set_read_timeout(Some(PRODUCT_DEADLINE))
                .expect("terminal control drain bound");
            let mut control = String::new();
            match reader.read_to_string(&mut control) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
                Err(error) => panic!("terminal control drain: {error}; {control}"),
            }
            control
        };
        let drained = Instant::now() + PIPE_DRAIN_KILL_SWITCH;
        let stdout = pipe_text_by(&stdout, drained);
        let stderr = pipe_text_by(&stderr, drained);
        for (pid, handle) in retained {
            assert_process_signaled(&handle, &format!("facade-owned descendant {pid}"));
        }
        println!(
            "KELD_GH1188_FACADE_OUTPUT status={status}\nstdout:\n{stdout}\nstderr:\n{stderr}\ncontrol:\n{control}"
        );
        (status, stderr, control)
    }

    pub(crate) fn quit(mut self) -> String {
        self.retain_descendants();
        self.send("QUIT");
        self.read("QUIT_ACCEPTED");
        self.finish_quit()
    }

    /// Complete the same ordinary Quit after its actual acceptance was consumed
    /// by the sole continuous reader. All release/process assertions stay here.
    pub(crate) fn finish_quit(self) -> String {
        let (status, stderr) = self.finish(Instant::now() + ui_end_kill_switch());
        assert!(status.success(), "facade Quit failed: {status}\n{stderr}");
        assert!(
            stderr.contains("armed_by=Quit"),
            "missing terminal arm provenance:\n{stderr}"
        );
        assert!(
            stderr.contains("releasing=0"),
            "unretired native release:\n{stderr}"
        );
        assert!(
            stderr.contains("KELD_T2_RELEASE_WITNESS completed=true"),
            "native process release witness missing:\n{stderr}"
        );
        stderr
    }
}

fn record(
    reader: &mut BufReader<TcpStream>,
    host: &mut SignedStateProcessGuard,
    expected: &str,
) -> Value {
    let line = read_control_line_or_host_failure(reader, host.child_mut(), expected);
    let (kind, json) = line
        .split_once(' ')
        .unwrap_or_else(|| panic!("malformed facade record: {line}"));
    assert_eq!(kind, expected, "actual facade response: {line}");
    serde_json::from_str(json).expect("facade record JSON")
}

pub(crate) fn number(value: &Value, field: &str) -> u64 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("missing numeric {field}: {value}"))
}

pub(crate) fn marker_number(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .expect("numeric marker operand")
        .parse()
        .expect("native marker number")
}

pub(crate) fn ordered(trace: &str, markers: &[String]) {
    let mut cursor = 0;
    for marker in markers {
        let found = trace[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing ordered marker {marker} after {cursor}:\n{trace}"));
        cursor += found + marker.len();
    }
}

fn generation(
    control: &TcpListener,
    host: &mut SignedStateProcessGuard,
) -> (BufReader<TcpStream>, TcpStream, u32, String, Value) {
    let stream = accept_control_or_host_failure(
        control,
        host.child_mut(),
        Instant::now() + startup_kill_switch(),
    );
    let writer = stream.try_clone().expect("facade control writer");
    let mut reader = BufReader::new(stream);
    let bound = record(&mut reader, host, "BOUND");
    let pid = u32::try_from(bound["pid"].as_u64().expect("Bun PID")).expect("Win32 PID");
    host.observe_bun(pid);
    let link = bound["link"].as_str().expect("Host-minted link").to_owned();
    let ready = record(&mut reader, host, "READY");
    assert_eq!(ready["ready"], true, "public app Ready");
    (reader, writer, pid, link, ready)
}
