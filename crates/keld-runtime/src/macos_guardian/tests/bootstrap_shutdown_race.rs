//! Native host S emitted after genuine KGC1 authentication but before the
//! Guardian initializes its supervisor. Bootstrap must not misread that S.
use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use super::super::host_liveness::{
    observe_failed_initial_preparation_shutdown, observe_pre_generation_shutdown,
};
use super::super::{
    GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, RuntimeError, SUPERVISED_ORDERLY_ACK,
    SUPERVISED_ORDERLY_SHUTDOWN, SUPERVISED_QUIT_ACCEPTED, run_guarded_primary,
};
use super::test_support::verified_real_bun;

const ROLE_ENV: &str = "KELD_TEST_GUARDED_IMMEDIATE_SHUTDOWN_ROLE";
const GATE_ENV: &str = "KELD_TEST_GUARDED_POST_AUTH_GATE";
const POST_PROBE_GATE_ENV: &str = "KELD_TEST_GUARDED_POST_IDLE_PROBE_GATE";
const FACTORY_ERROR_GATE_ENV: &str = "KELD_TEST_GUARDED_FACTORY_ERROR_GATE";
const BUN_MARKER_ENV: &str = "KELD_TEST_GUARDED_EARLY_BUN_MARKER";
const VERIFIED_BUN_ENV: &str = "KELD_TEST_GUARDED_VERIFIED_BUN_PATH";
const POST_S_SPAWN_DENIAL_ENV: &str = "KELD_TEST_GUARDED_POST_S_SPAWN_DENIAL";
const ROLE_TEST: &str =
    "macos_guardian::tests::bootstrap_shutdown_race::immediate_shutdown_guardian_role";

// Test-only injection runs after the genuine Guardian control authentication,
// before supervisor bootstrap. It sends no authority records; a parent-held
// stream releases the real Guardian after host S was actually flushed.
fn hold_guardian_at_stage_gate(environment: &str, marker: &str) -> Result<(), RuntimeError> {
    let Some(path) = std::env::var_os(environment) else {
        return Ok(());
    };
    let mut stream = UnixStream::connect(path).map_err(|source| {
        super::super::lifecycle_error("macOS test post-auth gate connect", source)
    })?;
    writeln!(stream, "{marker} {}", std::process::id())
        .map_err(|source| super::super::lifecycle_error("macOS test post-auth notice", source))?;
    stream
        .set_read_timeout(Some(GUARDIAN_SHUTDOWN_DEADLINE))
        .map_err(|source| super::super::lifecycle_error("macOS test post-auth deadline", source))?;
    let mut release = [0_u8; 1];
    stream
        .read_exact(&mut release)
        .map_err(|source| super::super::lifecycle_error("macOS test post-auth release", source))?;
    if release != *b"R" {
        return Err(super::super::lifecycle_error(
            "macOS test post-auth release",
            io::Error::new(io::ErrorKind::PermissionDenied, "invalid test release"),
        ));
    }
    Ok(())
}

pub(in crate::macos_guardian) fn hold_guardian_after_authenticated_control()
-> Result<(), RuntimeError> {
    hold_guardian_at_stage_gate(GATE_ENV, "AUTHENTICATED")
}

pub(in crate::macos_guardian) fn hold_guardian_after_idle_probe() -> Result<(), RuntimeError> {
    hold_guardian_at_stage_gate(POST_PROBE_GATE_ENV, "PROBED")
}

