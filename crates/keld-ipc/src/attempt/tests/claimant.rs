//! The claim's binding and the claimant's checks on real Windows pipes (KEL-53
//! §4 "Candidate connect-back": *Locator*, *Claim* and *Transcript*; §7 "8
//! (keld-attempt codec)" `KELD-AC1` and `KELD-AR1` cells and the handle cell
//! of "8 (claimant binding)"): the owner names its endpoint from the IDs its
//! `KELD-AC1` carries, one claim binds the three IDs, both nonces and both
//! process IDs, and the claimant refuses before `KELD-AA1` a challenge that
//! fails the locator or names another server process, then refuses a receipt
//! that differs from its transcript.
//!
//! Oracles: the spec's purpose-`1` locator golden vector, record bytes laid
//! out by the spec table's offsets, a raw owner that writes and reads records
//! itself, end of file observed on the raw owner's own end, and the
//! independent handle-flag read.

use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::HANDLE_FLAG_INHERIT;

use super::oracle::independent_handle_and_pipe_flags;
use super::peer::{
    Ids, TestPin, admit_own, claim, connect_back_endpoint, expect_end_of_file, fresh_ids, join,
    kill_switch, per_user_security, pipe_token, read, spawn_admitting_owner, spawn_owner, write,
};
use crate::APP_LINK_IO_DEADLINE;
use crate::attempt::claim::ClaimRefusal;
use crate::attempt::records::{
    AttemptChallenge, AttemptReadPosition, AttemptRecord, AttemptRecordError,
};
use crate::attempt::{
    AttemptFailureClass, AttemptHealthWindowFailure, ConnectBackIds, WindowsAttemptEndpoint,
    WindowsAttemptEndpointError, WindowsAttemptExchangeError,
};
use crate::token::SessionToken;
use crate::windows_named_pipe::WaitOutcome;

/// KEL-53 §4 *Locator* golden vector: installation `11`×32, attempt `22`×32,
/// health channel `33`×32, purpose `1`.
const GOLDEN_NAME: &str =
    r"\\.\pipe\keld-attempt-a56a565b56c571bd19b06b8e62845fa5a14c28fecd5611c94e4c90e8a1641ba3";

/// The owner names its endpoint from exactly the IDs that its `KELD-AC1`
/// carries: the golden IDs give the golden name, and the shipped claimant,
/// whose locator check runs before `KELD-AA1`, is accepted on it with those
/// IDs in its transcript.
#[test]
fn the_endpoint_name_derives_from_the_ids_its_challenge_carries() -> io::Result<()> {
    let ids = Ids {
        installation: [0x11; 32],
        attempt: [0x22; 32],
        channel: [0x33; 32],
    };
    let endpoint = connect_back_endpoint(ids)?;
    assert_eq!(endpoint.endpoint(), GOLDEN_NAME);
    let pin = TestPin::own()?;
    let (owner, _) = spawn_admitting_owner(endpoint, &pin);
    let candidate = claim(GOLDEN_NAME, &ids.installation)?.map_err(io::Error::other)?;
    let transcript = candidate.transcript();
    assert_eq!(transcript.installation_id(), &[0x11; 32]);
    assert_eq!(transcript.attempt_id(), &[0x22; 32]);
    assert_eq!(transcript.health_channel_id(), &[0x33; 32]);
    let (owner, refusals) = join(owner)?;
    assert!(refusals.is_empty(), "{refusals:?}");
    assert_eq!(owner.map_err(io::Error::other)?.transcript(), transcript);
    Ok(())
}

/// The locator refuses IDs before any pipe exists.
#[test]
fn a_connect_back_endpoint_refuses_ids_the_locator_refuses() -> io::Result<()> {
    let security = per_user_security()?;
    let id = *SessionToken::random()?.as_bytes();
    let other = *SessionToken::random()?.as_bytes();
    for (installation, attempt, channel) in [
        ([0; 32], id, other),
        (id, [0; 32], other),
        (id, other, [0; 32]),
        (id, id, other),
        (id, other, other),
        (other, id, other),
    ] {
        assert!(matches!(
            WindowsAttemptEndpoint::create_connect_back(
                &installation,
                &attempt,
                &channel,
                &security
            ),
            Err(WindowsAttemptEndpointError::Locator(_))
        ));
    }
    Ok(())
}

