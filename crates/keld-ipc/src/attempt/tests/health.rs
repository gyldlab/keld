//! The health records and the window at the owner, on real Windows pipes
//! after `KELD-AR1` (KEL-53 §4 "Candidate connect-back": *Health records* and
//! *Health sequence*; §7 "8 (keld-attempt codec)" health cells and "8 (health
//! sequence)"): `KELD-AB1` and `KELD-AY1` are read by position and `KELD-AB1`
//! must match exactly, a `KELD-AF1` class is admitted only at its position,
//! one byte, end of file or a launch exit fails the window, and steps out of
//! order are refused before any I/O.
//!
//! Oracles: record bytes laid out by the spec table's offsets, a candidate
//! that writes or withholds bytes itself, and the exact typed refusal. The
//! window's exact edges are the injected-clock table in `window/tests.rs`;
//! here the real window runs with a shortened length only where a scenario
//! needs it to end, and the shipped 30-second window otherwise, as a kill
//! switch on a refusal that must come first.

use std::io::{self, Write as _};
use std::time::{Duration, Instant};

use super::peer::TestPin;
use super::peer::{
    DIGEST, SHORT_WINDOW, claimed_pair, expect_end_of_file, is_end_of_file, kill_switch, ready_pair,
};
use crate::attempt::records::{AttemptReadPosition, AttemptRecord, AttemptRecordError};
use crate::attempt::{
    AttemptFailureClass, AttemptHealthWindowFailure, WindowsAttemptExchangeError,
    WindowsAttemptOwnerChannel, WindowsAttemptRollBack,
};

fn window_failure(result: Result<(), WindowsAttemptExchangeError>) -> AttemptHealthWindowFailure {
    match result {
        Err(WindowsAttemptExchangeError::HealthWindow { failure, .. }) => failure,
        other => panic!("expected a health-window refusal, got {other:?}"),
    }
}

/// §7 "8 (health sequence)": one byte after `KELD-AY1` fails health. The
/// shipped 30-second window runs; the refusal comes as the byte arrives.
#[test]
fn one_byte_after_ay1_fails_health() -> io::Result<()> {
    let (mut owner, mut candidate, _) = ready_pair(&DIGEST)?;
    candidate.stream_mut().write_all(&[0x00])?;
    let started = Instant::now();
    let failure = window_failure(owner.await_health_window(Duration::ZERO));
    assert_eq!(failure, AttemptHealthWindowFailure::ByteReceived);
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "the byte did not end the window"
    );
    Ok(())
}

/// The connection closing during the window fails health, and the rollback
/// then writes nothing.
#[test]
fn end_of_file_during_the_window_fails_health_and_rollback_sends_nothing() -> io::Result<()> {
    let (mut owner, candidate, _) = ready_pair(&DIGEST)?;
    drop(candidate);
    let failure = window_failure(owner.await_health_window(Duration::ZERO));
    assert_eq!(failure, AttemptHealthWindowFailure::EndOfFile);
    assert!(matches!(
        owner.roll_back(),
        WindowsAttemptRollBack::NotSentAfterEndOfFile
    ));
    Ok(())
}

/// A signaled launch handle at the window's end fails health even though
/// the connection stayed open and silent, and the rollback writes nothing.
#[test]
fn a_launch_exit_by_the_window_end_fails_health() -> io::Result<()> {
    let (mut owner, mut candidate, pin) = ready_pair(&DIGEST)?;
    pin.report_exit();
    let failure = window_failure(owner.await_window_of(SHORT_WINDOW));
    assert_eq!(failure, AttemptHealthWindowFailure::LaunchExited);
    assert!(matches!(
        owner.roll_back(),
        WindowsAttemptRollBack::NotSentAfterLaunchExit
    ));
    expect_end_of_file(candidate.stream_mut())
}

