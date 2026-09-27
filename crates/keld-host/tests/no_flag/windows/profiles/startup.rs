//! Signed persistent-profile startup acceptance and its explicit fixture prerequisite.

use crate::support::control::{accept_ready_generation, read_control_line};
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

#[test]
#[ignore = "requires a signed KEL-135 Windows host fixture"]
fn kel135_signed_host_persistent_profile_startup() {
    let signed_host = env::var_os("KELD_KEL135_SIGNED_HOST")
        .expect("KELD_KEL135_SIGNED_HOST must point to a signed keld-host.exe");
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
    let beacon = spawn_renderer_beacon(beacon_listener);
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write signed renderer");
    let stage = keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(&signed_host))
        .expect("stage the signed KEL-135 host");
    let mut host = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch signed host without KELD_DEV_LEASE");
    let host_pid = host.id();
    let (mut reader, mut writer, bun_pid, _) =
        accept_ready_generation(&control_listener, &mut host);
    expect_renderer_beacon(beacon, "signed host renderer beacon");
    let window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_eq!(window["title"], PRODUCT_TITLE);
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