/// One claim binds all three IDs, both fresh nonces and both process IDs:
/// both ends hold the same transcript, the nonces are fresh per connection,
/// and both process IDs are this process, the connected client and server.
#[test]
fn a_claim_binds_the_ids_both_nonces_and_both_process_ids() -> io::Result<()> {
    let mut nonces = Vec::new();
    for _ in 0..2 {
        let ids = fresh_ids()?;
        let endpoint = connect_back_endpoint(ids)?;
        let name = endpoint.endpoint().to_owned();
        let pin = TestPin::own()?;
        let (owner, _) = spawn_admitting_owner(endpoint, &pin);
        let candidate = claim(&name, &ids.installation)?.map_err(io::Error::other)?;
        let (owner, _) = join(owner)?;
        let owner = owner.map_err(io::Error::other)?;
        let transcript = *candidate.transcript();
        assert_eq!(owner.transcript(), &transcript);
        assert_eq!(transcript.installation_id(), &ids.installation);
        assert_eq!(transcript.attempt_id(), &ids.attempt);
        assert_eq!(transcript.health_channel_id(), &ids.channel);
        assert_eq!(transcript.client_pid(), std::process::id());
        assert_eq!(transcript.server_pid(), std::process::id());
        assert_ne!(transcript.client_nonce(), transcript.server_nonce());
        nonces.push(*transcript.client_nonce());
        nonces.push(*transcript.server_nonce());
    }
    assert_ne!(
        nonces[0], nonces[2],
        "client nonces repeat across connections"
    );
    assert_ne!(
        nonces[1], nonces[3],
        "server nonces repeat across connections"
    );
    Ok(())
}

/// §7 codec row: an `KELD-AC1` whose IDs fail the locator check is refused
/// before `KELD-AA1`. The owner's endpoint is bound to IDs whose health
/// channel differs from the IDs its name derives from; the claimant refuses
/// and closes, and the owner observes end of file where `KELD-AA1` was due.
#[test]
fn a_challenge_whose_ids_fail_the_locator_check_is_refused_before_aa1() -> io::Result<()> {
    let ids = fresh_ids()?;
    let name = crate::attempt::windows_attempt_connect_back_endpoint(
        &ids.installation,
        &ids.attempt,
        &ids.channel,
    )
    .map_err(io::Error::other)?;
    let foreign_channel = ConnectBackIds {
        installation: ids.installation,
        attempt: ids.attempt,
        health_channel: *SessionToken::random()?.as_bytes(),
    };
    let endpoint =
        WindowsAttemptEndpoint::create_named(&name, foreign_channel, &per_user_security()?)
            .map_err(io::Error::other)?;
    let pin = TestPin::own()?;
    let (refusals, refused) = mpsc::channel();
    let deadline = Instant::now() + Duration::from_secs(3);
    let owner = spawn_owner(
        endpoint,
        deadline,
        APP_LINK_IO_DEADLINE,
        admit_own(&pin),
        pipe_token,
        refusals,
    );
    let result = claim(&name, &ids.installation)?;
    assert!(
        matches!(
            result,
            Err(WindowsAttemptExchangeError::Record(
                AttemptRecordError::LocatorMismatch
            ))
        ),
        "{result:?}"
    );
    refused
        .recv_timeout(super::peer::KILL_SWITCH)
        .map_err(io::Error::other)?;
    let (outcome, refusals) = join(owner)?;
    assert!(
        matches!(outcome, Err(WindowsAttemptExchangeError::ClaimDeadline)),
        "{outcome:?}"
    );
    match refusals.as_slice() {
        [ClaimRefusal::Record(AttemptRecordError::Io { source })] => {
            assert_eq!(
                source.kind(),
                io::ErrorKind::UnexpectedEof,
                "no KELD-AA1 arrived"
            );
        }
        other => panic!("expected end of file where KELD-AA1 was due: {other:?}"),
    }
    Ok(())
}

/// §7 codec row: an `KELD-AC1` whose server process ID differs from
/// `GetNamedPipeServerProcessId` is refused before `KELD-AA1`. A raw owner
/// states another process ID with the right IDs; it then reads end of file.
#[test]
fn a_challenge_from_another_server_process_is_refused_before_aa1() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let candidate = std::thread::spawn(move || claim(&name, &ids.installation));
    assert_eq!(
        endpoint.server.accept_until(Some(kill_switch()))?,
        WaitOutcome::Ready
    );
    let mut owner = endpoint.server.stream()?;
    owner.set_absolute_deadline(Some(kill_switch()));
    let AttemptRecord::Claim(_) =
        read(&mut owner, AttemptReadPosition::OwnerClaim).map_err(io::Error::other)?
    else {
        panic!("the owner's first position admits only KELD-AH1");
    };
    let stated = std::process::id().wrapping_add(1);
    write(
        &mut owner,
        AttemptRecord::Challenge(AttemptChallenge::new(
            ids.attempt,
            ids.channel,
            SessionToken::random()?,
            stated,
        )),
    )?;
    let result = candidate
        .join()
        .map_err(|_| io::Error::other("candidate thread panicked"))??;
    match result {
        Err(WindowsAttemptExchangeError::Record(AttemptRecordError::ServerProcessMismatch {
            challenged,
            connected,
        })) => {
            assert_eq!(challenged, stated);
            assert_eq!(connected, std::process::id());
        }
        other => panic!("expected the server process mismatch: {other:?}"),
    }
    expect_end_of_file(&mut owner)
}