/// §7 codec row: a one-field mutation of `KELD-AB1` is refused, for each of
/// its three fields at the spec table's offsets, and so is a digest other
/// than the journaled one.
#[test]
fn a_boot_acknowledgement_that_differs_in_one_field_is_refused() -> io::Result<()> {
    for offset in [8, 40, 72] {
        let (mut owner, mut candidate, _) = claimed_pair()?;
        let transcript = *candidate.transcript();
        let mut boot = Vec::new();
        AttemptRecord::BootAcknowledgement(
            crate::attempt::records::AttemptBootAcknowledgement::new(
                *transcript.attempt_id(),
                *transcript.health_channel_id(),
                DIGEST,
            ),
        )
        .write_to(&mut boot)
        .map_err(io::Error::other)?;
        boot[offset] ^= 0x01;
        candidate.stream_mut().write_all(&boot)?;
        let result = owner.read_boot(&DIGEST, kill_switch());
        assert!(
            matches!(
                result,
                Err(WindowsAttemptExchangeError::Record(
                    AttemptRecordError::BootMismatch
                ))
            ),
            "offset {offset}: {result:?}"
        );
    }
    let (mut owner, mut candidate, _) = claimed_pair()?;
    candidate.send_boot(&[0x67; 32]).map_err(io::Error::other)?;
    assert!(matches!(
        owner.read_boot(&DIGEST, kill_switch()),
        Err(WindowsAttemptExchangeError::Record(
            AttemptRecordError::BootMismatch
        ))
    ));
    Ok(())
}

/// §7 codec row: a `KELD-AF1` class is admitted only at its position. Before
/// `KELD-AB1` classes `1` and `3` report a failure and class `2` is refused;
/// after it classes `2` and `3` report and class `1` is refused. The
/// candidate refuses to send a class the owner would refuse, before sending.
#[test]
fn a_failure_class_is_admitted_only_at_its_position() -> io::Result<()> {
    use AttemptFailureClass::{ApplicationExitBeforeReady, BootError, BootstrapReadRefused};
    for (booted, class, admitted) in [
        (false, BootstrapReadRefused, true),
        (false, BootError, true),
        (false, ApplicationExitBeforeReady, false),
        (true, ApplicationExitBeforeReady, true),
        (true, BootError, true),
        (true, BootstrapReadRefused, false),
    ] {
        let (mut owner, mut candidate, _) = claimed_pair()?;
        if booted {
            candidate.send_boot(&DIGEST).map_err(io::Error::other)?;
            owner
                .read_boot(&DIGEST, kill_switch())
                .map_err(io::Error::other)?;
        }
        let read = |owner: &mut WindowsAttemptOwnerChannel<TestPin>| {
            if booted {
                owner.read_ready(kill_switch())
            } else {
                owner.read_boot(&DIGEST, kill_switch())
            }
        };
        if admitted {
            candidate.send_failure(class).map_err(io::Error::other)?;
            let result = read(&mut owner);
            assert!(
                matches!(result, Err(WindowsAttemptExchangeError::CandidateFailure { class: reported }) if reported == class),
                "{class:?}: {result:?}"
            );
        } else {
            let refused = candidate.send_failure(class);
            assert!(
                matches!(
                    refused,
                    Err(WindowsAttemptExchangeError::Record(
                        AttemptRecordError::FailureClassNotAdmitted { .. }
                    ))
                ),
                "{class:?}: {refused:?}"
            );
            candidate.stream_mut().write_all(&[
                b'K',
                b'E',
                b'L',
                b'D',
                b'-',
                b'A',
                b'F',
                b'1',
                class as u8,
            ])?;
            let result = read(&mut owner);
            assert!(
                matches!(
                    result,
                    Err(WindowsAttemptExchangeError::Record(
                        AttemptRecordError::FailureClassNotAdmitted { class: refused, .. }
                    )) if refused == class
                ),
                "{class:?}: {result:?}"
            );
        }
    }
    Ok(())
}

