//! Native generation recovery and terminal host-error contracts.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::support::control::{
    accept_control_or_host_failure, accept_ready_generation, assert_no_control_connection,
    parse_descendant_pid, parse_hello, read_control_line, read_control_line_or_host_failure,
    try_read_control_line,
};
use crate::support::process::{assert_process_signaled_within, open_process_for_wait};
use crate::support::product::ProductFixture;
use crate::support::renderer::{
    HeldRendererRequest, expect_renderer_beacon, spawn_renderer_beacon,
};
use crate::{
    DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE, dev_stage_command, process_exists,
    stalled_navigation_end_kill_switch, ui_end_kill_switch, wait_child, wait_for_host_window,
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
    let status = wait_child(&mut child, Instant::now() + ui_end_kill_switch());
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
    let (g2_pid, _g2_link) = parse_hello(&g2_hello);
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
    let status = wait_child(&mut child, Instant::now() + ui_end_kill_switch());
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
    let status = wait_child(&mut child, Instant::now() + ui_end_kill_switch());
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
    let status = wait_child(&mut child, Instant::now() + ui_end_kill_switch());
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
    assert_no_control_connection(
        &control_listener,
        "pre-Ready child reached the control service",
    );
    assert!(!process_exists(host_pid), "failed host remained live");
}

/// GH-1182 R1: Bun exits after its authenticated HELLO while the initial window
/// is live and its first navigation is held open ([`HeldRendererRequest`]), so
/// the window cannot be `Ready` first. The pre-Ready recovery gate is still
/// closed, so the host fails startup instead of provisioning a successor
/// (architecture 06 §1).
#[test]
fn windows_post_hello_pre_ready_exit_fails_startup_without_successor() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind post-HELLO control");
    control_listener
        .set_nonblocking(true)
        .expect("nonblocking post-HELLO control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let held = HeldRendererRequest::bind();
    held.write_index(&fixture.project);
    let marker = fixture.project.join("post-hello-attempt");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage post-HELLO host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .env("KELD_T3_EXIT_AFTER_HELLO", "1")
        .env("KELD_T3_PRE_READY_MARKER", &marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch post-HELLO host");
    let host_pid = child.id();
    let control = accept_control_or_host_failure(
        &control_listener,
        &mut child,
        Instant::now() + PRODUCT_DEADLINE,
    );
    let mut reader = BufReader::new(control.try_clone().expect("clone post-HELLO control"));
    let mut writer = control;
    parse_hello(&read_control_line_or_host_failure(
        &mut reader,
        &mut child,
        "post-HELLO HELLO",
    ));
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, &mut child, "post-HELLO DESCENDANT"),
        "DESCENDANT 0"
    );
    // The 15 s initial-navigation deadline (KELD-WV-005) is still armed. The
    // exit below must reach the owner before it fires; if it ever lost that
    // race, the stderr assertion fails with KELD-WV-005 rather than passing.
    held.await_connection(&mut child);

    writer
        .write_all(b"EXIT_PRE_READY\n")
        .expect("request post-HELLO exit");
    writer.flush().expect("flush post-HELLO exit");
    // The live window makes this a host-fatal Bun exit with a WebView2 UI end.
    let status = wait_child(&mut child, Instant::now() + ui_end_kill_switch());
    assert!(
        !status.success(),
        "post-HELLO pre-Ready exit became success"
    );
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("captured post-HELLO stderr")
        .read_to_string(&mut stderr)
        .expect("read post-HELLO stderr");
    assert!(
        stderr.contains("Bun terminated before the initial window became ready"),
        "{stderr}"
    );
    assert!(!stderr.contains("KELD-RUNTIME-002"), "{stderr}");
    // Corroboration only: this counts the window attempt; the held connection
    // above is what proves the window was live and navigating.
    assert!(stderr.contains("window=1]"), "{stderr}");
    let attempts = fs::read_dir(&fixture.project)
        .expect("list post-HELLO markers")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("post-hello-attempt.")
        })
        .count();
    assert_eq!(
        attempts, 1,
        "a post-HELLO pre-Ready exit provisioned a successor"
    );
    assert_no_control_connection(&control_listener, "a successor reached the control service");
    assert!(!process_exists(host_pid), "failed host remained live");
    held.release();
}

