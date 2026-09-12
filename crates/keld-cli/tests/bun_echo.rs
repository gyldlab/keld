//! Dev session integration: Bun main + kipc echo (no window).

#![allow(clippy::expect_used)] // extra test crate: expect is the assertion oracle

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use keld_cli::create::create_project;
use keld_cli::echo_link::EchoServer;
use keld_ipc::link::{read_frame, write_frame};
use keld_ipc::{
    APP_LINK_IO_DEADLINE, AppLinkDeadlines, BootstrapAdmission, BootstrapListener,
    BootstrapRejection, BootstrapRejectionObserver, BootstrapStream, ChannelId, CorrelationId,
    ECHO_CHANNEL, EchoRequest, EchoResponse, FrameHeader, FrameKind, LIFECYCLE_CHANNEL,
    LifecycleEvent, LifecycleRequest, LifecycleResponse,
};

struct ObservedChild {
    child: Option<Child>,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
}

impl ObservedChild {
    fn wait_for_output(mut self, timeout: Duration) -> Result<Output, String> {
        let child = self.child.as_mut().expect("observed child present");
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < timeout => {
                    thread::park_timeout(Duration::from_millis(10));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let status = child
                        .wait()
                        .map_err(|error| format!("reap timed-out child: {error}"))?;
                    self.child.take();
                    let output = self.output(status)?;
                    return Err(format!(
                        "child did not exit within {timeout:?}; stdout={} stderr={}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
                Err(error) => return Err(format!("poll child exit: {error}")),
            }
        };
        self.child.take();
        self.output(status)
    }

    fn output(&self, status: std::process::ExitStatus) -> Result<Output, String> {
        Ok(Output {
            status,
            stdout: std::fs::read(&self.stdout_path)
                .map_err(|error| format!("read child stdout: {error}"))?,
            stderr: std::fs::read(&self.stderr_path)
                .map_err(|error| format!("read child stderr: {error}"))?,
        })
    }
}

impl Drop for ObservedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().is_ok_and(|status| status.is_none()) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

fn spawn_observed(command: &mut Command, output_root: &Path) -> ObservedChild {
    let stdout_path = output_root.join("child.stdout.log");
    let stderr_path = output_root.join("child.stderr.log");
    let stdout = File::create(&stdout_path).expect("create child stdout log");
    let stderr = File::create(&stderr_path).expect("create child stderr log");
    let child = command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn observed child");
    ObservedChild {
        child: Some(child),
        stdout_path,
        stderr_path,
    }
}

struct IgnoreBootstrapRejections;

impl BootstrapRejectionObserver for IgnoreBootstrapRejections {
    fn rejected(&self, _rejection: BootstrapRejection) {}
}

fn accept_generated_main(
    listener: &BootstrapListener,
    deadline: Instant,
) -> Result<BootstrapStream, String> {
    match listener.accept_authenticated_until(deadline, &IgnoreBootstrapRejections) {
        Ok(BootstrapAdmission::Authenticated(stream)) => Ok(stream),
        Ok(BootstrapAdmission::DeadlineElapsed) => {
            Err("generated main did not authenticate before the admission deadline".to_owned())
        }
        Ok(BootstrapAdmission::Cancelled) => {
            Err("generated-main admission was cancelled".to_owned())
        }
        Err(error) => Err(format!("accept generated-main app-link: {error}")),
    }
}

fn wait_for_output(child: ObservedChild, timeout: Duration) -> Result<Output, String> {
    child.wait_for_output(timeout)
}

fn output_diagnostics(output: &Output) -> String {
    format!(
        "status={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn finish_lifecycle_fixture(
    server: thread::JoinHandle<Result<(), String>>,
    child: ObservedChild,
    child_timeout: Duration,
) -> Result<Output, String> {
    let child_result = wait_for_output(child, child_timeout);
    match (server.join(), child_result) {
        (Ok(Ok(())), output) => output,
        (Ok(Err(server_error)), Ok(output)) => Err(format!(
            "wire server failed: {server_error}; child {}",
            output_diagnostics(&output)
        )),
        (Ok(Err(server_error)), Err(child_error)) => {
            Err(format!("wire server failed: {server_error}; {child_error}"))
        }
        (Err(_), Ok(output)) => Err(format!(
            "wire server panicked; child {}",
            output_diagnostics(&output)
        )),
        (Err(_), Err(child_error)) => Err(format!("wire server panicked; {child_error}")),
    }
}

fn fixture_admission_deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn send_ready_then_reply_to_stock_echo<S: Read + Write>(stream: &mut S) -> CorrelationId {
    let ready = keld_ipc::codec::encode(&LifecycleEvent::Ready).expect("encode Ready");
    write_frame(
        stream,
        FrameKind::Event,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(0),
        &ready,
    )
    .expect("send Ready before Echo Reply");

    let (echo_header, echo_payload) = read_frame(stream).expect("read Echo Call");
    assert_eq!(echo_header.kind, FrameKind::Call);
    assert_eq!(echo_header.flags, 0);
    assert_eq!(echo_header.channel, ECHO_CHANNEL);
    assert_ne!(echo_header.corr, CorrelationId(0));
    let request: EchoRequest = keld_ipc::codec::decode(&echo_payload).expect("decode Echo");
    assert_eq!(request.message, "keld");
    assert_eq!(request.count, 1);
    let echo_reply = keld_ipc::codec::encode(&EchoResponse {
        message: request.message,
        count: request.count,
    })
    .expect("encode Echo Reply");
    write_frame(
        stream,
        FrameKind::Reply,
        0,
        ECHO_CHANNEL,
        echo_header.corr,
        &echo_reply,
    )
    .expect("send Echo Reply");
    echo_header.corr
}

fn spawn_generated_main(project: &Path, link: &str) -> ObservedChild {
    spawn_observed(
        Command::new("bun")
            .args(["run", "src/main.ts"])
            .current_dir(project)
            .env("KELD_APP_LINK", link),
        project,
    )
}

fn add_entrypoint_diagnostic(project: &Path) {
    const GUARD: &str = "if (import.meta.main) {";
    const DIAGNOSTIC: &str = r#"console.error("KEL185_ENTRYPOINT_DIAGNOSTIC " + JSON.stringify({
  schema: "kel185-entrypoint-diagnostic/v1",
  bunVersion: Bun.version,
  bunRevision: Bun.revision,
  execPath: process.execPath,
  pid: process.pid,
  cwd: process.cwd(),
  argv: process.argv,
  importMetaMain: import.meta.main,
  importMetaPath: import.meta.path,
  keldAppLinkPresent: typeof process.env.KELD_APP_LINK === "string" && process.env.KELD_APP_LINK.length > 0,
}));

