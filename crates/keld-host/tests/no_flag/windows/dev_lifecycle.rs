//! Native delegated dev lifecycle, parent death and retained process cleanup.

use crate::support::control::{
    accept_ready_generation, accept_ready_generation_with_descendant,
    accept_ready_generation_with_lease, read_control_line,
};
use crate::support::handles::assert_dev_lease_handle_isolation;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use crate::support::process::{PIPE_DRAIN_KILL_SWITCH, pipe_text_by, read_pipe_on_thread};
use crate::support::process::{
    assert_process_signaled, open_process_for_wait, process_exists, terminate_test_process,
    wait_child, wait_for_child_process, wait_for_cleanup_sentinel, wait_process_gone,
};
use crate::support::product::ProductFixture;
use crate::support::product_cycle::run_product_cycle;
use crate::support::renderer::{expect_renderer_beacon, spawn_renderer_beacon};
use crate::support::window::wait_for_host_window;
use crate::{
    DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE, dev_stage_count, prepare_keld_dev_helper,
    ui_end_kill_switch, wait_for_dev_stage_count,
};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::windows::io::AsRawHandle as _;
use std::process::{Command, Stdio};
use std::time::Instant;

#[test]
fn shipping_windows_keld_dev_delegates_and_cleans_the_orderly_stage() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind shipping control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind shipping beacon");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let beacon = spawn_renderer_beacon(beacon_listener);
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><p id=exact>shipping</p><script>\
             const leaked=(typeof process!=='undefined'&&process.env?.KELD_APP_LINK)||globalThis.KELD_APP_LINK;\
             const image=document.createElement('img');\
             image.src='http://127.0.0.1:{beacon_port}/'+(leaked?'leaked':'ready')+'.png';\
             document.body.append(image);</script>\n"
        ),
    )
    .expect("write shipping renderer");
    let helper = prepare_keld_dev_helper(&fixture);
    let mut cli = Command::new(&helper)
        .arg("keld_dev_windows_helper")
        .arg("--exact")
        .arg("--nocapture")
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch shipping keld dev helper");
    let host_pid =
        wait_for_child_process(&mut cli, "keld-host.exe", Instant::now() + PRODUCT_DEADLINE);
    let (mut reader, mut writer, bun_pid, _) = accept_ready_generation(&control_listener, &mut cli);
    expect_renderer_beacon(beacon, "shipping renderer beacon");
    let window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_eq!(window["title"], PRODUCT_TITLE);
    writer.write_all(b"QUIT\n").expect("shipping Quit");
    writer.flush().expect("flush shipping Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    // The CLI returns after its delegated host, whose exit includes the UI end.
    let status = wait_child(&mut cli, Instant::now() + ui_end_kill_switch());
    if !status.success() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        cli.stdout
            .take()
            .expect("captured shipping stdout")
            .read_to_string(&mut stdout)
            .expect("read shipping stdout");
        cli.stderr
            .take()
            .expect("captured shipping stderr")
            .read_to_string(&mut stderr)
            .expect("read shipping stderr");
        panic!("shipping keld dev exited with {status}\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }
    assert!(
        !process_exists(host_pid),
        "delegated host survived orderly exit"
    );
    assert!(
        !process_exists(bun_pid),
        "delegated Bun survived orderly exit"
    );
    assert_eq!(dev_stage_count(&fixture.project), 0, "orderly stage leaked");
}

/// AC3/AC23: `keld dev` stages a schema-2 project and the host's own refusal answers
/// it, in every build, with the stage cleaned by the same sentinel (GH-1188 D2, D15).
#[test]
fn shipping_windows_keld_dev_reports_the_host_refusal_of_a_facade_stage() {
    let fixture = ProductFixture::schema_two();
    let helper = prepare_keld_dev_helper(&fixture);
    let mut cli = Command::new(&helper)
        .arg("keld_dev_windows_helper")
        .arg("--exact")
        .arg("--nocapture")
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch facade keld dev helper");
    let status = wait_child(&mut cli, Instant::now() + PRODUCT_DEADLINE);
    let output = crate::support::process::exited_child_output(&mut cli);
    assert!(!status.success(), "a refused boot became success: {output}");
    assert!(output.contains("KELD-CORE-034"), "{output}");
    assert!(!output.contains("KELD-CLI-047"), "{output}");
    wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
}

/// AC19: on Windows only the `keld dev` cleanup process deletes the stage, once the
/// attempt Job reports zero processes. The Quit and the AC2 terminal are this facade
/// boot's two exits; the gate itself is existing code, so this is a positive control
/// of the boot kind, with no fixture to leave a member alive at it (AC19 *NC*).
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[test]
fn shipping_windows_keld_dev_cleans_a_facade_stage_after_the_attempt_job_empties() {
    for stalled in [false, true] {
        let fixture = ProductFixture::schema_two();
        let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind facade control");
        let control_port = control_listener
            .local_addr()
            .expect("facade control address")
            .port();
        let helper = prepare_keld_dev_helper(&fixture);
        let mut command = Command::new(&helper);
        command
            .arg("keld_dev_windows_helper")
            .arg("--exact")
            .arg("--nocapture")
            .env("KELD_T4_HELPER_PROJECT", &fixture.project)
            .env("KELD_T1B_CONTROL", control_port.to_string())
            .env("KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stalled {
            command.env("KELD_T1B_STALL_BEFORE_INIT", "1");
        }
        let mut cli = command.spawn().expect("launch facade keld dev helper");
        let stdout = read_pipe_on_thread(cli.stdout.take().expect("captured CLI stdout"));
        let stderr = read_pipe_on_thread(cli.stderr.take().expect("captured CLI stderr"));
        let host_pid =
            wait_for_child_process(&mut cli, "keld-host.exe", Instant::now() + PRODUCT_DEADLINE);
        let cleanup_pid = wait_for_cleanup_sentinel(&mut cli, Instant::now() + PRODUCT_DEADLINE);
        let bun_pid = if stalled {
            let (mut reader, pid) = accept_hello(&control_listener, &mut cli);
            assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
            pid
        } else {
            let (mut reader, mut writer, pid, _) =
                accept_ready_generation(&control_listener, &mut cli);
            assert_eq!(dev_stage_count(&fixture.project), 1, "the live stage");
            writer.write_all(b"QUIT\n").expect("facade Quit");
            writer.flush().expect("flush facade Quit");
            assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
            assert_eq!(read_control_line(&mut reader), "LINK_EOF");
            pid
        };
        let status = wait_child(
            &mut cli,
            Instant::now() + keld_wv::INITIAL_MILESTONE_DEADLINE + ui_end_kill_switch(),
        );
        let drained = Instant::now() + PIPE_DRAIN_KILL_SWITCH;
        let output = format!(
            "stdout:\n{}\nstderr:\n{}",
            pipe_text_by(&stdout, drained),
            pipe_text_by(&stderr, drained)
        );
        assert_eq!(
            status.success(),
            !stalled,
            "stalled={stalled} {status}\n{output}"
        );
        if stalled {
            assert_eq!(
                output.matches("KELD_T1B_TERMINAL_ENTERED").count(),
                1,
                "{output}"
            );
        }
        wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
        // The zero count was observed first, so a stage deleted under a live family fails.
        assert!(!process_exists(host_pid), "stage deleted under the host");
        assert!(!process_exists(bun_pid), "stage deleted under Bun");
        wait_process_gone(cleanup_pid, Instant::now() + PRODUCT_DEADLINE);
        println!(
            "KELD_GH1188_AC19 stalled={stalled} host_pid={host_pid} bun_pid={bun_pid} sentinel_pid={cleanup_pid} stage_count=0"
        );
    }
}

/// The next generation's `HELLO`, before any `READY`.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn accept_hello(
    listener: &TcpListener,
    child: &mut std::process::Child,
) -> (std::io::BufReader<std::net::TcpStream>, u32) {
    let control = crate::support::control::accept_control_or_host_failure(
        listener,
        child,
        Instant::now() + PRODUCT_DEADLINE,
    );
    control
        .set_read_timeout(Some(crate::startup_kill_switch()))
        .expect("facade control read deadline");
    let mut reader = std::io::BufReader::new(control);
    let (pid, _) = crate::support::control::parse_hello(&read_control_line(&mut reader));
    (reader, pid)
}

