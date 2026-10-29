//! Recover Bun authority while retaining the same host and renderer window.

use crate::support::{
    PRODUCT_DEADLINE,
    control::{
        accept_control_or_host_failure, assert_nonzero_descendant, expect_ready_and_echoes,
        read_control_line,
    },
    process::{
        StrictGeneration, wait_child, wait_for_strict_generation, wait_process_identity_gone,
    },
    project::{DARK_BG, PRODUCT_TITLE, ProductFixture},
    renderer::{HeldRendererRequest, expect_renderer_beacon, spawn_renderer_beacon},
};
use std::{
    fs,
    io::{BufReader, Read as _, Write as _},
    net::TcpListener,
    os::unix::net::UnixListener,
    path::Path,
    process::{Child, Command, Stdio},
    time::Instant,
};

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
    let beacon = spawn_renderer_beacon(beacon_listener);
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
    expect_renderer_beacon(beacon, "initial renderer beacon");
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

/// GH-1182 R1 on Linux: Bun exits after its authenticated HELLO while the initial
/// window is live and its first navigation is held open ([`HeldRendererRequest`]),
/// so the window cannot be `Ready` first. The pre-Ready recovery gate is still
/// closed, so the host fails startup instead of provisioning a successor
/// (architecture 06 §1).
#[test]
fn linux_post_hello_pre_ready_exit_fails_startup_without_successor() {
    let fixture = ProductFixture::new();
    let control_path = fixture.root.path().join("post-hello-control.sock");
    let listener = UnixListener::bind(&control_path).expect("bind post-HELLO control");
    listener
        .set_nonblocking(true)
        .expect("nonblocking post-HELLO control");
    let held = HeldRendererRequest::bind();
    held.write_index(&fixture.project);
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage post-HELLO host");
    let mut host = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", &control_path)
        .env("KELD_T3_EXIT_AFTER_HELLO", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch post-HELLO host");
    let (_reader, mut writer, generation, _link) = accept_generation(&listener, &mut host);
    held.await_connection(&mut host);

    // The initial navigation deadline (`keld_wv::INITIAL_MILESTONE_DEADLINE`)
    // keeps running while the load is held. If it fires first, the host fails
    // with that deadline instead, and the message assertion below rejects it.
    writer
        .write_all(b"EXIT_PRE_READY\n")
        .expect("request post-HELLO exit");
    writer.flush().expect("flush post-HELLO exit");
    let status = wait_child(&mut host, Instant::now() + PRODUCT_DEADLINE);
    assert!(
        !status.success(),
        "post-HELLO pre-Ready exit became success"
    );
    // Prove the sandbox is gone before reading to the end of the host's stderr.
    wait_process_identity_gone(&generation.bun, Instant::now() + PRODUCT_DEADLINE);
    wait_process_identity_gone(&generation.descendant, Instant::now() + PRODUCT_DEADLINE);
    let mut stderr = String::new();
    host.stderr
        .take()
        .expect("captured post-HELLO stderr")
        .read_to_string(&mut stderr)
        .expect("read post-HELLO stderr");
    assert!(
        stderr.contains("Bun terminated before the initial window became ready"),
        "{stderr}"
    );
    assert!(!stderr.contains("KELD-RUNTIME-002"), "{stderr}");
    assert!(
        matches!(
            listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "a successor reached the control service"
    );
    held.release();
}
