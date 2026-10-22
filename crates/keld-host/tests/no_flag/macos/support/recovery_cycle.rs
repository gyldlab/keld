use crate::support::DARK_BG;
use crate::support::EVENT_DEADLINE;
use crate::support::MARKER;
use crate::support::TITLE;
use crate::support::control::read_control_line;
use crate::support::control::wait_child_output_observing;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::await_same_native_windows;
use crate::support::native_window::native_window_rows;
use crate::support::native_window::native_windows;
use crate::support::process::await_process_gone;
use crate::support::process::kill_pid;
use crate::support::process::parent_process;
use crate::support::process::process_exists;
use crate::support::process::process_group;
use crate::support::process::signal_process_group;
use crate::support::product::ProductFixture;
use crate::support::renderer::Beacon;
use control_failure::ControlRecord;
use control_failure::RecoveryControlFailure;
use std::fs;
use std::io::BufReader;
use std::io::Read as _;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::Child;
use std::process::ChildStdin;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Instant;

pub(crate) struct RecoveryCycle {
    pub(crate) host: Option<Child>,
    pub(crate) dev_lease_writer: Option<ChildStdin>,
    pub(crate) host_pid: u32,
    pub(crate) listener: UnixListener,
    pub(crate) window: Vec<u32>,
    pub(crate) current: Option<RecoveryGeneration>,
    pub(crate) process_groups: Vec<u32>,
}

#[derive(Clone)]
pub(crate) struct RecoveryEvidence {
    pub(crate) guardian_pid: u32,
    pub(crate) bun_pid: u32,
    pub(crate) descendant_pid: u32,
    pub(crate) app_link: String,
    pub(crate) endpoint: PathBuf,
    pub(crate) token: String,
}

