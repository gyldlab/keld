//! Ordered shutdown and fresh authority on a successful relaunch.

use crate::support::{
    PRODUCT_DEADLINE,
    control::{accept_control_or_host_failure, assert_nonzero_descendant, read_control_line},
    process::{wait_child, wait_for_strict_generation, wait_process_identity_gone},
    project::{DARK_BG, PRODUCT_TITLE, ProductFixture},
    renderer::serve_renderer_beacon,
};
use std::{
    fs,
    io::{BufReader, Read as _, Write as _},
    net::TcpListener,
    os::unix::net::UnixListener,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Instant,
};

#[test]
fn linux_no_flag_host_owns_window_two_calls_ordered_quit_and_relaunch() {
    let fixture = ProductFixture::new();
    let first = run_product_cycle(&fixture, "first");
    let second = run_product_cycle(&fixture, "second");

    assert_ne!(first.host_pid, second.host_pid, "relaunch needs a new host");
    assert_ne!(first.bun_pid, second.bun_pid, "relaunch needs a new Bun");
    assert_ne!(
        first.descendant_pid, second.descendant_pid,
        "relaunch needs a new descendant"
    );
    assert_ne!(first.app_link, second.app_link, "authority must be fresh");
    eprintln!(
        "KEL96_T4_LINUX_EVIDENCE session={} display={} first_host={} first_bun={} first_descendant={} second_host={} second_bun={} second_descendant={}",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| String::from("unknown")),
        std::env::var("WAYLAND_DISPLAY")
            .or_else(|_| std::env::var("DISPLAY"))
            .unwrap_or_else(|_| String::from("unavailable")),
        first.host_pid,
        first.bun_pid,
        first.descendant_pid,
        second.host_pid,
        second.bun_pid,
        second.descendant_pid,
    );
}

pub(crate) struct ProductEvidence {
    pub(crate) host_pid: u32,
    pub(crate) bun_pid: u32,
    pub(crate) descendant_pid: u32,
    pub(crate) app_link: String,
}

pub(crate) fn run_product_cycle(fixture: &ProductFixture, label: &str) -> ProductEvidence {
    let control_path = fixture.root.path().join(format!("control-{label}.sock"));
    let control_listener = UnixListener::bind(&control_path).expect("bind control listener");
    control_listener
        .set_nonblocking(true)
        .expect("nonblocking control listener");
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind renderer beacon");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
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
    .expect("stage Linux product host");
    let mut child = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", &control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch staged no-flag Linux host");
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
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let inner_bun_pid = fields
        .next()
        .expect("Bun pid")
        .parse::<u32>()
        .expect("numeric Bun pid");
    assert_ne!(inner_bun_pid, 0);
    let app_link = fields.next().expect("app link").to_owned();
    assert!(fields.next().is_none(), "{hello}");
    assert_nonzero_descendant(&read_control_line(&mut reader));
    let generation = wait_for_strict_generation(host_pid, Instant::now() + PRODUCT_DEADLINE);
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("WebKitGTK renderer requested the exact beacon");
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");

    writer.write_all(b"QUIT\n").expect("request Quit");
    writer.flush().expect("flush Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    if !status.success() {
        let mut stderr = String::new();
        child
            .stderr
            .take()
            .expect("host stderr")
            .read_to_string(&mut stderr)
            .expect("read host stderr");
        panic!("host exited with {status}: {stderr}");
    }
    wait_process_identity_gone(&generation.bun, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&generation.descendant, Instant::now() + PRODUCT_DEADLINE);
    beacon.join().expect("renderer beacon thread");
    drop(stage);

    ProductEvidence {
        host_pid,
        bun_pid: generation.bun.pid,
        descendant_pid: generation.descendant.pid,
        app_link,
    }
}