#[test]
fn shipping_windows_cli_death_reaps_the_delegated_host_and_bun() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind lease control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind lease beacon");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let beacon = spawn_renderer_beacon(beacon_listener);
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><p id=exact>lease</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write lease renderer");
    let helper = prepare_keld_dev_helper(&fixture);
    let mut cli = Command::new(&helper)
        .arg("keld_dev_windows_helper")
        .arg("--exact")
        .arg("--nocapture")
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .env("KELD_TEST_WINDOWS_LEASE_CENSUS", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch leased keld dev helper");
    let cli_pid = cli.id();
    let host_pid =
        wait_for_child_process(&mut cli, "keld-host.exe", Instant::now() + PRODUCT_DEADLINE);
    let cleanup_pid = wait_for_cleanup_sentinel(&mut cli, Instant::now() + PRODUCT_DEADLINE);
    let (mut reader, _writer, bun_pid, _, lease_handle) =
        accept_ready_generation_with_lease(&control_listener, &mut cli);
    expect_renderer_beacon(beacon, "lease renderer beacon");
    let _window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    let controller_pipe = cli
        .stdout
        .as_ref()
        .expect("captured CLI stdout for File object type")
        .as_raw_handle()
        .cast();
    let handle_census = assert_dev_lease_handle_isolation(
        cli_pid,
        host_pid,
        bun_pid,
        controller_pipe,
        lease_handle,
    );

    cli.kill()
        .expect("kill only the terminal-facing CLI helper");
    let cli_status = wait_child(&mut cli, Instant::now() + PRODUCT_DEADLINE);
    assert!(!cli_status.success(), "killed CLI reported success");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    // The closed lease starts the host's ordered shutdown, including its UI end.
    wait_process_gone(host_pid, Instant::now() + ui_end_kill_switch());
    wait_process_gone(bun_pid, Instant::now() + PRODUCT_DEADLINE);
    wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
    wait_process_gone(cleanup_pid, Instant::now() + PRODUCT_DEADLINE);
    println!(
        "KELD_WINDOWS_STAGE_CLEANUP cli_pid={cli_pid} host_pid={host_pid} bun_pid={bun_pid} \
         sentinel_pid={cleanup_pid} stage_count=0 cli_handles={} host_handles={} bun_handles={} \
         lease_host_handle=0x{:x} lease_inheritable=false",
        handle_census.cli_count,
        handle_census.host_count,
        handle_census.bun_count,
        handle_census.lease_host_handle,
    );
}

