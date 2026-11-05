//! Installed facade resources and independent process observations for GH1188.
//! The existing launch gate and `WindowsProcessJob` own this fixture's family.

use super::facade_browser_observation::{BrowserObservation, descendants, observe_browser};
pub(super) use super::facade_browser_observation::{browser_count, field, number};
use super::sessions::{LifecycleObservation, SeamedHost, read_lifecycle_record};
use crate::facade_window::fixture::FacadeFixture;
use crate::support::control::{accept_control_or_host_failure, try_read_control_line};
use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::process::{
    PIPE_DRAIN_KILL_SWITCH, PipeText, assert_process_signaled, open_process_for_wait, pipe_text_by,
    process_times, process_tree_report, read_pipe_on_thread,
};
use crate::support::profile_run::{ProfileTestRoot, assert_profile_test_root_feature};
use crate::{PRODUCT_DEADLINE, startup_kill_switch, ui_end_kill_switch};
use keld_runtime::windows_job::{
    WINDOWS_LAUNCH_GATE_ATTEMPT_JOB_V1, WINDOWS_LAUNCH_GATE_ENV, WindowsProcessJob,
    release_host_start_v1,
};
use serde_json::Value;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub(super) const ALL_RECEIPTS_WITHHELD: &str = "KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY";
pub(super) const LATER_RECEIPT_WITHHELD: &str =
    "KELD_TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT";
pub(super) const FAIL_BUILD: &str = "KELD_TEST_WINDOWS_FAIL_BUILD";

pub(super) struct InstalledFacade {
    fixture: FacadeFixture,
    installed: InstalledApp,
    root: ProfileTestRoot,
    control: TcpListener,
}

impl InstalledFacade {
    pub(super) fn new() -> Self {
        let fixture = FacadeFixture::new();
        let installed = InstalledApp::install(
            &fixture.project.project,
            &fixture_env(SIGNED_HOST_A_P1),
            &fixture_env(INSTALLER_ENV),
        );
        let root = ProfileTestRoot::new();
        assert_profile_test_root_feature(installed.host(), root.path());
        let control = TcpListener::bind(("127.0.0.1", 0)).expect("installed facade control");
        Self {
            fixture,
            installed,
            root,
            control,
        }
    }

    pub(super) fn start(&self, hooks: &[(&str, &str)]) -> InstalledRun<'_> {
        let mut job = WindowsProcessJob::create().expect("create fixture attempt Job");
        let mut command = self.installed.command();
        command
            .env(
                "KELD_T1B_CONTROL",
                self.control
                    .local_addr()
                    .expect("control address")
                    .port()
                    .to_string(),
            )
            .env("KELD_PROFILE_TEST_ROOT", self.root.path())
            .env(
                "TEMP",
                self.root.path().parent().expect("isolated temp parent"),
            )
            .env(
                "TMP",
                self.root.path().parent().expect("isolated temp parent"),
            )
            .env("KELD_TEST_WINDOWS_T2_OBSERVE", "1")
            .env(WINDOWS_LAUNCH_GATE_ENV, WINDOWS_LAUNCH_GATE_ATTEMPT_JOB_V1)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for name in [
            ALL_RECEIPTS_WITHHELD,
            LATER_RECEIPT_WITHHELD,
            FAIL_BUILD,
            "KELD_TEST_WINDOWS_NATIVE_HOLD",
            "KELD_TEST_WINDOWS_PAGE_READBACK",
            "KELD_TEST_WINDOWS_SECOND_SESSION",
            "KELD_TEST_WINDOWS_ENVIRONMENT_DESCENDANT",
            "KELD_TEST_WINDOWS_HANG_GUARD_SECS",
        ] {
            command.env_remove(name);
        }
        command.envs(hooks.iter().copied());
        let mut host = SeamedHost::from_child(command.spawn().expect("start gated installed Host"));
        host.start_live_stderr();
        // This existing product gate precedes listeners, Bun and native resources.
        // An already-running Host must never be assigned after its children escaped.
        job.assign_child(&host.host)
            .expect("assign/read back exact gated Host");
        release_host_start_v1(host.host.stdin.as_mut().expect("private Host start pipe"))
            .expect("release assigned Host");
        let stream = accept_control_or_host_failure(
            &self.control,
            &mut host.host,
            Instant::now() + startup_kill_switch(),
        );
        let writer = stream.try_clone().expect("facade control writer");
        let mut reader = BufReader::new(stream);
        let bound = read_record(&mut reader, "BOUND", Instant::now() + PRODUCT_DEADLINE);
        let bun_pid = number(&bound, "pid");
        let bun = open_process_for_wait(bun_pid, false);
        let ready = read_record(&mut reader, "READY", Instant::now() + startup_kill_switch());
        let host_handle = open_process_for_wait(host.pid, false);
        InstalledRun {
            host,
            job,
            fixture: self,
            reader: Some(reader),
            writer,
            bun_pid,
            bun,
            host_handle,
            ready,
            retained: Vec::new(),
            control_capture: None,
        }
    }

    pub(super) fn lifecycle(&self) -> LifecycleObservation {
        // Read only at a write-free boundary: Ready, acknowledged veto, a live
        // non-family member blocking release, or after the exact Host retired.
        read_lifecycle_record(self.root.path())
    }

    pub(super) fn profile_dir(&self) -> PathBuf {
        let paths = fs::read_dir(self.root.path().join("Keld/profiles/v1"))
            .expect("installed namespaces")
            .map(|entry| entry.expect("namespace entry").path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        assert_eq!(
            paths.len(),
            1,
            "one authenticated installed namespace: {paths:?}"
        );
        paths[0].clone()
    }

    pub(super) fn close(self) {
        let Self {
            fixture,
            installed,
            root,
            ..
        } = self;
        drop(installed);
        root.remove();
        fixture.close();
    }
}

