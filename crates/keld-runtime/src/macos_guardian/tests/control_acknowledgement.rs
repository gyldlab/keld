use std::io::{self, Write as _};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::macos_guardian::generation_host::{
    check_initial_recovery_decision, is_initial_recovery_denied,
};

use super::super::{
    CONTROL_CLEAR, CONTROL_PREPARE, CONTROL_PREPARED, GENERATION_CONTROL_POLL,
    GUARDIAN_SHUTDOWN_DEADLINE, GenerationControlPeer, GenerationControlReader,
    GenerationControlRecord, GuardedPrimary, HostGenerationState, RecoveryState,
    SUPERVISED_ORDERLY_ACK, SUPERVISED_ORDERLY_SHUTDOWN, SUPERVISED_QUIT_ACCEPTED,
    SUPERVISED_QUIT_ACK, SUPERVISED_QUIT_ACK_DEADLINE, control_exchange,
    finish_generation_control_eof, lock_recovery, read_generation_control, runtime_io_kind,
    write_generation_control,
};

fn owned_acknowledgment_primary() -> GuardedPrimary {
    acknowledging_primary(SUPERVISED_QUIT_ACCEPTED, SUPERVISED_QUIT_ACK)
}

/// A real Bun peer on the host's own liveness/acknowledgment pipes that
/// requires exactly `control` as its first byte, answers with `ack`, and then
/// exits zero at the host's liveness EOF.
fn acknowledging_primary(control: u8, ack: [u8; 3]) -> GuardedPrimary {
    // This owns a real ACK pipe peer, not a guardian/native mock claim.
    // The staged-Host case above separately proves the actual guardian.
    let ack = std::str::from_utf8(&ack).expect("ASCII acknowledgment");
    let script = format!(
        r#"
const input = Bun.stdin.stream().getReader();
const first = await input.read();
if (first.done || first.value.byteLength !== 1 || first.value[0] !== {control}) process.exit(2);
process.stdout.write("{ack}");
while (!(await input.read()).done) {{}}
"#
    );
    let mut child = Command::new(super::test_support::verified_real_bun())
        .args(["-e", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("owned Q ACK producer");
    let writer = child.stdin.take().expect("sole Q/liveness writer");
    let ack = child.stdout.take().expect("sole actual ACK reader");
    let (_tx, updates_rx) = mpsc::channel();
    GuardedPrimary {
        child,
        liveness_writer: Some(writer),
        quit_ack_reader: ack,
        current_generation: Arc::new(Mutex::new(None)),
        recovery_decision: Arc::new(Mutex::new(RecoveryState::new())),
        updates_rx,
        worker: None,
        shutdown_attributed: false,
        shutdown_failed: false,
        orderly_acknowledged: false,
        shutdown_sent: None,
        #[cfg(test)]
        shutdown_denial_fence: None,
    }
}

#[test]
fn control_eof_acknowledgment_is_order_independent_and_attributed() {
    for ack_first in [true, false] {
        let (mut host, peer) = std::os::unix::net::UnixStream::pair().expect("real EOF pair");
        let mut primary = owned_acknowledgment_primary();
        primary.deny_recovery();
        if ack_first {
            primary.accept_shutdown().expect("actual Q ACK before EOF");
        }
        let state_for_reader = Arc::clone(&primary.recovery_decision);
        let (observed_tx, observed_rx) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let mut reader = GenerationControlReader::new();
            let eof = reader.poll(&mut host).expect_err("actual peer EOF");
            let ack_at_eof = lock_recovery(&state_for_reader).shutdown_acknowledged;
            observed_tx
                .send(ack_at_eof)
                .expect("record EOF before later ACK");
            finish_generation_control_eof(
                eof,
                reader.filled,
                &mut HostGenerationState::default(),
                &Mutex::new(None),
                &state_for_reader,
            )
        });
        drop(peer);
        let observed = observed_rx
            .recv_timeout(SUPERVISED_QUIT_ACK_DEADLINE)
            .expect("real EOF receipt");
        if !ack_first {
            primary
                .accept_shutdown()
                .expect("actual Q ACK after observed EOF");
        }
        let result = reader.join().expect("one owned EOF reader joins");
        let status = primary.shutdown().expect("ACK peer EOF and owned reap");
        let repeat_status = primary
            .shutdown()
            .expect("a successful attributed shutdown remains an idempotent success");
        println!(
            "KEL143_RUNTIME_ACK_EOF ack_first={ack_first} ack_at_eof={observed} peer_status={status:?}"
        );
        assert_eq!(observed, ack_first);
        result.expect("either ordered actual EOF/ACK completes cleanly");
        assert!(status.success());
        assert!(repeat_status.success());
    }
}

