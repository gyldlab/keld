//! Real-Linux KEL-96/T4 no-flag host acceptance.

#![cfg(target_os = "linux")]
#![allow(clippy::expect_used, clippy::panic)] // process and filesystem observations are assertion oracles

use std::fs;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const PRODUCT_TITLE: &str = "KEL96 T4 Linux Fixture";
const PRODUCT_DEADLINE: Duration = Duration::from_secs(20);

#[test]
fn keld_dev_linux_helper() {
    let Some(project) = std::env::var_os("KELD_T4_HELPER_PROJECT") else {
        return;
    };
    keld_cli::dev::run_dev(Path::new(&project)).expect("shipping Linux keld dev helper");
}

#[test]
fn linux_stage_is_owner_private_new_inode_and_byte_consistent() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("KEL-96/T4 must stage the Linux no-flag host");

    assert_eq!(
        stage.host().file_name().and_then(|name| name.to_str()),
        Some("keld-host")
    );
    assert_eq!(
        fs::metadata(stage.root())
            .expect("stage metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o700
    );
    let source = fs::metadata(env!("CARGO_BIN_EXE_keld-host")).expect("source host metadata");
    let copied = fs::metadata(stage.host()).expect("staged host metadata");
    assert_ne!(
        (source.dev(), source.ino()),
        (copied.dev(), copied.ino()),
        "the stage must contain a copy, never a hard link"
    );
    assert_eq!(
        fs::read(stage.host()).expect("read staged host"),
        fs::read(env!("CARGO_BIN_EXE_keld-host")).expect("read source host")
    );
    assert_ne!(copied.permissions().mode() & 0o100, 0);
    assert_eq!(copied.permissions().mode() & 0o222, 0);
}

#[test]
fn linux_invalid_boot_and_lease_fail_before_app_resources() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage invalid-boot host");
    fs::set_permissions(
        stage.root().join("keld.boot.json"),
        fs::Permissions::from_mode(0o600),
    )
    .expect("make descriptor mutable for negative fixture");
    fs::write(
        stage.root().join("keld.boot.json"),
        br#"{"schema":1,"name":"invalid","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"foreign":true}"#,
    )
    .expect("write invalid descriptor");
    let invalid_boot = Command::new(stage.host())
        .current_dir(stage.root())
        .output()
        .expect("launch invalid boot");
    assert!(!invalid_boot.status.success());
    let stderr = String::from_utf8(invalid_boot.stderr).expect("invalid boot stderr");
    assert!(stderr.contains("KELD-CORE-035"), "{stderr}");
    assert!(stderr.contains("listener=0 child=0 window=0"), "{stderr}");

    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage invalid-lease host");
    let invalid_lease = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::null())
        .output()
        .expect("launch invalid lease");
    assert!(!invalid_lease.status.success());
    let stderr = String::from_utf8(invalid_lease.stderr).expect("invalid lease stderr");
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(
        stderr.contains("requires the CLI-owned pipe reader"),
        "{stderr}"
    );
    assert!(stderr.contains("listener=0 child=0 window=0"), "{stderr}");
}