impl RecoveryCycle {
    pub(crate) fn launch(fixture: &ProductFixture, name: &str) -> Self {
        let beacon = Beacon::bind(MARKER);
        fs::write(
            fixture.project.join("index.html"),
            format!(
                "<!doctype html>{DARK_BG}<title>{TITLE}</title><p id=marker>{MARKER}</p><img src=\"http://127.0.0.1:{}/{MARKER}\">\n",
                beacon.port()
            ),
        )
        .expect("T3 renderer with exact beacon");
        let stage = fixture.stage();
        let control_path = fixture.root.path().join(format!("{name}.sock"));
        let listener = UnixListener::bind(&control_path).expect("bind T3 fixture control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking T3 fixture control");
        let mut presentation = fixture.observe_initial_window();
        let mut command = Command::new(stage.host());
        command
            .env("KELD_DEV_LEASE", "stdin-v1")
            .stdin(Stdio::piped())
            .env("KELD_T1B_CONTROL", &control_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("launch T3 no-flag host");
        let dev_lease_writer = child.stdin.take();
        let host_pid = child.id();
        let mut cleanup = ShippingLaunchCleanup::new(child);
        let mut current = RecoveryGeneration::accept(&listener, "initial");
        current.expect_ready_and_echoes();
        beacon.assert_exact();
        assert_eq!(parent_process(current.guardian_pid), host_pid);
        assert_eq!(process_group(current.bun_pid), current.bun_pid);
        assert_eq!(process_group(current.descendant_pid), current.bun_pid);
        let first_group = current.bun_pid;
        cleanup.bun_group = Some(first_group);
        let mut cycle = Self {
            host: Some(cleanup.release()),
            dev_lease_writer,
            host_pid,
            listener,
            window: Vec::new(),
            current: Some(current),
            process_groups: vec![first_group],
        };
        let window = presentation.expect_initial(host_pid, "initial T3 native window");
        assert_eq!(window.len(), 1, "initial T3 native window: {window:?}");
        cycle.window = window;
        cycle
    }

    pub(crate) fn crash_and_recover(&mut self) -> RecoveryEvidence {
        self.trigger_and_recover(b"CRASH\n")
    }

    pub(crate) fn close_link_and_recover(&mut self) -> RecoveryEvidence {
        self.trigger_and_recover(b"CLOSE_LINK\n")
    }

    pub(crate) fn trigger_and_recover(&mut self, command: &[u8]) -> RecoveryEvidence {
        let mut retired = self.current.take().expect("live generation");
        let evidence = retired.evidence();
        retired
            .writer
            .write_all(command)
            .expect("terminate current T3 generation");
        let mut successor = match RecoveryGeneration::try_accept(&self.listener) {
            Ok(successor) => successor,
            Err(error) => {
                let output = self.wait_host();
                panic!("{}", replacement_control_failure_summary(error, &output));
            }
        };
        successor.expect_ready_and_echoes();
        assert_eq!(
            evidence.guardian_pid, successor.guardian_pid,
            "recovery replaced the persistent guardian"
        );
        assert_ne!(
            evidence.bun_pid, successor.bun_pid,
            "Bun generation was reused"
        );
        assert_ne!(
            evidence.app_link, successor.app_link,
            "successor reused the retired endpoint/token"
        );
        assert_ne!(
            evidence.endpoint, successor.endpoint,
            "successor reused endpoint"
        );
        assert_ne!(evidence.token, successor.token(), "successor reused token");
        assert_eq!(
            await_same_native_windows(self.host_pid, TITLE, &self.window),
            self.window,
            "Bun recovery replaced or closed the host-owned native window; target-PID CoreGraphics rows: {}",
            native_window_rows(self.host_pid)
        );
        assert!(
            UnixStream::connect(&evidence.endpoint).is_err(),
            "retired generation endpoint accepted a stale reconnect"
        );
        await_process_gone(evidence.bun_pid);
        await_process_gone(evidence.descendant_pid);
        assert!(
            process_exists(self.host_pid) && process_exists(evidence.guardian_pid),
            "recoverable Bun crash terminated the host or guardian"
        );
        self.process_groups.push(successor.bun_pid);
        self.current = Some(successor);
        evidence
    }

    pub(crate) fn current_evidence(&self) -> RecoveryEvidence {
        self.current
            .as_ref()
            .expect("current generation")
            .evidence()
    }

    pub(crate) fn quit_and_expect_success(&mut self) {
        let current = self.current.as_mut().expect("current generation");
        current
            .writer
            .write_all(b"QUIT\n")
            .expect("Quit T3 generation");
        current.expect_line("QUIT_REPLY");
        current.expect_line("LINK_EOF");
        let output = self.wait_host();
        assert!(
            output.status.success(),
            "T3 orderly exit failed: {output:?}"
        );
        assert!(
            matches!(self.listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "accepted Quit provisioned a successor generation"
        );
        self.assert_current_group_gone();
    }

    pub(crate) fn kill_host_and_expect_current_group_reaped(&mut self) {
        let status = self
            .host
            .as_mut()
            .expect("live T3 host")
            .kill()
            .and_then(|()| self.host.as_mut().expect("live T3 host").wait())
            .expect("SIGKILL only recovered host");
        assert_eq!(status.signal(), Some(9));
        self.host.take();
        self.current
            .as_mut()
            .expect("current generation")
            .expect_line("LINK_EOF");
        self.assert_current_group_gone();
    }

    pub(crate) fn kill_guardian_and_expect_current_group_reaped(&mut self) {
        let current = self.current_evidence();
        kill_pid(current.guardian_pid);
        let output = self.wait_host();
        assert!(!output.status.success(), "guardian death became success");
        let stderr = String::from_utf8(output.stderr).expect("guardian-death stderr UTF-8");
        assert!(stderr.contains("KELD-CORE-033"), "{stderr}");
        assert!(stderr.contains("KELD-RUNTIME-013"), "{stderr}");
        self.assert_current_group_gone();
    }

    pub(crate) fn wait_host(&mut self) -> Output {
        self.wait_host_observing(|| {})
    }

    pub(crate) fn wait_host_observing(&mut self, observe: impl FnMut()) -> Output {
        // Observing exit must not inject CLI death: lease EOF starts accepted
        // shutdown and can overtake even an acknowledged Bun crash.
        let output = wait_child_output_observing(
            self.host.take().expect("live T3 host"),
            EVENT_DEADLINE,
            observe,
        );
        drop(self.dev_lease_writer.take());
        output
    }

    pub(crate) fn assert_current_group_gone(&mut self) {
        if let Some(current) = &self.current {
            await_process_gone(current.bun_pid);
            await_process_gone(current.descendant_pid);
            await_process_gone(current.guardian_pid);
        }
        assert_eq!(native_windows(self.host_pid, TITLE), [] as [u32; 0]);
        self.process_groups.clear();
    }
}

impl Drop for RecoveryCycle {
    fn drop(&mut self) {
        if let Some(host) = self.host.as_mut() {
            let _ = host.kill();
            let _ = host.wait();
        }
        for group in &self.process_groups {
            let _ = signal_process_group("-KILL", *group);
        }
    }
}

/// Recovery admission is a fixture failure, never permission to dump raw
/// Host stdout/stderr or private control fields into test diagnostics.
/// The failure site is a typed, payload-free value, so its name is the whole
/// diagnostic; it is a hint, not an IPC or process authority.
fn replacement_control_failure_summary(error: RecoveryControlFailure, output: &Output) -> String {
    format!(
        "recovery replacement unavailable error_site={error:?} host_success={} stdout_bytes={} stderr_bytes={}",
        output.status.success(),
        output.stdout.len(),
        output.stderr.len()
    )
}

pub(crate) struct RecoveryGeneration {
    pub(crate) guardian_pid: u32,
    pub(crate) bun_pid: u32,
    pub(crate) descendant_pid: u32,
    pub(crate) app_link: String,
    pub(crate) endpoint: PathBuf,
    pub(crate) reader: BufReader<UnixStream>,
    pub(crate) writer: UnixStream,
}

impl RecoveryGeneration {
    pub(crate) fn accept(listener: &UnixListener, label: &str) -> Self {
        Self::try_accept(listener).unwrap_or_else(|error| panic!("{label}: {error:?}"))
    }

    fn try_accept(listener: &UnixListener) -> Result<Self, RecoveryControlFailure> {
        let deadline = Instant::now() + EVENT_DEADLINE;
        let (stream, hello, descendant) =
            collect_initial_control_until(listener, deadline, &mut |_| {})?;
        let mut hello_fields = hello.split_whitespace();
        if hello_fields.next() != Some("HELLO") {
            return Err(RecoveryControlFailure::RecordMalformed(
                ControlRecord::Hello,
            ));
        }
        let bun_pid = numeric_control_field(hello_fields.next(), ControlRecord::Hello)?;
        let app_link = hello_fields
            .next()
            .ok_or(RecoveryControlFailure::HelloAppLinkMissing)?
            .to_owned();
        let endpoint = PathBuf::from(
            app_link
                .rsplit_once('#')
                .ok_or(RecoveryControlFailure::HelloAppLinkMalformed)?
                .0,
        );
        let mut descendant_fields = descendant.split_whitespace();
        if descendant_fields.next() != Some("DESCENDANT") {
            return Err(RecoveryControlFailure::RecordMalformed(
                ControlRecord::Descendant,
            ));
        }
        let descendant_pid =
            numeric_control_field(descendant_fields.next(), ControlRecord::Descendant)?;
        let guardian_pid = parent_process(bun_pid);
        if Instant::now() >= deadline {
            return Err(RecoveryControlFailure::HandshakeAfterDeadline);
        }
        let reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|error| RecoveryControlFailure::ReaderClone(error.kind()))?,
        );
        Ok(Self {
            guardian_pid,
            bun_pid,
            descendant_pid,
            app_link,
            endpoint,
            reader,
            writer: stream,
        })
    }

