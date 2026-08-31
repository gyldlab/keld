//! Real-macOS proof for the proposed host-death guardian contract.
#![cfg(target_os = "macos")]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles
#![allow(clippy::zombie_processes)] // the leader is killed with its descendant group; the controller proves both PIDs gone

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use keld_ipc::link::handshake_client;
use keld_ipc::{BootstrapListener, parse_app_link};
use keld_runtime::macos_guardian;

const ROLE_ENV: &str = "KELD_TEST_MACOS_REAPER_ROLE";
const CONTROL_ENV: &str = "KELD_TEST_MACOS_REAPER_CONTROL";
const APP_LINK_ENV: &str = "KELD_TEST_MACOS_REAPER_APP_LINK";
const APP_LINK_LEAK_ENV: &str = "KELD_TEST_MACOS_REAPER_APP_LINK_LEAK";
const EOF_WITNESS_ENV: &str = "KELD_TEST_MACOS_REAPER_EOF_WITNESS";
const REGISTERED_LINK_ENV: &str = "KELD_TEST_MACOS_REAPER_REGISTERED_LINK";
const TEST_EXE_ENV: &str = "KELD_TEST_MACOS_REAPER_EXE";
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
        "descendant" => run_descendant(&control),
        "peer" => run_link_peer(&control),
        "leaker" => thread::park(),
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

    run_host_death_cycle(&listener, &fixture.control, false);
    run_host_death_cycle(&listener, &fixture.control, false);
}

#[test]
fn app_link_eof_witness_detects_a_leaked_server_descriptor() {
    let fixture = Fixture::new();
    let listener = UnixListener::bind(&fixture.control).expect("bind controller socket");
    listener
        .set_nonblocking(true)
        .expect("make controller socket nonblocking");

    run_host_death_cycle(&listener, &fixture.control, true);
}

#[test]
fn guardian_death_is_fatal_to_the_live_host_and_leaves_no_group() {
    if env::var_os(ROLE_ENV).is_some() {
        return;
    }
    let fixture = Fixture::new();
    let listener = UnixListener::bind(&fixture.control).expect("bind controller socket");
    listener
        .set_nonblocking(true)
        .expect("make controller socket nonblocking");
    let cycle_started = Instant::now();
    let host = spawn_host(&fixture.control, false);
    let host_pid = host.id();
    let mut cleanup = CycleCleanup::new(host);
    let ready = await_ready(
        &listener,
        &fixture.control,
        host_pid,
        cycle_started,
        &mut cleanup,
        None,
        false,
    );

    kill_pid(ready.guardian_pid);
    let deadline = Instant::now() + EVENT_DEADLINE;
    let mut fatal = false;
    let mut link_eof = false;
    while !fatal || !link_eof {
        let line = accept_line_before(&listener, deadline);
        if line == "LINK_EOF" {
            link_eof = true;
        } else if line.starts_with(&format!("FATAL {} KELD-RUNTIME-013:", ready.leader_pid)) {
            fatal = true;
        } else {
            panic!("unexpected guardian-death event: {line}");
        }
    }

    let mut host = cleanup.host.take().expect("live host child");
    let host_status = host.wait().expect("wait fatal-session host");
    assert!(
        host_status.success(),
        "host failed to surface guardian death: {host_status:?}"
    );
    let mut peer = cleanup.peer.take().expect("live app-link peer");
    let peer_status = peer.wait().expect("wait revoked app-link peer");
    assert!(
        peer_status.success(),
        "app-link peer failed: {peer_status:?}"
    );
    assert!(
        !ready.session_dir.exists(),
        "fatal guardian exit must leave no stale app-link locator"
    );
    assert!(
        !ready.registered_link.exists(),
        "fatal guardian exit must revoke the registered socket"
    );
    await_process_gone(ready.leader_pid);
    await_process_gone(ready.descendant_pid);
    await_process_gone(ready.guardian_pid);
    cleanup.group_gone = true;
}

fn run_host_death_cycle(listener: &UnixListener, control: &Path, leak_app_link: bool) {
    let mut eof_witness = EofWitness::bind(control);
    let cycle_started = Instant::now();
    let host = spawn_host(control, leak_app_link);
    let host_pid = host.id();
    let mut cleanup = CycleCleanup::new(host);
    let ready = await_ready(
        listener,
        control,
        host_pid,
        cycle_started,
        &mut cleanup,
        Some(eof_witness.path()),
        leak_app_link,
    );
    eof_witness.accept_before(Instant::now() + EVENT_DEADLINE);

    let mut host = cleanup.host.take().expect("live host child");
    host.kill().expect("SIGKILL host only");
    let host_status = host.wait().expect("wait for killed host");
    assert_eq!(
        host_status.signal(),
        Some(9),
        "controller must kill only the host with SIGKILL: {host_status:?}"
    );

    await_cleanup(
        listener,
        &ready,
        &mut cleanup,
        &mut eof_witness,
        leak_app_link,
    );
    await_process_gone(ready.leader_pid);
    await_process_gone(ready.descendant_pid);
    await_process_gone(ready.guardian_pid);
    cleanup.group_gone = true;
}