/// GH-1182 (PR #1300 review): a live Bun ends only its app link after its
/// authenticated HELLO, with the first navigation held so the window cannot be
/// `Ready`, while its process and control link stay up. The owner's typed
/// pre-Ready cause must be the host's primary error, not a cleanup detail of
/// the `WebView2` loop's own failure, and no successor may be provisioned.
#[test]
fn windows_live_pre_ready_link_close_reports_owner_cause_first_without_successor() {
    let fixture = ProductFixture::new();
    let control_listener =
        TcpListener::bind(("127.0.0.1", 0)).expect("bind pre-Ready link-close control");
    control_listener
        .set_nonblocking(true)
        .expect("nonblocking pre-Ready link-close control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let held = HeldRendererRequest::bind();
    held.write_index(&fixture.project);
    let generations = fixture.project.join("pre-ready-link-close-generation");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage pre-Ready link-close host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .env("KELD_T3_EXIT_AFTER_HELLO", "1")
        .env("KELD_T4_GENERATION_MARKER", &generations)
        .env("KELD_T4_JOB_DESCENDANT", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch pre-Ready link-close host");
    let host_pid = child.id();
    let control = accept_control_or_host_failure(
        &control_listener,
        &mut child,
        Instant::now() + PRODUCT_DEADLINE,
    );
    let mut reader = BufReader::new(control.try_clone().expect("clone link-close control"));
    let mut writer = control;
    let (bun_pid, _link) = parse_hello(&read_control_line_or_host_failure(
        &mut reader,
        &mut child,
        "pre-Ready link-close HELLO",
    ));
    // Opened while both are live, so each exit is observed on its exact process.
    let bun = open_process_for_wait(bun_pid, false);
    let descendant = open_process_for_wait(
        parse_descendant_pid(&read_control_line_or_host_failure(
            &mut reader,
            &mut child,
            "pre-Ready link-close DESCENDANT",
        )),
        false,
    );
    // The 15 s initial-navigation deadline (KELD-WV-005) is still armed; the
    // first-line assertion below rejects a run that lost that race.
    held.await_connection(&mut child);

    writer
        .write_all(b"CLOSE_PRE_READY_LINK\n")
        .expect("request pre-Ready app-link close");
    writer.flush().expect("flush pre-Ready app-link close");
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, &mut child, "pre-Ready link-close ack"),
        "PRE_READY_LINK_CLOSED"
    );
    let status = wait_child(&mut child, Instant::now() + ui_end_kill_switch());
    assert!(
        !status.success(),
        "a pre-Ready app-link close became success"
    );
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("captured pre-Ready link-close stderr")
        .read_to_string(&mut stderr)
        .expect("read pre-Ready link-close stderr");
    assert!(
        stderr.trim_start().starts_with(
            "KELD-CORE-037: no-flag host failed during primary generation before Ready"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains("Bun terminated before the initial window became ready"),
        "{stderr}"
    );
    assert_process_signaled_within(&bun, "pre-Ready link-close Bun", PRODUCT_DEADLINE);
    assert_process_signaled_within(
        &descendant,
        "pre-Ready link-close descendant",
        PRODUCT_DEADLINE,
    );
    assert_eq!(
        fs::read_to_string(&generations).expect("read pre-Ready link-close generation marker"),
        "1\n",
        "a pre-Ready app-link close provisioned a successor generation"
    );
    assert_no_control_connection(&control_listener, "a successor reached the control service");
    assert!(!process_exists(host_pid), "failed host remained live");
    held.release();
}

/// GH-1182 R2: the initial navigation never completes. Its renderer connection
/// is held ([`HeldRendererRequest`]), so `NavigationCompleted` cannot fire and
/// the window never becomes `Ready`. The `WebView2` initial-navigation deadline
/// (`keld_wv::INITIAL_MILESTONE_DEADLINE`) must then fail startup with
/// `KELD-WV-005`, take the Bun generation down with the host, and provision no
/// successor.
#[test]
fn windows_stalled_initial_navigation_fails_startup_at_its_deadline() {
    let fixture = ProductFixture::new();
    let control_listener =
        TcpListener::bind(("127.0.0.1", 0)).expect("bind stalled-navigation control");
    control_listener
        .set_nonblocking(true)
        .expect("nonblocking stalled-navigation control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let held = HeldRendererRequest::bind();
    held.write_index(&fixture.project);
    let generations = fixture.project.join("stalled-generation");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage stalled-navigation host");
    let mut child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .env("KELD_T4_GENERATION_MARKER", &generations)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch stalled-navigation host");
    let host_pid = child.id();
    let control = accept_control_or_host_failure(
        &control_listener,
        &mut child,
        Instant::now() + PRODUCT_DEADLINE,
    );
    let mut reader = BufReader::new(control.try_clone().expect("clone stalled control"));
    let hello = read_control_line_or_host_failure(&mut reader, &mut child, "stalled HELLO");
    let (bun_pid, _link) = parse_hello(&hello);
    // Opened while Bun is live, so its exit is observed on this exact process.
    let bun = open_process_for_wait(bun_pid, false);
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, &mut child, "stalled DESCENDANT"),
        "DESCENDANT 0"
    );
    held.await_connection(&mut child);

    let status = wait_child(
        &mut child,
        Instant::now() + stalled_navigation_end_kill_switch(),
    );
    assert!(!status.success(), "stalled navigation became success");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("captured stalled-navigation stderr")
        .read_to_string(&mut stderr)
        .expect("read stalled-navigation stderr");
    assert!(stderr.contains("KELD-WV-005"), "{stderr}");
    assert!(
        stderr.contains("initial renderer navigation did not finish before the startup deadline"),
        "{stderr}"
    );
    assert!(!stderr.contains("KELD-RUNTIME-002"), "{stderr}");
    let span = first_navigation_deadline_span(&stderr);
    assert!(
        span + DEADLINE_ANCHOR_TOLERANCE >= keld_wv::INITIAL_MILESTONE_DEADLINE,
        "navigation deadline fired after {span:?}, before {:?}: {stderr}",
        keld_wv::INITIAL_MILESTONE_DEADLINE
    );
    assert_process_signaled_within(&bun, "stalled-navigation Bun", PRODUCT_DEADLINE);
    assert_eq!(
        fs::read_to_string(&generations).expect("read stalled generation marker"),
        "1\n",
        "a stalled navigation provisioned a successor generation"
    );
    assert_no_control_connection(&control_listener, "a successor reached the control service");
    assert!(!process_exists(host_pid), "failed host remained live");
    held.release();
}

