//! Combined schema4/Legacy recovery, with actual Host and public FS callers.
use super::facade_profile_recovery_fixture::RecoveryFixture;
use super::project::SENTINEL;
use crate::support::EVENT_DEADLINE;
use crate::support::control::{read_control_line, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::native_window::native_window_rows;
use crate::support::process::{await_process_gone, kill_pid, parent_process, process_group};
use crate::support::recovery_cycle::RecoveryGeneration;
use std::fs;
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::process::{Command, Stdio};

#[test]
fn schema_four_legacy_recovery_freezes_policy_and_windowless_ready() {
    let fixture = RecoveryFixture::new();
    let mut child = Command::new(fixture.stage.host())
        .current_dir(fixture.stage.root())
        .process_group(0)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T1B_CONTROL", &fixture.control_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("real shipping Host");
    let host_pid = child.id();
    let _lease = child.stdin.take().expect("retain dev lease");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let result = catch_unwind(AssertUnwindSafe(|| {
        recover(&fixture, host_pid, &mut cleanup)
    }));
    if result.is_err() {
        let _ = cleanup.cli.as_mut().expect("failed Host").kill();
    }
    let output = wait_child_output(cleanup.cli.take().expect("Host capture"), EVENT_DEADLINE);
    eprintln!("KEL78_SCHEMA4_RECOVERY_OUTPUT {output:?}");
    if let Err(failure) = result {
        resume_unwind(failure);
    }
    let (g1, g2, guardian) = result.expect("observed recovery");
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).expect("Host stderr");
    assert_eq!(
        stderr
            .matches("legacy — zero-authority claim forfeited")
            .count(),
        1,
        "{stderr}"
    );
    for pid in [g1, g2, guardian, host_pid] {
        await_process_gone(pid);
    }
    assert!(
        !fixture.stage.root().exists(),
        "actual Host did not delete its stage"
    );
    eprintln!(
        "KEL78_SCHEMA4_RECOVERY_PASS g1={g1} g2={g2} guardian={guardian} one_ready_each=true zero_native_windows=true stale_token_refused=true original_grants=true cleanup=true"
    );
}

fn recover(
    fixture: &RecoveryFixture,
    host: u32,
    cleanup: &mut ShippingLaunchCleanup,
) -> (u32, u32, u32) {
    let mut g1 = RecoveryGeneration::try_accept(&fixture.control, "G1").expect("G1 control");
    assert_eq!(parent_process(g1.guardian_pid), host);
    assert_eq!(process_group(g1.bun_pid), g1.bun_pid);
    cleanup.record_authenticated_groups(process_group(host), process_group(g1.bun_pid));
    g1.writer.write_all(b"ADMIT\n").expect("admit G1");
    expect_record(&mut g1, "READY");
    expect_record(
        &mut g1,
        "ROLE FACADE_HINT 1 RECEIVE_WINDOW 1 MIRROR_WINDOWS 0",
    );
    assert_rendererless(host);
    g1.writer
        .write_all(b"RUN1\n")
        .expect("G1 broker operations");
    expect_record(&mut g1, "FS 1 [1,0,255] KELD-GUARD002 1");
    assert_eq!(
        fs::read(&fixture.target).expect("G1 actual bytes"),
        [1, 0, 255]
    );
    assert_eq!(
        fs::read(&fixture.project.outside).expect("G1 denied sentinel"),
        SENTINEL
    );
    fixture
        .project
        .replace_facade_profile_policy(&fixture.stage, &fixture.digest);
    g1.writer
        .write_all(b"CHECK_READY\n")
        .expect("G1 Ready census after policy replacement");
    expect_record(&mut g1, "READY_COUNT 1 OPEN_COUNT 1");
    kill_pid(g1.bun_pid);
    let mut g2 = RecoveryGeneration::try_accept(&fixture.control, "G2").expect("G2 control");
    assert_ne!(g1.bun_pid, g2.bun_pid);
    assert_eq!(g1.guardian_pid, g2.guardian_pid);
    assert_eq!(parent_process(g2.guardian_pid), host);
    assert_eq!(process_group(g2.bun_pid), g2.bun_pid);
    cleanup.record_authenticated_groups(process_group(host), process_group(g2.bun_pid));
    let stale = super::assert_retired_credentials(&g1.app_link, &g2.app_link, &fixture.stage);
    eprintln!("KEL78_SCHEMA4_STALE_TOKEN_PROBE {stale:?}");
    eprintln!(
        "KEL78_SCHEMA4_GENERATIONS host={host} guardian={} g1={} g2={} endpoints_changed={} tokens_changed={}",
        g2.guardian_pid,
        g1.bun_pid,
        g2.bun_pid,
        g1.endpoint != g2.endpoint,
        g1.token() != g2.token()
    );
    await_process_gone(g1.bun_pid);
    g2.writer
        .write_all(b"ADMIT\n")
        .expect("fresh G2 credentials");
    expect_record(&mut g2, "READY");
    expect_record(
        &mut g2,
        "ROLE FACADE_HINT 1 RECEIVE_WINDOW 1 MIRROR_WINDOWS 0",
    );
    assert_rendererless(host);
    g2.writer
        .write_all(b"RUN2\n")
        .expect("G2 broker operations after both policy replacements");
    expect_record(&mut g2, "FS 2 [2,0,255] KELD-GUARD002 1");
    assert_eq!(
        fs::read(&fixture.target).expect("G2 actual bytes"),
        [2, 0, 255]
    );
    assert_eq!(
        fs::read(&fixture.project.outside).expect("G2 denied sentinel"),
        SENTINEL
    );
    g2.writer.write_all(b"QUIT\n").expect("healthy Quit");
    expect_record(&mut g2, "QUIT_REPLY");
    expect_record(&mut g2, "READY_COUNT 1 OPEN_COUNT 1");
    expect_record(&mut g2, "LINK_EOF");
    (g1.bun_pid, g2.bun_pid, g2.guardian_pid)
}

fn expect_record(generation: &mut RecoveryGeneration, expected: &str) {
    let actual = read_control_line(&mut generation.reader);
    eprintln!("KEL78_SCHEMA4_ROLE_RECORD {actual}");
    assert_eq!(actual, expected);
}

fn assert_rendererless(host: u32) {
    let text = native_window_rows(host);
    let census: serde_json::Value = serde_json::from_str(&text).expect("actual native census");
    assert_eq!(census["capture_preflight"], true, "{text}");
    assert_eq!(census["pid"], host);
    eprintln!("KEL78_SCHEMA4_NATIVE_CENSUS {text}");
    assert!(
        census["rows"]
            .as_array()
            .expect("window rows")
            .iter()
            .all(|row| row["layer"] != 0),
        "{text}"
    );
}
