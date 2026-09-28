//! Native generation recovery and terminal host-error contracts.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use crate::support::control::{
    accept_control_or_host_failure, accept_ready_generation, parse_descendant_pid,
    read_control_line, read_control_line_or_host_failure, try_read_control_line,
};
use crate::support::product::ProductFixture;
use crate::support::renderer::{expect_renderer_beacon, spawn_renderer_beacon};
use crate::{
    DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE, dev_stage_command, process_exists, wait_child,
    wait_for_host_window,
};

#[test]
fn windows_no_flag_host_recovers_bun_in_the_same_native_window() {
    run_same_window_recovery("CRASH");
}

#[test]
fn windows_link_only_failure_uses_the_supervisor_owned_restart_path() {
    run_same_window_recovery("CLOSE_LINK");
}

#[test]
fn windows_status_zero_self_termination_keeps_pid_and_status_in_the_host_error() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind exit-zero control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage exit-zero host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch exit-zero host");
    let (_reader, mut writer, bun_pid, _link) =
        accept_ready_generation(&control_listener, &mut child);

    writer.write_all(b"EXIT0\n").expect("request status zero");
    writer.flush().expect("flush status-zero request");
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    assert!(
        !status.success(),
        "status-zero Bun exit became host success"
    );
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("captured exit-zero stderr")
        .read_to_string(&mut stderr)
        .expect("read exit-zero stderr");
    assert!(stderr.trim_start().starts_with("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains(&bun_pid.to_string()), "{stderr}");
    assert!(stderr.contains("status Some(0)"), "{stderr}");
}

#[test]
fn windows_fast_revoked_g2_is_never_installed_ahead_of_g3() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fast-g2 control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let marker = fixture.project.join("generation-attempt");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage fast-g2 host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .env("KELD_T4_GENERATION_MARKER", &marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch fast-g2 host");
    let host_pid = child.id();

    let (_g1_reader, mut g1_writer, g1_pid, _g1_link) =
        accept_ready_generation(&control_listener, &mut child);
    let g1_window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    g1_writer.write_all(b"CRASH\n").expect("crash g1");
    g1_writer.flush().expect("flush g1 crash");

    let g2 = accept_control_or_host_failure(
        &control_listener,
        &mut child,
        Instant::now() + PRODUCT_DEADLINE,
    );
    g2.set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("g2 control deadline");
    let mut g2_writer = g2.try_clone().expect("g2 control acknowledgment writer");
    let mut g2_reader = BufReader::new(g2);
    let g2_hello = read_control_line_or_host_failure(&mut g2_reader, &mut child, "g2 HELLO");
    let mut g2_fields = g2_hello.split_whitespace();
    assert_eq!(g2_fields.next(), Some("HELLO"), "{g2_hello}");
    let g2_pid = g2_fields
        .next()
        .expect("g2 pid")
        .parse::<u32>()
        .expect("numeric g2 pid");
    let g2_descendant =
        read_control_line_or_host_failure(&mut g2_reader, &mut child, "g2 DESCENDANT");
    assert_eq!(parse_descendant_pid(&g2_descendant), 0);
    g2_writer
        .write_all(b"G2_WITNESSED\n")
        .expect("acknowledge observed g2 records");
    g2_writer
        .flush()
        .expect("flush g2 observation acknowledgment");

    let (_g3_reader, mut g3_writer, g3_pid, _g3_link) =
        accept_ready_generation(&control_listener, &mut child);
    let g3_window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_ne!(g1_pid, g2_pid);
    assert_ne!(g2_pid, g3_pid);
    assert_eq!(g1_window["handle"], g3_window["handle"]);

    g3_writer.write_all(b"QUIT\n").expect("request g3 Quit");
    g3_writer.flush().expect("flush g3 Quit");
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    assert!(status.success(), "fast-g2 recovery host exited {status}");
}

#[test]
fn windows_crash_loop_keeps_core033_as_the_outer_host_error() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind crash-loop control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage crash-loop host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch crash-loop host");

    for _ in 0..2 {
        let (_reader, mut writer, _pid, _link) =
            accept_ready_generation(&control_listener, &mut child);
        writer.write_all(b"CRASH\n").expect("crash generation");
        writer.flush().expect("flush generation crash");
    }
    let (mut reader, mut writer, _pid, _link) =
        accept_ready_generation(&control_listener, &mut child);
    writer
        .write_all(b"CRASH_ACKED\n")
        .expect("request acknowledged threshold crash");
    writer.flush().expect("flush acknowledged threshold crash");
    let acknowledgement = try_read_control_line(&mut reader, Instant::now() + PRODUCT_DEADLINE);
    let acknowledgement = match acknowledgement {
        Ok(line) => line,
        Err(error) => {
            let status = if let Some(status) = child
                .try_wait()
                .expect("observe host after missing CRASH_ACK")
            {
                status
            } else {
                let _ = child.kill();
                child.wait().expect("reap host after missing CRASH_ACK")
            };
            panic!("fixture did not acknowledge CRASH before host exit {status}: {error}");
        }
    };
    assert_eq!(acknowledgement, "CRASH_ACK");
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    assert!(
        !status.success(),
        "acknowledged crash loop became host success: {status}"
    );
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("captured crash-loop stderr")
        .read_to_string(&mut stderr)
        .expect("read crash-loop stderr");
    assert!(stderr.trim_start().starts_with("KELD-CORE-033"), "{stderr}");
    assert!(stderr.contains("KELD-RUNTIME-002"), "{stderr}");
}

