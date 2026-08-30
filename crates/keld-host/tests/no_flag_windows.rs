//! Real-Windows KEL-96/T4 no-flag host acceptance.

#![cfg(windows)]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: process and OS observations are assertion oracles

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

const PRODUCT_TITLE: &str = "KEL96 T4 Windows Fixture";
const PRODUCT_DEADLINE: Duration = Duration::from_secs(20);

#[test]
fn keld_dev_windows_helper() {
    let Some(project) = std::env::var_os("KELD_T4_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(Path::new(&project)).expect("shipping Windows keld dev helper");
}

#[test]
fn windows_stage_is_current_user_protected_and_byte_consistent() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("KEL-96/T4 must stage the Windows no-flag host");

    assert_eq!(
        stage.host().file_name().and_then(|name| name.to_str()),
        Some("keld-host.exe")
    );
    assert_eq!(
        fs::read(stage.host()).expect("read staged host"),
        fs::read(env!("CARGO_BIN_EXE_keld-host")).expect("read source host"),
        "the staged executable must copy the exact already-built host bytes"
    );
    assert!(stage.root().join("keld.boot.json").is_file());
    assert!(stage.root().join("keld.permissions.jsonc").is_file());
    assert!(stage.root().join("src/main.ts").is_file());
    assert!(stage.root().join("index.html").is_file());

    let acl = acl_observation(stage.root());
    assert_eq!(acl["protected"], true, "stage DACL must reject inheritance");
    assert_eq!(acl["count"], 1, "stage DACL must contain one ACE: {acl}");
    assert_eq!(
        acl["sid"], acl["current"],
        "only TokenUser may access the stage"
    );
    assert_eq!(acl["rights"], "FullControl");
    assert_eq!(acl["kind"], "Allow");
    assert_eq!(acl["inherited"], false);
    assert_eq!(acl["inheritance"], "ContainerInherit, ObjectInherit");
    assert_eq!(acl["propagation"], "None");
}

#[test]
fn windows_stage_namespace_is_pinned_until_the_host_owner_releases_it() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage namespace-pinning fixture");
    let keld_root = fixture.project.join(".keld");
    let moved = fixture.project.join(".keld-moved");

    fs::rename(&keld_root, &moved)
        .expect_err("a retained no-share-delete handle must pin the staged pathname chain");
    assert!(stage.host().is_file(), "pinned host path disappeared");

    drop(stage);
    fs::rename(&keld_root, &moved).expect("releasing the stage must release namespace guards");
    fs::rename(&moved, &keld_root).expect("restore fixture namespace");
}

#[test]
fn windows_stage_rejects_a_dev_junction_before_writing_through_it() {
    let fixture = StageFixture::new();
    let external = tempfile::tempdir().expect("external junction target");
    fs::create_dir(fixture.project.join(".keld")).expect("create .keld parent");
    let junction = fixture.project.join(".keld/dev");
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "New-Item -ItemType Junction -Path $env:KELD_TEST_JUNCTION -Target $env:KELD_TEST_TARGET | Out-Null",
        ])
        .env("KELD_TEST_JUNCTION", &junction)
        .env("KELD_TEST_TARGET", external.path())
        .output()
        .expect("create dev junction");
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let error = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect_err("a reparse-point dev root must fail before staging");
    assert!(error.to_string().contains("launch namespace"), "{error}");
    assert_eq!(
        fs::read_dir(external.path())
            .expect("read external target")
            .filter_map(Result::ok)
            .count(),
        0,
        "staging wrote through the rejected junction"
    );
}

