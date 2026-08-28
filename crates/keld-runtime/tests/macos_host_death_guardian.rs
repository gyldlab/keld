//! Real-macOS proof for the proposed host-death guardian contract.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles
#![allow(clippy::zombie_processes)] // the leader is killed with its descendant group; the controller proves both PIDs gone

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ROLE_ENV: &str = "KELD_TEST_MACOS_REAPER_ROLE";
const CONTROL_ENV: &str = "KELD_TEST_MACOS_REAPER_CONTROL";
const ROLE_TEST: &str = "macos_host_death_guardian_role";
const EVENT_DEADLINE: Duration = Duration::from_secs(10);
const PROCESS_GONE_DEADLINE: Duration = Duration::from_secs(5);

static NEXT_PATH_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn macos_host_death_guardian_role() {
    let Some(role) = env::var_os(ROLE_ENV) else {
        return;
    };
    let control = PathBuf::from(env::var_os(CONTROL_ENV).expect("control path"));
    match role.to_str().expect("UTF-8 role") {
        "host" => run_host(&control),
        "guardian" => run_guardian(&control),
        "leader" => run_leader(&control),
        "descendant" => run_descendant(&control),
        other => panic!("unknown macOS reaper role: {other}"),
    }
}

#[test]
fn guardian_reaps_the_enrolled_group_after_host_sigkill_and_allows_relaunch() {
    let fixture = Fixture::new();
    let listener = UnixListener::bind(&fixture.control).expect("bind controller socket");
    listener
        .set_nonblocking(true)
        .expect("make controller socket nonblocking");

    run_host_death_cycle(&listener, &fixture.control);
    run_host_death_cycle(&listener, &fixture.control);
}

fn run_host_death_cycle(listener: &UnixListener, control: &Path) {
    let host = spawn_role("host", control, None);
    let host_pid = host.id();
    let mut cleanup = CycleCleanup::new(host);
    let mut guardian_pid = None;
    let mut guardian_group_pid = None;
    let mut leader_event_pid = None;
    let mut descendant_pid = None;

    let ready_deadline = Instant::now() + EVENT_DEADLINE;
    while guardian_pid.is_none()
        || guardian_group_pid.is_none()
        || leader_event_pid.is_none()
        || descendant_pid.is_none()
    {
        let line = accept_line_before(listener, ready_deadline);
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("GUARDIAN") => {
                guardian_pid = Some(parse_pid(fields.next(), &line));
                let group = parse_pid(fields.next(), &line);
                guardian_group_pid = Some(group);
                cleanup.group = Some(group);
            }
            Some("LEADER") => leader_event_pid = Some(parse_pid(fields.next(), &line)),
            Some("DESCENDANT") => descendant_pid = Some(parse_pid(fields.next(), &line)),
            event => panic!("unexpected readiness event {event:?}: {line}"),
        }
    }

    let leader_pid = guardian_group_pid.expect("guardian registered group");
    assert_eq!(
        leader_event_pid,
        Some(leader_pid),
        "guardian and group leader must identify the same process group"
    );
    let descendant_pid = descendant_pid.expect("descendant ready");
    let guardian_pid = guardian_pid.expect("guardian ready");
    assert!(
        guardian_pid != host_pid && guardian_pid != leader_pid,
        "guardian, host, and Bun group leader must be different processes"
    );
    assert_eq!(
        process_group(leader_pid),
        leader_pid,
        "Bun leader must own its isolated process group"
    );
    assert_eq!(
        process_group(descendant_pid),
        leader_pid,
        "Bun descendants must remain enrolled in the leader's process group"
    );

    let mut host = cleanup.host.take().expect("live host child");
    host.kill().expect("SIGKILL host only");
    let host_status = host.wait().expect("wait for killed host");
    assert_eq!(
        host_status.signal(),
        Some(9),
        "controller must kill only the host with SIGKILL: {host_status:?}"
    );

    let reaped = accept_line_before(listener, Instant::now() + EVENT_DEADLINE);
    assert_eq!(reaped, format!("REAPED {leader_pid}"));
    await_process_gone(leader_pid);
    await_process_gone(descendant_pid);
    await_process_gone(guardian_pid);
    cleanup.group_gone = true;
}