#[test]
fn linux_no_flag_host_owns_window_two_calls_ordered_quit_and_relaunch() {
    let fixture = ProductFixture::new();
    let first = run_product_cycle(&fixture, "first");
    let second = run_product_cycle(&fixture, "second");

    assert_ne!(first.host_pid, second.host_pid, "relaunch needs a new host");
    assert_ne!(first.bun_pid, second.bun_pid, "relaunch needs a new Bun");
    assert_ne!(first.app_link, second.app_link, "authority must be fresh");
    eprintln!(
        "KEL96_T4_LINUX_EVIDENCE session={} display={} first_host={} first_bun={} second_host={} second_bun={}",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| String::from("unknown")),
        std::env::var("WAYLAND_DISPLAY")
            .or_else(|_| std::env::var("DISPLAY"))
            .unwrap_or_else(|_| String::from("unavailable")),
        first.host_pid,
        first.bun_pid,
        second.host_pid,
        second.bun_pid,
    );
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
            "<!doctype html><title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
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
        .env("KELD_T4_SKIP_DESCENDANT", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch recovery host");
    let host_pid = host.id();
    let (mut g1_reader, mut g1_writer, g1_pid, g1_link) = accept_generation(&listener, &mut host);
    beacon_rx
        .recv_timeout(PRODUCT_DEADLINE)
        .expect("initial renderer beacon");
    expect_ready_and_echoes(&mut g1_reader);
    g1_writer
        .write_all(b"CRASH\n")
        .expect("crash generation one");
    g1_writer.flush().expect("flush generation-one crash");
    drop((g1_reader, g1_writer));

    let (mut g2_reader, mut g2_writer, g2_pid, g2_link) = accept_generation(&listener, &mut host);
    expect_ready_and_echoes(&mut g2_reader);
    assert_ne!(g1_pid, g2_pid, "recovery must spawn a fresh Bun process");
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
    wait_process_gone(g1_pid, Instant::now() + PRODUCT_DEADLINE);
    wait_process_gone(g2_pid, Instant::now() + PRODUCT_DEADLINE);
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
            "<!doctype html><title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("dev renderer");
    let mut cli = Command::new(&helper)
        .args(["--exact", "keld_dev_linux_helper", "--nocapture"])
        .current_dir(&fixture.project)
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env("KELD_T1B_CONTROL", &control_path)
        .env("KELD_T4_SKIP_DESCENDANT", "1")
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
    let bun_pid = fields
        .next()
        .expect("dev Bun pid")
        .parse::<u32>()
        .expect("numeric dev Bun pid");
    let host_pid = parent_pid(bun_pid);
    assert_eq!(
        parent_pid(host_pid),
        cli_pid,
        "CLI must launch only the host"
    );
    assert_ne!(host_pid, cli_pid, "CLI cannot own the Bun process");
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
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
    wait_process_gone(host_pid, Instant::now() + PRODUCT_DEADLINE);
    wait_process_gone(bun_pid, Instant::now() + PRODUCT_DEADLINE);
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
            "<!doctype html><title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("death renderer");
    let mut cli = Command::new(&helper)
        .args(["--exact", "keld_dev_linux_helper", "--nocapture"])
        .current_dir(&fixture.project)
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env("KELD_T1B_CONTROL", &control_path)
        .env("KELD_T4_SKIP_DESCENDANT", "1")
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
    let bun_pid = fields
        .next()
        .expect("death Bun pid")
        .parse::<u32>()
        .expect("numeric death Bun pid");
    let host_pid = parent_pid(bun_pid);
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
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
    wait_process_gone(host_pid, Instant::now() + PRODUCT_DEADLINE);
    wait_process_gone(bun_pid, Instant::now() + PRODUCT_DEADLINE);
    wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
    beacon.join().expect("death beacon thread");
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
            "export default { name: \"Linux no-flag\", entry: \"src/main.ts\", renderer: \"index.html\" } as const;\n",
        )
        .expect("project config");
        fs::write(project.join("src/main.ts"), "console.log('linux');\n").expect("entry");
        fs::write(
            project.join("index.html"),
            "<!doctype html><h1>Linux</h1>\n",
        )
        .expect("renderer");
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
        fs::write(project.join("index.html"), "<!doctype html>\n").expect("renderer");
        Self { root, project }
    }
}

fn prepare_keld_dev_helper(fixture: &ProductFixture) -> std::path::PathBuf {
    let helper_dir = fixture.root.path().join("bin");
    fs::create_dir(&helper_dir).expect("helper directory");
    let helper = helper_dir.join("keld-dev-helper");
    fs::copy(std::env::current_exe().expect("test executable"), &helper).expect("copy helper");
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).expect("helper mode");
    let developer_host = helper_dir.join("keld-host");
    fs::copy(env!("CARGO_BIN_EXE_keld-host"), &developer_host).expect("copy sibling host");
    fs::set_permissions(&developer_host, fs::Permissions::from_mode(0o500)).expect("host mode");
    helper
}

struct ProductEvidence {
    host_pid: u32,
    bun_pid: u32,
    app_link: String,
}

