//! Actual authenticated Guardian selection across a Supervisor-owned restart.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use keld_ipc::{BootstrapAdmission, BootstrapRejection, BootstrapRejectionObserver};

use super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardedPrimary, GuardianBootstrap, lock_generation,
    run_guarded_primary,
};
use super::test_support::verified_real_bun;

const ROLE_ENV: &str = "KELD_TEST_STARTUP_SELECTION_ROOT";
const BUN_ENV: &str = "KELD_TEST_STARTUP_SELECTION_BUN";
const AUTH_FAILURE_ENV: &str = "KELD_TEST_STARTUP_SELECTION_AUTH_FAILURE";
const ROLE_TEST: &str = "macos_guardian::tests::startup_selection::startup_selection_guardian_role";

#[test]
fn startup_selection_guardian_role() {
    let Some(root) = std::env::var_os(ROLE_ENV).map(PathBuf::from) else {
        return;
    };
    let bun = std::env::var_os(BUN_ENV).expect("parent verified the real Bun image");
    let authentication_failure = std::env::var_os(AUTH_FAILURE_ENV).is_some();
    let refusal_receipt = root.join("authentication-refused");
    let ack = fs::OpenOptions::new()
        .write(true)
        .open("/dev/fd/3")
        .expect("original host-only KSA writer");
    let script = r"
const fs = require('node:fs');
const crypto = require('node:crypto');
const destination = process.env.KELD_TEST_SELECTION_GENERATION_FILE;
const observed = [
  process.pid,
  process.env.KELD_TEST_SELECTED_INPUT,
  crypto.createHash('sha256').update(process.env.KELD_APP_LINK).digest('hex'),
  process.env.KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION === undefined
];
fs.writeFileSync(`${destination}.partial`, observed.join('\n') + '\n');
fs.renameSync(`${destination}.partial`, destination);
setInterval(() => {}, 1000);
";
    let result = run_guarded_primary(
        io::stdin().lock(),
        move || {
            let selected = fs::read_to_string(root.join("selected-input"))
                .expect("fixture input read only by one-shot selection");
            let mut receipt = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(root.join("selection-once"))
                .expect("selection cannot execute twice");
            receipt.write_all(b"SELECTED").expect("selection receipt");
            let mut command_number = 0_u32;
            Ok(move |app_link: &str| {
                command_number += 1;
                let mut command = Command::new(&bun);
                command
                    .args(["--no-env-file", "--no-install", "-e", script])
                    .env("KELD_APP_LINK", app_link)
                    .env("KELD_TEST_SELECTED_INPUT", &selected)
                    .env(
                        "KELD_TEST_SELECTION_GENERATION_FILE",
                        root.join(format!("generation-{command_number}.receipt")),
                    )
                    .env_remove("BUN_OPTIONS")
                    .env_remove("BUN_INSPECT_PRELOAD")
                    .env_remove("NODE_PATH");
                Ok(command)
            })
        },
        ack,
    );
    if authentication_failure {
        let error = result.expect_err("wrong registration token must reject authentication");
        assert!(
            error
                .to_string()
                .contains("macOS guardian private registration authentication"),
            "an unrelated bootstrap failure is not the authentication oracle: {error}"
        );
        fs::write(refusal_receipt, b"AUTHENTICATION_REFUSED")
            .expect("typed client authentication refusal receipt");
    } else {
        assert!(
            result.is_ok(),
            "authenticated restarted Guardian failed: {result:?}"
        );
    }
}