/// The host's acknowledgment for a startup denied before its first PREPARE.
#[derive(Clone, Copy)]
enum DenialAcknowledgment {
    /// The host's orderly S, answered by the peer's genuine KSA.
    OrderlyS,
    /// An accepted Quit (Q), answered by the peer's genuine KQA.
    AcceptedQuit,
}

/// Runs one host shutdown whose generation worker ends with the product's
/// own typed first-PREPARE recovery denial, with no registered group.
fn denied_first_prepare_shutdown(
    acknowledgment: DenialAcknowledgment,
) -> Result<std::process::ExitStatus, crate::RuntimeError> {
    let mut primary = match acknowledgment {
        DenialAcknowledgment::OrderlyS => {
            acknowledging_primary(SUPERVISED_ORDERLY_SHUTDOWN, SUPERVISED_ORDERLY_ACK)
        }
        DenialAcknowledgment::AcceptedQuit => {
            acknowledging_primary(SUPERVISED_QUIT_ACCEPTED, SUPERVISED_QUIT_ACK)
        }
    };
    let recovery = Arc::clone(&primary.recovery_decision);
    primary.worker = Some(std::thread::spawn(move || {
        // The worker's real first-PREPARE decision, taken once the host has
        // denied recovery (an observed state, bounded by the shutdown budget).
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        loop {
            let state = lock_recovery(&recovery);
            if state.denied {
                return check_initial_recovery_decision(&state, Some(deadline));
            }
            drop(state);
            if Instant::now() >= deadline {
                return Err(crate::macos_guardian::lifecycle_error(
                    "test first-PREPARE decision",
                    io::Error::other("host never denied recovery"),
                ));
            }
            std::thread::yield_now();
        }
    }));
    if matches!(acknowledgment, DenialAcknowledgment::AcceptedQuit) {
        primary.accept_shutdown()?;
    }
    primary.shutdown()
}

#[test]
fn verified_ksa_reconciles_the_first_prepare_recovery_denial() {
    // Architecture 06: the host accepts its privately typed first-PREPARE
    // recovery-denial result after verified KSA, permanent initial denial,
    // and no registered group.
    let status = denied_first_prepare_shutdown(DenialAcknowledgment::OrderlyS)
        .expect("an S acknowledged by KSA retires the denied first PREPARE");
    assert!(status.success(), "{status:?}");
}

#[test]
fn accepted_quit_cannot_reconcile_the_first_prepare_recovery_denial() {
    // A KQA sets the same acknowledged flag as KSA but is not the verified
    // KSA the specification requires, so the typed denial stays fatal.
    let error = denied_first_prepare_shutdown(DenialAcknowledgment::AcceptedQuit)
        .expect_err("a Quit acknowledgment cannot make a denied first PREPARE orderly");
    assert!(
        is_initial_recovery_denied(&error),
        "the worker's typed denial must survive the Quit shutdown: {error}"
    );
}

#[test]
fn control_eof_without_ack_cannot_succeed_from_denial() {
    let (mut host, peer) = std::os::unix::net::UnixStream::pair().expect("real EOF pair");
    drop(peer);
    let mut reader = GenerationControlReader::new();
    let eof = reader.poll(&mut host).expect_err("real peer EOF");
    let recovery = Mutex::new(RecoveryState::new());
    lock_recovery(&recovery).deny();
    let began = Instant::now();
    let error = finish_generation_control_eof(
        eof,
        reader.filled,
        &mut HostGenerationState::default(),
        &Mutex::new(None),
        &recovery,
    )
    .expect_err("denial alone cannot fabricate Q ACK");
    assert!(
        began.elapsed() >= SUPERVISED_QUIT_ACK_DEADLINE,
        "existing attribution budget was lost"
    );
    assert_eq!(runtime_io_kind(&error), Some(io::ErrorKind::UnexpectedEof));
}

