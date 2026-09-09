//! Real Windows composition evidence for KEL-210's LPAC/app-link contract.
//!
//! This preserves the existing KEL-101 current-user-only pipe policy. It is a
//! compatibility prerequisite test, not an implementation of LPAC product startup.
//! `AppContainer` and capability count are observed; LPAC opt-out is configuration
//! provenance here, not independent token or containment qualification.

#![cfg(windows)]
#![allow(clippy::expect_used)] // independent process, token, and byte assertions

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::os::windows::io::AsHandle as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use keld_ipc::bootstrap::{
    WindowsNamedPipeBootstrapAdmission, WindowsNamedPipeBootstrapCancellation,
    WindowsNamedPipeBootstrapListener, WindowsNamedPipeBootstrapStream,
};
use keld_ipc::link::handshake_client;
use keld_ipc::token::parse_app_link;
use keld_ipc::{BootstrapRejection, BootstrapRejectionObserver};
use keld_runtime::windows_lpac::{
    WindowsLpacChild, WindowsLpacPathAccess, WindowsLpacProfile, WindowsLpacStdio,
};
const ENDPOINT_ENV: &str = "KELD_210_TEST_PIPE_ENDPOINT";
const CHILD_TEST: &str = "lpac_pipe_open_child";

#[derive(Default)]
struct Rejections(Mutex<Vec<BootstrapRejection>>);

impl BootstrapRejectionObserver for Rejections {
    fn rejected(&self, rejection: BootstrapRejection) {
        self.0.lock().expect("rejection recorder").push(rejection);
    }
}

#[test]
#[ignore = "subprocess entry; exercised by the non-ignored parent with real OS oracles"]
fn lpac_pipe_open_child() {
    let endpoint = env::var(ENDPOINT_ENV).expect("parent supplies an owned endpoint");
    match WindowsNamedPipeBootstrapStream::connect(&endpoint) {
        Ok(stream) => {
            drop(stream);
            println!("KEL210_PIPE_OPEN allowed=true");
        }
        Err(error) => {
            println!(
                "KEL210_PIPE_OPEN allowed=false kind={:?} raw={:?}",
                error.kind(),
                error.raw_os_error()
            );
        }
    }
}

#[test]
fn current_user_only_pipe_denies_configured_lpac_client_and_preserves_owner_authentication() {
    // Two fresh instances also prove the first observation does not poison a
    // subsequent listener. No change to the production pipe's security descriptor.
    for _ in 0..2 {
        observe_current_policy();
    }
}

fn observe_current_policy() {
    let root = tempfile::tempdir().expect("owned test root");
    println!("KEL210_FIXTURE root={}", root.path().display());
    let runtime = root.path().join("runtime");
    let private = root.path().join("private");
    fs::create_dir(&runtime).expect("runtime directory");
    fs::create_dir(&private).expect("private directory");
    let executable = runtime.join("pipe-client.exe");
    fs::copy(env::current_exe().expect("current test image"), &executable)
        .expect("copy reviewed test image");

    let listener =
        Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("current pipe policy"));
    let app_link = listener.app_link();
    let endpoint = listener.endpoint().to_owned();
    let cancellations = listener.cancellation();
    let seen = Arc::new(Rejections::default());
    let (finished_tx, finished_rx) = mpsc::channel();
    let server_listener = Arc::clone(&listener);
    let server_seen = Arc::clone(&seen);
    let server = thread::spawn(move || {
        let admitted = server_listener
            .accept_authenticated_until(Instant::now() + Duration::from_secs(20), &*server_seen)
            .expect("bounded bootstrap admission");
        let WindowsNamedPipeBootstrapAdmission::Authenticated(stream) = admitted else {
            return false;
        };
        // Retain the server until the real client consumed HELLO and closed;
        // dropping it immediately could discard a pending reply on Windows.
        let consumed = finished_rx.recv_timeout(Duration::from_secs(5)).is_ok();
        drop(stream);
        consumed
    });

    let server = BootstrapWorker {
        worker: Some(server),
        cancellation: cancellations,
    };
    let (exit, lpac_output) =
        run_lpac_client(root.path(), &runtime, &private, &executable, &endpoint);

    // The same image/API outside LPAC must actually open the very same pipe;
    // an always-failing client or an absent endpoint cannot pass this control.
    let normal_output = run_normal_client(&executable, &endpoint);
    let (name, token) = parse_app_link(&app_link).expect("host-minted link");
    let mut normal = WindowsNamedPipeBootstrapStream::connect(name).expect("owner connects");
    handshake_client(&mut normal, &token).expect("owner consumes authenticated HELLO");
    drop(normal);
    finished_tx.send(()).expect("client completion witness");
    let server_completed = server.finish();

    assert_eq!(exit, 0, "LPAC fixture execution failed: {lpac_output}");
    assert!(
        lpac_output.contains("allowed=false kind=PermissionDenied raw=Some(5)"),
        "current LPAC/pipe policy premise was refuted: {lpac_output}"
    );
    assert!(
        normal_output.contains("KEL210_PIPE_OPEN allowed=true"),
        "same-image normal-token positive control failed: {normal_output}"
    );
    assert!(server_completed, "intended client was not authenticated");
    assert!(
        seen.0
            .lock()
            .expect("rejections")
            .iter()
            .all(|item| *item != BootstrapRejection::HelloAuth),
        "OS-open denial must not be misreported as wrong-token HELLO"
    );
    root.close()
        .expect("all owned fixture resources removed after child reap");
    println!(
        "KEL210_COMPOSITION configured_lpac_open=ERROR_ACCESS_DENIED normal_open=allowed normal_hello=authenticated"
    );
}

