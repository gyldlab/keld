// Real macOS processes, using the shipping GuardedPrimary registration path.
// The separate Guardian dies from SIGKILL while Bun's pre-exec gate is held.
use std::io::{self, Write as _};
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use super::super::process_group::{
    GroupIdentity, terminate_attested_group, validate_guardian_child_group,
};
use super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardedPrimary, GuardianBootstrap, run_guarded_primary,
};
use super::test_support::{await_spawned_gate, verified_real_bun};

const CRASH_ROLE_ENV: &str = "KELD_TEST_PREEXEC_GUARDIAN_ROLE";
const CRASH_MARKER_ENV: &str = "KELD_TEST_PREEXEC_BUN_MARKER";
const CRASH_GATE_ENV: &str = "KELD_TEST_GUARDIAN_PREEXEC_GATE";
const CRASH_BUN_ENV: &str = "KELD_TEST_GUARDIAN_BUN_BINARY";
const CRASH_ROLE_TEST: &str =
    "macos_guardian::tests::guardian_crash_pre_registration::guarded_preexec_guardian_role";

#[path = "guardian_crash_pre_registration/cleanup_authority.rs"]
mod cleanup_authority;
mod retained_ack;

#[test]
fn guarded_preexec_guardian_role() {
    if std::env::var_os(CRASH_ROLE_ENV).is_none() {
        return;
    }
    let marker = std::env::var_os(CRASH_MARKER_ENV).expect("Bun marker path");
    let script = r#"
const { spawn } = require("node:child_process");
const { writeFileSync } = require("node:fs");
const descendant = spawn("/bin/sleep", ["60"], { stdio: "ignore" });
if (!descendant.pid) process.exit(70);
writeFileSync(process.env.KELD_TEST_PREEXEC_BUN_MARKER, `${process.pid} ${descendant.pid}`);
setInterval(() => {}, 1000);
"#;
    let bun = std::env::var_os(CRASH_BUN_ENV).expect("verified real Bun executable path");
    if let Err(error) = run_guarded_primary(
        io::stdin().lock(),
        move |_| {
            let mut command = Command::new(&bun);
            command
                .args(["-e", script])
                .env(CRASH_MARKER_ENV, &marker)
                .stdin(Stdio::null());
            Ok(command)
        },
        io::stdout(),
    ) {
        eprintln!("KELD_TEST_PREEXEC_GUARDIAN_FAILURE: {error}");
    }
}

struct GuardianCrashFixture {
    primary: GuardedPrimary,
    /// Kernel birth acquired while this group was the original Guardian's
    /// live direct child. A private fixture marker/PGID alone grants nothing.
    group_birth: Option<GroupIdentity>,
}

impl Drop for GuardianCrashFixture {
    fn drop(&mut self) {
        // The failure cleanup is intentionally separate from assertions. An
        // old marker PID or current same-group membership is NOT authority
        // to signal a possibly recycled/foreign numeric group.
        let _ = self.primary.child.kill();
        let _ = self.primary.poll_fatal();
        // A synthetic test Guardian has no generation worker or liveness
        // writer. Retain its actual Child until reaped even if poll_fatal saw
        // the SIGKILL before the kernel published its terminal status.
        let _ = self.primary.child.wait();
        if let Some(group) = self.group_birth.take() {
            let _ = terminate_attested_group(group);
        }
    }
}

fn guard_crash_fixture(
    socket: &std::path::Path,
    marker: &std::path::Path,
    bun: &std::path::Path,
) -> GuardianCrashFixture {
    let command = guarded_role_command(Some(socket), marker, bun);
    let pending = GuardianBootstrap::spawn_supervised(command).expect("actual guardian bootstrap");
    let primary = pending
        .register_guarded_primary_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("actual authenticated Guardian control bootstrap");
    GuardianCrashFixture {
        primary,
        group_birth: None,
    }
}

/// Shared real-subprocess fixture. Omitting the pre-record gate intentionally
/// reaches the later host-retained REGISTERED ACK seam instead.
fn guarded_role_command(
    pre_record_gate: Option<&std::path::Path>,
    marker: &std::path::Path,
    bun: &std::path::Path,
) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args(["--exact", CRASH_ROLE_TEST, "--nocapture"])
        .env(CRASH_ROLE_ENV, "guardian")
        .env_remove(CRASH_GATE_ENV)
        .env(CRASH_BUN_ENV, bun)
        .env(CRASH_MARKER_ENV, marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .process_group(0);
    if let Some(socket) = pre_record_gate {
        command.env(CRASH_GATE_ENV, socket);
    }
    command
}