"#;
    let main_path = project.join("src/main.ts");
    let main = std::fs::read_to_string(&main_path).expect("read generated main");
    assert_eq!(
        main.matches(GUARD).count(),
        1,
        "generated main must have one entrypoint guard"
    );
    std::fs::write(
        &main_path,
        main.replacen(GUARD, &format!("{DIAGNOSTIC}{GUARD}"), 1),
    )
    .expect("write diagnostic generated main");
}

const NO_CLIENT_ADMISSION_CHILD: &str = "created_template_server_admission_without_client_child";

/// The lifecycle wire fixture must report a generated-main launch/admission
/// failure instead of leaving its server worker blocked for the test binary's
/// lifetime. The child isolates the deliberately absent client so the parent
/// can reap the current unbounded implementation with an independent kill switch.
#[test]
fn created_template_server_admission_without_client_is_bounded() {
    let output_dir = tempfile::tempdir().expect("no-client output tempdir");
    let test_binary = std::env::current_exe().expect("current bun_echo test binary");
    let child = spawn_observed(
        Command::new(test_binary).args([
            "--exact",
            NO_CLIENT_ADMISSION_CHILD,
            "--ignored",
            "--nocapture",
        ]),
        output_dir.path(),
    );

    let output = wait_for_output(child, Duration::from_secs(2))
        .expect("no-client admission fixture must terminate with a bounded result");
    assert!(
        output.status.success(),
        "no-client admission fixture failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "spawned only by the bounded no-client admission parent"]
fn created_template_server_admission_without_client_child() {
    let listener = BootstrapListener::bind().expect("bind no-client app-link");
    let error = accept_generated_main(&listener, Instant::now() + Duration::from_millis(250))
        .expect_err("no-client admission must report its absolute deadline");
    assert!(error.contains("admission deadline"), "{error}");
}

/// A generated main that exits before authenticating must leave its status and
/// stderr in every KEL-185 lifecycle fixture failure path.
#[test]
fn created_template_pre_auth_failure_preserves_child_diagnostics() {
    const MARKER: &str = "KEL185_PREAUTH_FAILURE";

    for path in ["last-window-closed", "stalled-frame", "early-close"] {
        let dir = tempfile::tempdir().expect("tempdir");
        create_project(dir.path(), "app").expect("create");
        let project = dir.path().join("app");
        std::fs::write(
            project.join("src/main.ts"),
            format!("console.error({MARKER:?}); process.exit(23);\n"),
        )
        .expect("write pre-auth failure main");

        let listener = BootstrapListener::bind().expect("bind app-link");
        let link = listener.app_link();
        let admission_deadline = Instant::now() + Duration::from_millis(250);
        let server = thread::spawn(move || {
            accept_generated_main(&listener, admission_deadline)
                .map(|_| ())
                .map_err(|error| format!("{path}: {error}"))
        });
        let child = spawn_generated_main(&project, &link);

        let error = finish_lifecycle_fixture(server, child, Duration::from_secs(2))
            .expect_err("pre-auth failure is not a successful lifecycle session");
        assert!(error.contains(path), "{error}");
        assert!(error.contains("status=Some(23)"), "{error}");
        assert!(error.contains(MARKER), "{error}");
    }
}

/// Diagnostic-only fixture for the hosted Windows zero-output exit. The three
/// lifecycle wire tests below remain untouched stock generators, so this
/// record cannot make their acceptance pass. A false entrypoint flag fails
/// with the redacted identity record captured through the same file-backed path.
#[test]
fn created_template_entrypoint_identity_precedes_the_main_guard() {
    const PREFIX: &str = "KEL185_ENTRYPOINT_DIAGNOSTIC ";

    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");
    add_entrypoint_diagnostic(&project);

    let listener = BootstrapListener::bind().expect("bind app-link");
    let link = listener.app_link();
    let admission_deadline = Instant::now() + Duration::from_secs(2);
    let server =
        thread::spawn(move || accept_generated_main(&listener, admission_deadline).map(|_| ()));
    let child = spawn_generated_main(&project, &link);
    let output = wait_for_output(child, Duration::from_secs(4)).expect("bounded diagnostic child");
    let admission = server.join().expect("diagnostic server thread");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let record = stderr
        .lines()
        .find_map(|line| line.strip_prefix(PREFIX))
        .unwrap_or_else(|| {
            panic!(
                "diagnostic record missing from file-backed stderr; child {}",
                output_diagnostics(&output)
            )
        });
    let record: serde_json::Value = serde_json::from_str(record).unwrap_or_else(|error| {
        panic!(
            "invalid entrypoint JSON: {error}; raw_record={record:?}; child {}",
            output_diagnostics(&output)
        )
    });
    let evidence = || format!("identity={record}; child {}", output_diagnostics(&output));
    assert_eq!(
        record["schema"],
        "kel185-entrypoint-diagnostic/v1",
        "{}",
        evidence()
    );
    assert_eq!(record["keldAppLinkPresent"], true, "{}", evidence());
    assert!(
        record["bunVersion"].as_str().is_some_and(|v| !v.is_empty()),
        "{}",
        evidence()
    );
    assert!(
        record["bunRevision"]
            .as_str()
            .is_some_and(|v| !v.is_empty()),
        "{}",
        evidence()
    );
    assert!(
        record["execPath"].as_str().is_some_and(|v| !v.is_empty()),
        "{}",
        evidence()
    );
    assert!(
        record["pid"].as_u64().is_some_and(|pid| pid > 0),
        "{}",
        evidence()
    );
    assert!(
        record["cwd"].as_str().is_some_and(|v| !v.is_empty()),
        "{}",
        evidence()
    );
    assert!(
        record["argv"].as_array().is_some_and(|v| !v.is_empty()),
        "{}",
        evidence()
    );
    assert!(
        record["importMetaPath"]
            .as_str()
            .is_some_and(|v| !v.is_empty()),
        "{}",
        evidence()
    );
    assert_eq!(
        record["importMetaMain"],
        true,
        "generated entrypoint guard would be skipped; {}",
        evidence()
    );
    admission.unwrap_or_else(|error| {
        panic!(
            "main=true diagnostic child did not authenticate: {error}; {}",
            evidence()
        )
    });
    println!("{PREFIX}{record}");
}

#[test]
fn bun_main_runs_ipc_echo_with_unique_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main_ts = dir.path().join("main.ts");
    std::fs::write(
        &main_ts,
        r#"
const link = process.env.KELD_APP_LINK;
if (!link) {
  console.error("KELD-CLI-010: KELD_APP_LINK is unset");
  process.exit(1);
}
const keld = process.env.KELD_BIN ?? "keld";
const message = process.env.KELD_ECHO_MESSAGE;
const count = process.env.KELD_ECHO_COUNT;
if (!message || !count) {
  console.error("KELD-CLI-011: KELD_ECHO_MESSAGE/COUNT unset");
  process.exit(1);
}
const proc = Bun.spawn(
  [keld, "ipc-client", "echo", "--link", link, "--message", message, "--count", count],
  { stdout: "inherit", stderr: "inherit", env: process.env },
);
const code = await proc.exited;
if (code !== 0) process.exit(code);
console.log("kel30: main process ready (IPC echo ok)");
"#,
    )
    .expect("write main.ts");

    let (ready_tx, ready_rx) = mpsc::channel();
    let server = EchoServer::start(&ready_tx).expect("bind echo server");
    ready_rx.recv().expect("server ready");
    let link = server.link();

    let unique = format!("kel30-{}", std::process::id());
    let keld_bin = env!("CARGO_BIN_EXE_keld");
    let output = Command::new("bun")
        .arg("run")
        .arg(&main_ts)
        .env("KELD_APP_LINK", &link)
        .env("KELD_BIN", keld_bin)
        .env("KELD_ECHO_MESSAGE", &unique)
        .env("KELD_ECHO_COUNT", "9")
        .output()
        .expect("spawn bun");

    server.join().expect("server join");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "bun failed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains(&format!("ipc-echo ok: message={unique:?} count=9")),
        "missing typed echo fields in stdout={stdout}"
    );
    assert!(
        stdout.contains("main process ready"),
        "missing ready line in stdout={stdout}"
    );
    assert!(
        !stdout.contains("message=\"keld\" count=1"),
        "must not print the hardcoded demo payload: {stdout}"
    );
}