fn run_lpac_client(
    root: &Path,
    runtime: &Path,
    private: &Path,
    executable: &Path,
    endpoint: &str,
) -> (u32, String) {
    let profile_name = format!(
        "keld-210-{}",
        root.file_name()
            .expect("unique root basename")
            .to_string_lossy()
    );
    let profile =
        WindowsLpacProfile::create(OsStr::new(&profile_name)).expect("fresh LPAC profile");
    profile
        .grant_path(root, WindowsLpacPathAccess::Traverse)
        .expect("root traverse");
    profile
        .grant_path(runtime, WindowsLpacPathAccess::ReadExecute)
        .expect("runtime ACL");
    profile
        .grant_path(private, WindowsLpacPathAccess::RolePrivate)
        .expect("private output ACL");
    let mut output = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(private.join("lpac-output.txt"))
        .expect("owned output sink");
    let input = File::open("NUL").expect("null stdin");
    let arguments: Vec<OsString> = ["--exact", CHILD_TEST, "--ignored", "--nocapture"]
        .into_iter()
        .map(OsString::from)
        .collect();
    let mut environment = vec![(OsString::from(ENDPOINT_ENV), OsString::from(endpoint))];
    for key in ["SystemRoot", "WINDIR", "USERPROFILE", "LOCALAPPDATA"] {
        if let Some(value) = env::var_os(key) {
            environment.push((OsString::from(key), value));
        }
    }
    environment.push((OsString::from("TEMP"), private.as_os_str().to_owned()));
    environment.push((OsString::from("TMP"), private.as_os_str().to_owned()));
    let child = profile
        .spawn_suspended(
            executable,
            &arguments,
            &environment,
            Some(private),
            Some(WindowsLpacStdio {
                stdin: input.as_handle(),
                stdout: output.as_handle(),
                stderr: output.as_handle(),
            }),
            &[],
        )
        .expect("suspended LPAC client");
    let mut child = ReapedChild(child);
    let token = child.0.observe_token().expect("actual AppContainer token");
    assert!(token.is_app_container);
    // Configuration provenance only: observe_token does not query the LPAC flag.
    // This transport prerequisite does not certify All Application Packages opt-out.
    assert!(token.all_application_packages_opt_out_configured);
    assert_eq!(token.capability_count, 0);
    child.0.resume().expect("resume inspected client");
    let exit = child.0.wait(10_000).expect("bounded LPAC client exit");
    output.seek(SeekFrom::Start(0)).expect("rewind output");
    let mut lpac_output = String::new();
    output
        .read_to_string(&mut lpac_output)
        .expect("read LPAC observation");

    (exit, lpac_output)
}

// These guards join/reap on assertion failures as well as the successful path.
struct ReapedChild(WindowsLpacChild);

impl Drop for ReapedChild {
    fn drop(&mut self) {
        if self.0.wait(0).is_ok() {
            return;
        }
        let termination = self.0.terminate(1);
        let reaped = self.0.wait(5_000);
        if let Ok(exit) = &reaped {
            eprintln!("KEL210_CLEANUP child_reaped=true exit={exit}");
        }
        if let Err(error) = &termination {
            eprintln!("owned child termination failed: {error}");
        }
        if let Err(error) = &reaped {
            eprintln!("owned child reap failed: {error}");
        }
        if !thread::panicking() {
            reaped.expect("owned child must be reaped before releasing profile/resources");
        }
    }
}

struct BootstrapWorker {
    worker: Option<thread::JoinHandle<bool>>,
    cancellation: WindowsNamedPipeBootstrapCancellation,
}

impl BootstrapWorker {
    fn finish(mut self) -> bool {
        self.worker
            .take()
            .expect("owned worker")
            .join()
            .expect("bootstrap worker joins")
    }
}

impl Drop for BootstrapWorker {
    fn drop(&mut self) {
        let cancellation = self.cancellation.cancel();
        if let Err(error) = &cancellation {
            eprintln!("owned listener cancellation failed: {error}");
        }
        if let Some(worker) = self.worker.take() {
            let joined = worker.join();
            eprintln!("KEL210_CLEANUP worker_joined=true");
            if joined.is_err() {
                eprintln!("owned bootstrap worker panicked");
            }
            if !thread::panicking() {
                cancellation.expect("listener cancellation");
                joined.expect("cancelled worker joins");
            }
        } else if !thread::panicking() {
            cancellation.expect("idempotent listener cancellation");
        }
    }
}

fn run_normal_client(executable: &Path, endpoint: &str) -> String {
    let mut child = Command::new(executable)
        .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
        .env(ENDPOINT_ENV, endpoint)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("same-image normal-token control");
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().expect("control status").is_none() {
        if Instant::now() >= deadline {
            child.kill().expect("kill only the owned timed-out control");
            child.wait().expect("reap timed-out control");
            break;
        }
        thread::yield_now();
    }
    let output = child.wait_with_output().expect("control output");
    assert!(output.status.success(), "normal control failed");
    String::from_utf8(output.stdout).expect("UTF-8 control observation")
}
