//! Shared existing ordered product-cycle observer and returned evidence.

use std::fs;
use std::io::{BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use super::control::{accept_control_or_host_failure, parse_descendant_pid, read_control_line};
use super::product::ProductFixture;
use super::renderer::{expect_renderer_beacon, spawn_renderer_beacon};
use crate::{
    DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE, dev_stage_command, process_exists, wait_child,
    wait_for_host_window,
};

pub(crate) struct ProductEvidence {
    pub(crate) host_pid: u32,
    pub(crate) bun_pid: u32,
    pub(crate) app_link: String,
}

pub(crate) fn run_product_cycle(fixture: &ProductFixture, label: &str) -> ProductEvidence {
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind control listener");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind beacon listener");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let beacon = spawn_renderer_beacon(beacon_listener);
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><p id=exact>{label}</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write renderer");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage Windows product host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch staged no-flag Windows host");
    let host_pid = child.id();
    let control = accept_control_or_host_failure(
        &control_listener,
        &mut child,
        Instant::now() + PRODUCT_DEADLINE,
    );
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("control read deadline");
    let mut writer = control.try_clone().expect("control writer clone");
    let mut reader = BufReader::new(control);

    let hello = read_control_line(&mut reader);
    let mut hello_fields = hello.split_whitespace();
    assert_eq!(hello_fields.next(), Some("HELLO"), "{hello}");
    let bun_pid = hello_fields
        .next()
        .expect("HELLO pid")
        .parse::<u32>()
        .expect("numeric Bun pid");
    let app_link = hello_fields.next().expect("HELLO app link").to_owned();
    assert!(hello_fields.next().is_none(), "{hello}");
    let descendant_record = read_control_line(&mut reader);
    assert_eq!(parse_descendant_pid(&descendant_record), 0);

    expect_renderer_beacon(beacon, "WebView2 renderer requested the exact beacon");
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");
    let window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_ne!(window["handle"].as_u64(), Some(0), "{window}");
    assert_eq!(window["title"], PRODUCT_TITLE, "{window}");

    writer.write_all(b"QUIT\n").expect("request lifecycle Quit");
    writer.flush().expect("flush lifecycle Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    assert!(status.success(), "host exited with {status}");
    assert!(
        !process_exists(bun_pid),
        "Bun {bun_pid} survived orderly host exit"
    );

    ProductEvidence {
        host_pid,
        bun_pid,
        app_link,
    }
}
