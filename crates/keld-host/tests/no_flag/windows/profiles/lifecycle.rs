//! Signed-profile concurrency and running-crash lifecycle acceptance on an installed
//! package (KEL-254 T3 Part B restores these rows on installed provenance).

use crate::support::control::{accept_ready_generation, read_control_line};
use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::process::{
    assert_process_signaled, open_process_for_wait, process_exists, terminate_test_process,
    wait_child, wait_for_process_signal,
};
use crate::support::product::ProductFixture;
use crate::support::profile_run::{
    ProfileTestRoot, SignedProfileStateCase, SignedProfileStateRun, actual_webview_process,
    record_profile_state_case, user_data_folder,
};
use crate::support::profile_server::ProfileStateServer;
use crate::support::signed_identity::signed_fixture_profile_namespace;
use crate::support::signed_process::SignedStateProcessGuard;
use crate::support::window::wait_for_host_window;
use crate::{PRODUCT_DEADLINE, startup_kill_switch, ui_end_kill_switch};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Stdio;
use std::time::Instant;
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;

/// One fresh installation of the signed A/P1 host with the default product renderer.
fn installed_product() -> (ProductFixture, InstalledApp) {
    let fixture = ProductFixture::new();
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
    );
    (fixture, installed)
}

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1 and KELD_KEL254_INSTALLER_FIXTURE (KEL-254 installed-host operator fixtures)"]
fn kel135_signed_host_profile_concurrency() {
    let (_fixture, installed) = installed_product();
    let first_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind first control");
    let first_port = first_listener
        .local_addr()
        .expect("first control address")
        .port();
    let first_child = installed
        .command()
        .env("KELD_T1B_CONTROL", first_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch first signed host without KELD_DEV_LEASE");
    let mut first = SignedStateProcessGuard::new(first_child);
    let (mut reader, mut writer, bun_pid, _) =
        accept_ready_generation(&first_listener, first.child_mut());
    first.observe_bun(bun_pid);
    let _window = wait_for_host_window(first.host_pid(), Instant::now() + PRODUCT_DEADLINE);

    let second_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind second control");
    let second_port = second_listener
        .local_addr()
        .expect("second control address")
        .port();
    let mut second = installed
        .command()
        .env("KELD_T1B_CONTROL", second_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch competing signed host without KELD_DEV_LEASE");
    let second_pid = second.id();
    let status = wait_child(&mut second, Instant::now() + PRODUCT_DEADLINE);
    let mut stdout = String::new();
    let mut stderr = String::new();
    second
        .stdout
        .take()
        .expect("captured competing host stdout")
        .read_to_string(&mut stdout)
        .expect("read competing host stdout");
    second
        .stderr
        .take()
        .expect("captured competing host stderr")
        .read_to_string(&mut stderr)
        .expect("read competing host stderr");
    assert!(!status.success(), "competing host unexpectedly succeeded");
    assert!(
        stderr.contains("KELD-WV-009") && stderr.contains("already in use"),
        "competing host must fail with profile-in-use, status={status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !process_exists(second_pid),
        "competing host survived its profile-in-use rejection"
    );

    writer.write_all(b"QUIT\n").expect("first signed host Quit");
    writer.flush().expect("flush first signed host Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = first.wait(Instant::now() + ui_end_kill_switch());
    assert!(status.success(), "first signed host exited with {status}");
}

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1 and KELD_KEL254_INSTALLER_FIXTURE (KEL-254 installed-host operator fixtures)"]
fn kel135_signed_host_running_crash_releases_profile() {
    let (_fixture, installed) = installed_product();
    let first_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind crashing control");
    let first_port = first_listener
        .local_addr()
        .expect("crashing control address")
        .port();
    let first_child = installed
        .command()
        .env("KELD_T1B_CONTROL", first_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch crashing signed host without KELD_DEV_LEASE");
    let mut first = SignedStateProcessGuard::new(first_child);
    let (_reader, _writer, bun_pid, _) =
        accept_ready_generation(&first_listener, first.child_mut());
    first.observe_bun(bun_pid);
    let host = open_process_for_wait(first.host_pid(), true);
    let _window = wait_for_host_window(first.host_pid(), Instant::now() + PRODUCT_DEADLINE);
    terminate_test_process(&host);
    assert_process_signaled(&host, "signed running host");
    let status = first.wait(Instant::now() + PRODUCT_DEADLINE);
    assert!(
        !status.success(),
        "terminated signed host unexpectedly succeeded"
    );

    let second_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind recovered control");
    let second_port = second_listener
        .local_addr()
        .expect("recovered control address")
        .port();
    let second_child = installed
        .command()
        .env("KELD_T1B_CONTROL", second_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch recovered signed host without KELD_DEV_LEASE");
    let mut second = SignedStateProcessGuard::new(second_child);
    let (mut reader, mut writer, bun_pid, _) =
        accept_ready_generation(&second_listener, second.child_mut());
    second.observe_bun(bun_pid);
    let _window = wait_for_host_window(second.host_pid(), Instant::now() + PRODUCT_DEADLINE);
    writer
        .write_all(b"QUIT\n")
        .expect("recovered signed host Quit");
    writer.flush().expect("flush recovered signed host Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = second.wait(Instant::now() + ui_end_kill_switch());
    assert!(
        status.success(),
        "recovered signed host exited with {status}"
    );
}

#[test]
#[ignore = "requires signed A/P1 and B/P1 KEL-254 hosts, matching KEL-135 identity fixtures, profile-test-root feature, installer fixture, and Windows WebView2"]
fn kel135_signed_profiles_run_concurrently_and_survive_other_app_close() {
    let context = ConcurrentProfileContext::new();
    let run_a = start_case(&context, &context.fixture_a, &context.case_a());
    let host_a = run_a.installed_host().to_path_buf();
    let window_a = wait_for_host_window(run_a.process.host_pid(), deadline());
    let app_a = actual_webview_process(
        run_a.process.host_pid(),
        &host_a,
        &context.udf_a,
        deadline(),
    );
    let first_webview_pid = u32::try_from(app_a["browser_pid"].as_u64().expect("browser PID"))
        .expect("browser PID fits Windows");
    let first_webview_process = open_process_for_wait(first_webview_pid, false);

    let run_b = start_case(&context, &context.fixture_b, &context.case_b());
    let host_b = run_b.installed_host().to_path_buf();
    let window_b = wait_for_host_window(run_b.process.host_pid(), deadline());
    let app_b = actual_webview_process(
        run_b.process.host_pid(),
        &host_b,
        &context.udf_b,
        deadline(),
    );
    let second_webview_pid = u32::try_from(app_b["browser_pid"].as_u64().expect("browser PID"))
        .expect("browser PID fits Windows");
    let second_webview_process = open_process_for_wait(second_webview_pid, false);

    assert_ne!(
        window_a["handle"], window_b["handle"],
        "apps own distinct windows"
    );
    assert!(process_exists(run_a.process.host_pid()) && process_exists(run_b.process.host_pid()));
    assert_eq!(
        wait_for_process_signal(&first_webview_process, 0),
        WAIT_TIMEOUT,
        "App A WebView2 must still be live while App B is live"
    );
    assert_eq!(
        wait_for_process_signal(&second_webview_process, 0),
        WAIT_TIMEOUT,
        "App B WebView2 must still be live while App A is live"
    );
    println!(
        "KELD_KEL135_CONCURRENT_PROFILE {}",
        serde_json::json!({
            "origin": format!("http://{}", context.server.address()),
            "namespace_a": context.namespace_a,
            "namespace_b": context.namespace_b,
            "app_a": app_a,
            "app_b": app_b,
            "host_pids": [run_a.process.host_pid(), run_b.process.host_pid()],
            "window_handles": [window_a["handle"], window_b["handle"]],
            "same_origin": true,
            "simultaneous": true,
        })
    );

    // A remains live through B's independent setup and the real browser release wait.
    run_a.finish();
    assert_process_signaled(&first_webview_process, "App A WebView2");
    assert!(
        process_exists(run_b.process.host_pid()),
        "closing A ended App B host"
    );
    assert_eq!(
        wait_for_process_signal(&second_webview_process, 0),
        WAIT_TIMEOUT,
        "closing A ended App B WebView2"
    );
    let survivor = actual_webview_process(
        run_b.process.host_pid(),
        &host_b,
        &context.udf_b,
        deadline(),
    );
    println!(
        "KELD_KEL135_PROFILE_SURVIVOR {}",
        serde_json::json!({"closed_host": "A", "host_alive": true, "webview2_alive": true, "process": survivor})
    );
    run_b.finish();
    assert_process_signaled(&second_webview_process, "App B WebView2");

    start_case(&context, &context.fixture_b, &context.restart_case()).finish();
    start_case(&context, &context.fixture_a, &context.restart_case_a()).finish();
    assert!(
        context.udf_a.is_dir() && context.udf_b.is_dir(),
        "both app profiles persist independently"
    );
    context.root.remove();
}

struct ConcurrentProfileContext {
    host_a: OsString,
    host_b: OsString,
    installer: OsString,
    namespace_a: String,
    namespace_b: String,
    fixture_a: ProductFixture,
    fixture_b: ProductFixture,
    root: ProfileTestRoot,
    server: ProfileStateServer,
    control: TcpListener,
    nonce: String,
    value_a: String,
    value_b: String,
    udf_a: std::path::PathBuf,
    udf_b: std::path::PathBuf,
}

impl ConcurrentProfileContext {
    fn new() -> Self {
        let host_a = fixture_env(SIGNED_HOST_A_P1);
        let host_b = fixture_env("KELD_KEL254_SIGNED_HOST_B_P1");
        let identity_a = fixture_env("KELD_KEL135_SIGNED_IDENTITY_A_P1");
        let identity_b = fixture_env("KELD_KEL135_SIGNED_IDENTITY_B_P1");
        let installer = fixture_env(INSTALLER_ENV);
        let namespace_a = signed_fixture_profile_namespace(&identity_a, Some(&host_a));
        let namespace_b = signed_fixture_profile_namespace(&identity_b, Some(&host_b));
        assert_ne!(
            namespace_a, namespace_b,
            "signed apps require distinct profiles"
        );
        let root = ProfileTestRoot::new();
        let udf_a = user_data_folder(root.path(), &namespace_a);
        let udf_b = user_data_folder(root.path(), &namespace_b);
        assert_ne!(udf_a, udf_b);
        let nonce = format!("{}-concurrent", std::process::id());
        Self {
            host_a,
            host_b,
            installer,
            namespace_a,
            namespace_b,
            fixture_a: ProductFixture::new(),
            fixture_b: ProductFixture::new(),
            root,
            server: ProfileStateServer::new(),
            control: TcpListener::bind(("127.0.0.1", 0)).expect("bind concurrent control"),
            value_a: format!("{nonce}-a"),
            value_b: format!("{nonce}-b"),
            nonce,
            udf_a,
            udf_b,
        }
    }

    fn case_a(&self) -> SignedProfileStateCase<'_> {
        SignedProfileStateCase {
            name: "concurrent-a-seed",
            host: &self.host_a,
            installer: &self.installer,
            before: "",
            after: &self.value_a,
        }
    }

    fn case_b(&self) -> SignedProfileStateCase<'_> {
        SignedProfileStateCase {
            name: "concurrent-b-isolated",
            host: &self.host_b,
            installer: &self.installer,
            before: "",
            after: &self.value_b,
        }
    }

    fn restart_case(&self) -> SignedProfileStateCase<'_> {
        SignedProfileStateCase {
            name: "concurrent-b-restart",
            host: &self.host_b,
            installer: &self.installer,
            before: &self.value_b,
            after: &self.value_b,
        }
    }

    fn restart_case_a(&self) -> SignedProfileStateCase<'_> {
        SignedProfileStateCase {
            name: "concurrent-a-restart",
            host: &self.host_a,
            installer: &self.installer,
            before: &self.value_a,
            after: &self.value_a,
        }
    }
}

fn start_case(
    context: &ConcurrentProfileContext,
    fixture: &ProductFixture,
    case: &SignedProfileStateCase<'_>,
) -> SignedProfileStateRun {
    // Launch through the renderer's state report spans the host's WebView2 startup.
    let deadline = Instant::now() + startup_kill_switch();
    context.server.expect_case(case.name, deadline);
    let run = SignedProfileStateRun::start_with_profile_root(
        fixture,
        &context.control,
        context.server.address(),
        &context.nonce,
        case,
        deadline,
        Some(context.root.path()),
    );
    let observed = context.server.wait_for_case(case.name, deadline);
    if case.name == "concurrent-b-isolated" {
        observed.before.assert_value("", case.name, "before");
    }
    record_profile_state_case(
        &observed,
        case,
        &context.nonce,
        context.server.address(),
        run.process.host_pid(),
        run.bun_pid,
    );
    run
}

fn deadline() -> Instant {
    Instant::now() + PRODUCT_DEADLINE
}