/// §7 codec row: a `KELD-AR1` that differs from the transcript in one field
/// is refused. A raw owner returns the claimant's `KELD-AA1` with its server
/// nonce flipped (offsets `136..168` of the spec table) as `KELD-AR1`.
#[test]
fn a_receipt_that_differs_from_the_transcript_is_refused() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let candidate = std::thread::spawn(move || claim(&name, &ids.installation));
    assert_eq!(
        endpoint.server.accept_until(Some(kill_switch()))?,
        WaitOutcome::Ready
    );
    let mut owner = endpoint.server.stream()?;
    owner.set_absolute_deadline(Some(kill_switch()));
    read(&mut owner, AttemptReadPosition::OwnerClaim).map_err(io::Error::other)?;
    write(
        &mut owner,
        AttemptRecord::Challenge(AttemptChallenge::new(
            ids.attempt,
            ids.channel,
            SessionToken::random()?,
            std::process::id(),
        )),
    )?;
    let AttemptRecord::Acknowledgement(acknowledged) =
        read(&mut owner, AttemptReadPosition::OwnerAcknowledgement).map_err(io::Error::other)?
    else {
        panic!("the owner's second position admits only KELD-AA1");
    };
    let mut receipt = Vec::new();
    AttemptRecord::Receipt(acknowledged)
        .write_to(&mut receipt)
        .map_err(io::Error::other)?;
    receipt[150] ^= 0x01;
    std::io::Write::write_all(&mut owner, &receipt)?;
    let result = candidate
        .join()
        .map_err(|_| io::Error::other("candidate thread panicked"))??;
    assert!(
        matches!(
            result,
            Err(WindowsAttemptExchangeError::Record(
                AttemptRecordError::TranscriptMismatch
            ))
        ),
        "{result:?}"
    );
    Ok(())
}

/// Claimant-binding row: the candidate's connected handle is not
/// inheritable, by the independent handle-flag read.
#[test]
fn the_candidates_connected_handle_is_not_inheritable() -> io::Result<()> {
    let (_owner, mut candidate, _) = super::peer::claimed_pair()?;
    let (handle_flags, _) = candidate
        .stream_mut()
        .inspect_owned_pipe(independent_handle_and_pipe_flags)?;
    assert_eq!(handle_flags & HANDLE_FLAG_INHERIT, 0);
    Ok(())
}

#[test]
fn every_exchange_error_names_its_code_and_fix() {
    let cases = [
        (
            WindowsAttemptExchangeError::Endpoint(WindowsAttemptEndpointError::EndpointShape),
            "KELD-IPC-008",
            "keld-attempt",
        ),
        (
            WindowsAttemptExchangeError::Record(AttemptRecordError::LocatorMismatch),
            "KELD-IPC-016",
            "End the exchange",
        ),
        (
            WindowsAttemptExchangeError::ClaimDeadline,
            "KELD-IPC-018",
            "never extend this deadline",
        ),
        (
            WindowsAttemptExchangeError::OutOfSequence {
                step: "read KELD-AY1",
                phase: "after KELD-AR1",
            },
            "KELD-IPC-019",
            "nothing was sent or read",
        ),
        (
            WindowsAttemptExchangeError::CandidateFailure {
                class: AttemptFailureClass::BootError,
            },
            "KELD-IPC-020",
            "class 3",
        ),
        (
            WindowsAttemptExchangeError::HealthWindow {
                failure: AttemptHealthWindowFailure::ByteReceived,
                source: None,
            },
            "KELD-IPC-020",
            "KELD-AK1 rolled back once",
        ),
        (
            WindowsAttemptExchangeError::HealthRolledBack,
            "KELD-IPC-021",
            "Never arm the recovery gate",
        ),
    ];
    for (error, code, fix) in cases {
        let text = error.to_string();
        assert!(text.starts_with(code), "{text}");
        assert!(text.contains(fix), "{text}");
    }
}