#[test]
fn ipc_echo_binary_prints_demo_fields() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .arg("ipc-echo")
        .output()
        .expect("spawn ipc-echo");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "ipc-echo failed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("ipc-echo ok: message=\"keld\" count=1"),
        "stdout={stdout}"
    );
}

#[test]
fn ipc_client_missing_link_is_cli_040() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .args(["ipc-client", "echo"])
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-CLI-040"), "{stderr}");
    assert!(stderr.contains("--link"), "{stderr}");
}

#[test]
fn ipc_client_link_flag_without_value_is_cli_040() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .args(["ipc-client", "echo", "--link"])
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-CLI-040"), "{stderr}");
    assert!(stderr.contains("requires a value"), "{stderr}");
}

#[test]
fn ipc_client_link_without_token_is_ipc_007() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .args([
            "ipc-client",
            "echo",
            "--link",
            "/no/such/keld-echo-kel30.sock",
        ])
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-IPC-007"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("ipc-echo ok"),
        "must not print a fabricated reply"
    );
}

#[test]
fn ipc_client_missing_socket_is_ipc_001() {
    let hex = "11".repeat(32);
    #[cfg(unix)]
    let link = format!("/no/such/keld-echo-kel30.sock#{hex}");
    #[cfg(windows)]
    let link = format!("1#{hex}");
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .args([
            "ipc-client",
            "echo",
            "--link",
            &link,
            "--message",
            "should-not-echo",
            "--count",
            "3",
        ])
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-IPC-001"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("should-not-echo"),
        "must not print a fabricated reply"
    );
}

