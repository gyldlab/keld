//! Host process output is real native data, not a diagnostic authority, and the failure
//! summary names the failing fixture site from a typed value instead of classifying text.
//! The `LineReadFailed` site (for example `ConnectionReset`) is not driven here: `AF_UNIX` on
//! Darwin reports EOF, not a reset, when the peer closes with unread data, and the arm maps
//! `error.kind()` one to one.
use super::deadline_tests::native_listener;
use super::{
    NativeRecoveryStage, RecoveryControlFailure, RecoveryGeneration, collect_initial_control_until,
    read_initial_line_until, replacement_control_failure_summary,
};
use std::io::{ErrorKind, Write as _};
use std::os::unix::net::UnixStream;
use std::process::{Command, Output};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const RESCUE_LIMIT: Duration = Duration::from_secs(2);
const PRIVATE: &str = "KELD_TEST_PRIVATE_RECOVERY_OUTPUT_TOKEN_SENTINEL";

/// A real owned subprocess whose stdout and stderr land in an `Output`, as in the
/// recovery error path. No app/Guardian process or raw PID is involved.
fn owned_output(script: &str) -> Output {
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .output()
        .expect("owned synthetic native producer");
    assert!(output.status.success(), "synthetic native producer");
    output
}

#[test]
fn real_child_private_output_is_not_printed_on_recovery_admission_failure() {
    let output = owned_output(&format!(
        "printf '%s\\n' '{PRIVATE}'; printf '%s\\n' '{PRIVATE}' >&2"
    ));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(PRIVATE)
            && String::from_utf8_lossy(&output.stderr).contains(PRIVATE),
        "native producer must deliver private bytes on both real pipes"
    );

    // The same private bytes also arrive in a real control record. The failure that
    // the real fixture reports for it is a typed site and cannot carry them.
    let (_root, listener, address) = native_listener();
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let peer = thread::spawn(move || {
        let mut peer = UnixStream::connect(&address).expect("connect real Unix control peer");
        peer.set_write_timeout(Some(RESCUE_LIMIT))
            .expect("bounded control writer");
        peer.write_all(format!("HELLO 54321 {PRIVATE}\nDESCENDANT 65432\n").as_bytes())
            .expect("write hostile control records");
        // Keep the live peer through stream normalization; see the closed-peer test.
        let _ = release_rx.recv_timeout(RESCUE_LIMIT);
    });
    let failure = RecoveryGeneration::try_accept(&listener)
        .map(|_| ())
        .expect_err("a HELLO app link without a token separator must be refused");
    let _ = release_tx.send(());
    peer.join().expect("join hostile control peer");
    assert_eq!(failure, RecoveryControlFailure::HelloAppLinkMalformed);

    let result = replacement_control_failure_summary(failure, &output);
    assert!(
        !result.contains(PRIVATE),
        "recovery diagnostics leaked private native process bytes"
    );
    assert!(
        result.contains("error_site=HelloAppLinkMalformed")
            && result.contains("host_success=true")
            && result.contains("stdout_bytes=")
            && result.contains("stderr_bytes="),
        "recovery failure must retain the typed site and count-only diagnostic evidence"
    );
}

#[test]
fn recovery_control_failure_callsite_uses_only_safe_summary() {
    // Source-binding catches a regression where a safe helper still exists
    // but the actual panic silently starts formatting the raw Output again.
    let source = include_str!("../recovery_cycle.rs");
    assert!(
        source.contains("panic!(\"{}\", replacement_control_failure_summary(error, &output));"),
        "replacement failure panic must consume only the sanitized summary"
    );
    assert!(
        !source.contains("panic!(\"{error}; host output: {output:?}\")"),
        "replacement failure cannot inline private stdout or stderr"
    );
}

#[test]
fn real_peer_closed_before_accept_names_the_socket_option_site_not_a_deadline() {
    // Darwin refuses SO_RCVTIMEO on an accepted stream whose peer already closed with
    // EINVAL. That is a socket-option error, not a timeout, and not a bad HELLO.
    let (_root, listener, address) = native_listener();
    drop(UnixStream::connect(&address).expect("connect then close a real Unix peer"));
    let failure =
        collect_initial_control_until(&listener, Instant::now() + RESCUE_LIMIT, &mut |_| {})
            .map(|_| ())
            .expect_err("a peer closed before accept must fail at the read-timeout option");
    assert_eq!(
        failure,
        RecoveryControlFailure::SetReadTimeout(ErrorKind::InvalidInput)
    );
    let summary = replacement_control_failure_summary(failure, &owned_output("true"));
    assert!(
        summary.contains("error_site=SetReadTimeout(InvalidInput)")
            && !summary.contains("Deadline"),
        "{summary}"
    );
}

#[test]
fn real_clock_expiry_after_the_final_normalize_is_a_deadline_not_a_normalize_failure() {
    let (_root, listener, address) = native_listener();
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let peer = thread::spawn(move || {
        let mut peer = UnixStream::connect(&address).expect("connect real Unix control peer");
        peer.set_write_timeout(Some(RESCUE_LIMIT))
            .expect("bounded control writer");
        peer.write_all(b"HELLO 54321 /tmp/owned-fixture.sock#test-only\nDESCENDANT 65432\n")
            .expect("write timely control records");
        // The live peer stays through normalization so only the clock can fail.
        let _ = release_rx.recv_timeout(RESCUE_LIMIT + RESCUE_LIMIT);
    });
    let deadline = Instant::now() + RESCUE_LIMIT;
    let failure = collect_initial_control_until(&listener, deadline, &mut |stage| {
        if let NativeRecoveryStage::AfterNormalize = stage {
            // Cross the original clock after both records and the blocking-mode
            // switch succeeded, the way the late-accept and late-LF cuts do.
            while Instant::now() < deadline {
                thread::yield_now();
            }
        }
    })
    .map(|_| ())
    .expect_err("the original clock expired after the final normalize");
    let _ = release_tx.send(());
    peer.join().expect("join control peer");
    assert_eq!(failure, RecoveryControlFailure::NormalizedAfterDeadline);
    let summary = replacement_control_failure_summary(failure, &owned_output("true"));
    assert!(
        summary.contains("error_site=NormalizedAfterDeadline") && !summary.contains("Normalize("),
        "{summary}"
    );
}

#[test]
fn real_native_eof_before_lf_names_the_eof_site_not_unclassified() {
    let (mut reader, mut writer) = UnixStream::pair().expect("owned Unix byte pair");
    reader.set_nonblocking(true).expect("poll native line");
    writer
        .write_all(b"HELLO 1")
        .expect("write an unfinished record");
    drop(writer);
    let failure =
        read_initial_line_until(&mut reader, Instant::now() + RESCUE_LIMIT, 0, &mut |_| {})
            .expect_err("peer EOF before LF must be refused");
    assert_eq!(failure, RecoveryControlFailure::LineEof);
    let summary = replacement_control_failure_summary(failure, &owned_output("true"));
    assert!(summary.contains("error_site=LineEof"), "{summary}");
}