pub(super) struct InstalledRun<'a> {
    // On assertion unwind, the existing Child guard runs before the attempt Job
    // closes. Its fallback cleanup is reported and never product pass evidence.
    pub(super) host: SeamedHost,
    job: WindowsProcessJob,
    fixture: &'a InstalledFacade,
    reader: Option<BufReader<TcpStream>>,
    writer: TcpStream,
    control_capture: Option<PipeText>,
    pub(super) bun_pid: u32,
    bun: OwnedHandle,
    host_handle: OwnedHandle,
    pub(super) ready: Value,
    retained: Vec<(u32, OwnedHandle)>,
}

impl InstalledRun<'_> {
    pub(super) fn pid(&self) -> u32 {
        self.host.pid
    }

    pub(super) fn send(&mut self, command: &str) {
        self.try_send(command).expect("write facade stimulus");
    }

    pub(super) fn try_send(&mut self, command: &str) -> std::io::Result<()> {
        writeln!(self.writer, "{command}")?;
        self.writer.flush()
    }

    pub(super) fn read(&mut self, expected: &str) -> Value {
        read_record(
            self.reader.as_mut().expect("synchronous control owner"),
            expected,
            Instant::now() + PRODUCT_DEADLINE,
        )
    }

    /// Transfer the one reader before a hostile terminal. Winsock can discard
    /// unread kernel-buffered bytes on reset; do not wait for a later CIM census.
    pub(super) fn capture_control(&mut self) {
        assert!(self.control_capture.is_none(), "control already captured");
        self.control_capture = Some(read_pipe_on_thread(
            self.reader
                .take()
                .expect("transfer the only control reader"),
        ));
    }

    pub(super) fn marker(
        &mut self,
        prefix: &str,
        after: usize,
        deadline: Instant,
    ) -> (String, usize) {
        self.host.wait_stderr_line(prefix, after, deadline)
    }

    /// Missing live receipt is an eligibility result, not a successful scenario.
    /// The final captured trace is still retained after finish, including a line
    /// delivered to the reader thread after Host termination.
    pub(super) fn marker_if_delivered(
        &mut self,
        prefix: &str,
        after: usize,
        deadline: Instant,
    ) -> Option<(String, usize)> {
        loop {
            if let Some(line) = self.host.stderr_line_if_present(prefix, after) {
                return Some(line);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }

    pub(super) fn browser(&self, previous: Option<u32>, terminate: bool) -> BrowserObservation {
        observe_browser(
            self.pid(),
            self.fixture.installed.host(),
            &self.fixture.profile_dir().join("webview2"),
            previous,
            terminate,
        )
    }

    pub(super) fn retain_descendants(&mut self) -> String {
        let tree = process_tree_report(self.pid());
        for (image, pid, parent) in descendants(&tree) {
            if self.retained.iter().all(|(old, _)| *old != pid) {
                let handle = open_process_for_wait(pid, false);
                let times = process_times(handle.as_raw_handle().cast());
                println!(
                    "KELD_GH1188_INSTALLED_RETAIN host={} host_birth={} pid={pid} parent={parent} image={image} birth={} exit={}",
                    self.pid(),
                    self.host.birth,
                    times.creation,
                    times.exit,
                );
                self.retained.push((pid, handle));
            }
        }
        assert!(
            self.retained.iter().any(|(pid, _)| *pid == self.bun_pid),
            "live Bun absent from native family: {tree}"
        );
        tree
    }

    /// Two identical kernel accounting snapshots bracket an actual facade
    /// round trip and native family census. A later startup child is still
    /// conservatively detected by the final lifetime total comparison.
    pub(super) fn settled_accounting(&mut self, id: u32) -> (u32, u32) {
        let deadline = Instant::now() + PRODUCT_DEADLINE;
        loop {
            let before = self
                .job
                .process_accounting()
                .expect("pre-stimulus Job accounting");
            self.send(&format!("STATE {id}"));
            assert_eq!(self.read("STATE")["live"], true);
            self.retain_descendants();
            let after = self
                .job
                .process_accounting()
                .expect("settled Job accounting");
            if before == after {
                assert!(
                    after.0 >= 3,
                    "Host, Bun and actual browser must be at risk: {after:?}"
                );
                return after;
            }
            assert!(
                Instant::now() < deadline,
                "native family never settled: {before:?} -> {after:?}"
            );
        }
    }

    pub(super) fn finish(self) -> FinishedRun {
        let Self {
            host,
            job,
            reader,
            writer,
            control_capture,
            bun_pid,
            bun,
            host_handle,
            retained,
            ..
        } = self;
        let pid = host.pid;
        let (status, stdout, stderr) = host.finish_by(Instant::now() + ui_end_kill_switch());
        // Preserve failure evidence before any retained-process assertion can unwind.
        // This preliminary record does not attest product or fixture cleanup.
        println!(
            "KELD_GH1188_INSTALLED_EXIT_OBSERVED host={pid} bun={bun_pid} status={status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        println!(
            "KELD_GH1188_INSTALLED_JOB_BEFORE_WAITS host={pid} accounting={:?}",
            job.process_accounting(),
        );
        assert_process_signaled(&host_handle, "installed facade Host");
        assert_process_signaled(&bun, "installed facade Bun");
        for (child_pid, handle) in &retained {
            assert_process_signaled(
                handle,
                &format!("retained installed descendant {child_pid}"),
            );
        }
        let unread_control = if let Some(captured) = control_capture {
            // A reset diagnostic is preserved verbatim. Only positively captured
            // records establish eligibility; absence is not process-start proof.
            let text = pipe_text_by(&captured, Instant::now() + PIPE_DRAIN_KILL_SWITCH);
            assert!(
                !text.contains("<pipe still open"),
                "control reader did not retire: {text}"
            );
            text
        } else {
            let mut reader = reader.expect("retired synchronous control owner");
            reader
                .get_mut()
                .set_read_timeout(Some(PIPE_DRAIN_KILL_SWITCH))
                .expect("retired control drain deadline");
            let mut text = String::new();
            if let Err(error) = reader.read_to_string(&mut text) {
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset,
                    "retired control drain failed: {error}"
                );
                write!(text, "\n<retired control reset: {error}>")
                    .expect("append control diagnostic");
            }
            text
        };
        drop(writer);
        let census = process_tree_report(pid);
        assert!(
            census.contains("host_alive=False") && descendants(&census).is_empty(),
            "owned native processes survived Host exit: {census}"
        );
        let deadline = Instant::now() + PRODUCT_DEADLINE;
        let accounting = loop {
            let accounting = job
                .process_accounting()
                .expect("final retained Job accounting");
            if accounting.0 == 0 {
                break accounting;
            }
            assert!(
                Instant::now() < deadline,
                "fixture Job still has live owned processes before fallback cleanup: {accounting:?}"
            );
            std::thread::park_timeout(Duration::from_millis(10));
        };
        // This is after product cleanup, never the cause of a passing empty census.
        drop(job);
        println!(
            "KELD_GH1188_INSTALLED_OUTPUT host={pid} bun={bun_pid} status={status} job_active={} job_total={}\nstdout:\n{stdout}\nstderr:\n{stderr}\nunread_control:\n{unread_control}\ncensus:\n{census}",
            accounting.0, accounting.1,
        );
        FinishedRun {
            status,
            stderr,
            unread_control,
            accounting,
        }
    }
}

pub(super) struct FinishedRun {
    pub(super) status: ExitStatus,
    pub(super) stderr: String,
    pub(super) unread_control: String,
    pub(super) accounting: (u32, u32),
}

fn read_record(reader: &mut BufReader<TcpStream>, expected: &str, deadline: Instant) -> Value {
    let line = try_read_control_line(reader, deadline).expect("bounded actual facade record");
    let (kind, value) = line.split_once(' ').expect("facade record shape");
    assert_eq!(kind, expected, "actual facade response: {line}");
    serde_json::from_str(value).expect("facade record JSON")
}