struct ReadyState {
    guardian_pid: u32,
    leader_pid: u32,
    descendant_pid: u32,
    endpoint: PathBuf,
    session_dir: PathBuf,
    registered_link: PathBuf,
}

fn await_ready(
    listener: &UnixListener,
    control: &Path,
    host_pid: u32,
    cycle_started: Instant,
    cleanup: &mut CycleCleanup,
    eof_witness: Option<&Path>,
    expect_leaker: bool,
) -> ReadyState {
    let mut guardian_pid = None;
    let mut guardian_group_pid = None;
    let mut leader_event_pid = None;
    let mut descendant_pid = None;
    let mut app_link = None;
    let mut registered_link = None;
    let mut link_bound = false;
    let mut peer_bound = false;
    let mut guardian_ready_after = None;

    let ready_deadline = Instant::now() + EVENT_DEADLINE;
    while guardian_pid.is_none()
        || guardian_group_pid.is_none()
        || leader_event_pid.is_none()
        || descendant_pid.is_none()
        || app_link.is_none()
        || registered_link.is_none()
        || !link_bound
        || !peer_bound
        || (expect_leaker && cleanup.leaker.is_none())
    {
        let line = accept_line_before(listener, ready_deadline);
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("GUARDIAN") => {
                guardian_pid = Some(parse_pid(fields.next(), &line));
                let group = parse_pid(fields.next(), &line);
                guardian_group_pid = Some(group);
                guardian_ready_after = Some(cycle_started.elapsed());
                cleanup.group = Some(group);
            }
            Some("LEADER") => leader_event_pid = Some(parse_pid(fields.next(), &line)),
            Some("DESCENDANT") => descendant_pid = Some(parse_pid(fields.next(), &line)),
            Some("APP_LINK") => {
                let app_link_value = fields
                    .next()
                    .unwrap_or_else(|| panic!("APP_LINK event omitted value: {line}"))
                    .to_owned();
                cleanup.peer = Some(spawn_link_peer(control, &app_link_value, eof_witness));
                app_link = Some(app_link_value);
            }
            Some("REGISTERED_LINK") => {
                registered_link =
                    Some(PathBuf::from(fields.next().unwrap_or_else(|| {
                        panic!("REGISTERED_LINK event omitted path: {line}")
                    })));
            }
            Some("LINK_BOUND") => link_bound = true,
            Some("PEER_BOUND") => peer_bound = true,
            Some("LEAKER") => cleanup.leaker = Some(parse_pid(fields.next(), &line)),
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
    let app_link = app_link.expect("app link ready");
    let registered_link = registered_link.expect("registered link ready");
    let (endpoint, _) = parse_app_link(&app_link).expect("parse controller app link");
    let endpoint = PathBuf::from(endpoint);
    let session_dir = endpoint
        .parent()
        .expect("app-link endpoint has session directory")
        .to_path_buf();
    let ready = ReadyState {
        guardian_pid,
        leader_pid,
        descendant_pid,
        endpoint,
        session_dir,
        registered_link,
    };
    verify_ready_ownership(host_pid, &ready, guardian_ready_after);
    ready
}

fn verify_ready_ownership(
    host_pid: u32,
    ready: &ReadyState,
    guardian_ready_after: Option<Duration>,
) {
    assert!(
        fs::metadata(&ready.registered_link)
            .expect("registered link metadata")
            .file_type()
            .is_socket(),
        "registered link must be a real Unix socket before host death"
    );
    assert!(
        ready.guardian_pid != host_pid && ready.guardian_pid != ready.leader_pid,
        "guardian, host, and Bun group leader must be different processes"
    );
    assert_eq!(
        process_group(ready.leader_pid),
        ready.leader_pid,
        "Bun leader must own its isolated process group"
    );
    assert_eq!(
        process_group(ready.descendant_pid),
        ready.leader_pid,
        "Bun descendants must remain enrolled in the leader's process group"
    );
    eprintln!(
        "KELD_GUARDIAN_EVIDENCE registration_us={} guardian_rss_kib={} leader_rss_kib={}",
        guardian_ready_after
            .expect("guardian registration timing")
            .as_micros(),
        process_rss_kib(ready.guardian_pid),
        process_rss_kib(ready.leader_pid)
    );
}

fn await_cleanup(
    listener: &UnixListener,
    ready: &ReadyState,
    cleanup: &mut CycleCleanup,
    eof_witness: &mut EofWitness,
    leak_app_link: bool,
) {
    let cleanup_deadline = Instant::now() + EVENT_DEADLINE;
    let mut guardian_revoked = false;
    let mut reaped = false;
    while !guardian_revoked || !reaped {
        let line = accept_line_before(listener, cleanup_deadline);
        match line.as_str() {
            "GUARDIAN_REVOKED" => {
                assert!(
                    !ready.registered_link.exists(),
                    "guardian revocation must unlink the registered socket"
                );
                assert!(
                    !ready
                        .registered_link
                        .parent()
                        .expect("registered socket has owner directory")
                        .exists(),
                    "guardian revocation must remove the registered owner directory"
                );
                guardian_revoked = true;
            }
            "LINK_EOF" => {
                panic!("peer reported EOF before the controller released its delayed report")
            }
            value if value == format!("REAPED {}", ready.leader_pid) => {
                assert!(
                    guardian_revoked,
                    "registered-resource revocation must precede reap completion"
                );
                reaped = true;
            }
            other => panic!("unexpected cleanup event: {other}"),
        }
    }
    if leak_app_link {
        // REAPED is the synchronization boundary: the guardian finished while
        // the independent leaker still owns a server descriptor, so a real
        // peer EOF cannot already be present.
        eof_witness.assert_no_eof_observed();
        let leaker = cleanup
            .leaker
            .take()
            .expect("live app-link descriptor leaker");
        kill_pid(leaker);
        await_process_gone(leaker);
    }
    // Actual socket EOF is proved on this persistent channel. The separate
    // LINK_EOF controller connection is deliberately released only after
    // REAPED, proving its arrival order is not the cleanup oracle.
    eof_witness.await_eof_observed();
    eof_witness.release_link_eof_report();
    let link_eof_deadline = Instant::now() + EVENT_DEADLINE;
    assert_eq!(
        accept_line_before(listener, link_eof_deadline),
        "LINK_EOF",
        "peer must report its witnessed EOF after the controller releases it"
    );
    let stale = UnixStream::connect(&ready.endpoint).expect_err("revoked locator must stay closed");
    assert!(
        matches!(
            stale.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        ),
        "unexpected stale locator error: {stale}"
    );
    let registered_stale = UnixStream::connect(&ready.registered_link)
        .expect_err("revoked registered socket must stay closed");
    assert_eq!(
        registered_stale.kind(),
        std::io::ErrorKind::NotFound,
        "registered locator must be unlinked, not merely unlistened"
    );
    let mut peer = cleanup.peer.take().expect("live app-link peer");
    let peer_status = peer.wait().expect("wait revoked app-link peer");
    assert!(
        peer_status.success(),
        "app-link peer failed: {peer_status:?}"
    );
}

fn run_host(control: &Path) {
    let bootstrap = BootstrapListener::bind().expect("bind real app-link bootstrap");
    let app_link = bootstrap.app_link();
    send_event(control, &format!("APP_LINK {app_link}"));

    let registered_dir = control.with_extension(format!("registered-{}", std::process::id()));
    fs::create_dir(&registered_dir).expect("create registered link directory");
    let mut registered_permissions = fs::metadata(&registered_dir)
        .expect("registered directory metadata")
        .permissions();
    registered_permissions.set_mode(0o700);
    fs::set_permissions(&registered_dir, registered_permissions)
        .expect("set registered directory permissions");
    let registered_link = registered_dir.join("grant.sock");
    let registered_listener =
        UnixListener::bind(&registered_link).expect("bind actual registered link");
    send_event(
        control,
        &format!("REGISTERED_LINK {}", registered_link.display()),
    );

    let mut guardian_command = Command::new(env::current_exe().expect("current test binary"));
    guardian_command
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, "guardian")
        .env(CONTROL_ENV, control)
        .env(REGISTERED_LINK_ENV, &registered_link)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let pending = macos_guardian::GuardianBootstrap::spawn(guardian_command)
        .expect("spawn production guardian bootstrap");
    let guardian_pid = pending.guardian_pid().expect("pending guardian pid");
    let mut guardian = pending
        .register_until(Instant::now() + EVENT_DEADLINE)
        .expect("authenticate production guardian registration");
    let group_pid = guardian.group_pid().expect("registered Bun group");
    send_event(control, &format!("GUARDIAN {guardian_pid} {group_pid}"));
    let authenticated_stream = bootstrap
        .accept_authenticated()
        .expect("accept real app-link peer")
        .expect("peer authenticates before host death");
    let _leaker = env::var_os(APP_LINK_LEAK_ENV).map(|_| {
        let leaked: OwnedFd = authenticated_stream
            .try_clone()
            .expect("clone accepted app-link descriptor")
            .into();
        let mut command = role_command("leaker", control);
        command.stdin(Stdio::from(leaked));
        let child = command.spawn().expect("spawn app-link descriptor leaker");
        send_event(control, &format!("LEAKER {}", child.id()));
        child
    });
    send_event(control, "LINK_BOUND");
    match guardian.wait_fatal() {
        Err(error @ keld_runtime::RuntimeError::GuardianExited { .. }) => {
            drop(authenticated_stream);
            drop(registered_listener);
            revoke_registered_link(&registered_link).expect("host fatal-path link revocation");
            send_event(control, &format!("FATAL {group_pid} {error}"));
        }
        Err(error) => panic!("guardian watcher failed: {error}"),
        Ok(()) => panic!("guardian watcher returned without a fatal event"),
    }
}

