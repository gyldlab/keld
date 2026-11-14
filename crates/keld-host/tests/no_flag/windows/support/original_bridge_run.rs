//! The existing process/pipe/profile owners around a real shipping CLI launch.
//! CLI, delegated Host and cleanup sentinel remain distinct observed identities.
use crate::facade_window::fixture::{ephemeral_profile_dir, retained_descendants};
use crate::support::control::accept_control_or_host_failure;
use crate::support::process::{
    PIPE_DRAIN_KILL_SWITCH, PipeText, assert_process_signaled, open_process_for_wait,
    pipe_line_after, pipe_text_by, read_pipe_on_thread, wait_for_child_process,
    wait_for_cleanup_sentinel, wait_process_gone,
};
use crate::support::profile_run::{
    ProfileTestRoot, actual_webview_process, assert_profile_test_root_feature,
};
use crate::support::signed_process::SignedStateProcessGuard;
use crate::{
    PRODUCT_DEADLINE, dev_stage_count, startup_kill_switch, ui_end_kill_switch,
    wait_for_dev_stage_count,
};
mod boot8_lease;
mod fixture;
mod native_capacity;
mod observe;
mod queued_close;
mod resource_work;
mod retained_failure;
mod selected_diagnostic;
mod startup_failure;

pub(crate) use fixture::Fixture;
pub(crate) use queued_close::{QueuedCloseRecord, QueuedCloseTrace};
pub(crate) use resource_work::{ResourceRecord, ResourceTrace};

use serde_json::Value;
use std::fs;
use std::io::Write as _;
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::OwnedHandle;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn assert_d_path(path: &Path) {
    let text = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    assert!(
        text.starts_with("d:\\"),
        "fixture paths must be on D: {}",
        path.display()
    );
    assert!(path.is_absolute());
}

pub(crate) struct OriginalRun {
    // On assertion failure the existing emergency guard precedes every profile
    // and HANDLE drop. Emergency cleanup never counts as a passing native proof.
    cli: SignedStateProcessGuard,
    listener: TcpListener,
    writer: TcpStream,
    control: PipeText,
    retired_controls: Vec<(u32, PipeText)>,
    stdout: PipeText,
    stderr: PipeText,
    control_after: usize,
    page_after: usize,
    native_after: usize,
    host_pid: u32,
    sentinel_pid: u32,
    retained: Vec<(u32, OwnedHandle)>,
    preclosed_browser_pid: Option<u32>,
    project: PathBuf,
    stage_host: PathBuf,
    profiles: ProfileTestRoot,
    pub(crate) hello: Value,
}

// The discovery helpers still fail on early exit. Their Child no longer owns
// these pipes, so report the existing captures before resuming the same panic.
fn discover_startup(
    cli: &mut SignedStateProcessGuard,
    listener: &TcpListener,
    stdout: &PipeText,
    stderr: &PipeText,
    deadline: Instant,
) -> (u32, OwnedHandle, u32, OwnedHandle, TcpStream) {
    let mut phase = "Host discovery";
    let observed = catch_unwind(AssertUnwindSafe(|| {
        let host_pid = wait_for_child_process(cli.child_mut(), "keld-host.exe", deadline);
        let host = open_process_for_wait(host_pid, false);
        phase = "cleanup sentinel discovery";
        let sentinel_pid = wait_for_cleanup_sentinel(cli.child_mut(), deadline);
        let sentinel = open_process_for_wait(sentinel_pid, false);
        phase = "control bind";
        let socket = accept_control_or_host_failure(listener, cli.child_mut(), deadline);
        (host_pid, host, sentinel_pid, sentinel, socket)
    }));
    match observed {
        Ok(value) => value,
        Err(cause) => {
            let status = cli.child_mut().try_wait();
            let drained = Instant::now() + PIPE_DRAIN_KILL_SWITCH;
            eprintln!(
                "KELD_ORIGINAL_STARTUP_CAPTURE phase={phase}; CLI {status:?}\nstdout:\n{}\nstderr:\n{}",
                pipe_text_by(stdout, drained),
                pipe_text_by(stderr, drained)
            );
            resume_unwind(cause)
        }
    }
}