#[test]
fn created_template_main_runs_ipc_echo() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");

    // Template stays alive for the host window; `run_dev_echo` awaits the
    // ready marker then reaps (KEL-30). Do not `.output()` the stay-alive child.
    let result = keld_cli::dev::run_dev_echo(&project).expect("dev echo");
    let stdout = result.stdout;
    assert!(
        stdout.contains("ipc-echo ok: message=\"keld\" count=1"),
        "template must speak kipc echo: stdout={stdout}"
    );
    assert!(
        stdout.contains("app: main process ready (IPC echo ok)"),
        "substituted project name missing: stdout={stdout}"
    );
    assert!(
        !stdout.contains("{{name}}"),
        "unsubstituted template leaked: stdout={stdout}"
    );
}

/// KEL-185 regression: the untouched generated main must consume the host's
/// lifecycle event and answer Quit on its already-authenticated app-link.
///
/// The Rust server is the wire oracle. It deliberately sends `Ready` before
/// the Echo Reply, then requires `LastWindowClosed`, a correlated `Quit`, its
/// Reply, and EOF. A second `HELLO`, a competing reader, or the old permanent
/// park fails at the exact frame where it appears. The timeout only reaps a faulty child.
#[test]
fn created_template_quits_after_last_window_closed_on_the_same_link() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");

    let listener = BootstrapListener::bind().expect("bind app-link");
    let link = listener.app_link();
    let admission_deadline = fixture_admission_deadline();
    let server = thread::spawn(move || -> Result<(), String> {
        let mut stream = accept_generated_main(&listener, admission_deadline)?;
        stream
            .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
            .expect("set app-link deadlines");

        let echo_corr = send_ready_then_reply_to_stock_echo(&mut stream);

        // Window lifetime is intentionally longer than the request/reply I/O
        // deadline. The timeout is the contract measurement: no message is
        // expected, and the sender stays live so disconnect cannot satisfy it.
        let (_idle_guard, idle_window) = mpsc::channel::<()>();
        assert!(matches!(
            idle_window.recv_timeout(APP_LINK_IO_DEADLINE + Duration::from_millis(250)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));

        let ping_channel = ChannelId(91);
        let ping_corr = CorrelationId(0x185);
        write_frame(
            &mut stream,
            FrameKind::Ping,
            0,
            ping_channel,
            ping_corr,
            &[],
        )
        .expect("send lifecycle liveness Ping");
        let (ping_header, ping_payload) = read_frame(&mut stream).expect("read echoed Ping");
        assert_eq!(ping_header.kind, FrameKind::Ping);
        assert_eq!(ping_header.flags, 0);
        assert_eq!(ping_header.channel, ping_channel);
        assert_eq!(ping_header.corr, ping_corr);
        assert!(ping_payload.is_empty());

        let closed =
            keld_ipc::codec::encode(&LifecycleEvent::LastWindowClosed).expect("encode close");
        write_frame(
            &mut stream,
            FrameKind::Event,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            &closed,
        )
        .expect("send LastWindowClosed");

        let (quit_header, quit_payload) = read_frame(&mut stream).expect("read Quit Call");
        assert_eq!(quit_header.kind, FrameKind::Call, "no second HELLO");
        assert_eq!(quit_header.flags, 0);
        assert_eq!(quit_header.channel, LIFECYCLE_CHANNEL);
        assert_ne!(quit_header.corr, CorrelationId(0));
        assert_ne!(quit_header.corr, echo_corr);
        let request: LifecycleRequest =
            keld_ipc::codec::decode(&quit_payload).expect("decode Quit Call");
        assert_eq!(request, LifecycleRequest::Quit);
        let quit_reply =
            keld_ipc::codec::encode(&LifecycleResponse::Quit).expect("encode Quit Reply");
        write_frame(
            &mut stream,
            FrameKind::Reply,
            0,
            LIFECYCLE_CHANNEL,
            quit_header.corr,
            &quit_reply,
        )
        .expect("send correlated Quit Reply");

        let mut trailing = [0_u8; 1];
        assert_eq!(
            stream.read(&mut trailing).expect("read client EOF"),
            0,
            "client must close after consuming the Quit Reply"
        );
        Ok(())
    });

    let child = spawn_generated_main(&project, &link);
    let output = finish_lifecycle_fixture(server, child, Duration::from_secs(10))
        .expect("generated main must exit");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "generated main failed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("ipc-echo ok: message=\"keld\" count=1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("app: main process ready (IPC echo ok)"),
        "{stdout}"
    );
}