fn role_command(root: &Path, bun: &Path, reject_authentication: bool) -> Command {
    let mut command = Command::new("/bin/sh");
    let launcher = if reject_authentication {
        // Preserve the real endpoint and token encoding, changing one token
        // nibble. The existing listener and client perform the actual failed
        // authentication; the fixture does not fabricate a protocol response.
        r#"exec 3>&1; exec 1>&2
link=$KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION
endpoint=${link%#*}
token=${link##*#}
suffix=${token#?}
case $token in 0*) token="1$suffix" ;; *) token="0$suffix" ;; esac
export KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION="$endpoint#$token"
exec "$@""#
    } else {
        "exec 3>&1; exec 1>&2; exec \"$@\""
    };
    command
        .args(["-c", launcher, "guardian-selection-test"])
        .arg(std::env::current_exe().expect("actual runtime libtest executable"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, root)
        .env(BUN_ENV, bun)
        .env_remove(AUTH_FAILURE_ENV)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if reject_authentication {
        command.env(AUTH_FAILURE_ENV, "1");
    }
    command
}

struct GenerationObservation {
    selected: String,
    app_link_digest: String,
    registration_absent: bool,
}

fn await_generation(
    primary: &mut GuardedPrimary,
    root: &Path,
    expected_attempt: u32,
) -> GenerationObservation {
    let receipt = root.join(format!("generation-{expected_attempt}.receipt"));
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        primary
            .poll_fatal()
            .expect("owned Guardian remains healthy");
        if let Ok(bytes) = fs::read_to_string(&receipt) {
            let fields: Vec<_> = bytes.lines().collect();
            let [pid, selected, digest, registration_absent] = fields.as_slice() else {
                panic!("actual Bun receipt must contain exactly four fields");
            };
            let registered = lock_generation(&primary.current_generation);
            let generation = registered.as_ref().expect("Host retains executing Bun");
            assert_eq!(generation.attempt, expected_attempt);
            assert_eq!(
                pid.parse::<u32>().expect("actual Bun PID"),
                generation.group_pid
            );
            return GenerationObservation {
                selected: (*selected).to_owned(),
                app_link_digest: (*digest).to_owned(),
                registration_absent: registration_absent
                    .parse()
                    .expect("registration observation"),
            };
        }
        assert!(Instant::now() < deadline, "successor Bun did not execute");
        thread::yield_now();
    }
}

#[test]
fn authenticated_selection_survives_a_real_supervisor_restart() {
    let temp = tempfile::tempdir().expect("owned selection restart fixture");
    fs::write(temp.path().join("selected-input"), b"captured-before-g1")
        .expect("first selected input");
    let bun = verified_real_bun();
    let pending = GuardianBootstrap::spawn_supervised(role_command(temp.path(), &bun, false))
        .expect("spawn actual Guardian");
    let mut primary = pending
        .register_guarded_primary_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("authenticate actual Guardian");
    let first = await_generation(&mut primary, temp.path(), 1);
    assert_eq!(first.selected, "captured-before-g1");
    assert!(first.registration_absent);
    assert_eq!(
        fs::read(temp.path().join("selection-once")).expect("one selection receipt"),
        b"SELECTED"
    );

    fs::write(temp.path().join("selected-input"), b"changed-after-g1")
        .expect("change only input after selection");
    primary.arm_recovery();
    primary
        .fail_current_generation(1)
        .expect("retire the exact owned first group");
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    while primary.group_pid().is_some() {
        primary
            .poll_fatal()
            .expect("Guardian owns first-generation retirement");
        assert!(Instant::now() < deadline, "first group was not cleared");
        thread::yield_now();
    }
    // The existing host owner releases recovery only after retirement. It
    // preserves its own native CLEAR fence; the test never mints a generation.
    primary.arm_recovery();
    let second = await_generation(&mut primary, temp.path(), 2);
    assert_eq!(second.selected, "captured-before-g1");
    assert!(second.registration_absent);
    let first_link = &first.app_link_digest;
    let second_link = &second.app_link_digest;
    assert_eq!(first_link.len(), 64);
    assert_eq!(second_link.len(), 64);
    assert_ne!(
        first_link, second_link,
        "the actual successor receives fresh bootstrap data"
    );
    assert!(!temp.path().join("generation-3.receipt").exists());
    assert_eq!(
        fs::read(temp.path().join("selection-once")).expect("one selection receipt"),
        b"SELECTED"
    );

    let status = primary
        .shutdown()
        .expect("orderly shutdown after actual restart");
    assert!(status.success(), "Guardian exit: {status:?}");
    assert!(
        primary.group_pid().is_none(),
        "native generation was not retired"
    );
    assert!(
        primary
            .child
            .try_wait()
            .expect("Guardian process observation")
            .is_some(),
        "actual Guardian must be reaped before fallback cleanup"
    );
}

#[derive(Default)]
struct RejectionCounter(AtomicUsize);

impl BootstrapRejectionObserver for RejectionCounter {
    fn rejected(&self, _rejection: BootstrapRejection) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn await_role_exit(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        if let Some(status) = child.try_wait().expect("actual Guardian status") {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "authentication-refused Guardian did not exit"
        );
        thread::yield_now();
    }
}

#[test]
fn failed_registration_authentication_never_calls_the_selector() {
    let temp = tempfile::tempdir().expect("owned authentication refusal fixture");
    fs::write(temp.path().join("selected-input"), b"must-not-be-selected")
        .expect("selection witness input");
    let bun = verified_real_bun();
    let mut pending = GuardianBootstrap::spawn_supervised(role_command(temp.path(), &bun, true))
        .expect("spawn Guardian with one modified token nibble");
    let observer = RejectionCounter::default();
    let admission = pending
        .registration
        .accept_authenticated_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE, &observer)
        .expect("actual registration listener handles rejection");
    assert!(matches!(admission, BootstrapAdmission::DeadlineElapsed));
    assert_eq!(
        observer.0.load(Ordering::Relaxed),
        1,
        "real failed authentication observed"
    );
    let status = await_role_exit(pending.child.as_mut().expect("owned Guardian child"));
    assert!(status.success(), "typed refusal role failed: {status:?}");
    assert_eq!(
        fs::read(temp.path().join("authentication-refused")).expect("typed authentication refusal"),
        b"AUTHENTICATION_REFUSED"
    );
    assert!(
        !temp.path().join("selection-once").exists(),
        "selector ran before authentication"
    );
    assert!(
        !temp.path().join("generation-1.receipt").exists(),
        "candidate Bun executed"
    );
}