fn run_product_cycle(fixture: &ProductFixture, label: &str) -> ProductEvidence {
    let control_path = fixture.project.join(format!("control-{label}.sock"));
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
            "<!doctype html><title>{PRODUCT_TITLE}</title><p id=exact>{label}</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
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
        // Descendant ownership is the separate KEL-78 Linux artifact. This
        // KEL-96 row proves only its specified orderly direct-child contract.
        .env("KELD_T4_SKIP_DESCENDANT", "1")
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
    let bun_pid = fields
        .next()
        .expect("Bun pid")
        .parse::<u32>()
        .expect("numeric Bun pid");
    let app_link = fields.next().expect("app link").to_owned();
    assert!(fields.next().is_none(), "{hello}");
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
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
    wait_process_gone(bun_pid, Instant::now() + PRODUCT_DEADLINE);
    beacon.join().expect("renderer beacon thread");
    drop(stage);

    ProductEvidence {
        host_pid,
        bun_pid,
        app_link,
    }
}

fn accept_generation(
    listener: &UnixListener,
    host: &mut Child,
) -> (
    BufReader<std::os::unix::net::UnixStream>,
    std::os::unix::net::UnixStream,
    u32,
    String,
) {
    let control = accept_control_or_host_failure(listener, host, Instant::now() + PRODUCT_DEADLINE);
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("generation control deadline");
    let writer = control.try_clone().expect("generation control writer");
    let mut reader = BufReader::new(control);
    let hello = read_control_line(&mut reader);
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let pid = fields
        .next()
        .expect("generation pid")
        .parse::<u32>()
        .expect("numeric generation pid");
    let link = fields.next().expect("generation app link").to_owned();
    assert!(fields.next().is_none(), "{hello}");
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
    (reader, writer, pid, link)
}

fn expect_ready_and_echoes(reader: &mut BufReader<std::os::unix::net::UnixStream>) {
    assert_eq!(read_control_line(reader), "READY");
    assert_eq!(read_control_line(reader), "ECHO1");
    assert_eq!(read_control_line(reader), "ECHO2");
}

fn serve_renderer_beacon(listener: &TcpListener, observed: &mpsc::Sender<()>) {
    let (mut stream, _) = listener.accept().expect("accept renderer beacon");
    stream
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("beacon deadline");
    let mut request = [0_u8; 2048];
    let read = stream.read(&mut request).expect("read renderer beacon");
    assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /ready.png "));
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .expect("reply renderer beacon");
    observed.send(()).expect("publish renderer beacon");
}

fn accept_control_or_host_failure(
    listener: &UnixListener,
    child: &mut Child,
    deadline: Instant,
) -> std::os::unix::net::UnixStream {
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("control accept failed: {error}"),
        }
        if let Some(status) = child.try_wait().expect("observe host") {
            let mut stderr = String::new();
            child
                .stderr
                .take()
                .expect("host stderr")
                .read_to_string(&mut stderr)
                .expect("read host stderr");
            panic!("host exited before control bind: {status}: {stderr}");
        }
        assert!(Instant::now() < deadline, "control accept timed out");
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn read_control_line(reader: &mut BufReader<std::os::unix::net::UnixStream>) -> String {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read control line");
    assert!(line.ends_with('\n'), "incomplete control line: {line:?}");
    line.pop();
    line
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

fn wait_child_output(mut child: Child, deadline: Instant) -> std::process::Output {
    let status = wait_child(&mut child, deadline);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .expect("captured stdout")
        .read_to_end(&mut stdout)
        .expect("read stdout");
    child
        .stderr
        .take()
        .expect("captured stderr")
        .read_to_end(&mut stderr)
        .expect("read stderr");
    std::process::Output {
        status,
        stdout,
        stderr,
    }
}

fn parent_pid(pid: u32) -> u32 {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).expect("process status");
    status
        .lines()
        .find_map(|line| line.strip_prefix("PPid:\t"))
        .expect("PPid line")
        .trim()
        .parse()
        .expect("numeric parent pid")
}

fn dev_stage_count(project: &Path) -> usize {
    fs::read_dir(project.join(".keld/dev"))
        .map_or(0, |entries| entries.filter_map(Result::ok).count())
}

fn wait_for_dev_stage_count(project: &Path, expected: usize, deadline: Instant) {
    loop {
        let observed = dev_stage_count(project);
        if observed == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected} dev stages, observed {observed}"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn wait_process_gone(pid: u32, deadline: Instant) {
    while Path::new(&format!("/proc/{pid}")).exists() {
        assert!(
            Instant::now() < deadline,
            "process {pid} survived host exit"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}
