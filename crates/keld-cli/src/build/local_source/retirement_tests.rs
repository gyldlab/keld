//! Caller-level family retirement: independent retained process-object oracles.

use std::io::{Read as _, Write as _};
use std::os::windows::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use keld_ipc::{
    AppLinkDeadlines as _, BootstrapRejection, BootstrapRejectionObserver,
    WindowsNamedPipeBootstrapAdmission, WindowsNamedPipeBootstrapListener,
    WindowsNamedPipeBootstrapStream,
};
use keld_runtime::windows_job::WindowsProcessPeer;
use windows_sys::Win32::System::Threading::DETACHED_PROCESS;

use super::{CaptureOptions, MAX_IMAGE, os_args, pin_file, run_captured_with_options, run_owned};

const WAIT: Duration = Duration::from_secs(20);
const DIRECT_LINK: &str = "KELD_TEST_SOURCE_DIRECT_LINK";
const DESCENDANT_LINK: &str = "KELD_TEST_SOURCE_DESCENDANT_LINK";
const DIRECT_SELECTOR: &str =
    "build::local_source::retirement_tests::source_tool_leaves_descendant";
const DESCENDANT_SELECTOR: &str =
    "build::local_source::retirement_tests::source_descendant_holds_stdio";

struct IgnoreRejections;

impl BootstrapRejectionObserver for IgnoreRejections {
    fn rejected(&self, _rejection: BootstrapRejection) {}
}

/// The process handle is opened while the authenticated pipe peer is blocked.
/// Emergency release is separate from, and after, the product observation.
struct ObservedFixture {
    stream: WindowsNamedPipeBootstrapStream,
    peer: WindowsProcessPeer,
}

impl ObservedFixture {
    fn accept(listener: &WindowsNamedPipeBootstrapListener, deadline: Instant) -> Self {
        let stream = match listener
            .accept_authenticated_until(deadline, &IgnoreRejections)
            .expect("accept the selected fixture")
        {
            WindowsNamedPipeBootstrapAdmission::Authenticated(stream) => stream,
            other => panic!("selected fixture did not authenticate: {other:?}"),
        };
        stream
            .set_app_link_read_deadline(Some(WAIT))
            .expect("bound terminal provenance observation");
        stream
            .set_app_link_write_deadline(Some(WAIT))
            .expect("bound fixture release");
        let peer = WindowsProcessPeer::open(
            stream.peer_process_id().expect("OS pipe peer PID"),
            stream.peer_session_id().expect("OS pipe peer session"),
        )
        .expect("retain this exact live fixture process object");
        assert!(!peer.has_exited().expect("live prerequisite"));
        Self { stream, peer }
    }

    fn release(&mut self) {
        self.stream.write_all(b"Q").expect("release direct fixture");
    }

    fn assert_no_cooperative_exit(&mut self) {
        let mut marker = [0_u8; 1];
        assert_eq!(
            self.stream
                .read(&mut marker)
                .expect("terminal provenance must be conclusive, not an I/O error"),
            0,
            "descendant completed its own gate before product retirement"
        );
    }
}

impl Drop for ObservedFixture {
    fn drop(&mut self) {
        // The fixture cooperates only for emergency cleanup. Success assertions
        // use snapshots taken before this release and never count it as reap.
        if self.peer.has_exited().is_ok_and(|exited| !exited) {
            let _ = self.stream.write_all(b"Q");
            let _ = self.stream.shutdown_app_link();
            let _ = self.peer.wait_until_exited(WAIT);
        }
    }
}

fn wait_for_test_release(variable: &str) {
    let link = std::env::var(variable).expect("private fixture invocation");
    let (endpoint, token) = keld_ipc::parse_app_link(&link).expect("fixture link");
    let mut stream = WindowsNamedPipeBootstrapStream::connect(endpoint).expect("fixture connect");
    stream
        .set_app_link_read_deadline(Some(WAIT))
        .expect("bound fixture lifetime");
    stream
        .set_app_link_write_deadline(Some(WAIT))
        .expect("bound fixture handshake");
    keld_ipc::link::handshake_client(&mut stream, &token).expect("fixture handshake");
    let _ = stream.read_exact(&mut [0_u8; 1]);
    if variable == DESCENDANT_LINK && stream.write_all(b"E").is_err() {
        // Never turn failure to report cooperative completion into a false
        // product-exit signal. The assigned Job owns this fixture. Normal
        // failures retire/drop it; isolated negative controls close their
        // deliberately retained Job when the bounded test process ends.
        loop {
            std::thread::park();
        }
    }
    // E precedes every descendant self-exit, including timeout and EOF. The
    // observer rejects E; only Job termination can yield EOF without it.
}

#[test]
#[ignore = "private subprocess entry point"]
#[expect(
    clippy::zombie_processes,
    reason = "The enclosing production Job owns descendant retirement; waiting here would invalidate the retained-handle caller oracle."
)]
fn source_tool_leaves_descendant() {
    let _descendant = Command::new(std::env::current_exe().expect("fixture executable"))
        .args(["--exact", DESCENDANT_SELECTOR, "--ignored", "--nocapture"])
        .creation_flags(DETACHED_PROCESS)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn a descendant that retains the tool writers");
    // Dropping std::process::Child does not terminate the descendant. The
    // production runner's inherited Job, not this fixture, must retire it.
    wait_for_test_release(DIRECT_LINK);
}

#[test]
#[ignore = "private subprocess entry point"]
fn source_descendant_holds_stdio() {
    wait_for_test_release(DESCENDANT_LINK);
}

