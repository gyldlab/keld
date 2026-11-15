//! Genuine generic KGR1 Guardian SIGKILL before the first process-group record.
//! Unlike the legacy `run_with_registration` fixture, this uses shipping
//! `macos_guardian::run` and `SpawnAdmission`'s pre-exec permit pipeline.
use std::io;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use super::super::process_group::{
    GroupIdentity, observe_group_identity, terminate_attested_group,
};
use super::super::{GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, RuntimeError, run};
use super::test_support::{await_spawned_gate, kernel_process_image, verified_real_bun};

mod cleanup_authority;

fn stop_guardian_at_pre_registration_gate(
    original_birth: GroupIdentity,
) -> Result<(), RuntimeError> {
    // The original private Guardian was launched in its own process group.
    // A saved numeric PID from a delayed observer cannot signal a recycled
    // outsider after concurrent host-side registration rejection/reap.
    terminate_attested_group(original_birth)
}

const ROLE_ENV: &str = "KELD_TEST_GENERIC_PRE_REGISTER_ROLE";
const MARKER_ENV: &str = "KELD_TEST_GENERIC_PRE_REGISTER_MARKER";
const BUN_ENV: &str = "KELD_TEST_GENERIC_PRE_REGISTER_BUN";
const GATE_ENV: &str = "KELD_TEST_GUARDIAN_PREEXEC_GATE";
const ROLE_TEST: &str =
    "macos_guardian::tests::generic_pre_registration_crash::generic_kgr1_guardian_role";

#[test]
fn generic_kgr1_guardian_role() {
    if std::env::var_os(ROLE_ENV).is_none() {
        return;
    }
    let bun = std::env::var_os(BUN_ENV).expect("verified real Bun executable");
    let marker = std::env::var_os(MARKER_ENV).expect("private native Bun marker");
    let script = r#"
const { spawn } = require("node:child_process");
const { writeFileSync } = require("node:fs");
const child = spawn("/bin/sleep", ["60"], { stdio: "ignore" });
if (!child.pid) process.exit(70);
writeFileSync(process.env.KELD_TEST_GENERIC_PRE_REGISTER_MARKER, `${process.pid} ${child.pid}`);
setInterval(() => {}, 1000);
"#;
    let mut command = Command::new(bun);
    command
        .args(["-e", script])
        .env(MARKER_ENV, marker)
        .stdin(Stdio::null());
    // The real Guardian dies while its registration reporter is test-fenced.
    // No normal outcome is possible in this process role.
    let result = run(command, io::stdin().lock(), || Ok(()));
    panic!("real KGR1 Guardian returned before forced SIGKILL: {result:?}");
}

#[test]
fn real_generic_guardian_sigkill_before_kgr1_registration_denies_bun_exec() {
    let bun = verified_real_bun();
    let temp = tempfile::tempdir().expect("private pre-KGR1 crash fixture");
    let socket = temp.path().join("gate.sock");
    let marker = temp.path().join("bun-and-descendant.pid");
    let listener = std::os::unix::net::UnixListener::bind(&socket)
        .expect("actual pre-registration gate event socket");
    listener
        .set_nonblocking(true)
        .expect("bounded gate observation");

    let mut command = Command::new(std::env::current_exe().expect("libtest executable"));
    command
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, "guardian")
        .env(BUN_ENV, bun)
        .env(MARKER_ENV, &marker)
        .env(GATE_ENV, &socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .process_group(0);
    let bootstrap = GuardianBootstrap::spawn(command).expect("real generic Guardian bootstrap");
    let guardian_pid = bootstrap
        .guardian_pid()
        .expect("host retains true Guardian child");
    let guardian_birth =
        observe_group_identity(guardian_pid).expect("pin original isolated native Guardian birth");
    // This is a test-only external observer; ownership and final wait remain
    // with the production GuardianBootstrap::register_until call below.
    let observer = thread::spawn(move || {
        let (_held, group, announced_guardian) = await_spawned_gate(&listener);
        assert_eq!(
            announced_guardian, guardian_pid,
            "event must come from exact Guardian"
        );
        let group_pid = Pid::from_raw(group);
        assert_eq!(
            getpgid(Some(group_pid)),
            Ok(group_pid),
            "real pre-exec child owns its isolated group"
        );
        let test_image = kernel_process_image(i32::try_from(std::process::id()).expect("PID"));
        let held_until = Instant::now() + Duration::from_millis(100);
        loop {
            assert_eq!(
                kernel_process_image(group),
                test_image,
                "generic KGR1 child exec'd Bun before host enrolled its group"
            );
            assert!(
                !marker.exists(),
                "unregistered KGR1 Bun/descendant executed before Guardian crash"
            );
            if Instant::now() >= held_until {
                break;
            }
            thread::yield_now();
        }
        let killed_at = Instant::now();
        assert_eq!(
            observe_group_identity(guardian_pid).expect("original Guardian must still be live"),
            guardian_birth,
            "the original Guardian must retain its birth at the true crash cut"
        );
        stop_guardian_at_pre_registration_gate(guardian_birth)
            .expect("SIGKILL attested original Guardian group, not host or a recycled outsider");
        // Guardian's pre-exec permit writer must be parent-only. Otherwise the
        // child would wait for its independent five-second polling timeout.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if kill(Pid::from_raw(-group), None) == Err(Errno::ESRCH)
                && kill(group_pid, None) == Err(Errno::ESRCH)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "pre-exec child survived Guardian SIGKILL until its own poll timeout"
            );
            thread::yield_now();
        }
        assert!(
            killed_at.elapsed() < Duration::from_secs(3),
            "failed to promptly deny exec after a true Guardian SIGKILL"
        );
        assert!(
            !marker.exists(),
            "real KGR1 Bun or descendant executed without host retention"
        );
    });
    let result = bootstrap.register_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE);
    let refusal = result.as_ref().err().map(ToString::to_string);
    drop(result); // Unexpected registered owner is still cleaned up before asserting.
    observer
        .join()
        .expect("real Guardian SIGKILL observer assertions");
    assert!(
        refusal
            .as_ref()
            .is_some_and(|error| error.contains("registration")),
        "host must fail closed without a KGR1 registration record: {refusal:?}"
    );
}