#[test]
fn immediate_shutdown_guardian_role() {
    let Some(marker) = std::env::var_os(ROLE_ENV) else {
        return;
    };
    // Only the independent test parent may launch Bun --version. Running
    // that preflight *inside* the Guardian would defeat the negative oracle
    // that no Bun image executes before authentic host admission.
    let bun = std::env::var_os(VERIFIED_BUN_ENV)
        .map(std::path::PathBuf::from)
        .expect("parent verified native Bun before the Guardian was spawned");
    // libtest prints its own progress on stdout. The fixed shell launcher
    // saved the real KSA pipe on fd 3 before redirecting all harness output
    // to stderr, so no fake "running 1 test" bytes can satisfy an ACK.
    let ack = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/fd/3")
        .expect("original OS shutdown ACK writer");
    let factory_fault = std::env::var_os(FACTORY_ERROR_GATE_ENV).is_some();
    let post_s_spawn_denial = std::env::var_os(POST_S_SPAWN_DENIAL_ENV).is_some();
    let result = run_guarded_primary(
        io::stdin().lock(),
        move |_link| {
            if factory_fault {
                // Only reached after the real host minted and replied to
                // initial KGC1 PREPARE. Release once the host sent real S.
                hold_guardian_at_stage_gate(FACTORY_ERROR_GATE_ENV, "PREPARED")?;
                return Err(super::super::lifecycle_error(
                    "test native factory failure after PREPARED",
                    // A local factory can also return UnexpectedEof. Its I/O
                    // kind must never be confused with the host-rejected
                    // first PREPARE transport EOF after valid PREPARED.
                    io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "intentional native factory startup failure",
                    ),
                ));
            }
            let mut command = Command::new(&bun);
            command
                .args(["-e", "require('node:fs').writeFileSync(process.env.KELD_TEST_GUARDED_EARLY_BUN_MARKER, 'EXECUTED')"])
                .env(BUN_MARKER_ENV, &marker);
            Ok(command)
        },
        ack,
    );
    if factory_fault {
        // A separate authenticated parent-side pipe and marker also prove
        // that the *original* native factory fault survived cleanup and S,
        // rather than accepting an unrelated Guardian/worker failure as
        // adequate proof just because both paths return Err.
        let original_fault_retained = result.as_ref().err().is_some_and(|error| {
            let report = error.to_string();
            report.contains("test native factory failure after PREPARED")
                && report.contains("intentional native factory startup failure")
        });
        if original_fault_retained {
            eprintln!("KELD_TEST_FACTORY_ERROR_RETAINED");
        } else {
            eprintln!("KELD_TEST_FACTORY_ERROR_SUPPRESSED {result:?}");
        }
    } else if post_s_spawn_denial {
        // The host's native registration refusal can make this initial
        // spawn fail before Supervisor startup. The independent parent
        // proves no REGISTERED was emitted and the pre-exec group retired.
        // A missing KSA must not be converted into a false clean result.
        let refusal = result.expect_err("host-denied SPAWNED must not start Bun cleanly");
        eprintln!("KELD_TEST_POST_S_SPAWN_TERMINAL {refusal}");
    } else {
        assert!(
            result.is_ok(),
            "authenticated early S must be handled as orderly shutdown, not malformed bootstrap: {result:?}"
        );
    }
}

fn receive_held_milestone(listener: &UnixListener, marker: &str) -> (UnixStream, u32) {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_nonblocking(true)
                    .expect("bounded test-only receipt");
                let mut bytes = Vec::with_capacity(36);
                loop {
                    let mut byte = [0_u8; 1];
                    match stream.read(&mut byte) {
                        Ok(1) if byte[0] == b'\n' => break,
                        Ok(1) => {
                            bytes.push(byte[0]);
                            assert!(bytes.len() <= 64, "bounded native authentication receipt");
                        }
                        Ok(0) => panic!("actual Guardian closed partial authentication receipt"),
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                        Err(e) => panic!("read held-authentication notice: {e}"),
                        Ok(_) => unreachable!("one-byte receipt"),
                    }
                    assert!(
                        Instant::now() < deadline,
                        "partial authentication notice timed out"
                    );
                    thread::yield_now();
                }
                let receipt = String::from_utf8(bytes).expect("ASCII authenticated notice");
                let raw = receipt
                    .strip_prefix(&format!("{marker} "))
                    .expect("native authenticated milestone");
                return (stream, raw.parse().expect("actual Guardian process ID"));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "Guardian did not reach KGC1 auth gate"
                );
                thread::yield_now();
            }
            Err(e) => panic!("accept native Guardian-auth milestone: {e}"),
        }
    }
}