#[test]
fn windows_host_validates_the_staged_descriptor_before_platform_session_start() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage Windows no-flag host");
    fs::write(
        stage.root().join("keld.boot.json"),
        br#"{"schema":1,"name":"invalid","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"foreign":true}"#,
    )
    .expect("replace descriptor with an invalid closed-schema document");

    let output = Command::new(stage.host())
        .current_dir(stage.root())
        .output()
        .expect("launch staged Windows host");
    assert!(!output.status.success(), "invalid boot became success");
    let stderr = String::from_utf8(output.stderr).expect("host stderr is UTF-8");
    assert!(stderr.contains("KELD-CORE-035"), "{stderr}");
    assert!(stderr.contains("unknown field"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

#[test]
fn windows_host_rejects_an_added_stage_acl_principal() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage ACL-negative host");
    let script = r"
$acl = New-Object System.Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true, $false)
$current = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$ownerRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
  $current,
  [System.Security.AccessControl.FileSystemRights]::FullControl,
  [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
  [System.Security.AccessControl.PropagationFlags]::None,
  [System.Security.AccessControl.AccessControlType]::Allow)
$world = New-Object System.Security.Principal.SecurityIdentifier('S-1-1-0')
$worldRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
  $world,
  [System.Security.AccessControl.FileSystemRights]::ReadAndExecute,
  [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
  [System.Security.AccessControl.PropagationFlags]::None,
  [System.Security.AccessControl.AccessControlType]::Allow)
$acl.AddAccessRule($ownerRule)
$acl.AddAccessRule($worldRule)
[System.IO.Directory]::SetAccessControl($env:KELD_TEST_ACL_PATH, $acl)
";
    let mutation = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("KELD_TEST_ACL_PATH", stage.root())
        .output()
        .expect("add foreign stage ACE");
    assert!(
        mutation.status.success(),
        "ACL mutation failed: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );

    let output = Command::new(stage.host())
        .current_dir(stage.root())
        .output()
        .expect("launch ACL-negative host");
    assert!(!output.status.success(), "foreign stage ACE became success");
    let stderr = String::from_utf8(output.stderr).expect("ACL-negative stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-036"), "{stderr}");
    assert!(stderr.contains("expected one access rule"), "{stderr}");
}

#[test]
fn windows_no_flag_host_owns_window_two_calls_ordered_quit_and_relaunch() {
    let fixture = ProductFixture::new();
    let first = run_product_cycle(&fixture, "first");
    let second = run_product_cycle(&fixture, "second");

    assert_ne!(
        first.host_pid, second.host_pid,
        "relaunch must be a new host process"
    );
    assert_ne!(
        first.bun_pid, second.bun_pid,
        "relaunch must be a new Bun process"
    );
    assert_ne!(
        first.app_link, second.app_link,
        "relaunch must mint fresh authority"
    );
}

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
    let mut child = Command::new(stage.host())
        .current_dir(stage.root())
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
    let mut child = Command::new(stage.host())
        .current_dir(stage.root())
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
    let mut g2_reader = BufReader::new(g2);
    let g2_hello = read_control_line_or_host_failure(&mut g2_reader, &mut child, "g2 HELLO");
    let mut g2_fields = g2_hello.split_whitespace();
    assert_eq!(g2_fields.next(), Some("HELLO"), "{g2_hello}");
    let g2_pid = g2_fields
        .next()
        .expect("g2 pid")
        .parse::<u32>()
        .expect("numeric g2 pid");
    assert_eq!(
        read_control_line_or_host_failure(&mut g2_reader, &mut child, "g2 DESCENDANT"),
        "DESCENDANT 0"
    );

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