#[test]
fn real_control_eof_preserves_partial_and_unattributed_failures() {
    for partial in [false, true] {
        let (mut host, mut peer) = std::os::unix::net::UnixStream::pair().expect("real EOF pair");
        if partial {
            let mut encoded = Vec::new();
            write_generation_control(
                &mut encoded,
                &GenerationControlRecord::new(CONTROL_CLEAR, 1),
            )
            .expect("canonical fixed record");
            peer.write_all(&encoded[..1]).expect("actual partial input");
        }
        drop(peer);
        let mut reader = GenerationControlReader::new();
        let eof = reader
            .poll(&mut host)
            .expect_err("actual partial/unattributed EOF");
        assert_eq!(reader.filled, usize::from(partial));
        let recovery = Mutex::new(RecoveryState::new());
        if partial {
            let mut recovery = lock_recovery(&recovery);
            recovery.deny();
            recovery.shutdown_acknowledged = true;
        }
        let error = finish_generation_control_eof(
            eof,
            reader.filled,
            &mut HostGenerationState::default(),
            &Mutex::new(None),
            &recovery,
        )
        .expect_err("unowned/partial closure remains fatal");
        assert_eq!(runtime_io_kind(&error), Some(io::ErrorKind::UnexpectedEof));
    }
}

#[test]
fn successor_prepare_zero_byte_reply_wait_outlives_transition_budget() {
    let (client, mut peer) = std::os::unix::net::UnixStream::pair().expect("real control pair");
    client
        .set_read_timeout(Some(GENERATION_CONTROL_POLL))
        .expect("existing read poll");
    client
        .set_write_timeout(Some(GENERATION_CONTROL_POLL))
        .expect("existing write poll");
    peer.set_read_timeout(Some(GUARDIAN_SHUTDOWN_DEADLINE))
        .expect("bounded request receipt");
    peer.set_write_timeout(Some(GENERATION_CONTROL_POLL))
        .expect("existing reply write poll");
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client_thread = std::thread::spawn(move || {
        let control = Arc::new(Mutex::new(GenerationControlPeer {
            stream: client,
            reader: GenerationControlReader::new(),
        }));
        let result = control_exchange(
            &control,
            &GenerationControlRecord::new(CONTROL_PREPARE, 2),
            CONTROL_PREPARED,
            2,
        );
        let _ = result_tx.send(result);
    });
    let request = read_generation_control(&mut peer);
    let began = Instant::now();
    let release_at = began + GUARDIAN_SHUTDOWN_DEADLINE + Duration::from_secs(1);
    let mut early = None;
    let mut disconnected = false;
    if request.is_ok() {
        // The request receipt, not a guessed startup delay, starts this
        // independent measured interval. No reply bytes exist before release.
        while Instant::now() < release_at {
            match result_rx.try_recv() {
                Ok(result) => {
                    early = Some(result);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
    }
    let held = began.elapsed();
    let reply = GenerationControlRecord::new(CONTROL_PREPARED, 2)
        .with_payload(b"test-prepared-reply".to_vec());
    let written = if request.is_ok() && early.is_none() && !disconnected {
        Some(write_generation_control(&mut peer, &reply))
    } else {
        None
    };
    let terminal = if written.as_ref().is_some_and(Result::is_ok) {
        Some(result_rx.recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE))
    } else {
        None
    };
    // Closing the retained peer makes every unfinished read observe real
    // EOF. Join before the first assertion, including early-error schedules.
    drop(peer);
    let joined = client_thread.join();
    println!(
        "KEL143_ZERO_BYTE held_ns={} early={early:?} disconnected={disconnected} written={written:?} terminal={terminal:?}",
        held.as_nanos()
    );
    joined.expect("sole control reader joined");
    let request = request.expect("actual complete Prepare2 receipt");
    assert_eq!(request.kind, CONTROL_PREPARE);
    assert_eq!(request.attempt, 2);
    assert_eq!(request.pid, 0);
    assert_eq!(request.cause, 0);
    assert_eq!(request.payload, Vec::<u8>::new());
    assert!(
        early.is_none() && !disconnected,
        "zero-byte successor completed before owned reply release: {early:?}"
    );
    assert!(held >= GUARDIAN_SHUTDOWN_DEADLINE + Duration::from_secs(1));
    written
        .expect("one owned response release")
        .expect("complete canonical Prepared2 write");
    let response = terminal
        .expect("post-release result")
        .expect("bounded result collection")
        .expect("actual successor exchange succeeded");
    assert_eq!(response.kind, CONTROL_PREPARED);
    assert_eq!(response.attempt, 2);
    assert_eq!(response.pid, 0);
    assert_eq!(response.cause, 0);
    assert_eq!(response.payload, reply.payload);
}