fn run_guardian(control: &Path) {
    let control_for_revoke = control.to_path_buf();
    let registered_link =
        PathBuf::from(env::var_os(REGISTERED_LINK_ENV).expect("registered link path"));
    let command = bun_leader_command(control);
    let report = macos_guardian::run(command, std::io::stdin().lock(), move || {
        revoke_registered_link(&registered_link)?;
        try_send_event(&control_for_revoke, "GUARDIAN_REVOKED")
    })
    .expect("run production guardian API");
    let leader_pid = report.leader_pid();
    let leader_status = report.leader_status();
    assert_eq!(
        leader_status.signal(),
        Some(9),
        "Bun group leader must die by guardian SIGKILL: {leader_status:?}"
    );

    send_event(control, &format!("REAPED {leader_pid}"));
}

fn run_descendant(control: &Path) {
    let pid = std::process::id();
    send_event(control, &format!("LEADER {}", parent_process(pid)));
    send_event(control, &format!("DESCENDANT {pid}"));
    thread::park();
}

fn run_link_peer(control: &Path) {
    let mut eof_witness = env::var_os(EOF_WITNESS_ENV).map(|path| {
        UnixStream::connect(path).expect("connect EOF witness channel before app-link use")
    });
    let link = env::var(APP_LINK_ENV).expect("peer app link");
    let (endpoint, token) = parse_app_link(&link).expect("parse peer app link");
    let mut stream = UnixStream::connect(endpoint).expect("connect real app link");
    handshake_client(&mut stream, &token).expect("authenticate real app link");
    send_event(control, "PEER_BOUND");

    let mut unexpected = Vec::new();
    stream
        .read_to_end(&mut unexpected)
        .expect("observe app-link revocation EOF");
    assert!(unexpected.is_empty(), "revoked app link emitted no data");
    if let Some(witness) = eof_witness.as_mut() {
        witness
            .write_all(b"EOF_OBSERVED\n")
            .expect("report independently witnessed EOF");
        let mut release = [0_u8; 1];
        witness
            .read_exact(&mut release)
            .expect("wait for controller report release");
        assert_eq!(release, [b'R'], "unexpected EOF-report release byte");
    }
    send_event(control, "LINK_EOF");
}