fn assert_completed_streams(out: &str, trace: &str, control: &str) {
    assert!(
        trace.contains("armed_by=Quit"),
        "actual ordinary terminal provenance"
    );
    assert!(
        trace.contains("KELD_T2_RELEASE_WITNESS completed=true"),
        "actual native release"
    );
    assert!(
        trace.contains("releasing=0"),
        "no remaining release obligation"
    );
    for (name, captured) in [("stdout", out), ("stderr", trace), ("control", control)] {
        assert!(
            !captured.contains("<pipe still open"),
            "actual {name} capture did not reach EOF: {captured}"
        );
        assert!(
            !captured.contains("<pipe read failed:"),
            "actual {name} capture ended with a read error: {captured}"
        );
    }
}

impl OriginalRun {
    /// Only a separately qualified instrumented CLI/Host pair can run this fixture.
    /// These observation hooks never supply the default-image Q5 qualification.
    pub(crate) fn start(cli_path: &Path, project: &Path, fixture: Fixture) -> Self {
        Self::start_with_native_hold(cli_path, project, fixture, None)
    }

    fn start_with_native_hold(
        cli_path: &Path,
        project: &Path,
        fixture: Fixture,
        native_hold: Option<&Path>,
    ) -> Self {
        assert_d_path(cli_path);
        assert_d_path(project);
        assert_d_path(&std::env::temp_dir());
        assert_eq!(
            cli_path.file_name().and_then(|n| n.to_str()),
            Some("keld.exe"),
            "use shipping CLI, never a test helper selector"
        );
        assert_eq!(dev_stage_count(project), 0, "no pre-existing dev stage");
        fixture.verify(project);
        let profiles = ProfileTestRoot::new();
        let adjacent_host = cli_path.parent().expect("CLI parent").join("keld-host.exe");
        assert_profile_test_root_feature(&adjacent_host, profiles.path());
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("owned control listener");
        let mut command = Command::new(cli_path);
        command
            .arg("dev")
            .current_dir(project)
            .env(
                "KELD_T1B_CONTROL",
                listener.local_addr().expect("port").port().to_string(),
            )
            .env("KELD_PROFILE_TEST_ROOT", profiles.path())
            .env(
                "TEMP",
                profiles.path().parent().expect("D temporary directory"),
            )
            .env(
                "TMP",
                profiles.path().parent().expect("D temporary directory"),
            )
            .env("KELD_TEST_WINDOWS_T2_OBSERVE", "1")
            .env("KELD_TEST_WINDOWS_PAGE_READBACK", "25")
            .env_remove("KELD_DEV_LEASE")
            .env_remove("KELD_TEST_WINDOWS_FAIL_BUILD")
            .env_remove("KELD_TEST_WINDOWS_NATIVE_HOLD")
            .env_remove("KELD_TEST_PRIMARY_VALUE_HOLD")
            .env_remove("KELD_TEST_X11_NATIVE_PRESSURE")
            .env_remove("KELD_TEST_WINDOWS_HANG_GUARD_SECS")
            .env_remove("KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY")
            .env_remove("KELD_TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if fixture.observes_frame() {
            command.env(keld_wv::TEST_WINDOWS_FRAME_INGRESS_ENV, "1");
        } else {
            command.env_remove(keld_wv::TEST_WINDOWS_FRAME_INGRESS_ENV);
        }
        fixture.configure_native_hold(&mut command, native_hold);
        let mut cli = SignedStateProcessGuard::new(command.spawn().expect("shipping keld dev"));
        let stdout = read_pipe_on_thread(cli.child_mut().stdout.take().expect("CLI stdout"));
        let stderr = read_pipe_on_thread(cli.child_mut().stderr.take().expect("CLI stderr"));
        let deadline = Instant::now() + startup_kill_switch();
        let (host_pid, host, sentinel_pid, sentinel, socket) =
            discover_startup(&mut cli, &listener, &stdout, &stderr, deadline);
        let writer = socket.try_clone().expect("control writer");
        let control = read_pipe_on_thread(socket);
        let stages: Vec<_> = fs::read_dir(project.join(".keld/dev"))
            .expect("CLI stage parent")
            .map(|entry| entry.expect("CLI stage").path())
            .collect();
        assert_eq!(stages.len(), 1, "one actual CLI-owned stage");
        let stage_host = stages[0].join("keld-host.exe");
        assert!(stage_host.is_file());
        let mut run = Self {
            cli,
            listener,
            writer,
            control,
            retired_controls: Vec::new(),
            stdout,
            stderr,
            control_after: 0,
            page_after: 0,
            native_after: 0,
            host_pid,
            sentinel_pid,
            retained: vec![(host_pid, host), (sentinel_pid, sentinel)],
            preclosed_browser_pid: None,
            project: project.to_owned(),
            stage_host,
            profiles,
            hello: Value::Null,
        };
        // Socket connection and app readiness can complete in either order.
        run.hello = run.record_after("HELLO ", 0, deadline);
        let bun = u32::try_from(run.hello["bunPid"].as_u64().expect("actual Bun pid"))
            .expect("Bun pid fits u32");
        run.cli.observe_bun(bun);
        run.retained.push((bun, open_process_for_wait(bun, false)));
        assert_eq!(run.record_after("READY ", 0, deadline)["windows"], 0);
        run
    }

