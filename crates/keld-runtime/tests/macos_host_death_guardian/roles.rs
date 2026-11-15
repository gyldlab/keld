//! Concrete host, Guardian, descendant and peer process roles.
use super::support::{parent_process, revoke_registered_link, send_event, try_send_event};
use super::{
    APP_LINK_ENV, APP_LINK_LEAK_ENV, CONTROL_ENV, EOF_WITNESS_ENV, REGISTERED_LINK_ENV, ROLE_ENV,
    ROLE_TEST, TEST_EXE_ENV,
};
use super::{
    BootstrapListener, Child, Command, EVENT_DEADLINE, ExitStatusExt, Instant, OwnedFd, Path,
    PathBuf, PermissionsExt, Read, Stdio, UnixListener, UnixStream, Write, env, fs,
    handshake_client, macos_guardian, parse_app_link, thread,
};

pub(super) fn run_host(control: &Path) {
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

pub(super) fn run_guardian(control: &Path) {
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

pub(super) fn run_descendant(control: &Path) {
    let pid = std::process::id();
    send_event(control, &format!("LEADER {}", parent_process(pid)));
    send_event(control, &format!("DESCENDANT {pid}"));
    thread::park();
}

pub(super) fn run_link_peer(control: &Path) {
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

pub(super) fn spawn_host(control: &Path, leak_app_link: bool) -> Child {
    let mut command = role_command("host", control);
    if leak_app_link {
        command.env(APP_LINK_LEAK_ENV, "1");
    }
    command.spawn().expect("spawn host role")
}

pub(super) fn spawn_link_peer(control: &Path, app_link: &str, eof_witness: Option<&Path>) -> Child {
    let mut command = role_command("peer", control);
    command.env(APP_LINK_ENV, app_link);
    if let Some(path) = eof_witness {
        command.env(EOF_WITNESS_ENV, path);
    }
    command.spawn().expect("spawn real app-link peer")
}