#[test]
fn shipping_windows_host_death_reaps_bun_descendant_deletes_stage_and_relaunches() {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind host-death control");
    let control_port = control_listener
        .local_addr()
        .expect("host-death control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind host-death beacon");
    let beacon_port = beacon_listener
        .local_addr()
        .expect("host-death beacon address")
        .port();
    let beacon = spawn_renderer_beacon(beacon_listener);
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><p id=exact>host-death</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write host-death renderer");
    let helper = prepare_keld_dev_helper(&fixture);
    let mut cli = Command::new(&helper)
        .arg("keld_dev_windows_helper")
        .arg("--exact")
        .arg("--nocapture")
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .env("KELD_T4_JOB_DESCENDANT", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch host-death keld dev helper");
    let cli_pid = cli.id();
    let host_pid =
        wait_for_child_process(&mut cli, "keld-host.exe", Instant::now() + PRODUCT_DEADLINE);
    let cleanup_pid = wait_for_cleanup_sentinel(&mut cli, Instant::now() + PRODUCT_DEADLINE);
    let (reader, _writer, bun_pid, _, descendant_pid) =
        accept_ready_generation_with_descendant(&control_listener, &mut cli);
    expect_renderer_beacon(beacon, "host-death renderer beacon");
    let _window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);

    let host = open_process_for_wait(host_pid, true);
    let bun = open_process_for_wait(bun_pid, false);
    let descendant = open_process_for_wait(descendant_pid, false);
    terminate_test_process(&host);
    assert_process_signaled(&host, "no-flag host");
    assert_process_signaled(&bun, "Bun direct child");
    assert_process_signaled(&descendant, "Bun descendant");
    drop(host);
    drop(reader);

    let cli_status = wait_child(&mut cli, Instant::now() + PRODUCT_DEADLINE);
    assert!(!cli_status.success(), "host death became CLI success");
    wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
    wait_process_gone(cleanup_pid, Instant::now() + PRODUCT_DEADLINE);

    let relaunched = run_product_cycle(&fixture, "post-host-death");
    println!(
        "KELD_WINDOWS_HOST_DEATH cli_pid={cli_pid} host_pid={host_pid} bun_pid={bun_pid} \
         descendant_pid={descendant_pid} sentinel_pid={cleanup_pid} stage_count=0 \
         relaunch_host_pid={} relaunch_bun_pid={}",
        relaunched.host_pid, relaunched.bun_pid
    );
}
