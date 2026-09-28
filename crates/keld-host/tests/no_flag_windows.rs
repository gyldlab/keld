//! Real-Windows KEL-96/T4 no-flag host acceptance.

#![cfg(windows)]
#![allow(unsafe_code)] // test-only raw cross-process handle-table observation
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: process and OS observations are assertion oracles
#![deny(unsafe_op_in_unsafe_fn)]

use std::env;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::OwnedHandle;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[path = "no_flag/windows/media.rs"]
mod media;

#[path = "no_flag/windows/profiles/mod.rs"]
mod profiles;

#[path = "no_flag/windows/dev_lifecycle.rs"]
mod dev_lifecycle;

#[path = "no_flag/windows/console.rs"]
mod console;

#[path = "no_flag/windows/lifecycle.rs"]
mod lifecycle;

#[path = "no_flag/windows/recovery.rs"]
mod recovery;

#[path = "no_flag/windows/staging.rs"]
mod staging;

#[path = "no_flag/windows/control_contract.rs"]
mod control_contract;

#[path = "no_flag/windows/renderer_http_contract.rs"]
mod renderer_http_contract;

#[path = "no_flag/windows/renderer_publication.rs"]
mod renderer_publication;

#[path = "no_flag/windows/renderer_deadlines.rs"]
mod renderer_deadlines;

#[path = "no_flag/windows/renderer_connections.rs"]
mod renderer_connections;

#[path = "no_flag/windows/support/mod.rs"]
mod support;

#[path = "../../keld-wv/tests/fixtures/windows_renderer_http.rs"]
mod windows_renderer_http;

use console::{
    accept_console_timeout_readiness, run_console_ctrl_c_case, run_console_timeout_fixture,
    run_isolated_console_case,
};
use support::cross_user::{
    profile_request_address, profile_test_token_sids, profile_test_user_sid,
    read_profile_coordinator, write_profile_coordinator,
};
use support::process::{
    assert_process_signaled, process_exists, wait_child, wait_for_process_signal,
};
use support::product::ProductFixture;
use support::profile_response::validate_profile_state_atom;
use support::profile_run::{SignedProfileStateCase, SignedProfileStateRun};
use support::signed_identity::signed_fixture_profile_namespace;
use support::window::wait_for_host_window;

use serde_json::Value;
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;

