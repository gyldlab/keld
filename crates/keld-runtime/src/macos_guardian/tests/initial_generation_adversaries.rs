//! The first authenticated KGC1 record is bounded even when the peer
//! sends zero bytes. Later idle gaps between fully processed generations
//! remain owned by the existing guarded runtime lifecycle.
use std::io::{self, Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::super::{
    CONTROL_PREPARE, GenerationControlRecord, GuardianBootstrap, REGISTRATION_ENV, RecoveryState,
    handshake_client, parse_app_link, run_generation_host, write_generation_control,
};

const ROLE_ENV: &str = "KELD_TEST_ZERO_KGC1_ROLE";
const CASE_ENV: &str = "KELD_TEST_ZERO_KGC1_CASE";
const RECEIPT_ENV: &str = "KELD_TEST_ZERO_KGC1_RECEIPT";
const ROLE_TEST: &str =
    "macos_guardian::tests::initial_generation_adversaries::zero_first_kgc1_guardian_role";

#[test]
fn zero_first_kgc1_guardian_role() {
    let Some(role) = std::env::var_os(ROLE_ENV) else {
        return;
    };
    assert_eq!(role, "guardian", "actual exact libtest role");
    let kind = std::env::var(CASE_ENV).expect("one zero/partial KGC1 adversary");
    let receipt = std::env::var_os(RECEIPT_ENV).expect("actual role witness path");
    let link = std::env::var(REGISTRATION_ENV).expect("one-use Guardian link");
    let (endpoint, token) = parse_app_link(&link).expect("private authenticated KGC1 link");
    let mut control = UnixStream::connect(endpoint).expect("connect genuine KGC1 endpoint");
    handshake_client(&mut control, &token).expect("authenticate exact Guardian peer");
    // Publish the independent role-selection witness before any deliberately
    // slow writes. The parent must not infer a selected test from libtest's
    // exit status alone.
    std::fs::write(&receipt, kind.as_bytes()).expect("publish exact role selection");
    let mut dripped = None;
    match kind.as_str() {
        "zero" => {}
        "partial" => control
            .write_all(b"KGC1")
            .expect("send partial private record"),
        "drip" => {
            // A byte every 10ms is faster than the 20ms socket read timeout,
            // so the old reader stayed *inside* one poll call until its
            // independent five-second frame deadline rather than honoring
            // the original two-second authenticated bootstrap deadline.
            let mut delivered = 0_usize;
            for _ in 0..400 {
                match control.write_all(b"K") {
                    Ok(()) => delivered += 1,
                    Err(source)
                        if matches!(
                            source.kind(),
                            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                        ) =>
                    {
                        break;
                    }
                    Err(source) => panic!("send KGC1 adversarial byte: {source}"),
                }
                thread::sleep(Duration::from_millis(10));
            }
            dripped = Some(delivered);
        }
        other => panic!("unknown first-record adversary {other}"),
    }
    if let Some(delivered) = dripped {
        // The parent reads this only after the role exits. It proves the
        // adversary actually wrote individual bytes instead of passing on a
        // zero-byte timeout or an early scheduling gap.
        std::fs::write(&receipt, format!("drip:{delivered}"))
            .expect("record actual adversarial byte-delivery progress");
    }
    // Hold control open without another byte while the host's existing
    // initial-authentication deadline expires. EOF must not be fabricated by
    // the test to turn the zero-byte case into a different failure.
    let mut shutdown = [0_u8; 1];
    assert_eq!(
        io::stdin()
            .read(&mut shutdown)
            .expect("wait for host liveness EOF"),
        0,
        "host denied KGC1 by closing only the private liveness writer"
    );
}

fn native_initial_record_denial(case: &str) {
    let temp = tempfile::tempdir().expect("owned authenticated KGC1 adversary");
    let marker = temp.path().join("selected-role");
    let mut command = Command::new(std::env::current_exe().expect("libtest process"));
    command
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, "guardian")
        .env(CASE_ENV, case)
        .env(RECEIPT_ENV, &marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let pending =
        GuardianBootstrap::spawn_supervised(command).expect("real persistent Guardian bootstrap");
    let original_deadline = Instant::now() + Duration::from_secs(2);
    let mut primary = pending
        .register_guarded_primary_until(original_deadline)
        .expect("actual one-use authenticated Guardian stream");
    let deadline = original_deadline + Duration::from_secs(1);
    let error = loop {
        match primary.poll_fatal() {
            Ok(()) => {
                assert!(
                    Instant::now() < deadline,
                    "zero-byte KGC1 host worker hung forever"
                );
                thread::yield_now();
            }
            Err(error) => break error,
        }
    };
    let observed_failure = Instant::now();
    let no_group = primary.group_pid().is_none();
    primary.liveness_writer.take();
    let status = primary
        .child
        .wait()
        .expect("reap exact authenticated Guardian after host denial");
    let selected = std::fs::read_to_string(marker).expect("exact libtest role executed");
    if case == "drip" {
        let delivered: usize = selected
            .strip_prefix("drip:")
            .expect("exact selected role must publish drip progress")
            .parse()
            .expect("numeric successful single-byte writes");
        assert!(
            delivered >= 10,
            "this adversary did not actually send enough single-byte records: {delivered}"
        );
    } else {
        assert_eq!(selected, case);
    }
    assert!(
        status.success(),
        "Guardian role's zero/partial assertions failed"
    );
    assert!(
        error.to_string().contains("first KGC1 record"),
        "host returned wrong authenticated admission denial for {case}: {error}"
    );
    assert!(
        no_group,
        "zero/partial initial control cannot authorize a native process group"
    );
    assert!(
        observed_failure <= deadline,
        "authenticated KGC1 first record exceeded the original deadline's bounded I/O margin"
    );
}

#[test]
fn authenticated_zero_byte_first_kgc1_cannot_hold_host_generation_worker() {
    native_initial_record_denial("zero");
}

#[test]
fn authenticated_partial_first_kgc1_cannot_extend_original_deadline() {
    native_initial_record_denial("partial");
}

#[test]
fn authenticated_byte_drip_first_kgc1_cannot_extend_original_deadline() {
    native_initial_record_denial("drip");
}

#[test]
fn generation_host_cannot_accept_buffered_first_record_after_original_deadline() {
    let (host, mut guardian) = std::os::unix::net::UnixStream::pair().expect("actual KGC1 pair");
    write_generation_control(
        &mut guardian,
        &GenerationControlRecord::new(CONTROL_PREPARE, 1),
    )
    .expect("complete KGC1 Prepare already buffered");
    let (updates_tx, _updates_rx) = mpsc::channel();
    let current = Mutex::new(None);
    let recovery = Mutex::new(RecoveryState::new());
    let error = run_generation_host(
        super::super::generation_host::GenerationHostAdmission::new(
            std::process::id(),
            Instant::now()
                .checked_sub(Duration::from_millis(1))
                .expect("monotonic clock has at least one millisecond of history"),
        ),
        host,
        &updates_tx,
        &current,
        &recovery,
        None,
        None,
    )
    .expect_err("a fully buffered late PREPARE cannot extend original authentication");
    assert!(error.to_string().contains("first KGC1 record"), "{error}");
    let mut reply = [0_u8; 1];
    assert_eq!(
        guardian.read(&mut reply).expect("observe host control EOF"),
        0,
        "expired first KGC1 deadline must not publish PREPARED authority"
    );
    assert!(
        current
            .lock()
            .expect("no generation was registered")
            .is_none()
    );
}
