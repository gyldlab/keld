//! Real-Linux KEL-96/T4 no-flag host acceptance.

#![cfg(target_os = "linux")]
#![allow(clippy::expect_used, clippy::panic)] // process and filesystem observations are assertion oracles

#[path = "no_flag/linux/lifecycle.rs"]
mod lifecycle;

use lifecycle::run_product_cycle;

#[path = "no_flag/linux/staging.rs"]
mod staging;
#[path = "no_flag/linux/support/mod.rs"]
mod support;

use std::{
    fs,
    io::{BufReader, Write as _},
    net::TcpListener,
    os::unix::net::UnixListener,
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Instant,
};
use support::{
    PRODUCT_DEADLINE,
    control::{
        accept_control_or_host_failure, assert_nonzero_descendant, expect_ready_and_echoes,
        read_control_line,
    },
    process::{
        StrictGeneration, descendant_identities, process_stat, sigkill_identity, wait_child,
        wait_child_output, wait_for_direct_host, wait_for_strict_generation,
        wait_process_identity_gone,
    },
    project::{DARK_BG, PRODUCT_TITLE, ProductFixture, prepare_keld_dev_helper},
    renderer::serve_renderer_beacon,
    stage::{dev_stage_count, wait_for_dev_stage_count},
};

#[test]
fn keld_dev_linux_helper() {
    let Some(project) = std::env::var_os("KELD_T4_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(Path::new(&project)).expect("shipping Linux keld dev helper");
}

#[test]
fn linux_no_flag_host_recovers_bun_in_the_same_renderer_window() {
    let fixture = ProductFixture::new();
    let control_path = fixture.root.path().join("recovery-control.sock");
    let listener = UnixListener::bind(&control_path).expect("bind recovery control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking recovery control");
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind recovery beacon");
    let beacon_probe = beacon_listener.try_clone().expect("clone recovery beacon");
    let beacon_port = beacon_listener
        .local_addr()
        .expect("recovery beacon")
        .port();
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("recovery renderer");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage recovery host");
    let mut host = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", &control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch recovery host");
    let host_pid = host.id();
    let (mut g1_reader, mut g1_writer, g1, g1_link) = accept_generation(&listener, &mut host);
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("initial renderer beacon");
    expect_ready_and_echoes(&mut g1_reader);
    g1_writer
        .write_all(b"CRASH\n")
        .expect("crash generation one");
    g1_writer.flush().expect("flush generation-one crash");
    drop((g1_reader, g1_writer));
    wait_process_identity_gone(&g1.bun, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&g1.descendant, Instant::now() + PRODUCT_DEADLINE);

    let (mut g2_reader, mut g2_writer, g2, g2_link) = accept_generation(&listener, &mut host);
    expect_ready_and_echoes(&mut g2_reader);
    assert_ne!(
        g1.bun.pid, g2.bun.pid,
        "recovery must spawn a fresh Bun process"
    );
    assert_ne!(g1_link, g2_link, "recovery must mint fresh authority");
    assert_eq!(host.id(), host_pid, "recovery cannot replace the host");
    beacon_probe
        .set_nonblocking(true)
        .expect("nonblocking recovery beacon probe");
    assert!(
        matches!(
            beacon_probe.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "recovery reloaded or replaced the renderer"
    );

    g2_writer.write_all(b"QUIT\n").expect("quit generation two");
    g2_writer.flush().expect("flush generation-two quit");
    assert_eq!(read_control_line(&mut g2_reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut g2_reader), "LINK_EOF");
    let status = wait_child(&mut host, Instant::now() + PRODUCT_DEADLINE);
    assert!(status.success(), "recovery host exited with {status}");
    wait_process_identity_gone(&g2.bun, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&g2.descendant, Instant::now() + PRODUCT_DEADLINE);
    beacon.join().expect("recovery beacon thread");
}

#[test]
fn shipping_keld_dev_delegates_ownership_and_deletes_its_stage() {
    let fixture = ProductFixture::new();
    let helper = prepare_keld_dev_helper(&fixture);

    let control_path = fixture.root.path().join("dev-control.sock");
    let listener = UnixListener::bind(&control_path).expect("bind dev control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking dev control");
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind dev beacon");
    let beacon_port = beacon_listener
        .local_addr()
        .expect("dev beacon address")
        .port();
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("dev renderer");
    let mut cli = Command::new(&helper)
        .args(["--exact", "keld_dev_linux_helper", "--nocapture"])
        .current_dir(&fixture.project)
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env("KELD_T1B_CONTROL", &control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch shipping keld dev helper");
    let cli_pid = cli.id();
    let control =
        accept_control_or_host_failure(&listener, &mut cli, Instant::now() + PRODUCT_DEADLINE);
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("dev control deadline");
    let mut writer = control.try_clone().expect("dev control writer");
    let mut reader = BufReader::new(control);
    let hello = read_control_line(&mut reader);
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let inner_bun_pid = fields
        .next()
        .expect("dev Bun pid")
        .parse::<u32>()
        .expect("numeric dev Bun pid");
    assert_ne!(inner_bun_pid, 0);
    let host = wait_for_direct_host(cli_pid, Instant::now() + PRODUCT_DEADLINE);
    let generation = wait_for_strict_generation(host.pid, Instant::now() + PRODUCT_DEADLINE);
    assert_eq!(
        process_stat(host.pid).map(|(parent, _)| parent),
        Some(cli_pid),
        "CLI must launch only the host"
    );
    assert_ne!(
        generation.bun.pid, cli_pid,
        "CLI cannot own the Bun process"
    );
    assert_nonzero_descendant(&read_control_line(&mut reader));
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("shipping renderer beacon");
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");
    assert_eq!(dev_stage_count(&fixture.project), 1);

    writer.write_all(b"QUIT\n").expect("dev Quit");
    writer.flush().expect("flush dev Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let output = wait_child_output(cli, Instant::now() + PRODUCT_DEADLINE);
    assert!(output.status.success(), "shipping dev failed: {output:?}");
    let mut forwarded = String::from_utf8(output.stdout).expect("CLI stdout UTF-8");
    forwarded.push_str(&String::from_utf8(output.stderr).expect("CLI stderr UTF-8"));
    assert!(
        forwarded.contains("KEL96_T2_FORWARDED_LOG"),
        "shipping CLI lost the host-owned Bun log: {forwarded}"
    );
    wait_process_identity_gone(&host, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&generation.bun, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&generation.descendant, Instant::now() + PRODUCT_DEADLINE);
    wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
    beacon.join().expect("dev beacon thread");
}

#[test]
fn shipping_keld_dev_cli_death_reaps_host_bun_and_stage() {
    let fixture = ProductFixture::new();
    let helper = prepare_keld_dev_helper(&fixture);
    let control_path = fixture.root.path().join("dev-death-control.sock");
    let listener = UnixListener::bind(&control_path).expect("bind death control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking death control");
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind death beacon");
    let beacon_port = beacon_listener.local_addr().expect("death beacon").port();
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("death renderer");
    let mut cli = Command::new(&helper)
        .args(["--exact", "keld_dev_linux_helper", "--nocapture"])
        .current_dir(&fixture.project)
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env("KELD_T1B_CONTROL", &control_path)
        .env("KELD_T2_EXIT_ON_LINK_EOF", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch death helper");
    let control =
        accept_control_or_host_failure(&listener, &mut cli, Instant::now() + PRODUCT_DEADLINE);
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("death control deadline");
    let mut reader = BufReader::new(control);
    let hello = read_control_line(&mut reader);
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let inner_bun_pid = fields
        .next()
        .expect("death Bun pid")
        .parse::<u32>()
        .expect("numeric death Bun pid");
    assert_ne!(inner_bun_pid, 0);
    let host = wait_for_direct_host(cli.id(), Instant::now() + PRODUCT_DEADLINE);
    let generation = wait_for_strict_generation(host.pid, Instant::now() + PRODUCT_DEADLINE);
    assert_nonzero_descendant(&read_control_line(&mut reader));
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("death renderer beacon");
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");
    assert_eq!(dev_stage_count(&fixture.project), 1);

    cli.kill().expect("kill only the CLI");
    let status = cli.wait().expect("wait killed CLI");
    assert!(!status.success(), "killed CLI exited successfully");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    wait_process_identity_gone(&host, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&generation.bun, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&generation.descendant, Instant::now() + PRODUCT_DEADLINE);
    wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
    beacon.join().expect("death beacon thread");
}

#[test]
fn linux_host_only_death_reaps_strict_tree_deletes_stage_and_relaunches() {
    let fixture = ProductFixture::new();
    let helper = prepare_keld_dev_helper(&fixture);
    let control_path = fixture.root.path().join("host-death-control.sock");
    let listener = UnixListener::bind(&control_path).expect("bind host-death control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking host-death control");
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind host-death beacon");
    let beacon_port = beacon_listener
        .local_addr()
        .expect("host-death beacon")
        .port();
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("host-death renderer");
    let mut cli = Command::new(&helper)
        .args(["--exact", "keld_dev_linux_helper", "--nocapture"])
        .current_dir(&fixture.project)
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env("KELD_T1B_CONTROL", &control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch host-death helper");
    let cli_pid = cli.id();
    let control =
        accept_control_or_host_failure(&listener, &mut cli, Instant::now() + PRODUCT_DEADLINE);
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("host-death control deadline");
    let mut reader = BufReader::new(control);
    let hello = read_control_line(&mut reader);
    assert!(hello.starts_with("HELLO "), "{hello}");
    assert_nonzero_descendant(&read_control_line(&mut reader));
    let host = wait_for_direct_host(cli_pid, Instant::now() + PRODUCT_DEADLINE);
    let generation = wait_for_strict_generation(host.pid, Instant::now() + PRODUCT_DEADLINE);
    let tree = descendant_identities(host.pid);
    assert!(tree.len() >= 4, "incomplete strict product tree: {tree:?}");
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("host-death renderer beacon");
    expect_ready_and_echoes(&mut reader);
    assert_eq!(dev_stage_count(&fixture.project), 1);

    sigkill_identity(&host);
    for process in &tree {
        wait_process_identity_gone(process, Instant::now() + PRODUCT_DEADLINE);
    }
    wait_process_identity_gone(&generation.bun, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&generation.descendant, Instant::now() + PRODUCT_DEADLINE);
    let output = wait_child_output(cli, Instant::now() + PRODUCT_DEADLINE);
    assert!(!output.status.success(), "host kill must fail keld dev");
    let mut diagnostic = String::from_utf8(output.stdout).expect("host-death stdout UTF-8");
    diagnostic.push_str(&String::from_utf8(output.stderr).expect("host-death stderr UTF-8"));
    assert!(diagnostic.contains("KELD-CLI-048"), "{diagnostic}");
    wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
    beacon.join().expect("host-death beacon thread");

    let relaunched = run_product_cycle(&fixture, "after-host-death");
    assert_ne!(relaunched.host_pid, host.pid);
    eprintln!(
        "KEL96_T4_LINUX_HOST_DEATH host={} bun={} descendant={} reaped={} relaunch_host={}",
        host.pid,
        generation.bun.pid,
        generation.descendant.pid,
        tree.len(),
        relaunched.host_pid
    );
}

fn accept_generation(
    listener: &UnixListener,
    host: &mut Child,
) -> (
    BufReader<std::os::unix::net::UnixStream>,
    std::os::unix::net::UnixStream,
    StrictGeneration,
    String,
) {
    let host_pid = host.id();
    let control = accept_control_or_host_failure(listener, host, Instant::now() + PRODUCT_DEADLINE);
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("generation control deadline");
    let writer = control.try_clone().expect("generation control writer");
    let mut reader = BufReader::new(control);
    let hello = read_control_line(&mut reader);
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let inner_pid = fields
        .next()
        .expect("generation pid")
        .parse::<u32>()
        .expect("numeric generation pid");
    assert_ne!(inner_pid, 0);
    let link = fields.next().expect("generation app link").to_owned();
    assert!(fields.next().is_none(), "{hello}");
    assert_nonzero_descendant(&read_control_line(&mut reader));
    let generation = wait_for_strict_generation(host_pid, Instant::now() + PRODUCT_DEADLINE);
    (reader, writer, generation, link)
}