fn run_same_window_recovery(failure_command: &str) {
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind recovery control");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind recovery beacon");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html><title>{PRODUCT_TITLE}</title><p id=exact>{failure_command}</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write recovery renderer");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage recovery host");
    let mut child = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch recovery host");
    let host_pid = child.id();

    let (mut g1_reader, mut g1_writer, g1_pid, g1_link) =
        accept_ready_generation(&control_listener, &mut child);
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("initial renderer beacon");
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
    beacon.join().expect("recovery beacon thread");
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
    let mut child = Command::new(stage.host())
        .current_dir(stage.root())
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
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html><title>{PRODUCT_TITLE}</title><p id=exact>shipping</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
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
    let cli_pid = cli.id();
    let host_pid =
        wait_for_child_process(cli_pid, "keld-host.exe", Instant::now() + PRODUCT_DEADLINE);
    let (mut reader, mut writer, bun_pid, _) = accept_ready_generation(&control_listener, &mut cli);
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("shipping renderer beacon");
    let window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_eq!(window["title"], PRODUCT_TITLE);
    writer.write_all(b"QUIT\n").expect("shipping Quit");
    writer.flush().expect("flush shipping Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = wait_child(&mut cli, Instant::now() + PRODUCT_DEADLINE);
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
    beacon.join().expect("shipping beacon thread");
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
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html><title>{PRODUCT_TITLE}</title><p id=exact>lease</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch leased keld dev helper");
    let cli_pid = cli.id();
    let host_pid =
        wait_for_child_process(cli_pid, "keld-host.exe", Instant::now() + PRODUCT_DEADLINE);
    let (mut reader, _writer, bun_pid, _) = accept_ready_generation(&control_listener, &mut cli);
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("lease renderer beacon");
    let _window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);

    cli.kill()
        .expect("kill only the terminal-facing CLI helper");
    let cli_status = wait_child(&mut cli, Instant::now() + PRODUCT_DEADLINE);
    assert!(!cli_status.success(), "killed CLI reported success");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    wait_process_gone(host_pid, Instant::now() + PRODUCT_DEADLINE);
    wait_process_gone(bun_pid, Instant::now() + PRODUCT_DEADLINE);
    beacon.join().expect("lease beacon thread");
}

fn prepare_keld_dev_helper(fixture: &ProductFixture) -> std::path::PathBuf {
    let bin = fixture.root.path().join("bin");
    fs::create_dir(&bin).expect("helper bin directory");
    let helper = bin.join("keld-dev-helper.exe");
    fs::copy(
        std::env::current_exe().expect("current test executable"),
        &helper,
    )
    .expect("copy keld dev helper");
    fs::copy(env!("CARGO_BIN_EXE_keld-host"), bin.join("keld-host.exe"))
        .expect("copy sibling keld-host");
    helper
}

fn dev_stage_count(project: &Path) -> usize {
    fs::read_dir(project.join(".keld/dev"))
        .map_or(0, |entries| entries.filter_map(Result::ok).count())
}

fn accept_ready_generation(
    listener: &TcpListener,
    child: &mut Child,
) -> (BufReader<TcpStream>, TcpStream, u32, String) {
    let control =
        accept_control_or_host_failure(listener, child, Instant::now() + PRODUCT_DEADLINE);
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("generation control read deadline");
    let writer = control.try_clone().expect("generation control writer");
    let mut reader = BufReader::new(control);
    let hello = read_control_line_or_host_failure(&mut reader, child, "HELLO");
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let pid = fields
        .next()
        .expect("generation pid")
        .parse::<u32>()
        .expect("numeric generation pid");
    let link = fields.next().expect("generation app link").to_owned();
    assert!(fields.next().is_none(), "{hello}");
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, child, "DESCENDANT"),
        "DESCENDANT 0"
    );
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, child, "READY"),
        "READY"
    );
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, child, "ECHO1"),
        "ECHO1"
    );
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, child, "ECHO2"),
        "ECHO2"
    );
    (reader, writer, pid, link)
}

struct ProductEvidence {
    host_pid: u32,
    bun_pid: u32,
    app_link: String,
}