    pub(crate) fn expect_ready_and_echoes(&mut self) {
        self.expect_line("READY");
        self.expect_line("ECHO1");
        self.expect_line("ECHO2");
    }

    pub(crate) fn expect_line(&mut self, expected: &str) {
        assert_eq!(read_control_line(&mut self.reader), expected);
    }

    pub(crate) fn evidence(&self) -> RecoveryEvidence {
        RecoveryEvidence {
            guardian_pid: self.guardian_pid,
            bun_pid: self.bun_pid,
            descendant_pid: self.descendant_pid,
            app_link: self.app_link.clone(),
            endpoint: self.endpoint.clone(),
            token: self.token().to_owned(),
        }
    }

    pub(crate) fn token(&self) -> &str {
        self.app_link
            .rsplit_once('#')
            .expect("recovery app link token")
            .1
    }
}

const MAX_INITIAL_CONTROL_LINE_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug)]
enum NativeRecoveryStage {
    BeforeAccept,
    AfterAccept,
    BeforeByte {
        line: u8,
        received: usize,
    },
    AfterLf {
        line: u8,
    },
    /// The accepted stream is blocking again; only the final clock check remains.
    AfterNormalize,
}

fn numeric_control_field(
    field: Option<&str>,
    record: ControlRecord,
) -> Result<u32, RecoveryControlFailure> {
    field
        .ok_or(RecoveryControlFailure::RecordPidMissing(record))?
        .parse()
        .map_err(|_| RecoveryControlFailure::RecordPidInvalid(record))
}