fn run_host(control: &Path) {
    let mut guardian = Command::new(env::current_exe().expect("current test binary"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, "guardian")
        .env(CONTROL_ENV, control)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn guardian");

    let _host_liveness_writer = guardian.stdin.take().expect("guardian liveness writer");
    let status = guardian.wait().expect("wait for guardian");
    assert!(status.success(), "guardian failed: {status:?}");
}

fn run_guardian(control: &Path) {
    let mut leader = spawn_role("leader", control, Some(0));
    let leader_pid = leader.id();
    send_event(
        control,
        &format!("GUARDIAN {} {leader_pid}", std::process::id()),
    );

    let mut liveness = Vec::new();
    std::io::stdin()
        .read_to_end(&mut liveness)
        .expect("read host-liveness pipe");
    assert!(
        liveness.is_empty(),
        "liveness pipe carries no authority bytes"
    );

    kill_process_group(leader_pid);
    let leader_status = leader.wait().expect("reap Bun group leader");
    assert_eq!(
        leader_status.signal(),
        Some(9),
        "Bun group leader must die by guardian SIGKILL: {leader_status:?}"
    );

    send_event(control, &format!("REAPED {leader_pid}"));
}

fn run_leader(control: &Path) {
    let _descendant = spawn_role("descendant", control, None);
    send_event(control, &format!("LEADER {}", std::process::id()));
    thread::park();
}

fn run_descendant(control: &Path) {
    send_event(control, &format!("DESCENDANT {}", std::process::id()));
    thread::park();
}

fn spawn_role(role: &str, control: &Path, process_group: Option<i32>) -> Child {
    let mut command = Command::new(env::current_exe().expect("current test binary"));
    command
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, role)
        .env(CONTROL_ENV, control)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if let Some(group) = process_group {
        command.process_group(group);
    }
    command.spawn().expect("spawn role")
}

fn send_event(control: &Path, event: &str) {
    let mut stream = UnixStream::connect(control).expect("connect controller socket");
    stream.write_all(event.as_bytes()).expect("write event");
    stream.write_all(b"\n").expect("terminate event");
}

fn accept_line_before(listener: &UnixListener, deadline: Instant) -> String {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("normalize accepted controller stream");
                let mut line = String::new();
                BufReader::new(stream)
                    .read_line(&mut line)
                    .expect("read controller event");
                return line.trim_end().to_owned();
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "controller received no event before its deadline"
                );
                thread::yield_now();
            }
            Err(error) => panic!("accept controller event: {error}"),
        }
    }
}

fn parse_pid(field: Option<&str>, line: &str) -> u32 {
    field
        .unwrap_or_else(|| panic!("event omitted pid: {line}"))
        .parse()
        .unwrap_or_else(|error| panic!("event pid is invalid ({error}): {line}"))
}

fn kill_process_group(group: u32) {
    let status = Command::new("/bin/kill")
        .args(["-KILL", &format!("-{group}")])
        .status()
        .expect("invoke kill(2) process-group frontend");
    assert!(status.success(), "kill process group {group}: {status:?}");
}

fn process_exists(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn process_group(pid: u32) -> u32 {
    let output = Command::new("/bin/ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .expect("inspect process group");
    assert!(output.status.success(), "inspect process group for {pid}");
    String::from_utf8(output.stdout)
        .expect("process group output is UTF-8")
        .trim()
        .parse()
        .expect("process group output is numeric")
}

fn await_process_gone(pid: u32) {
    let deadline = Instant::now() + PROCESS_GONE_DEADLINE;
    while process_exists(pid) {
        assert!(
            Instant::now() < deadline,
            "process {pid} survived the guardian's bounded reap deadline"
        );
        thread::yield_now();
    }
}

struct CycleCleanup {
    host: Option<Child>,
    group: Option<u32>,
    group_gone: bool,
}

impl CycleCleanup {
    fn new(host: Child) -> Self {
        Self {
            host: Some(host),
            group: None,
            group_gone: false,
        }
    }
}

impl Drop for CycleCleanup {
    fn drop(&mut self) {
        if !self.group_gone
            && let Some(group) = self.group
        {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{group}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if let Some(host) = self.host.as_mut() {
            let _ = host.kill();
            let _ = host.wait();
        }
    }
}

struct Fixture {
    root: PathBuf,
    control: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let epoch_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after Unix epoch")
            .as_nanos();
        let root = PathBuf::from(format!(
            "/tmp/k78-{}-{epoch_nanos:x}-{id}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create owner fixture directory");
        let mut permissions = fs::metadata(&root).expect("fixture metadata").permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&root, permissions).expect("set fixture permissions");
        let control = root.join("control.sock");
        Self { root, control }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.control);
        let _ = fs::remove_dir(&self.root);
    }
}