fn run_same_window_recovery(failure_command: &str) {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind recovery control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind recovery beacon");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let beacon = spawn_renderer_beacon(beacon_listener);
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><p id=exact>{failure_command}</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write recovery renderer");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage recovery host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch recovery host");
    let host_pid = child.id();

    let (mut g1_reader, mut g1_writer, g1_pid, g1_link) =
        accept_ready_generation(&control_listener, &mut child);
    expect_renderer_beacon(beacon, "initial renderer beacon");
    let g1_window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    writeln!(g1_writer, "{failure_command}").expect("fail g1 app link or process");
    g1_writer.flush().expect("flush g1 failure command");
    let mut closed = String::new();
    let _ = g1_reader.read_line(&mut closed);

    let (mut g2_reader, mut g2_writer, g2_pid, g2_link) =
        accept_ready_generation(&control_listener, &mut child);
    let g2_window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_ne!(g1_pid, g2_pid, "recovery must spawn a new Bun process");
    assert_ne!(
        g1_link, g2_link,
        "recovery must mint fresh app-link authority"
    );
    assert_eq!(g1_window["handle"], g2_window["handle"], "HWND changed");
    assert_eq!(g2_window["title"], PRODUCT_TITLE);

    g2_writer.write_all(b"QUIT\n").expect("request g2 Quit");
    g2_writer.flush().expect("flush g2 Quit");
    assert_eq!(read_control_line(&mut g2_reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut g2_reader), "LINK_EOF");
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    assert!(status.success(), "recovery host exited with {status}");
    assert!(!process_exists(g1_pid), "retired g1 Bun survived");
    assert!(!process_exists(g2_pid), "g2 Bun survived Quit");
}

#[test]
fn windows_pre_ready_crash_denies_successor_before_provisioning() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind pre-ready control");
    control_listener
        .set_nonblocking(true)
        .expect("nonblocking pre-ready control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let marker = fixture.project.join("pre-ready-attempt");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage pre-ready host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .env("KELD_T3_CRASH_BEFORE_HELLO", "1")
        .env("KELD_T3_PRE_READY_MARKER", &marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch pre-ready host");
    let host_pid = child.id();
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    assert!(!status.success(), "pre-Ready crash became success");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("captured pre-ready stderr")
        .read_to_string(&mut stderr)
        .expect("read pre-ready stderr");
    assert!(
        stderr.contains("terminated before its initial authenticated generation bound"),
        "{stderr}"
    );
    let attempts = fs::read_dir(&fixture.project)
        .expect("list pre-ready markers")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("pre-ready-attempt.")
        })
        .count();
    assert_eq!(attempts, 1, "a pre-Ready crash provisioned a successor");
    assert!(
        matches!(
            control_listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "pre-Ready child reached the control service"
    );
    assert!(!process_exists(host_pid), "failed host remained live");
}
