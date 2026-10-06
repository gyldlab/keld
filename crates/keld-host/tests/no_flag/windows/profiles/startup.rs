//! Signed persistent-profile startup on an installed package, and the lease-less
//! staged-host refusal.

use crate::support::control::{accept_ready_generation, read_control_line};
use crate::support::installed::{
    INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env, open_for_delete,
};
use crate::support::process::{process_exists, wait_child};
use crate::support::product::ProductFixture;
use crate::support::renderer::{expect_renderer_beacon, spawn_renderer_beacon};
use crate::support::window::wait_for_host_window;
use crate::{DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE};
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;
use std::{env, fs};
use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;

/// KEL-254 AC2/AC11: signature success alone never admits installed mode. The
/// `KELD_KEL135_SIGNED_HOST` carrier is a KEL-135 operator-signed host without an
/// embedded `ExpectedAppIdentity` container, staged outside any installation.
#[test]
#[ignore = "requires a signed KEL-135 Windows host fixture without an expected-identity container"]
fn kel135_signed_lease_less_stage_is_refused_before_resources() {
    let signed_host = env::var_os("KELD_KEL135_SIGNED_HOST")
        .expect("KELD_KEL135_SIGNED_HOST must point to a signed keld-host.exe");
    let fixture = ProductFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(&signed_host))
        .expect("stage the signed KEL-135 host");
    let output = Command::new(stage.host())
        .current_dir(stage.root())
        .env_remove("KELD_DEV_LEASE")
        .output()
        .expect("launch signed host without KELD_DEV_LEASE");
    assert!(!output.status.success(), "a lease-less signed stage booted");
    let stderr = String::from_utf8(output.stderr).expect("signed refusal stderr UTF-8");
    // Verification passed on the pinned image, so the installed route read that handle
    // and found no embedded expectation; the dev-stage validator was never consulted.
    assert!(stderr.contains("KELD-UPDATE-019"), "{stderr}");
    assert!(stderr.contains("KELD-PACK-007"), "{stderr}");
    assert!(!stderr.contains("KELD-WV-009"), "{stderr}");
    assert!(stderr.contains("listener=0 child=0 window=0"), "{stderr}");
}

/// The installed host boots lease-less into its persistent profile and renderer
/// (KEL-254 T3 Part B restores this row on installed provenance).
#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1 and KELD_KEL254_INSTALLER_FIXTURE (KEL-254 installed-host operator fixtures)"]
fn kel135_signed_host_persistent_profile_startup() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind signed control");
    let control_port = control_listener
        .local_addr()
        .expect("signed control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind signed beacon");
    let beacon_port = beacon_listener
        .local_addr()
        .expect("signed beacon address")
        .port();
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write signed renderer");
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
    );
    // Positive control: before launch nothing pins the version directory, and the
    // owner-private per-user profile admits this user's DELETE open.
    drop(open_for_delete(installed.version_dir()).expect("an unpinned version directory"));
    let beacon = spawn_renderer_beacon(beacon_listener);
    let mut host = installed
        .command()
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch the installed host without KELD_DEV_LEASE");
    let host_pid = host.id();
    let (mut reader, mut writer, bun_pid, _) =
        accept_ready_generation(&control_listener, &mut host);
    expect_renderer_beacon(beacon, "installed host renderer beacon");
    let window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_eq!(window["title"], PRODUCT_TITLE);
    // The running session holds KEL-53's selection, which keeps the version directory
    // it boots from open without delete sharing until the session returns.
    let pinned = open_for_delete(installed.version_dir())
        .expect_err("the running installed session pins its version directory");
    assert_eq!(
        pinned.raw_os_error(),
        Some(i32::try_from(ERROR_SHARING_VIOLATION).expect("Win32 error fits i32")),
        "{pinned}"
    );
    writer.write_all(b"QUIT\n").expect("signed host Quit");
    writer.flush().expect("flush signed host Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = wait_child(&mut host, Instant::now() + PRODUCT_DEADLINE);
    assert!(status.success(), "signed host exited with {status}");
    assert!(
        !process_exists(bun_pid),
        "signed host Bun survived orderly exit"
    );
}