/// Dark background for fixture renderers, so a test run does not flash
/// white windows across the operator's desktop. Cosmetic only: no test
/// asserts on it, and the beacon/marker contracts are unchanged.
const DARK_BG: &str = "<style>html,body{background:#111;color:#eee}</style>";
const PRODUCT_TITLE: &str = "KEL96 T4 Windows Fixture";
const PRODUCT_DEADLINE: Duration = Duration::from_secs(20);
const RENDERER_ACCEPT_POLL: Duration = Duration::from_millis(10);
const CONTROL_LINE_LIMIT: usize = 4096;
#[test]
fn keld_dev_windows_helper() {
    let Some(project) = std::env::var_os("KELD_T4_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(Path::new(&project)).expect("shipping Windows keld dev helper");
}

fn profile_state_run_nonce(fixture: &ProductFixture) -> String {
    let leaf = fixture
        .root
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("UTF-8 product fixture nonce");
    format!("{}-{leaf}", std::process::id())
}

#[test]
#[ignore = "operator launches this controller under the second ordinary account"]
fn kel135_second_user_storage_helper() {
    let request_path = env::var_os("KELD_KEL135_SECOND_USER_REQUEST")
        .expect("operator supplies the live request path");
    let mut bytes = Vec::new();
    fs::File::open(request_path)
        .expect("open parent request")
        .take(u64::try_from(CONTROL_LINE_LIMIT + 1).expect("request limit fits u64"))
        .read_to_end(&mut bytes)
        .expect("read bounded parent request");
    assert!(
        bytes.len() <= CONTROL_LINE_LIMIT,
        "cross-user request is bounded"
    );
    let request: Value = serde_json::from_slice(&bytes).expect("cross-user request JSON");
    let coordinator = profile_request_address(&request, "coordinator");
    let address = profile_request_address(&request, "server");
    let nonce = request["nonce"].as_str().expect("request nonce");
    validate_profile_state_atom(nonce, false).expect("request nonce domain");
    let host = std::ffi::OsStr::new(request["host"].as_str().expect("signed host path"));
    let identity =
        std::ffi::OsStr::new(request["identity"].as_str().expect("signed identity path"));
    let namespace = signed_fixture_profile_namespace(identity, Some(host));
    let user_sid = profile_test_user_sid();
    let administrator = profile_test_token_sids("/groups")
        .iter()
        .any(|sid| sid == "S-1-5-32-544");
    assert!(
        !administrator,
        "the second user must not be an administrator"
    );
    let stream = TcpStream::connect_timeout(&coordinator, PRODUCT_DEADLINE)
        .expect("connect to waiting controller");
    let mut peer = BufReader::new(stream);
    write_profile_coordinator(
        peer.get_mut(),
        &serde_json::json!({
            "kind": "hello", "nonce": nonce, "namespace": namespace,
            "user_sid": user_sid, "administrator": administrator, "server": address.to_string(),
        }),
    );
    let fixture = ProductFixture::new();
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("second-user host control");
    loop {
        let message = read_profile_coordinator(&mut peer);
        if message["kind"] == "stop" {
            write_profile_coordinator(peer.get_mut(), &serde_json::json!({"kind": "stopped"}));
            break;
        }
        assert_eq!(message["kind"], "run");
        let case = SignedProfileStateCase {
            name: message["case"].as_str().expect("remote case name"),
            host,
            before: message["before"].as_str().expect("remote before value"),
            after: message["after"].as_str().expect("remote after value"),
        };
        let run = SignedProfileStateRun::start(
            &fixture,
            &control,
            address,
            nonce,
            &case,
            Instant::now() + PRODUCT_DEADLINE,
        );
        write_profile_coordinator(
            peer.get_mut(),
            &serde_json::json!({
                "kind": "ready", "case": case.name, "host_pid": run.process.host_pid(), "bun_pid": run.bun_pid,
            }),
        );
        assert_eq!(read_profile_coordinator(&mut peer)["kind"], "finish");
        run.finish();
        write_profile_coordinator(
            peer.get_mut(),
            &serde_json::json!({"kind": "finished", "case": case.name}),
        );
    }
}

#[test]
fn shipping_windows_ctrl_c_preserves_host_output_and_ordered_cleanup() {
    // A separate hidden console keeps the real broadcast away from nextest and
    // unrelated tests. Only the inner observer ignores Ctrl+C, after CLI spawn.
    if std::env::var_os("KELD_T4_CONSOLE_CASE").is_none() {
        let stdout = run_isolated_console_case(
            "shipping_windows_ctrl_c_preserves_host_output_and_ordered_cleanup",
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        print!("{stdout}");
        return;
    }
    // Emergency process ownership only: normal cleanup is asserted before this
    // observer exits. Forced death can still retain its temporary stage files.
    keld_runtime::windows_job::install_host_death_job()
        .expect("install isolated observer death Job before descendants");
    if let Ok(port) = env::var("KELD_T4_CONSOLE_TIMEOUT_PORT") {
        run_console_timeout_fixture("observer", port.parse().expect("timeout fixture port"));
        return;
    }
    run_console_ctrl_c_case();
}

#[test]
fn isolated_console_timeout_reaps_ready_descendants() {
    if let Ok(role) = env::var("KELD_T4_CONSOLE_TIMEOUT_ROLE") {
        let port = env::var("KELD_T4_CONSOLE_TIMEOUT_PORT")
            .expect("timeout fixture port")
            .parse()
            .expect("numeric timeout fixture port");
        run_console_timeout_fixture(&role, port);
        return;
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("timeout fixture controller");
    let port = listener
        .local_addr()
        .expect("timeout controller address")
        .port();
    let mut worker = Some(thread::spawn(move || {
        run_isolated_console_case(
            "shipping_windows_ctrl_c_preserves_host_output_and_ordered_cleanup",
            Some(port),
        )
    }));
    let mut controls = Vec::<(String, BufReader<TcpStream>)>::new();
    let mut processes = Vec::<(String, u32, OwnedHandle)>::new();
    let observation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let launcher_pid =
            accept_console_timeout_readiness(&listener, &mut controls, &mut processes);
        let (_, observer_pid, observer) = processes
            .iter()
            .find(|(role, _, _)| role == "observer")
            .expect("ready observer");
        assert_eq!(launcher_pid, *observer_pid);
        let (_, launcher) = controls
            .iter_mut()
            .find(|(role, _)| role == "launcher")
            .expect("launcher timeout control");
        launcher
            .get_mut()
            .write_all(b"TIMEOUT\n")
            .expect("force the ready timeout branch");
        launcher.get_mut().flush().expect("flush timeout command");
        // Observe death before joining capture: an unfixed descendant may keep
        // an inherited capture pipe open after the observer and launcher exit.
        assert_process_signaled(observer, "timeout observer");
        for role in ["direct", "descendant"] {
            let (_, _, process) = processes
                .iter()
                .find(|(found, _, _)| found == role)
                .expect("ready descendant handle");
            assert_process_signaled(process, role);
        }
        let error = worker
            .take()
            .expect("timeout launcher worker")
            .join()
            .expect("timeout launcher did not panic")
            .expect_err("the live observer must reach the timeout error");
        assert!(
            error.contains("isolated console regression timed out"),
            "{error}"
        );
        assert!(error.contains("KELD_CONSOLE_WAIT_EXPIRED"), "{error}");
        assert!(
            !error.contains("timeout fixture control failed:"),
            "a control failure cannot substitute for an expired native wait: {error}"
        );
        assert_eq!(
            wait_for_process_signal(observer, 0),
            WAIT_OBJECT_0,
            "observer not reaped: {error}"
        );
        assert!(error.contains("KELD_CONSOLE_TIMEOUT_REAPED"), "{error}");
        println!(
            "KELD_CONSOLE_TIMEOUT_TREE observer={observer_pid} direct_and_descendant_signaled=true\n{error}"
        );
    }));
    // Keep failure cleanup after the saved observation. EOF releases only these
    // benign fixture processes when the no-Job negative control leaves them live.
    drop(controls);
    drop(listener);
    if let Some(worker) = worker {
        let result = worker
            .join()
            .expect("reap timeout launcher after fixture failure");
        eprintln!("timeout fixture failure cleanup: {result:?}");
    }
    for (role, _, process) in &processes {
        assert_process_signaled(process, &format!("timeout fixture cleanup {role}"));
    }
    if let Err(failure) = observation {
        std::panic::resume_unwind(failure);
    }
}

fn prepare_keld_dev_helper(fixture: &ProductFixture) -> std::path::PathBuf {
    let bin = fixture.root.path().join("bin");
    fs::create_dir(&bin).expect("helper bin directory");
    let helper = bin.join("keld-dev-helper.exe");
    fs::copy(
        std::env::current_exe().expect("current test executable"),
        &helper,
    )
    .expect("copy keld dev helper");
    fs::copy(env!("CARGO_BIN_EXE_keld-host"), bin.join("keld-host.exe"))
        .expect("copy sibling keld-host");
    helper
}

fn dev_stage_count(project: &Path) -> usize {
    fs::read_dir(project.join(".keld/dev"))
        .map_or(0, |entries| entries.filter_map(Result::ok).count())
}

fn wait_for_dev_stage_count(project: &Path, expected: usize, deadline: Instant) {
    loop {
        let observed = dev_stage_count(project);
        if observed == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected} dev stages after cleanup, observed {observed}"
        );
        thread::park_timeout(Duration::from_millis(20));
    }
}

fn dev_stage_command(root: &Path, host: &Path) -> Command {
    let mut command = Command::new(host);
    command
        .current_dir(root)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped());
    command
}