    pub(crate) fn send(&mut self, command: &str) {
        writeln!(self.writer, "{command}").expect("fixture command");
        self.writer.flush().expect("flush fixture command");
    }

    fn assert_live(&mut self, phase: &str, deadline: Instant) {
        let status = self.cli.child_mut().try_wait().expect("observe actual CLI");
        assert!(
            status.is_none() && Instant::now() < deadline,
            "{phase} incomplete; CLI {status:?}\nstdout:\n{}\nstderr:\n{}\ncontrol:\n{}",
            pipe_text_by(&self.stdout, Instant::now()),
            pipe_text_by(&self.stderr, Instant::now()),
            pipe_text_by(&self.control, Instant::now())
        );
    }

    pub(crate) fn record(&mut self, prefix: &str) -> Value {
        self.record_after(
            prefix,
            self.control_after,
            Instant::now() + startup_kill_switch(),
        )
    }

    fn record_after(&mut self, prefix: &str, after: usize, deadline: Instant) -> Value {
        loop {
            let record = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pipe_line_after(&self.control, prefix, after)
            })) {
                Ok(record) => record,
                Err(panic) => {
                    // Preserve the helper's failure after reporting its existing captures.
                    let _ = writeln!(
                        std::io::stderr(),
                        "Original capture failed: prefix={prefix:?} after={after} cli={} host={} sentinel={} stage={}\nstdout:\n{}\nstderr:\n{}\ncontrol:\n{}",
                        self.cli.child_mut().id(),
                        self.host_pid,
                        self.sentinel_pid,
                        self.stage_host.display(),
                        pipe_text_by(&self.stdout, Instant::now()),
                        pipe_text_by(&self.stderr, Instant::now()),
                        pipe_text_by(&self.control, Instant::now())
                    );
                    std::panic::resume_unwind(panic);
                }
            };
            if let Some((line, after)) = record {
                self.control_after = self.control_after.max(after);
                return serde_json::from_str(line.strip_prefix(prefix).expect("prefix"))
                    .expect("actual bounded fixture JSON");
            }
            self.assert_live(prefix, deadline);
            thread::park_timeout(Duration::from_millis(10));
        }
    }

    pub(crate) fn finish(self) {
        self.finish_with(|_, _| {});
    }

    /// Inspect the actual complete captured streams only after every existing
    /// process/profile/EOF receipt succeeded. No extra capture owner is created.
    pub(crate) fn finish_with(mut self, inspect: impl FnOnce(&str, &str)) {
        let profile = ephemeral_profile_dir(self.profiles.path());
        let browser_pid = if let Some(pid) = self.preclosed_browser_pid {
            // The resource scenario retained this actual browser HANDLE before
            // closing its last controller. Metadata alone is never a receipt.
            assert!(self.retained.iter().any(|(actual, _)| *actual == pid));
            pid
        } else {
            let browser = actual_webview_process(
                self.host_pid,
                &self.stage_host,
                &profile.join("webview2"),
                Instant::now() + PRODUCT_DEADLINE,
            );
            u32::try_from(browser["browser_pid"].as_u64().expect("browser pid"))
                .expect("browser pid fits u32")
        };
        self.retained
            .extend(retained_descendants(self.cli.child_mut()));
        for expected in [
            self.host_pid,
            browser_pid,
            u32::try_from(self.hello["bunPid"].as_u64().expect("Bun")).expect("Bun u32"),
        ] {
            assert!(
                self.retained.iter().any(|(pid, _)| *pid == expected),
                "actual descendant HANDLE missing for {expected}"
            );
        }
        self.send("QUIT");
        self.record("QUIT_ACCEPTED ");
        let Self {
            cli,
            listener,
            writer,
            control,
            retired_controls,
            stdout,
            stderr,
            retained,
            profiles,
            project,
            sentinel_pid,
            ..
        } = self;
        let status = cli.wait(Instant::now() + ui_end_kill_switch());
        assert!(status.success(), "actual CLI Quit: {status}");
        wait_for_dev_stage_count(&project, 0, Instant::now() + PRODUCT_DEADLINE);
        wait_process_gone(sentinel_pid, Instant::now() + PRODUCT_DEADLINE);
        for (pid, handle) in &retained {
            assert_process_signaled(handle, &format!("actual original descendant {pid}"));
        }
        crate::support::control::assert_no_control_connection(
            &listener,
            "no successor control after actual Host shutdown",
        );
        drop(writer);
        let deadline = Instant::now() + PIPE_DRAIN_KILL_SWITCH;
        for (pid, capture) in &retired_controls {
            let text = pipe_text_by(capture, deadline);
            println!("KELD_ORIGINAL_RETIRED_CONTROL bun={pid}\n{text}");
            assert!(
                !text.contains("<pipe still open") && !text.contains("<pipe read failed:"),
                "retired actual Bun {pid} control must preserve clean EOF: {text}"
            );
        }
        let out = pipe_text_by(&stdout, deadline);
        let trace = pipe_text_by(&stderr, deadline);
        let control = pipe_text_by(&control, deadline);
        println!(
            "KELD_ORIGINAL_NATIVE_END status={status}\nstdout:\n{out}\nstderr:\n{trace}\ncontrol:\n{control}"
        );
        assert_completed_streams(&out, &trace, &control);
        // The fixture must not erase evidence of failed product profile deletion.
        // All actual native, process and EOF receipts above have already succeeded.
        let profiles = std::mem::ManuallyDrop::new(profiles);
        match fs::symlink_metadata(&profile) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            observed => panic!(
                "actual product profile not proven absent before fixture disposal: \
                 profile={} observation={observed:?}; retained fixture={}",
                profile.display(),
                profiles.path().display()
            ),
        }
        println!(
            "KELD_ORIGINAL_PROFILE_ABSENT_BEFORE_DISPOSAL {}",
            profile.display()
        );
        let profiles = std::mem::ManuallyDrop::into_inner(profiles);
        profiles.remove();
        inspect(&trace, &control);
    }
}
