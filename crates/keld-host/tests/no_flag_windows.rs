//! Real-Windows KEL-96/T4 no-flag host acceptance.

#![cfg(windows)]
#![allow(unsafe_code)] // test-only raw cross-process handle-table observation
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: process and OS observations are assertion oracles
#![deny(unsafe_op_in_unsafe_fn)]

use std::env;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::windows::io::OwnedHandle;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

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
use windows_renderer_http::{
    PendingRendererRequest, RendererRequestRead, accept_renderer_connection,
    read_renderer_request_line,
};

use console::{
    accept_console_timeout_readiness, run_console_ctrl_c_case, run_console_timeout_fixture,
    run_isolated_console_case,
};
use support::control::{
    accept_control_or_host_failure, accept_control_until, parse_descendant_pid, read_control_line,
    read_control_line_or_host_failure,
};
use support::process::{
    assert_process_signaled, process_exists, wait_child, wait_for_process_signal,
};
use support::product::ProductFixture;
use support::renderer::renderer_beacon_remaining;
use support::signed_identity::signed_fixture_profile_namespace;
use support::signed_process::SignedStateProcessGuard;
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

#[test]
#[ignore = "requires signed KEL-135 host and package-purge fixtures"]
fn kel135_signed_host_purge_removes_same_origin_state() {
    let signed_host = env::var_os("KELD_KEL135_SIGNED_HOST_A_P1")
        .expect("KELD_KEL135_SIGNED_HOST_A_P1 must point to a signed A/P1 host");
    let signed_purge = env::var_os("KELD_KEL135_SIGNED_PURGE_FIXTURE")
        .expect("KELD_KEL135_SIGNED_PURGE_FIXTURE must point to a signed A/P1 core fixture");
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind purge control");
    let state_server = ProfileStateServer::new();
    let run_nonce = profile_state_run_nonce(&fixture);
    let seeded_state = format!("{run_nonce}-seed");
    let recovered_state = format!("{run_nonce}-recovered");
    let seed = SignedProfileStateCase {
        name: "purge-seed",
        host: &signed_host,
        before: "",
        after: &seeded_state,
    };
    run_signed_profile_state_case(
        &fixture,
        &control_listener,
        &state_server,
        &run_nonce,
        &seed,
    );

    assert_signed_purge_success(run_signed_purge_fixture(&signed_purge, false));

    let recovered = SignedProfileStateCase {
        name: "purge-recovered",
        host: &signed_host,
        before: "",
        after: &recovered_state,
    };
    run_signed_profile_state_case(
        &fixture,
        &control_listener,
        &state_server,
        &run_nonce,
        &recovered,
    );
}

#[test]
#[ignore = "requires signed KEL-135 host and package-purge fixtures"]
fn kel135_signed_host_recovers_an_interrupted_purge() {
    let signed_host = env::var_os("KELD_KEL135_SIGNED_HOST_A_P1")
        .expect("KELD_KEL135_SIGNED_HOST_A_P1 must point to a signed A/P1 host");
    let signed_purge = env::var_os("KELD_KEL135_SIGNED_PURGE_FIXTURE")
        .expect("KELD_KEL135_SIGNED_PURGE_FIXTURE must point to a signed A/P1 core fixture");
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind recovery control");
    let state_server = ProfileStateServer::new();
    let run_nonce = profile_state_run_nonce(&fixture);
    let seeded_state = format!("{run_nonce}-seed");
    let recovered_state = format!("{run_nonce}-recovered");
    let seed = SignedProfileStateCase {
        name: "purge-crash-seed",
        host: &signed_host,
        before: "",
        after: &seeded_state,
    };
    run_signed_profile_state_case(
        &fixture,
        &control_listener,
        &state_server,
        &run_nonce,
        &seed,
    );

    let interrupted = run_signed_purge_fixture(&signed_purge, true);
    assert!(
        !interrupted.status.success(),
        "purge fault fixture unexpectedly survived the post-intent crash"
    );
    let interrupted_stdout =
        String::from_utf8(interrupted.stdout).expect("interrupted purge stdout UTF-8");
    assert!(
        interrupted_stdout.contains("KELD_KEL135_PURGE_FAULT prepared"),
        "purge fault fixture did not reach the durable prepared intent: {interrupted_stdout}"
    );
    assert_signed_purge_success(run_signed_purge_fixture(&signed_purge, false));

    let recovered = SignedProfileStateCase {
        name: "purge-crash-recovered",
        host: &signed_host,
        before: "",
        after: &recovered_state,
    };
    run_signed_profile_state_case(
        &fixture,
        &control_listener,
        &state_server,
        &run_nonce,
        &recovered,
    );
}