fn assert_runner_retires_descendant(capture: bool) {
    let executable = std::env::current_exe().expect("test executable");
    let tool = pin_file(&executable, MAX_IMAGE).expect("pin exact test executable");
    let cwd = executable.parent().expect("test executable directory");
    let direct_listener = WindowsNamedPipeBootstrapListener::bind().expect("direct fixture pipe");
    let descendant_listener =
        WindowsNamedPipeBootstrapListener::bind().expect("descendant fixture pipe");
    let mut environment = vec![
        (DIRECT_LINK.into(), direct_listener.app_link().into()),
        (
            DESCENDANT_LINK.into(),
            descendant_listener.app_link().into(),
        ),
    ];
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        environment.push(("SystemRoot".into(), system_root));
    }
    let args = os_args(&["--exact", DIRECT_SELECTOR, "--ignored", "--nocapture"]);
    let descendant_pin = OnceLock::<WindowsProcessPeer>::new();
    let deadline = Instant::now() + WAIT;
    std::thread::scope(|scope| {
        let runner = scope.spawn(|| {
            let result = if capture {
                let input = std::fs::File::open("NUL").expect("null fixture stdin");
                run_captured_with_options(
                    &tool,
                    &args,
                    &environment,
                    cwd,
                    &input,
                    CaptureOptions {
                        stdout_limit: 64 * 1024,
                        stderr_limit: 64 * 1024,
                        timeout: WAIT,
                        stderr_file: None,
                    },
                )
                .map(|_| ())
            } else {
                run_owned(&tool, &args, &environment, cwd, WAIT)
            };
            // Snapshot in the runner thread immediately after the actual caller
            // returns, not after the observer thread or emergency cleanup waits.
            let exited_at_return = descendant_pin
                .get()
                .map(|peer| peer.has_exited().expect("exact descendant process signal"));
            (result, exited_at_return)
        });
        let mut descendant = ObservedFixture::accept(&descendant_listener, deadline);
        let mut direct = ObservedFixture::accept(&direct_listener, deadline);
        assert_ne!(direct.peer.process_id(), descendant.peer.process_id());
        descendant_pin
            .set(
                descendant
                    .peer
                    .try_clone()
                    .expect("clone exact descendant pin"),
            )
            .expect("install observation before releasing the tool");
        assert!(
            !descendant
                .peer
                .has_exited()
                .expect("descendant remains blocked")
        );
        direct.release();
        let (result, exited_at_return) = runner.join().expect("runner did not panic");
        result.expect("successful tool and complete family retirement");
        assert_eq!(
            exited_at_return,
            Some(true),
            "runner returned before its parked descendant process object signaled"
        );
        descendant.assert_no_cooperative_exit();
        assert!(direct.peer.has_exited().expect("direct process exited"));
        assert!(
            descendant
                .peer
                .has_exited()
                .expect("descendant process exited")
        );
    });
}

#[test]
fn owned_runner_observes_descendant_exit_before_return() {
    assert_runner_retires_descendant(false);
}

#[test]
fn captured_runner_retires_descendant_holding_both_output_pipes() {
    assert_runner_retires_descendant(true);
}

/// The native driver copies this positive test executable before mutation, then
/// uses that immutable copy to supervise compilers and exact cases in outer Jobs.
/// A fresh process and fresh logs are required for every invocation.
#[test]
#[ignore = "private native qualification process-family guard"]
fn source_retirement_outer_guard() {
    use std::os::windows::io::AsHandle as _;

    use keld_runtime::windows_job::{WindowsLaunchedProcess, WindowsProcessJob};
    use keld_runtime::windows_lpac::{WindowsLpacStdio, WindowsSuspendedChild};

    let field = |name: &str| std::env::var(name).expect("explicit native guard invocation");
    let program = std::path::PathBuf::from(field("KELD_TEST_OUTER_PROGRAM"));
    let cwd = std::path::PathBuf::from(field("KELD_TEST_OUTER_CWD"));
    let words: Vec<String> =
        serde_json::from_str(&field("KELD_TEST_OUTER_ARGS")).expect("exact JSON argument array");
    let args: Vec<_> = words.into_iter().map(std::ffi::OsString::from).collect();
    let timeout_ms: u32 = field("KELD_TEST_OUTER_TIMEOUT_MS")
        .parse()
        .expect("finite native guard deadline");
    assert!((1..=3_600_000).contains(&timeout_ms));
    let stdout =
        std::fs::File::create_new(field("KELD_TEST_OUTER_STDOUT")).expect("new inner stdout log");
    let stderr =
        std::fs::File::create_new(field("KELD_TEST_OUTER_STDERR")).expect("new inner stderr log");
    let stdin = std::fs::File::open("NUL").expect("guard null stdin");
    let environment: Vec<_> = std::env::vars_os()
        .filter(|(name, _)| !name.to_string_lossy().starts_with("KELD_TEST_OUTER_"))
        .collect();
    let mut job = WindowsProcessJob::create().expect("outer qualification Job");
    let child = WindowsSuspendedChild::spawn_same_token_with_stdio(
        &program,
        &args,
        &environment,
        &cwd,
        WindowsLpacStdio {
            stdin: stdin.as_handle(),
            stdout: stdout.as_handle(),
            stderr: stderr.as_handle(),
        },
    )
    .expect("outer suspended launch");
    let mut process = WindowsLaunchedProcess::record(child).expect("outer exact launch record");
    let membership = job
        .assign_child(process.child())
        .expect("outer assignment before resume");
    process.resume(&membership).expect("outer guarded resume");
    let status = process.wait(timeout_ms);
    job.terminate_and_wait(process.child(), Duration::from_secs(10))
        .expect("outer family must be observed retired before a receipt");
    println!("KELD_CALLER_OUTER_FAMILY_RETIRED=1");
    match status {
        Ok(code) => println!("KELD_CALLER_INNER_EXIT={code}"),
        Err(error) => panic!("outer deadline/wait refused after family retirement: {error}"),
    }
}