/// A peer that starts a lifecycle frame and then stalls must not turn the
/// idle-event exception into an unbounded read. The first header byte starts
/// the adapter's absolute frame deadline; Bun must fail and close the link.
#[test]
fn created_template_rejects_a_stalled_lifecycle_frame() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");

    let listener = BootstrapListener::bind().expect("bind app-link");
    let link = listener.app_link();
    let admission_deadline = fixture_admission_deadline();
    let server = thread::spawn(move || -> Result<(), String> {
        let mut stream = accept_generated_main(&listener, admission_deadline)?;
        stream
            .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
            .expect("set app-link deadlines");
        send_ready_then_reply_to_stock_echo(&mut stream);

        let header = FrameHeader {
            kind: FrameKind::Event,
            flags: 0,
            channel: LIFECYCLE_CHANNEL,
            corr: CorrelationId(0),
            len: 1,
        }
        .encode();
        stream
            .write_all(&header[..1])
            .expect("send first byte of a lifecycle frame");
        stream
            .flush()
            .expect("make the stalled frame byte observable");
        stream
            .set_app_link_read_deadline(Some(APP_LINK_IO_DEADLINE + Duration::from_secs(2)))
            .expect("bound EOF observation");
        let mut trailing = [0_u8; 1];
        assert_eq!(
            stream
                .read(&mut trailing)
                .expect("client closes after frame timeout"),
            0,
            "timed-out client must close its app-link"
        );
        Ok(())
    });

    let child = spawn_generated_main(&project, &link);
    let output = finish_lifecycle_fixture(server, child, Duration::from_secs(10))
        .expect("bounded client failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "stalled frame must fail: {stderr}"
    );
    assert!(stderr.contains("KELD-IPC-006"), "{stderr}");
}