/// §7 codec row: a non-admitted magic followed by no further byte is refused
/// before its deadline: `KELD-AY1` where `KELD-AB1` is due, with the
/// connection then silent and a one-minute read deadline.
#[test]
fn a_non_admitted_magic_then_silence_is_refused_before_its_deadline() -> io::Result<()> {
    let (mut owner, mut candidate, _) = claimed_pair()?;
    candidate.stream_mut().write_all(b"KELD-AY1")?;
    let started = Instant::now();
    let result = owner.read_boot(&DIGEST, Instant::now() + Duration::from_mins(1));
    assert!(
        matches!(
            result,
            Err(WindowsAttemptExchangeError::Record(AttemptRecordError::MagicNotAdmitted {
                position: AttemptReadPosition::OwnerBoot,
                magic,
            })) if magic == *b"KELD-AY1"
        ),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(30));
    Ok(())
}

/// Each step called out of order is refused before any I/O: the record it
/// would have read or written is still the next one on the pipe.
#[test]
fn steps_out_of_order_are_refused_before_any_io() -> io::Result<()> {
    let (mut owner, mut candidate, _) = claimed_pair()?;
    assert!(matches!(
        candidate.send_ready(),
        Err(WindowsAttemptExchangeError::OutOfSequence { .. })
    ));
    assert!(matches!(
        owner.read_ready(kill_switch()),
        Err(WindowsAttemptExchangeError::OutOfSequence { .. })
    ));
    assert!(matches!(
        owner.await_health_window(Duration::ZERO),
        Err(WindowsAttemptExchangeError::OutOfSequence { .. })
    ));
    candidate.send_boot(&DIGEST).map_err(io::Error::other)?;
    owner
        .read_boot(&DIGEST, kill_switch())
        .map_err(io::Error::other)?;
    assert!(matches!(
        candidate.send_boot(&DIGEST),
        Err(WindowsAttemptExchangeError::OutOfSequence { .. })
    ));
    candidate.send_ready().map_err(io::Error::other)?;
    assert!(matches!(
        candidate.send_failure(AttemptFailureClass::BootError),
        Err(WindowsAttemptExchangeError::OutOfSequence { .. })
    ));
    owner.read_ready(kill_switch()).map_err(io::Error::other)?;
    let premature = owner.accept(kill_switch());
    assert!(
        matches!(
            premature,
            Err(WindowsAttemptExchangeError::OutOfSequence { .. })
        ),
        "KELD-AK1 accepted before the window: {premature:?}"
    );
    // Nothing was written: the refused accept dropped the owner's end.
    let unarmed = candidate.await_health_accepted(kill_switch());
    assert!(
        matches!(&unarmed, Err(error) if is_end_of_file(error)),
        "{unarmed:?}"
    );

    let (_owner, candidate, _) = claimed_pair()?;
    assert!(matches!(
        candidate.await_health_accepted(kill_switch()),
        Err(WindowsAttemptExchangeError::OutOfSequence { .. })
    ));
    Ok(())
}

/// After a failure only the rollback remains: the owner refuses the next
/// read, and its rollback still writes `KELD-AK1` rolled back.
#[test]
fn after_a_failure_only_the_rollback_remains() -> io::Result<()> {
    let (mut owner, mut candidate, _) = claimed_pair()?;
    candidate
        .send_failure(AttemptFailureClass::BootError)
        .map_err(io::Error::other)?;
    assert!(matches!(
        owner.read_boot(&DIGEST, kill_switch()),
        Err(WindowsAttemptExchangeError::CandidateFailure { .. })
    ));
    assert!(matches!(
        owner.read_ready(kill_switch()),
        Err(WindowsAttemptExchangeError::OutOfSequence { .. })
    ));
    assert!(matches!(owner.roll_back(), WindowsAttemptRollBack::Sent));
    assert!(matches!(
        candidate.await_health_accepted(kill_switch()),
        Err(WindowsAttemptExchangeError::HealthRolledBack)
    ));
    Ok(())
}