fn assert_native_held_shutdown(gate_env: &str, marker: &str) {
    let temp = tempfile::tempdir().expect("private immediate-shutdown witness");
    let socket = temp.path().join("authenticated.sock");
    let bun_marker = temp.path().join("must-not-exec-bun");
    let listener = UnixListener::bind(&socket).expect("own auth gate listener");
    listener
        .set_nonblocking(true)
        .expect("bounded auth observer");
    let bun = verified_real_bun();
    let mut command = Command::new("/bin/sh");
    command
        // The libtest runner must never write its status banner into the
        // shipping Guardian's KSA/KQA acknowledgment pipe. Retain only that
        // original owned pipe on fd 3 before execing the actual test binary.
        .args(["-c", "exec 3>&1; exec 1>&2; exec \"$@\"", "guardian-test-proxy"])
        .arg(std::env::current_exe().expect("native unit test process"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, &bun_marker)
        .env(VERIFIED_BUN_ENV, &bun)
        .env(gate_env, &socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let pending = GuardianBootstrap::spawn_supervised(command).expect("real KGC1 Guardian");
    let mut primary = pending
        .register_guarded_primary_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("host owns authenticated real Guardian");
    let (mut held, real_guardian_pid) = receive_held_milestone(&listener, marker);
    assert_eq!(real_guardian_pid, primary.guardian_pid());
    let (sent_tx, sent_rx) = mpsc::sync_channel(1);
    primary.shutdown_sent = Some(sent_tx);
    let shutdown = thread::spawn(move || {
        let result = primary.shutdown();
        (result, primary)
    });
    // A true shipping-path write/flush receipt, not a scheduling sleep:
    // Guardian remains blocked after its authentic link while S is queued.
    sent_rx
        .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("actual host S must be flushed before Guardian bootstrap resumes");
    assert!(
        !bun_marker.exists(),
        "real Bun executed despite host-requested shutdown before initial admission"
    );
    held.write_all(b"R")
        .expect("release Guardian after actual host S");
    let (outcome, mut primary) = shutdown.join().expect("host shutdown task joined");
    let status = outcome.expect("real Guardian must acknowledge concurrent shutdown");
    assert!(
        status.success(),
        "role failed genuine Q/S shutdown receipt: {status:?}"
    );
    assert!(
        primary.group_pid().is_none(),
        "no initial Bun group may be retained"
    );
    assert!(
        primary
            .child
            .try_wait()
            .expect("Guardian terminal status")
            .is_some(),
        "exact host Guardian process was reaped"
    );
    assert!(
        !bun_marker.exists(),
        "early shutdown permitted the Bun JavaScript marker"
    );
}

#[test]
fn native_host_orderly_shutdown_immediately_after_guardian_auth_is_acked() {
    assert_native_held_shutdown(GATE_ENV, "AUTHENTICATED");
}

#[test]
fn native_host_orderly_shutdown_after_idle_probe_before_first_prepare_is_acked() {
    assert_native_held_shutdown(POST_PROBE_GATE_ENV, "PROBED");
}

#[test]
fn pre_generation_quit_is_refused_without_forging_a_guardian_ack() {
    let (reader, writer) = nix::unistd::pipe().expect("native liveness pipe");
    rustix::io::write(&writer, &[SUPERVISED_QUIT_ACCEPTED])
        .expect("deliver unapproved early accepted-Quit byte");
    let mut reader = File::from(reader);
    let mut ack = Vec::new();
    let error = observe_pre_generation_shutdown(&mut reader, &mut ack)
        .expect_err("Quit cannot be accepted before a generation exists");
    assert!(
        error.to_string().contains("before the first authorized"),
        "{error}"
    );
    assert!(ack.is_empty(), "unattributed Quit must not forge KQA/KSA");
    drop(writer);
}

#[test]
fn pre_generation_duplicate_orderly_control_cannot_acknowledge_twice() {
    let (reader, writer) = nix::unistd::pipe().expect("native liveness pipe");
    rustix::io::write(
        &writer,
        &[SUPERVISED_ORDERLY_SHUTDOWN, SUPERVISED_ORDERLY_SHUTDOWN],
    )
    .expect("deliver two actual host-control bytes");
    let mut reader = File::from(reader);
    let mut ack = Vec::new();
    let error = observe_pre_generation_shutdown(&mut reader, &mut ack)
        .expect_err("second S before host EOF violates exclusive shutdown");
    assert!(
        error.to_string().contains("duplicate or conflicting"),
        "{error}"
    );
    assert_eq!(ack, SUPERVISED_ORDERLY_ACK, "first valid S alone gets KSA");
    drop(writer);
}

#[test]
fn malformed_pre_auth_liveness_has_priority_over_private_registration() {
    let (reader, writer) = nix::unistd::pipe().expect("native host-only liveness pipe");
    rustix::io::write(&writer, &[SUPERVISED_ORDERLY_SHUTDOWN])
        .expect("attempt S before authenticated KGC1");
    let reader = File::from(reader);
    let result = run_guarded_primary(
        reader,
        |_link| panic!("no private auth or Bun factory may start for malformed bootstrap"),
        io::sink(),
    );
    let error = result.expect_err("unattributed pre-auth S cannot become valid shutdown");
    assert!(
        error.to_string().contains("liveness descriptor validation"),
        "private socket must not be used before liveness admission: {error}"
    );
    drop(writer);
}

#[test]
fn no_host_s_after_failed_initial_preparation_does_not_forge_ksa() {
    let (reader, writer) = nix::unistd::pipe().expect("owned live host liveness pipe");
    let mut reader = File::from(reader);
    let mut ack = Vec::new();
    assert!(
        !observe_failed_initial_preparation_shutdown(&mut reader, &mut ack)
            .expect("live host with no S leaves startup error intact"),
        "a failed generation exchange is not an orderly-shutdown request"
    );
    assert!(ack.is_empty(), "no host S must mean no KSA");
    drop(writer);
    assert!(
        !observe_failed_initial_preparation_shutdown(&mut reader, &mut ack)
            .expect("bare host death retains the original startup failure"),
        "bare EOF is not an orderly shutdown"
    );
    assert!(ack.is_empty(), "bare EOF must not forge KSA");
}

#[path = "bootstrap_shutdown_race/factory_failure.rs"]
mod factory_failure;

#[path = "bootstrap_shutdown_race/denial_order.rs"]
mod denial_order;

#[path = "bootstrap_shutdown_race/post_s_spawn.rs"]
mod post_s_spawn;