/// Peer close while the stock app is waiting for a lifecycle Event remains a
/// hard link failure. It must reject and reap rather than returning to a park.
#[test]
fn created_template_reaps_after_early_lifecycle_link_close() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");

    let listener = BootstrapListener::bind().expect("bind app-link");
    let link = listener.app_link();
    let admission_deadline = fixture_admission_deadline();
    let server = thread::spawn(move || -> Result<(), String> {
        let mut stream = accept_generated_main(&listener, admission_deadline)?;
        stream
            .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
            .expect("set app-link deadlines");
        send_ready_then_reply_to_stock_echo(&mut stream);
        Ok(())
    });

    let child = spawn_generated_main(&project, &link);
    let output = finish_lifecycle_fixture(server, child, Duration::from_secs(5))
        .expect("bounded client failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "early close must fail: {stderr}");
    assert!(stderr.contains("KELD-IPC-001"), "{stderr}");
}

#[test]
fn kipc_ts_golden_vectors_pass_under_bun_test() {
    let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/hello");
    let output = Command::new("bun")
        .args(["test", "src/kipc.test.ts"])
        .current_dir(&hello)
        .output()
        .expect("bun must be on PATH (same contract as bun_echo)");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "kipc.ts golden vectors failed: stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn created_template_session_two_echoes_on_one_connection() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");
    std::fs::write(
        project.join("src/two.ts"),
        r#"
import { AppLinkSession } from "./kipc";

const link = process.env.KELD_APP_LINK;
if (!link) {
  console.error("KELD-CLI-010: KELD_APP_LINK is unset");
  process.exit(1);
}
const firstMsg = process.env.KELD_ECHO_A;
const secondMsg = process.env.KELD_ECHO_B;
if (!firstMsg || !secondMsg) {
  console.error("KELD-CLI-011: KELD_ECHO_A/B unset");
  process.exit(1);
}

const session = await AppLinkSession.connect(link);
try {
  const first = await session.echo({ message: firstMsg, count: 2 });
  const second = await session.echo({ message: secondMsg, count: 9 });
  console.log(`first=${JSON.stringify(first.message)}:${first.count}`);
  console.log(`second=${JSON.stringify(second.message)}:${second.count}`);
} finally {
  session.close();
}
"#,
    )
    .expect("write two.ts");

    let (ready_tx, ready_rx) = mpsc::channel();
    let server = EchoServer::start(&ready_tx).expect("bind echo server");
    ready_rx.recv().expect("server ready");
    let link = server.link();

    let unique_a = format!("kel30-a-{}", std::process::id());
    let unique_b = format!("kel30-b-{}", std::process::id());
    let output = Command::new("bun")
        .arg("run")
        .arg("src/two.ts")
        .current_dir(&project)
        .env("KELD_APP_LINK", &link)
        .env("KELD_ECHO_A", &unique_a)
        .env("KELD_ECHO_B", &unique_b)
        .output()
        .expect("spawn bun");

    server.join().expect("server join");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "two-call session failed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains(&format!("first={unique_a:?}:2")),
        "missing first unique echo: stdout={stdout}"
    );
    assert!(
        stdout.contains(&format!("second={unique_b:?}:9")),
        "missing second unique echo: stdout={stdout}"
    );
    assert_ne!(unique_a, unique_b);
    // two.ts prints `first=${JSON.stringify(message)}:${count}` (same for second),
    // never `message=... count=...`. A demo-payload leak must match that format.
    assert!(
        !stdout.contains("first=\"keld\":1") && !stdout.contains("second=\"keld\":1"),
        "must not print the hardcoded demo payload: {stdout}"
    );
}