fn run_product_cycle(fixture: &ProductFixture, label: &str) -> ProductEvidence {
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind control listener");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind beacon listener");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let (beacon_tx, beacon_rx) = mpsc::channel();
    let beacon = thread::spawn(move || serve_renderer_beacon(&beacon_listener, &beacon_tx));
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html><title>{PRODUCT_TITLE}</title><p id=exact>{label}</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write renderer");
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage Windows product host");
    let mut child = Command::new(stage.host())
        .current_dir(stage.root())
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
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");

    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("WebView2 renderer requested the exact beacon");
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
    beacon.join().expect("beacon thread");

    ProductEvidence {
        host_pid,
        bun_pid,
        app_link,
    }
}

fn serve_renderer_beacon(listener: &TcpListener, observed: &mpsc::Sender<()>) {
    let (mut stream, _) = listener.accept().expect("accept renderer beacon");
    stream
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("beacon read deadline");
    let mut request = [0_u8; 2048];
    let read = stream.read(&mut request).expect("read renderer beacon");
    let request = String::from_utf8_lossy(&request[..read]);
    assert!(request.starts_with("GET /ready.png "), "{request}");
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .expect("reply renderer beacon");
    observed.send(()).expect("publish renderer beacon");
}

fn accept_control_or_host_failure(
    listener: &TcpListener,
    child: &mut Child,
    deadline: Instant,
) -> TcpStream {
    listener
        .set_nonblocking(true)
        .expect("nonblocking product control listener");
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("blocking product control stream");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("product control accept failed: {error}"),
        }
        if let Some(status) = child.try_wait().expect("observe early host exit") {
            let mut stdout = String::new();
            let mut stderr = String::new();
            child
                .stdout
                .take()
                .expect("captured host stdout")
                .read_to_string(&mut stdout)
                .expect("read host stdout");
            child
                .stderr
                .take()
                .expect("captured host stderr")
                .read_to_string(&mut stderr)
                .expect("read host stderr");
            panic!(
                "host exited before control bind: {status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
            );
        }
        assert!(
            Instant::now() < deadline,
            "product control accept timed out"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn read_control_line(reader: &mut BufReader<TcpStream>) -> String {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read control line");
    assert!(line.ends_with('\n'), "incomplete control line: {line:?}");
    line.pop();
    line
}

fn read_control_line_or_host_failure(
    reader: &mut BufReader<TcpStream>,
    child: &mut Child,
    label: &str,
) -> String {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(_) if line.ends_with('\n') => {
            line.pop();
            line
        }
        result => {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Some(status) = child.try_wait().expect("observe failed host") {
                    let mut stdout = String::new();
                    let mut stderr = String::new();
                    child
                        .stdout
                        .take()
                        .expect("captured failed host stdout")
                        .read_to_string(&mut stdout)
                        .expect("read failed host stdout");
                    child
                        .stderr
                        .take()
                        .expect("captured failed host stderr")
                        .read_to_string(&mut stderr)
                        .expect("read failed host stderr");
                    panic!(
                        "host exited while awaiting {label}: {status}; read={result:?}; line={line:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
                    );
                }
                assert!(
                    Instant::now() < deadline,
                    "control failed while awaiting {label}: read={result:?}; line={line:?}"
                );
                thread::park_timeout(Duration::from_millis(10));
            }
        }
    }
}