fn accept_recovery_until(
    listener: &UnixListener,
    deadline: Instant,
    observe: &mut impl FnMut(NativeRecoveryStage),
) -> Result<UnixStream, RecoveryControlFailure> {
    loop {
        if Instant::now() >= deadline {
            return Err(RecoveryControlFailure::AcceptDeadline);
        }
        observe(NativeRecoveryStage::BeforeAccept);
        match listener.accept() {
            Ok((stream, _)) => {
                observe(NativeRecoveryStage::AfterAccept);
                if Instant::now() >= deadline {
                    return Err(RecoveryControlFailure::AcceptCompletedLate);
                }
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(RecoveryControlFailure::AcceptNoPeer);
                }
                thread::yield_now();
            }
            Err(error) => return Err(RecoveryControlFailure::AcceptFailed(error.kind())),
        }
    }
}

fn read_initial_line_until(
    stream: &mut UnixStream,
    deadline: Instant,
    line: u8,
    observe: &mut impl FnMut(NativeRecoveryStage),
) -> Result<String, RecoveryControlFailure> {
    // This is the same real Unix socket, without BufReader prefetch: a
    // delivered LF callback follows the actual native one-byte read.
    let mut bytes = Vec::with_capacity(128);
    let record = ControlRecord::of_line(line);
    loop {
        if Instant::now() >= deadline {
            return Err(RecoveryControlFailure::LineReadDeadline(record));
        }
        observe(NativeRecoveryStage::BeforeByte {
            line,
            received: bytes.len(),
        });
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => {
                observe(NativeRecoveryStage::AfterLf { line });
                if Instant::now() >= deadline {
                    return Err(RecoveryControlFailure::LineLfLate(record));
                }
                return String::from_utf8(bytes)
                    .map(|text| text.trim_end().to_owned())
                    .map_err(|_| RecoveryControlFailure::LineInvalidUtf8);
            }
            Ok(1) => {
                if bytes.len() >= MAX_INITIAL_CONTROL_LINE_BYTES {
                    return Err(RecoveryControlFailure::LineOversized);
                }
                bytes.push(byte[0]);
            }
            Ok(0) => return Err(RecoveryControlFailure::LineEof),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(RecoveryControlFailure::LineReadFailed(error.kind())),
            Ok(_) => unreachable!("one byte per native read"),
        }
        if Instant::now() >= deadline {
            return Err(RecoveryControlFailure::LineByteDeadline(record));
        }
        thread::yield_now();
    }
}

fn collect_initial_control_until(
    listener: &UnixListener,
    deadline: Instant,
    observe: &mut impl FnMut(NativeRecoveryStage),
) -> Result<(UnixStream, String, String), RecoveryControlFailure> {
    let mut stream = accept_recovery_until(listener, deadline, observe)?;
    // Preserve the original socket setup ordering while the accepted peer is
    // still alive. On Darwin setting SO_RCVTIMEO after an otherwise-complete
    // peer has closed can fail with EINVAL; that is not a bad HELLO.
    stream
        .set_nonblocking(false)
        .map_err(|error| RecoveryControlFailure::NormalizeBeforeRecords(error.kind()))?;
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .map_err(|error| RecoveryControlFailure::SetReadTimeout(error.kind()))?;
    // Native HELLO and DESCENDANT use nonblocking one-byte reads checked
    // against the earlier absolute clock. The socket timeout installed above
    // remains for the subsequent ordinary READY/ECHO reader.
    stream
        .set_nonblocking(true)
        .map_err(|error| RecoveryControlFailure::PollSetup(error.kind()))?;
    let hello = read_initial_line_until(&mut stream, deadline, 0, observe)?;
    let descendant = read_initial_line_until(&mut stream, deadline, 1, observe)?;
    if Instant::now() >= deadline {
        return Err(RecoveryControlFailure::RecordsCompletedLate);
    }
    stream
        .set_nonblocking(false)
        .map_err(|error| RecoveryControlFailure::NormalizeAfterRecords(error.kind()))?;
    observe(NativeRecoveryStage::AfterNormalize);
    if Instant::now() >= deadline {
        return Err(RecoveryControlFailure::NormalizedAfterDeadline);
    }
    Ok((stream, hello, descendant))
}

#[path = "recovery_cycle/control_failure.rs"]
mod control_failure;

#[cfg(test)]
#[path = "recovery_cycle/deadline_tests.rs"]
mod deadline_tests;

#[cfg(test)]
#[path = "recovery_cycle/private_diagnostics_tests.rs"]
mod private_diagnostics_tests;