#[test]
#[ignore = "requires signed KEL-135 identity and media fixtures"]
fn kel135_signed_profile_saved_media_grants_are_revoked() {
    let signed_identity = env::var_os("KELD_KEL135_SIGNED_IDENTITY_FIXTURE")
        .expect("KELD_KEL135_SIGNED_IDENTITY_FIXTURE must point to signed A/P1 core fixture");
    let media_fixture = env::var_os("KELD_KEL135_MEDIA_FIXTURE")
        .expect("KELD_KEL135_MEDIA_FIXTURE must point to the media-acceptance libtest");
    let namespace = signed_fixture_profile_namespace(&signed_identity, None);
    for (kind, run_id) in [
        ("camera", "f1e2d3c4b5a69788796a5b4c3d2e1f00"),
        ("microphone", "001f2e3d4c5b6a798897a6b5c4d3e2f1"),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve media origin");
        let address = listener.local_addr().expect("media origin address");
        drop(listener);
        let address = address.to_string();
        run_signed_media_phase(
            &media_fixture,
            &namespace,
            kind,
            run_id,
            &address,
            "seed",
            "resolved",
        );
        run_signed_media_phase(
            &media_fixture,
            &namespace,
            kind,
            run_id,
            &address,
            "deny",
            "error:NotAllowedError",
        );
    }
}

