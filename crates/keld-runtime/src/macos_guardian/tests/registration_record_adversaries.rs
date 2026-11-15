//! Real authenticated KGR1 records that must never authorize a Bun group.
//! These exercise the host bootstrap and OS child retirement, not a mock codec.
use std::io::{self, Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;

use super::super::{GuardianBootstrap, REGISTRATION_ENV, handshake_client, parse_app_link};

const ROLE_ENV: &str = "KELD_TEST_AUTHENTICATED_BAD_KGR1_ROLE";
const CASE_ENV: &str = "KELD_TEST_AUTHENTICATED_BAD_KGR1_CASE";
const RECEIPT_ENV: &str = "KELD_TEST_AUTHENTICATED_BAD_KGR1_RECEIPT";
const ROLE_TEST: &str =
    "macos_guardian::tests::registration_record_adversaries::bad_kgr1_guardian_role";

#[test]
fn bad_kgr1_guardian_role() {
    let Some(case) = std::env::var_os(ROLE_ENV) else {
        return;
    };
    assert_eq!(case, "guardian", "exact private role selector");
    let kind = std::env::var(CASE_ENV).expect("explicit bad record case");
    let receipt = std::env::var_os(RECEIPT_ENV).expect("private exact-role witness");
    let link = std::env::var(REGISTRATION_ENV).expect("host-minted private link");
    let (endpoint, token) = parse_app_link(&link).expect("actual registered link");
    let mut stream = UnixStream::connect(endpoint).expect("connect authenticated link");
    handshake_client(&mut stream, &token).expect("authenticate host-owned link");
    let bytes: Vec<u8> = match kind.as_str() {
        "zero" => Vec::new(),
        "partial" => b"KGR1".to_vec(),
        "wrong_magic" => {
            let mut record = b"KGRX".to_vec();
            record.extend_from_slice(&123_u32.to_be_bytes());
            record
        }
        "zero_pid" => b"KGR1\0\0\0\0".to_vec(),
        "oversized_pid" => {
            let mut record = b"KGR1".to_vec();
            record.extend_from_slice(&u32::MAX.to_be_bytes());
            record
        }
        other => panic!("unrecognized native KGR1 adversary {other}"),
    };
    stream
        .write_all(&bytes)
        .expect("write the exact adversarial record bytes");
    std::fs::write(receipt, kind.as_bytes()).expect("prove the real role executed");
    // Retain the authenticated stream for zero/partial cases so the *host's*
    // original admission deadline, not premature EOF, is the only refusal.
    let mut eof = [0_u8; 1];
    let count = io::stdin()
        .read(&mut eof)
        .expect("wait for actual host-liveness EOF");
    assert_eq!(
        count, 0,
        "bad registration must not receive an authority byte"
    );
}

fn adversarial_registration(case: &str, expected: &str) {
    let temp = tempfile::tempdir().expect("private authenticated record fixture");
    let receipt = temp.path().join("role-selected");
    let mut command = Command::new(std::env::current_exe().expect("native unit executable"));
    command
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, "guardian")
        .env(CASE_ENV, case)
        .env(RECEIPT_ENV, &receipt)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let bootstrap = GuardianBootstrap::spawn(command).expect("start genuine KGR1 Guardian");
    let guardian_pid = i32::try_from(bootstrap.guardian_pid().expect("retained Guardian"))
        .expect("kernel PID fits");
    let began = Instant::now();
    let result = bootstrap.register_until(began + Duration::from_millis(600));
    let refusal = result.as_ref().err().map(ToString::to_string);
    drop(result); // Unexpected successful owner still performs its own cleanup.
    let selected = std::fs::read_to_string(&receipt).expect("actual exact libtest role ran");
    assert_eq!(
        selected, case,
        "requested malformed KGR1 bytes reached role"
    );
    assert!(
        refusal
            .as_ref()
            .is_some_and(|message| message.contains(expected)),
        "malformed authenticated KGR1 must fail closed ({case}): {refusal:?}"
    );
    assert_eq!(
        kill(Pid::from_raw(guardian_pid), None),
        Err(Errno::ESRCH),
        "bad KGR1 admission must reap actual Guardian, never retain a process"
    );
    assert!(
        began.elapsed() < Duration::from_secs(3),
        "record refusal must not wait for a second unbounded deadline"
    );
}

#[test]
fn real_authenticated_zero_byte_kgr1_uses_original_deadline() {
    adversarial_registration("zero", "deadline");
}

#[test]
fn real_authenticated_partial_kgr1_uses_original_deadline() {
    adversarial_registration("partial", "deadline");
}

#[test]
fn real_authenticated_wrong_magic_kgr1_rejects_without_group_signal() {
    adversarial_registration("wrong_magic", "magic");
}

#[test]
fn real_authenticated_zero_pid_kgr1_rejects_without_group_signal() {
    adversarial_registration("zero_pid", "group is zero");
}

#[test]
fn real_authenticated_unrepresentable_pid_kgr1_rejects_without_group_signal() {
    adversarial_registration("oversized_pid", "exceeds c_int");
}