fn role_command(role: &str, control: &Path) -> Command {
    let mut command = Command::new(env::current_exe().expect("current test binary"));
    command
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, role)
        .env(CONTROL_ENV, control)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    command
}

fn bun_leader_command(control: &Path) -> Command {
    const SCRIPT: &str = r#"
const child = Bun.spawn(
  [Bun.env.KELD_TEST_MACOS_REAPER_EXE, "--exact", "macos_host_death_guardian_role", "--nocapture"],
  {
    env: { ...Bun.env, KELD_TEST_MACOS_REAPER_ROLE: "descendant" },
    stdin: "ignore",
    stdout: "ignore",
    stderr: "inherit",
  },
);
await child.exited;
process.exit(child.exitCode ?? 1);
"#;
    let mut command = Command::new("bun");
    command
        .args(["-e", SCRIPT])
        .env(CONTROL_ENV, control)
        .env_remove(REGISTERED_LINK_ENV)
        .env(
            TEST_EXE_ENV,
            env::current_exe().expect("current test binary"),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    command
}

fn spawn_host(control: &Path, leak_app_link: bool) -> Child {
    let mut command = role_command("host", control);
    if leak_app_link {
        command.env(APP_LINK_LEAK_ENV, "1");
    }
    command.spawn().expect("spawn host role")
}

fn spawn_link_peer(control: &Path, app_link: &str, eof_witness: Option<&Path>) -> Child {
    let mut command = role_command("peer", control);
    command.env(APP_LINK_ENV, app_link);
    if let Some(path) = eof_witness {
        command.env(EOF_WITNESS_ENV, path);
    }
    command.spawn().expect("spawn real app-link peer")
}

struct EofWitness {
    listener: UnixListener,
    path: PathBuf,
    stream: Option<UnixStream>,
}

impl EofWitness {
    fn bind(control: &Path) -> Self {
        let id = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let path = control.with_extension(format!("eof-{id}.sock"));
        let listener = UnixListener::bind(&path).expect("bind EOF witness socket");
        listener
            .set_nonblocking(true)
            .expect("make EOF witness socket nonblocking");
        Self {
            listener,
            path,
            stream: None,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn accept_before(&mut self, deadline: Instant) {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    self.stream = Some(stream);
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "peer did not connect its EOF witness before the deadline"
                    );
                    thread::yield_now();
                }
                Err(error) => panic!("accept EOF witness: {error}"),
            }
        }
    }

    fn assert_no_eof_observed(&mut self) {
        let stream = self.stream.as_mut().expect("connected EOF witness");
        stream
            .set_nonblocking(true)
            .expect("make EOF witness read nonblocking");
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte) {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Ok(0) => panic!("EOF witness peer exited before observing app-link EOF"),
            Ok(_) => panic!("leaked app-link descriptor did not suppress the EOF witness"),
            Err(error) => panic!("read EOF witness negative control: {error}"),
        }
        stream
            .set_nonblocking(false)
            .expect("restore blocking EOF witness read");
    }

    fn await_eof_observed(&mut self) {
        let stream = self.stream.as_mut().expect("connected EOF witness");
        stream
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("bound EOF witness read");
        let mut line = String::new();
        BufReader::new(&mut *stream)
            .read_line(&mut line)
            .expect("read independently witnessed EOF");
        assert_eq!(line, "EOF_OBSERVED\n", "unexpected EOF witness record");
        stream
            .set_read_timeout(None)
            .expect("clear EOF witness read deadline");
    }

    fn release_link_eof_report(&mut self) {
        self.stream
            .as_mut()
            .expect("connected EOF witness")
            .write_all(b"R")
            .expect("release peer LINK_EOF report");
    }
}