fn run_signed_media_phase(
    media_fixture: &std::ffi::OsStr,
    namespace: &str,
    kind: &str,
    run_id: &str,
    address: &str,
    phase: &str,
    expected: &str,
) {
    let output = Command::new(media_fixture)
        .args([
            "webview2::media_acceptance::tests::windows_saved_grant_phase_subprocess",
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KELD_PROFILE_SAVED_SIGNED_NAMESPACE", namespace)
        .env("KELD_PROFILE_SAVED_KIND", kind)
        .env("KELD_PROFILE_SAVED_RUN_ID", run_id)
        .env("KELD_PROFILE_SAVED_ADDRESS", address)
        .env("KELD_PROFILE_SAVED_PHASE", phase)
        .output()
        .expect("run signed media phase");
    let stdout = String::from_utf8(output.stdout).expect("signed media stdout UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("signed media stderr UTF-8");
    assert!(
        output.status.success(),
        "signed {kind} {phase} failed: {stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("KELD_PROFILE_SAVED_RESULT") && stdout.contains(expected),
        "signed {kind} {phase} receipt missing expected {expected}: {stdout}"
    );
}

fn run_signed_purge_fixture(
    signed_purge: &std::ffi::OsStr,
    crash_after_prepared: bool,
) -> std::process::Output {
    let mut command = Command::new(signed_purge);
    command.args([
        "app_session::tests::kel135_signed_package_purge_acceptance_fixture",
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]);
    if crash_after_prepared {
        command.env("KELD_KEL135_PURGE_CRASH_AFTER_PREPARED", "1");
    } else {
        command.env_remove("KELD_KEL135_PURGE_CRASH_AFTER_PREPARED");
    }
    command
        .output()
        .expect("run signed authenticated package-purge fixture")
}

fn assert_signed_purge_success(output: std::process::Output) {
    let stdout = String::from_utf8(output.stdout).expect("signed purge stdout UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("signed purge stderr UTF-8");
    assert!(
        output.status.success(),
        "signed package purge failed with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains("KELD_KEL135_SIGNED_PURGE profile_namespace="),
        "signed package purge omitted its identity receipt: {stdout}"
    );
}

struct SignedProfileStateCase<'a> {
    name: &'a str,
    host: &'a std::ffi::OsStr,
    before: &'a str,
    after: &'a str,
}

#[test]
#[ignore = "requires signed KEL-135 host fixtures and WebView2 state acceptance"]
fn kel135_signed_host_profile_state_isolation() {
    let primary_carrier = env::var_os("KELD_KEL135_SIGNED_HOST_A_P1")
        .expect("KELD_KEL135_SIGNED_HOST_A_P1 must point to a signed host");
    let sibling_carrier = env::var_os("KELD_KEL135_SIGNED_HOST_B_P1")
        .expect("KELD_KEL135_SIGNED_HOST_B_P1 must point to a signed host");
    let alternate_publisher_carrier = env::var_os("KELD_KEL135_SIGNED_HOST_A_P2")
        .expect("KELD_KEL135_SIGNED_HOST_A_P2 must point to a signed host");
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind state control");
    let state_server = ProfileStateServer::new();
    let run_nonce = profile_state_run_nonce(&fixture);
    let primary_state = format!("{run_nonce}-a");
    let sibling_state = format!("{run_nonce}-b");
    let publisher_two_value = format!("{run_nonce}-p2");
    let identity_paths = [
        ("A/P1", "KELD_KEL135_SIGNED_IDENTITY_A_P1", &primary_carrier),
        ("B/P1", "KELD_KEL135_SIGNED_IDENTITY_B_P1", &sibling_carrier),
        (
            "A/P2",
            "KELD_KEL135_SIGNED_IDENTITY_A_P2",
            &alternate_publisher_carrier,
        ),
    ];
    let identities = identity_paths.map(|(label, variable, carrier)| {
        let path = env::var_os(variable).expect("matching signed identity fixture is required");
        let namespace = signed_fixture_profile_namespace(&path, Some(carrier));
        assert_eq!(namespace.len(), 64, "{label} profile namespace width");
        assert!(namespace.bytes().all(|byte| byte.is_ascii_hexdigit()));
        (label, namespace)
    });
    assert_ne!(
        identities[0].1, identities[1].1,
        "A and B must have distinct authenticated identities"
    );
    assert_ne!(
        identities[0].1, identities[2].1,
        "publisher scopes must produce distinct identities"
    );
    println!(
        "KELD_KEL135_IDENTITIES {}",
        serde_json::json!({
            "user_sid": profile_test_user_sid(),
            "identities": identities,
            "origin": format!("http://{}", state_server.address()),
        })
    );
    let cases: [(&str, &std::ffi::OsStr, &str, &str); 9] = [
        ("a-seed", &primary_carrier, "", &primary_state),
        (
            "a-restart",
            &primary_carrier,
            &primary_state,
            &primary_state,
        ),
        ("b-isolated", &sibling_carrier, "", &sibling_state),
        (
            "a-after-b",
            &primary_carrier,
            &primary_state,
            &primary_state,
        ),
        (
            "a-p2-isolated",
            &alternate_publisher_carrier,
            "",
            &publisher_two_value,
        ),
        ("a-final", &primary_carrier, &primary_state, &primary_state),
        ("a-cleanup", &primary_carrier, &primary_state, ""),
        ("b-cleanup", &sibling_carrier, &sibling_state, ""),
        (
            "p2-cleanup",
            &alternate_publisher_carrier,
            &publisher_two_value,
            "",
        ),
    ];
    for (name, host, before, after) in cases {
        let case = SignedProfileStateCase {
            name,
            host,
            before,
            after,
        };
        run_signed_profile_state_case(
            &fixture,
            &control_listener,
            &state_server,
            &run_nonce,
            &case,
        );
    }
}

fn run_signed_profile_state_case(
    fixture: &ProductFixture,
    control_listener: &TcpListener,
    state_server: &ProfileStateServer,
    run_nonce: &str,
    case: &SignedProfileStateCase<'_>,
) {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    state_server.expect_case(case.name, deadline);
    let run = SignedProfileStateRun::start(
        fixture,
        control_listener,
        state_server.address(),
        run_nonce,
        case,
        deadline,
    );
    let observed = state_server.wait_for_case(case.name, deadline);
    record_profile_state_case(
        &observed,
        case,
        run_nonce,
        state_server.address(),
        run.process.host_pid(),
        run.bun_pid,
    );
    run.finish();
}

fn record_profile_state_case(
    observed: &ProfileStateObservation,
    case: &SignedProfileStateCase<'_>,
    run_nonce: &str,
    address: SocketAddr,
    host_pid: u32,
    bun_pid: u32,
) {
    assert_eq!(observed.nonce, run_nonce, "{} run nonce", case.name);
    observed
        .before
        .assert_value(case.before, case.name, "before");
    observed.after.assert_value(case.after, case.name, "after");
    println!(
        "KELD_KEL135_STORAGE {}",
        serde_json::json!({
            "case": case.name, "nonce": run_nonce, "origin": format!("http://{address}"),
            "before": observed.before.json(), "after": observed.after.json(),
            "host_pid": host_pid, "bun_pid": bun_pid,
        })
    );
}

struct SignedProfileStateRun {
    // Drop the process guard before releasing the staged namespace pins.
    process: SignedStateProcessGuard,
    reader: BufReader<TcpStream>,
    bun_pid: u32,
    deadline: Instant,
    case_name: String,
    _stage: keld_cli::boot::DevBootStage,
}

impl SignedProfileStateRun {
    fn start(
        fixture: &ProductFixture,
        control_listener: &TcpListener,
        address: SocketAddr,
        run_nonce: &str,
        case: &SignedProfileStateCase<'_>,
        deadline: Instant,
    ) -> Self {
        fs::write(
            fixture.project.join("index.html"),
            state_redirect_html(address, case.name, run_nonce, case.after),
        )
        .expect("write state renderer");
        let stage = keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(case.host))
            .expect("stage signed state host");
        let control_port = control_listener
            .local_addr()
            .expect("state control address")
            .port();
        let child = Command::new(stage.host())
            .current_dir(stage.root())
            .env("KELD_T1B_CONTROL", control_port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch signed state host");
        let mut process = SignedStateProcessGuard::new(child);
        let control =
            accept_control_or_host_failure(control_listener, process.child_mut(), deadline);
        control
            .set_read_timeout(Some(PRODUCT_DEADLINE))
            .expect("state control timeout");
        let mut reader = BufReader::new(control);
        let hello = read_control_line(&mut reader);
        let mut hello_fields = hello.split_whitespace();
        assert_eq!(hello_fields.next(), Some("HELLO"), "{hello}");
        let bun_pid = hello_fields
            .next()
            .expect("state Bun PID")
            .parse::<u32>()
            .expect("state numeric Bun PID");
        let _app_link = hello_fields.next().expect("state app link");
        assert!(hello_fields.next().is_none(), "{hello}");
        process.observe_bun(bun_pid);
        assert_eq!(parse_descendant_pid(&read_control_line(&mut reader)), 0);
        assert_eq!(
            read_control_line_or_host_failure(&mut reader, process.child_mut(), "state READY"),
            "READY"
        );
        assert_eq!(
            read_control_line_or_host_failure(&mut reader, process.child_mut(), "state ECHO1"),
            "ECHO1"
        );
        assert_eq!(
            read_control_line_or_host_failure(&mut reader, process.child_mut(), "state ECHO2"),
            "ECHO2"
        );

        Self {
            process,
            reader,
            bun_pid,
            deadline,
            case_name: case.name.to_owned(),
            _stage: stage,
        }
    }

    fn finish(mut self) {
        let host_pid = self.process.host_pid();
        let _window = wait_for_host_window(host_pid, self.deadline);
        self.reader
            .get_mut()
            .write_all(b"QUIT\n")
            .expect("state host Quit");
        self.reader
            .get_mut()
            .flush()
            .expect("flush state host Quit");
        assert_eq!(read_control_line(&mut self.reader), "QUIT_REPLY");
        assert_eq!(read_control_line(&mut self.reader), "LINK_EOF");
        let status = self.process.wait(self.deadline);
        assert!(
            status.success(),
            "{} host exited with {status}",
            self.case_name
        );
        assert!(
            !process_exists(self.bun_pid),
            "{} Bun survived exit",
            self.case_name
        );
    }
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

fn profile_test_user_sid() -> String {
    let identifiers = profile_test_token_sids("/user");
    assert_eq!(identifiers.len(), 1, "whoami must report one user SID");
    identifiers
        .into_iter()
        .next()
        .expect("one Windows user SID")
}

fn profile_test_token_sids(kind: &str) -> Vec<String> {
    let output = Command::new("whoami.exe")
        .args([kind, "/fo", "csv", "/nh"])
        .output()
        .expect("observe the fixture process's actual Windows user SID");
    assert!(output.status.success(), "whoami user observation failed");
    // The SID is ASCII even when the account-name column uses the local code page.
    let text = String::from_utf8_lossy(&output.stdout);
    text.split(|character: char| character == ',' || character == '"' || character.is_whitespace())
        .filter(|field| field.starts_with("S-1-"))
        .map(str::to_owned)
        .collect()
}

#[test]
#[ignore = "requires an operator-authenticated second ordinary user and shared signed fixtures"]
fn kel135_signed_host_cross_user_storage_isolation() {
    let host = env::var_os("KELD_KEL135_SIGNED_HOST_A_P1").expect("signed A/P1 host");
    let identity =
        env::var_os("KELD_KEL135_SIGNED_IDENTITY_A_P1").expect("signed A/P1 identity fixture");
    let shared = env::var_os("KELD_KEL135_SHARED_DIRECTORY")
        .expect("owned directory readable by the second user");
    let namespace = signed_fixture_profile_namespace(&identity, Some(&host));
    let first_sid = profile_test_user_sid();
    let fixture = ProductFixture::new();
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("first-user control");
    let server = ProfileStateServer::new();
    let nonce = profile_state_run_nonce(&fixture);
    let first_value = format!("{nonce}-u1");
    let second_value = format!("{nonce}-u2");
    let seed = SignedProfileStateCase {
        name: "u1-seed",
        host: &host,
        before: "",
        after: &first_value,
    };
    run_signed_profile_state_case(&fixture, &control, &server, &nonce, &seed);
    let coordinator = TcpListener::bind(("127.0.0.1", 0)).expect("cross-user coordinator");
    let mut request = tempfile::Builder::new()
        .prefix("kel135-user2-")
        .suffix(".json")
        .tempfile_in(shared)
        .expect("unique cross-user request file");
    serde_json::to_writer(
        request.as_file_mut(),
        &serde_json::json!({
            "coordinator": coordinator.local_addr().expect("coordinator address").to_string(),
            "server": server.address().to_string(), "nonce": nonce,
            "host": Path::new(&host), "identity": Path::new(&identity),
            "controller": env::current_exe().expect("current acceptance controller"),
        }),
    )
    .expect("write cross-user request");
    request
        .as_file_mut()
        .flush()
        .expect("publish complete request");
    println!(
        "KELD_KEL135_SECOND_USER_REQUEST {}",
        request.path().display()
    );
    std::io::stdout()
        .flush()
        .expect("publish operator action before waiting");
    let stream = accept_control_until(&coordinator, None, Instant::now() + Duration::from_mins(10));
    let mut peer = BufReader::new(stream);
    let greeting = read_profile_coordinator(&mut peer);
    let second_sid =
        validate_profile_second_user(&greeting, &nonce, &namespace, &first_sid, server.address())
            .expect("second user must be distinct, ordinary and bound to the same app/origin");
    println!(
        "KELD_KEL135_CROSS_USER {}",
        serde_json::json!({
            "first_sid": first_sid, "second_sid": second_sid,
            "namespace": namespace, "origin": format!("http://{}", server.address()),
        })
    );
    for (name, before, after) in [
        ("u2-isolated", "", second_value.as_str()),
        ("u2-restart", second_value.as_str(), second_value.as_str()),
        ("u2-cleanup", second_value.as_str(), ""),
    ] {
        let case = SignedProfileStateCase {
            name,
            host: &host,
            before,
            after,
        };
        run_remote_profile_state_case(&mut peer, &server, &nonce, &case);
    }
    write_profile_coordinator(peer.get_mut(), &serde_json::json!({"kind": "stop"}));
    assert_eq!(read_profile_coordinator(&mut peer)["kind"], "stopped");
    for (name, before, after) in [
        ("u1-after-u2", first_value.as_str(), first_value.as_str()),
        ("u1-cleanup", first_value.as_str(), ""),
    ] {
        let case = SignedProfileStateCase {
            name,
            host: &host,
            before,
            after,
        };
        run_signed_profile_state_case(&fixture, &control, &server, &nonce, &case);
    }
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

fn profile_request_address(request: &Value, field: &str) -> SocketAddr {
    let address: SocketAddr = request[field]
        .as_str()
        .expect("loopback address field")
        .parse()
        .expect("socket address");
    assert!(
        address.ip().is_loopback() && address.port() != 0,
        "fixture only contacts a live loopback endpoint"
    );
    address
}

fn validate_profile_second_user(
    greeting: &Value,
    nonce: &str,
    namespace: &str,
    first_sid: &str,
    address: SocketAddr,
) -> Result<String, String> {
    if greeting["kind"] != "hello"
        || greeting["nonce"] != nonce
        || greeting["namespace"] != namespace
        || greeting["server"] != address.to_string()
    {
        return Err("second-user context does not match the live request".to_owned());
    }
    let sid = greeting["user_sid"]
        .as_str()
        .ok_or("second-user SID is missing")?;
    if sid == first_sid || !sid.starts_with("S-1-") || greeting["administrator"] != false {
        return Err("second-user context is not a distinct ordinary user".to_owned());
    }
    Ok(sid.to_owned())
}

fn run_remote_profile_state_case(
    peer: &mut BufReader<TcpStream>,
    server: &ProfileStateServer,
    nonce: &str,
    case: &SignedProfileStateCase<'_>,
) {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    server.expect_case(case.name, deadline);
    write_profile_coordinator(
        peer.get_mut(),
        &serde_json::json!({
            "kind": "run", "case": case.name, "before": case.before, "after": case.after,
        }),
    );
    let ready = read_profile_coordinator(peer);
    assert_eq!(ready["kind"], "ready");
    assert_eq!(ready["case"], case.name);
    let pid = |field: &str| {
        u32::try_from(ready[field].as_u64().expect("observed native PID")).expect("PID width")
    };
    let observation = server.wait_for_case(case.name, deadline);
    record_profile_state_case(
        &observation,
        case,
        nonce,
        server.address(),
        pid("host_pid"),
        pid("bun_pid"),
    );
    write_profile_coordinator(peer.get_mut(), &serde_json::json!({"kind": "finish"}));
    let finished = read_profile_coordinator(peer);
    assert_eq!(finished["kind"], "finished");
    assert_eq!(finished["case"], case.name);
}

fn read_profile_coordinator(reader: &mut BufReader<TcpStream>) -> Value {
    serde_json::from_str(&read_control_line(reader)).expect("bounded coordinator JSON")
}

fn write_profile_coordinator(stream: &mut TcpStream, message: &Value) {
    let mut bytes = serde_json::to_vec(message).expect("coordinator JSON");
    bytes.push(b'\n');
    assert!(bytes.len() <= CONTROL_LINE_LIMIT);
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    let mut pending = bytes.as_slice();
    while !pending.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("coordinator write deadline");
        stream
            .set_write_timeout(Some(remaining))
            .expect("coordinator write timeout");
        let written = stream.write(pending).expect("write coordinator message");
        assert_ne!(written, 0, "coordinator stopped accepting bytes");
        pending = &pending[written..];
    }
}

#[test]
fn profile_second_user_requires_distinct_ordinary_context_and_the_same_origin() {
    let address: SocketAddr = "127.0.0.1:12345".parse().expect("fixture address");
    let good = serde_json::json!({
        "kind": "hello", "nonce": "run1", "namespace": "namespace-a",
        "user_sid": "S-1-5-21-2000", "administrator": false, "server": "127.0.0.1:12345",
    });
    assert_eq!(
        validate_profile_second_user(&good, "run1", "namespace-a", "S-1-5-21-1000", address)
            .expect("different ordinary user at the same origin"),
        "S-1-5-21-2000"
    );
    for (field, value) in [
        ("user_sid", Value::String("S-1-5-21-1000".to_owned())),
        ("administrator", Value::Bool(true)),
        ("administrator", Value::Null),
        ("namespace", Value::String("namespace-b".to_owned())),
        ("nonce", Value::String("old-run".to_owned())),
        ("server", Value::String("127.0.0.1:12346".to_owned())),
    ] {
        let mut wrong = good.clone();
        wrong[field] = value;
        assert!(
            validate_profile_second_user(&wrong, "run1", "namespace-a", "S-1-5-21-1000", address)
                .is_err(),
            "{field} mismatch cannot count as cross-user acceptance"
        );
    }
}

fn state_redirect_html(address: SocketAddr, case_name: &str, nonce: &str, value: &str) -> String {
    format!(
        "<!doctype html>{DARK_BG}<script>location.replace('http://127.0.0.1:{}/app?case={}&nonce={}&value={}')</script>\n",
        address.port(),
        case_name,
        nonce,
        value
    )
}

struct ProfileStateServer {
    address: SocketAddr,
    observations: mpsc::Receiver<Result<ProfileStateObservation, String>>,
    commands: mpsc::Sender<ProfileStateCommand>,
    worker: Option<thread::JoinHandle<()>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProfileStateObservation {
    case_name: String,
    nonce: String,
    before: ProfileStorageState,
    after: ProfileStorageState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProfileStorageState {
    cookie: String,
    local_storage: String,
    indexed_db: String,
    cache_storage: String,
    service_worker: String,
}

impl ProfileStorageState {
    fn from_fields(fields: &[(&str, &str)], phase: &str) -> Result<Self, String> {
        let value = |prefix: &str| {
            let name = if prefix.is_empty() {
                phase.to_owned()
            } else {
                format!("{prefix}_{phase}")
            };
            required_profile_state_field(fields, &name).map(str::to_owned)
        };
        Ok(Self {
            cookie: value("cookie")?,
            local_storage: value("")?,
            indexed_db: value("indexed_db")?,
            cache_storage: value("cache")?,
            service_worker: value("worker")?,
        })
    }

    fn values(&self) -> [(&str, &str); 5] {
        [
            ("cookie", &self.cookie),
            ("localStorage", &self.local_storage),
            ("IndexedDB", &self.indexed_db),
            ("CacheStorage", &self.cache_storage),
            ("serviceWorker", &self.service_worker),
        ]
    }

    fn assert_value(&self, expected: &str, case: &str, phase: &str) {
        for (store, value) in self.values() {
            assert_eq!(value, expected, "{case} {store} {phase}");
        }
    }

    fn is_uniform(&self, expected: &str) -> bool {
        self.values().iter().all(|(_, value)| *value == expected)
    }

    fn json(&self) -> Value {
        Value::Object(
            self.values()
                .into_iter()
                .map(|(store, value)| (store.to_owned(), Value::String(value.to_owned())))
                .collect(),
        )
    }
}

enum ProfileStateCommand {
    Expect {
        case_name: String,
        deadline: Instant,
    },
    Stop,
}

impl ProfileStateServer {
    fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind profile state server");
        let address = listener.local_addr().expect("profile state server address");
        listener
            .set_nonblocking(true)
            .expect("nonblocking profile state server");
        let (observed_tx, observations) = mpsc::channel();
        let (commands, command_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            if let Err(error) = serve_profile_state_loop(&listener, &command_rx, &observed_tx) {
                let _ = observed_tx.send(Err(error));
            }
        });
        Self {
            address,
            observations,
            commands,
            worker: Some(worker),
        }
    }

    fn address(&self) -> SocketAddr {
        self.address
    }

    fn expect_case(&self, case_name: &str, deadline: Instant) {
        self.commands
            .send(ProfileStateCommand::Expect {
                case_name: case_name.to_owned(),
                deadline,
            })
            .expect("arm profile state case");
    }

    fn wait_for_case(&self, expected: &str, deadline: Instant) -> ProfileStateObservation {
        let remaining = renderer_beacon_remaining(deadline, Instant::now())
            .expect("profile state deadline remains");
        let observation = self
            .observations
            .recv_timeout(remaining)
            .expect("profile state report")
            .unwrap_or_else(|error| panic!("profile state server failed: {error}"));
        assert_eq!(observation.case_name, expected, "profile state case order");
        observation
    }
}

impl Drop for ProfileStateServer {
    fn drop(&mut self) {
        let _ = self.commands.send(ProfileStateCommand::Stop);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_profile_state_loop(
    listener: &TcpListener,
    commands: &mpsc::Receiver<ProfileStateCommand>,
    observations: &mpsc::Sender<Result<ProfileStateObservation, String>>,
) -> Result<(), String> {
    let mut pending = Vec::<PendingRendererRequest>::new();
    let mut expected: Option<(String, Instant)> = None;
    let mut last_observation = None;
    loop {
        if expected.is_none() {
            match commands.recv_timeout(RENDERER_ACCEPT_POLL) {
                Ok(ProfileStateCommand::Expect {
                    case_name,
                    deadline,
                }) => expected = Some((case_name, deadline)),
                Ok(ProfileStateCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
        match commands.try_recv() {
            Ok(ProfileStateCommand::Stop) | Err(mpsc::TryRecvError::Disconnected) => {
                return Ok(());
            }
            Ok(ProfileStateCommand::Expect { case_name, .. }) => {
                return Err(format!(
                    "profile state case `{case_name}` armed before the prior case completed"
                ));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let (expected_case, deadline) = expected.as_ref().expect("armed profile state case");
        let remaining = renderer_beacon_remaining(*deadline, Instant::now())?;

        let mut index = 0;
        while index < pending.len() {
            let request = {
                let PendingRendererRequest { stream, request } = &mut pending[index];
                read_renderer_request_line(request, |buffer| stream.read(buffer))
            }?;
            match request {
                RendererRequestRead::Pending => index += 1,
                RendererRequestRead::Empty => {
                    pending.swap_remove(index);
                }
                RendererRequestRead::Complete(request) => {
                    let mut matched = pending.swap_remove(index);
                    let path = renderer_request_path(&request)?;
                    let response =
                        profile_state_response(path, expected_case, last_observation.as_ref())?;
                    matched.stream.set_nonblocking(false).map_err(|error| {
                        format!("set profile state reply stream blocking: {error}")
                    })?;
                    matched
                        .stream
                        .set_write_timeout(Some(remaining))
                        .map_err(|error| format!("set profile state reply deadline: {error}"))?;
                    write_profile_state_response(&mut matched.stream, &response)?;
                    if let Some(observation) = response.observation {
                        last_observation = Some(observation.clone());
                        observations
                            .send(Ok(observation))
                            .map_err(|_| "profile state observation owner ended".to_owned())?;
                        expected = None;
                        break;
                    }
                }
            }
        }

        if accept_renderer_connection(listener, &mut pending, "profile state server")? {
            continue;
        }

        thread::park_timeout(remaining.min(RENDERER_ACCEPT_POLL));
    }
}

struct ProfileStateResponse {
    status: &'static str,
    content_type: &'static str,
    body: String,
    observation: Option<ProfileStateObservation>,
}

fn renderer_request_path(request: &[u8]) -> Result<&str, String> {
    let request = std::str::from_utf8(request)
        .map_err(|error| format!("profile state request line is not UTF-8: {error}"))?;
    let mut fields = request.split_whitespace();
    if fields.next() != Some("GET") {
        return Err(format!("profile state request is not GET: {request}"));
    }
    let path = fields
        .next()
        .ok_or_else(|| format!("profile state request has no path: {request}"))?;
    if fields.next() != Some("HTTP/1.1") || fields.next().is_some() {
        return Err(format!("profile state request shape is invalid: {request}"));
    }
    Ok(path)
}

fn write_profile_state_response(
    stream: &mut TcpStream,
    response: &ProfileStateResponse,
) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|()| stream.write_all(response.body.as_bytes()))
        .map_err(|error| format!("write profile state response: {error}"))
}

fn profile_state_response(
    path: &str,
    expected_case: &str,
    last_observation: Option<&ProfileStateObservation>,
) -> Result<ProfileStateResponse, String> {
    if path == "/favicon.ico" {
        return Ok(ProfileStateResponse {
            status: "204 No Content",
            content_type: "text/plain",
            body: String::new(),
            observation: None,
        });
    }
    if let Some(query) = path.strip_prefix("/state?") {
        let fields = profile_state_query(query)?;
        let case_name = required_profile_state_field(&fields, "case")?;
        if let Some((_, error)) = fields.iter().find(|(key, _)| *key == "error") {
            return Err(format!(
                "browser storage case `{case_name}` failed: {error}"
            ));
        }
        if let Some((_, progress)) = fields.iter().find(|(key, _)| *key == "progress") {
            if case_name != expected_case {
                return Err("browser storage progress belongs to the wrong case".to_owned());
            }
            eprintln!("KELD_KEL135_STORAGE_PROGRESS case={case_name} phase={progress}");
            return Ok(empty_profile_state_response());
        }
        let observation = ProfileStateObservation {
            case_name: case_name.to_owned(),
            nonce: required_profile_state_field(&fields, "nonce")?.to_owned(),
            before: ProfileStorageState::from_fields(&fields, "before")?,
            after: ProfileStorageState::from_fields(&fields, "after")?,
        };
        if case_name != expected_case {
            if last_observation == Some(&observation) {
                return Ok(empty_profile_state_response());
            }
            return Err(format!(
                "profile state report case `{case_name}` did not match `{expected_case}`"
            ));
        }
        return Ok(ProfileStateResponse {
            status: "204 No Content",
            content_type: "text/plain",
            body: String::new(),
            observation: Some(observation),
        });
    }
    if let Some(query) = path.strip_prefix("/profile-worker.js?") {
        let fields = profile_state_query(query)?;
        let nonce = required_profile_state_field(&fields, "nonce")?;
        let value = required_profile_state_field(&fields, "value")?;
        validate_profile_state_atom(nonce, false)?;
        validate_profile_state_atom(value, false)?;
        let value = serde_json::to_string(value).expect("serialize fixture worker nonce");
        return Ok(ProfileStateResponse {
            status: "200 OK",
            content_type: "application/javascript",
            body: format!(
                "const nonce={value};self.addEventListener('install',e=>e.waitUntil(self.skipWaiting()));self.addEventListener('message',e=>{{if(e.data==='read-nonce'&&e.ports[0]){{e.ports[0].postMessage(nonce);e.ports[0].close();}}}});"
            ),
            observation: None,
        });
    }
    let Some(query) = path.strip_prefix("/app?") else {
        return Ok(ProfileStateResponse {
            status: "404 Not Found",
            content_type: "text/plain",
            body: String::new(),
            observation: None,
        });
    };
    let fields = profile_state_query(query)?;
    let case_name = required_profile_state_field(&fields, "case")?;
    let nonce = required_profile_state_field(&fields, "nonce")?;
    let value = required_profile_state_field(&fields, "value")?;
    validate_profile_state_atom(case_name, false)?;
    validate_profile_state_atom(nonce, false)?;
    validate_profile_state_atom(value, true)?;
    if case_name != expected_case {
        if last_observation.is_some_and(|prior| {
            prior.case_name == case_name && prior.nonce == nonce && prior.after.is_uniform(value)
        }) {
            return Ok(empty_profile_state_response());
        }
        return Err(format!(
            "profile state app case `{case_name}` did not match `{expected_case}`"
        ));
    }
    let config = serde_json::json!({ "caseName": case_name, "nonce": nonce, "value": value });
    let browser = include_str!("fixtures/profile_state.js");
    let body = format!(
        "<!doctype html>{DARK_BG}<script>globalThis.keldProfileState={config};\n{browser}</script>"
    );
    Ok(ProfileStateResponse {
        status: "200 OK",
        content_type: "text/html; charset=utf-8",
        body,
        observation: None,
    })
}

fn validate_profile_state_atom(value: &str, empty_allowed: bool) -> Result<(), String> {
    if (value.is_empty() && !empty_allowed)
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("profile state fixture identifier is invalid".to_owned());
    }
    Ok(())
}

fn empty_profile_state_response() -> ProfileStateResponse {
    ProfileStateResponse {
        status: "204 No Content",
        content_type: "text/plain",
        body: String::new(),
        observation: None,
    }
}

#[test]
fn profile_state_report_requires_the_complete_storage_census() {
    let fields = [
        "case=a-seed",
        "nonce=run1",
        "before=",
        "after=value-a",
        "cookie_before=",
        "cookie_after=value-a",
        "indexed_db_before=",
        "indexed_db_after=value-a",
        "cache_before=",
        "cache_after=value-a",
        "worker_before=",
        "worker_after=value-a",
    ];
    let complete = format!("/state?{}", fields.join("&"));
    assert!(profile_state_response(&complete, "a-seed", None).is_ok());
    for omitted in 2..fields.len() {
        let query = fields
            .iter()
            .enumerate()
            .filter_map(|(index, field)| (index != omitted).then_some(*field))
            .collect::<Vec<_>>()
            .join("&");
        assert!(
            profile_state_response(&format!("/state?{query}"), "a-seed", None).is_err(),
            "missing storage observation must fail: {}",
            fields[omitted]
        );
    }
}

#[test]
fn profile_state_browser_failure_cannot_become_an_empty_success() {
    let result = profile_state_response(
        "/state?case=a-seed&nonce=run1&error=indexedDB-write%3AAbortError",
        "a-seed",
        None,
    );
    assert_eq!(
        result.err().as_deref(),
        Some("browser storage case `a-seed` failed: indexedDB-write%3AAbortError")
    );
}

#[test]
fn profile_state_report_retains_each_store_observation() {
    let response = profile_state_response(
        "/state?case=a-restart&nonce=run1&before=local-a&after=local-b&cookie_before=cookie-a&cookie_after=cookie-b&indexed_db_before=idb-a&indexed_db_after=idb-b&cache_before=cache-a&cache_after=cache-b&worker_before=worker-a&worker_after=worker-b",
        "a-restart",
        None,
    )
    .expect("complete storage observation");
    let observed = response
        .observation
        .expect("semantic observation, not a resource reply");
    assert_eq!(
        observed.before.values(),
        [
            ("cookie", "cookie-a"),
            ("localStorage", "local-a"),
            ("IndexedDB", "idb-a"),
            ("CacheStorage", "cache-a"),
            ("serviceWorker", "worker-a"),
        ]
    );
    assert_eq!(
        observed.after.values(),
        [
            ("cookie", "cookie-b"),
            ("localStorage", "local-b"),
            ("IndexedDB", "idb-b"),
            ("CacheStorage", "cache-b"),
            ("serviceWorker", "worker-b"),
        ]
    );
}

fn profile_state_query(query: &str) -> Result<Vec<(&str, &str)>, String> {
    let mut fields = Vec::new();
    for pair in query.split('&') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("profile state query pair is malformed: {pair}"))?;
        if fields.iter().any(|(found, _)| *found == key) {
            return Err(format!("profile state query duplicates `{key}`"));
        }
        fields.push((key, value));
    }
    Ok(fields)
}

fn required_profile_state_field<'a>(
    fields: &'a [(&str, &str)],
    expected: &str,
) -> Result<&'a str, String> {
    fields
        .iter()
        .find_map(|(key, value)| (*key == expected).then_some(*value))
        .ok_or_else(|| format!("profile state query omits `{expected}`"))
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