fn group_and_child_gone(group: i32, descendant: Option<i32>) {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        let gone = matches!(
            kill(Pid::from_raw(-group), None),
            Err(nix::errno::Errno::ESRCH)
        ) && matches!(
            kill(Pid::from_raw(group), None),
            Err(nix::errno::Errno::ESRCH)
        ) && descendant.is_none_or(|pid| {
            matches!(
                kill(Pid::from_raw(pid), None),
                Err(nix::errno::Errno::ESRCH)
            )
        });
        if gone {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Bun group or descendant survived Guardian SIGKILL"
        );
        thread::sleep(Duration::from_millis(3));
    }
}

fn real_guardian_crash_at_gate(release_registration: bool) {
    let bun = verified_real_bun();
    let temp = tempfile::tempdir().expect("isolated crash fixture");
    let socket = temp.path().join("preexec.sock");
    let marker = temp.path().join("bun-and-descendant.pid");
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("test event socket");
    listener
        .set_nonblocking(true)
        .expect("bounded event accept");
    let mut fixture = guard_crash_fixture(&socket, &marker, &bun);
    let (mut gate, group, reported_guardian) = await_spawned_gate(&listener);
    assert_eq!(
        reported_guardian,
        fixture.primary.guardian_pid(),
        "unrelated PID cannot supply Guardian's gate receipt"
    );
    fixture.group_birth = Some(
        validate_guardian_child_group(
            u32::try_from(group).expect("real positive native group"),
            fixture.primary.guardian_pid(),
        )
        .expect("the pre-exec group belongs to the retained original Guardian"),
    );
    assert_eq!(
        getpgid(Some(Pid::from_raw(group))),
        Ok(Pid::from_raw(group)),
        "pre-exec child owns the isolated group"
    );
    assert_eq!(
        fixture.primary.group_pid(),
        None,
        "no premature registration"
    );
    assert!(
        !marker.exists(),
        "Bun executed before the host owned its group"
    );
    let mut descendant = None;
    if release_registration {
        gate.write_all(b"R").expect("permit host registration");
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        while fixture.primary.group_pid() != Some(group.cast_unsigned()) {
            fixture
                .primary
                .poll_fatal()
                .expect("registered Guardian healthy");
            assert!(Instant::now() < deadline, "group was not registered");
            thread::yield_now();
        }
        let ids = loop {
            if let Ok(contents) = std::fs::read_to_string(&marker) {
                let ids: Vec<i32> = contents
                    .split_whitespace()
                    .filter_map(|part| part.parse().ok())
                    .collect();
                if ids.len() == 2 {
                    break ids;
                }
            }
            assert!(Instant::now() < deadline, "real Bun did not execute");
            thread::yield_now();
        };
        assert_eq!(ids[0], group);
        assert_eq!(
            getpgid(Some(Pid::from_raw(ids[1]))),
            Ok(Pid::from_raw(group))
        );
        descendant = Some(ids[1]);
    }
    // SIGKILL is directed only at the independently spawned Guardian. Before
    // registration, the permit pipe must close promptly through EOF, rather
    // than making this test pass after the separate five-second poll timeout.
    let killed_at = Instant::now();
    fixture.primary.child.kill().expect("SIGKILL real Guardian");
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let error = loop {
        match fixture.primary.poll_fatal() {
            Ok(()) => {
                assert!(Instant::now() < deadline, "host missed Guardian SIGKILL");
                thread::yield_now();
            }
            Err(error) => break error,
        }
    };
    assert!(error.to_string().contains("guardian"), "{error}");
    group_and_child_gone(group, descendant);
    if !release_registration {
        assert!(
            killed_at.elapsed() < Duration::from_secs(3),
            "pre-exec child survived Guardian SIGKILL until its polling deadline; permit writer leaked into child"
        );
        assert!(!marker.exists(), "unregistered Bun was allowed to execute");
    }
    fixture.group_birth = None;
}

#[test]
fn real_guardian_sigkill_before_registration_cannot_run_bun_or_descendants() {
    real_guardian_crash_at_gate(false);
}

#[test]
fn real_guardian_sigkill_after_registration_reaps_bun_and_descendants() {
    real_guardian_crash_at_gate(true);
}