impl Drop for EofWitness {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn send_event(control: &Path, event: &str) {
    try_send_event(control, event).expect("send controller event");
}

fn try_send_event(control: &Path, event: &str) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(control)?;
    stream.write_all(event.as_bytes())?;
    stream.write_all(b"\n")
}

fn revoke_registered_link(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path)?;
    let owner_dir = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "registered link has no owner directory",
        )
    })?;
    fs::remove_dir(owner_dir)
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

fn process_exists(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn kill_pid(pid: u32) {
    let status = Command::new("/bin/kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .expect("invoke kill for one process");
    assert!(status.success(), "kill process {pid}: {status:?}");
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

fn parent_process(pid: u32) -> u32 {
    let output = Command::new("/bin/ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .expect("inspect parent process");
    assert!(output.status.success(), "inspect parent process for {pid}");
    String::from_utf8(output.stdout)
        .expect("parent process output is UTF-8")
        .trim()
        .parse()
        .expect("parent process output is numeric")
}

fn process_rss_kib(pid: u32) -> u64 {
    let output = Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .expect("inspect process RSS");
    assert!(output.status.success(), "inspect process RSS for {pid}");
    String::from_utf8(output.stdout)
        .expect("process RSS output is UTF-8")
        .trim()
        .parse()
        .expect("process RSS output is numeric")
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
    peer: Option<Child>,
    group: Option<u32>,
    leaker: Option<u32>,
    group_gone: bool,
}

impl CycleCleanup {
    fn new(host: Child) -> Self {
        Self {
            host: Some(host),
            peer: None,
            group: None,
            leaker: None,
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
        if let Some(peer) = self.peer.as_mut() {
            let _ = peer.kill();
            let _ = peer.wait();
        }
        if let Some(leaker) = self.leaker {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &leaker.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
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
