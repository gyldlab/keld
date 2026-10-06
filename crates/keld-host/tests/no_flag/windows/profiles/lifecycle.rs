//! Signed-profile concurrency and running-crash lifecycle acceptance on an installed
//! package (KEL-254 T3 Part B restores these rows on installed provenance).

use crate::PRODUCT_DEADLINE;
use crate::support::control::{accept_ready_generation, read_control_line};
use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::process::{
    assert_process_signaled, open_process_for_wait, process_exists, terminate_test_process,
    wait_child,
};
use crate::support::product::ProductFixture;
use crate::support::signed_process::SignedStateProcessGuard;
use crate::support::window::wait_for_host_window;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Stdio;
use std::time::Instant;

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
    let status = first.wait(Instant::now() + PRODUCT_DEADLINE);
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
    let status = second.wait(Instant::now() + PRODUCT_DEADLINE);
    assert!(
        status.success(),
        "recovered signed host exited with {status}"
    );
}