/// The `WebView2` loop takes its navigation-deadline anchor just before the
/// diagnostic records `loop_setup_at_ms`, and both stamps are floored to whole
/// milliseconds. The measured span can therefore fall short of the deadline by
/// that anchor-to-record gap (one lock acquisition, which a preempted thread can
/// stretch over a few scheduler quanta) plus 1 ms. A deadline armed shorter or
/// earlier falls short by far more.
const DEADLINE_ANCHOR_TOLERANCE: Duration = Duration::from_millis(100);

/// The host's own measurement of its initial-navigation deadline, from the
/// `first-navigation` diagnostic the `KELD-WV-005` failure carries: the span from
/// message-loop setup (where the deadline is armed) to the recorded expiry.
fn first_navigation_deadline_span(stderr: &str) -> Duration {
    let diagnostic = &stderr[stderr
        .find("first-navigation ")
        .unwrap_or_else(|| panic!("no first-navigation diagnostic: {stderr}"))..];
    let field = |key: &str| -> u64 {
        let start = diagnostic
            .find(key)
            .unwrap_or_else(|| panic!("no {key} in {diagnostic}"))
            + key.len();
        let value = diagnostic[start..]
            .trim_start_matches("Some(")
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or_default();
        value
            .parse()
            .unwrap_or_else(|_| panic!("non-numeric {key} in {diagnostic}"))
    };
    assert!(
        diagnostic.starts_with("first-navigation phase=navigation-deadline-elapsed "),
        "{diagnostic}"
    );
    let expired = field(" phase_at_ms=");
    let armed = field(" loop_setup_at_ms=");
    Duration::from_millis(
        expired
            .checked_sub(armed)
            .unwrap_or_else(|| panic!("deadline expiry precedes loop setup: {diagnostic}")),
    )
}