fn wait_child(child: &mut Child, deadline: Instant) -> ExitStatus {
    loop {
        if let Some(status) = child.try_wait().expect("observe host exit") {
            return status;
        }
        assert!(Instant::now() < deadline, "host exit timed out");
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn wait_for_host_window(pid: u32, deadline: Instant) -> Value {
    loop {
        let script = format!(
            "$p=Get-Process -Id {pid} -ErrorAction Stop; [pscustomobject]@{{handle=[uint64]$p.MainWindowHandle.ToInt64();title=$p.MainWindowTitle}} | ConvertTo-Json -Compress"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("query host window");
        if output.status.success() {
            let observation: Value =
                serde_json::from_slice(&output.stdout).expect("host window observation JSON");
            if observation["handle"]
                .as_u64()
                .is_some_and(|handle| handle != 0)
                && observation["title"] == PRODUCT_TITLE
            {
                return observation;
            }
        }
        assert!(
            Instant::now() < deadline,
            "host HWND/title observation timed out"
        );
        thread::park_timeout(Duration::from_millis(20));
    }
}

fn process_exists(pid: u32) -> bool {
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"),
        ])
        .status()
        .expect("query process")
        .success()
}

fn wait_process_gone(pid: u32, deadline: Instant) {
    while process_exists(pid) {
        assert!(Instant::now() < deadline, "process {pid} remained live");
        thread::park_timeout(Duration::from_millis(20));
    }
}

fn wait_for_child_process(parent_pid: u32, name: &str, deadline: Instant) -> u32 {
    loop {
        let script = format!(
            "$p=Get-CimInstance Win32_Process -Filter \"ParentProcessId={parent_pid}\" | Where-Object {{$_.Name -eq '{name}'}} | Select-Object -First 1 -ExpandProperty ProcessId; if ($p) {{$p}}"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("query delegated child process");
        if output.status.success()
            && let Ok(pid) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "delegated child `{name}` did not appear under {parent_pid}"
        );
        thread::park_timeout(Duration::from_millis(20));
    }
}

fn acl_observation(path: &Path) -> Value {
    let script = r"
$acl = Get-Acl -LiteralPath $env:KELD_TEST_ACL_PATH
$current = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$rules = @($acl.Access)
$sid = ''
$rights = ''
$kind = ''
$inherited = $false
$inheritance = ''
$propagation = ''
if ($rules.Count -eq 1) {
  $sid = $rules[0].IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value
  $rights = $rules[0].FileSystemRights.ToString()
  $kind = $rules[0].AccessControlType.ToString()
  $inherited = $rules[0].IsInherited
  $inheritance = $rules[0].InheritanceFlags.ToString()
  $propagation = $rules[0].PropagationFlags.ToString()
}
[pscustomobject]@{
  protected = $acl.AreAccessRulesProtected
  current = $current
  count = $rules.Count
  sid = $sid
  rights = $rights
  kind = $kind
  inherited = $inherited
  inheritance = $inheritance
  propagation = $propagation
} | ConvertTo-Json -Compress
";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("KELD_TEST_ACL_PATH", path)
        .output()
        .expect("query stage ACL with the Windows security API through PowerShell");
    assert!(
        output.status.success(),
        "ACL query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("PowerShell ACL observation is JSON")
}

struct StageFixture {
    _root: tempfile::TempDir,
    project: std::path::PathBuf,
}

impl StageFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("stage fixture root");
        let project = root.path().join("project");
        fs::create_dir_all(project.join("src")).expect("project src");
        fs::write(
            project.join("keld.config.ts"),
            "export default {\n  name: \"KEL96 T4 Fixture\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n} as const;\n",
        )
        .expect("project config");
        fs::write(project.join("src/main.ts"), "console.log('fixture');\n").expect("project entry");
        fs::write(project.join("index.html"), "<p id=exact>fixture</p>\n")
            .expect("project renderer");
        Self {
            _root: root,
            project,
        }
    }
}

struct ProductFixture {
    root: tempfile::TempDir,
    project: std::path::PathBuf,
}

impl ProductFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("product fixture root");
        let project = root.path().join("project");
        fs::create_dir_all(project.join("src")).expect("product project src");
        fs::write(
            project.join("keld.config.ts"),
            format!(
                "export default {{\n  name: \"{PRODUCT_TITLE}\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
            ),
        )
        .expect("product config");
        fs::write(
            project.join("src/main.ts"),
            format!(
                "{}{}",
                include_str!("../../../packages/@keld/electron/src/link.ts"),
                include_str!("fixtures/t1b_harness.ts")
            ),
        )
        .expect("product entry");
        fs::write(project.join("index.html"), "<!doctype html>\n").expect("product renderer");
        Self { root, project }
    }
}
