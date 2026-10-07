//! The owner's admission of a claim on real Windows pipes (KEL-53 §4
//! "Candidate connect-back": *Acceptance*, *Transcript* and *Refusal*; §7 "8
//! (keld-attempt codec)" `KELD-AH1` and `KELD-AA1` cells and the pipe and
//! one-shot cells of "8 (claimant binding)"): every refusal disconnects that
//! claimant and re-arms the same instance, no refusal consumes the one-shot,
//! and a failed `RevertToSelf` after the token read terminates the owner.
//!
//! Oracles: record bytes laid out by the spec table's offsets, a raw claimant
//! that writes records itself and observes end of file on its own end, each
//! refusal's reason as the owner reports it, and a child process's exit
//! status. Process facts are the caller's (S5 binds real processes); here the
//! caller's check is a test pin.

use std::io;
use std::process::Command;
use std::sync::mpsc;

use super::peer::{
    TestPin, admit_own, claim, connect_back_endpoint, expect_end_of_file, fresh_ids,
    is_end_of_file, join, kill_switch, pipe_token, raw_claim, raw_client, read, refused_claimant,
    spawn_admitting_owner, spawn_owner, write,
};
use crate::APP_LINK_IO_DEADLINE;
use crate::attempt::claim::ClaimRefusal;
use crate::attempt::records::{
    AttemptReadPosition, AttemptRecord, AttemptRecordError, AttemptTranscript,
};
use crate::token::SessionToken;

/// §7 codec row: an `KELD-AH1` with a foreign installation ID or another
/// client process ID is refused; the refused claimant is disconnected and the
/// same instance then accepts the real candidate, so neither refusal consumed
/// the one-shot (claimant-binding row).
#[test]
fn a_foreign_installation_or_process_is_refused_and_the_instance_rearms() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let pin = TestPin::own()?;
    let (owner, refused) = spawn_admitting_owner(endpoint, &pin);
    let foreign = *SessionToken::random()?.as_bytes();
    refused_claimant(&name, |client| {
        write(
            client,
            AttemptRecord::Claim(raw_claim(foreign, std::process::id())?),
        )
    })?;
    refused
        .recv_timeout(super::peer::KILL_SWITCH)
        .map_err(io::Error::other)?;
    let other_pid = std::process::id().wrapping_add(4);
    refused_claimant(&name, |client| {
        write(
            client,
            AttemptRecord::Claim(raw_claim(ids.installation, other_pid)?),
        )
    })?;
    refused
        .recv_timeout(super::peer::KILL_SWITCH)
        .map_err(io::Error::other)?;
    claim(&name, &ids.installation)?.map_err(io::Error::other)?;
    let (owner, refusals) = join(owner)?;
    owner.map_err(io::Error::other)?;
    match refusals.as_slice() {
        [
            ClaimRefusal::Record(AttemptRecordError::ForeignInstallation),
            ClaimRefusal::Record(AttemptRecordError::ClientProcessMismatch { claimed, connected }),
        ] => {
            assert_eq!(*claimed, other_pid);
            assert_eq!(*connected, std::process::id());
        }
        other => panic!("unexpected refusals: {other:?}"),
    }
    Ok(())
}

/// Claimant-binding row: the caller's claimant check refuses a claimant (as
/// `CompareObjectHandles` refuses a same-user copy), which reads end of file
/// where `KELD-AC1` was due; the same instance then accepts the next one.
/// Pins that name another process or session, or report the process exited,
/// are refused the same way, each before `KELD-AC1`: a raw claimant sends a
/// valid `KELD-AH1` and receives nothing more.
#[test]
fn a_refused_claimant_consumes_no_one_shot() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let own = TestPin::own()?;
    let exited = TestPin::own()?;
    exited.report_exit();
    let mut answers = vec![
        Some(own.clone()),
        Some(exited),
        Some(TestPin {
            session_id: own.session_id.wrapping_add(1),
            ..own.clone()
        }),
        Some(TestPin {
            process_id: own.process_id.wrapping_add(4),
            ..own.clone()
        }),
        None,
    ];
    let check = move |_: u32, _: u32, _: &crate::WindowsPeerTokenFacts| answers.pop().flatten();
    let (refusals, refused) = mpsc::channel();
    let owner = spawn_owner(
        endpoint,
        kill_switch(),
        APP_LINK_IO_DEADLINE,
        check,
        pipe_token,
        refusals,
    );
    let result = claim(&name, &ids.installation)?;
    assert!(
        matches!(&result, Err(error) if is_end_of_file(error)),
        "a refused claimant reads end of file: {result:?}"
    );
    refused
        .recv_timeout(super::peer::KILL_SWITCH)
        .map_err(io::Error::other)?;
    for _ in 0..3 {
        refused_claimant(&name, |client| {
            write(
                client,
                AttemptRecord::Claim(raw_claim(ids.installation, std::process::id())?),
            )
        })?;
        refused
            .recv_timeout(super::peer::KILL_SWITCH)
            .map_err(io::Error::other)?;
    }
    claim(&name, &ids.installation)?.map_err(io::Error::other)?;
    let (owner, refusals) = join(owner)?;
    owner.map_err(io::Error::other)?;
    assert!(
        matches!(
            refusals.as_slice(),
            [
                ClaimRefusal::ClaimantCheck,
                ClaimRefusal::PinMismatch,
                ClaimRefusal::PinMismatch,
                ClaimRefusal::ClaimantExited,
            ]
        ),
        "{refusals:?}"
    );
    Ok(())
}

/// Claimant-binding row: a token from another session that otherwise
/// matches is refused. No second session exists on a test host, so the
/// token reader is the seam: it reports the real token with another session
/// once, then the real token.
#[test]
fn a_token_from_another_session_is_refused() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let pin = TestPin::own()?;
    let mut first = true;
    let read_token = move |stream: &crate::windows_named_pipe::WindowsNamedPipeStream| {
        let mut facts = pipe_token(stream)?;
        if std::mem::take(&mut first) {
            facts.session_id = facts.session_id.wrapping_add(1);
        }
        Ok(facts)
    };
    let (refusals, refused) = mpsc::channel();
    let owner = spawn_owner(
        endpoint,
        kill_switch(),
        APP_LINK_IO_DEADLINE,
        admit_own(&pin),
        read_token,
        refusals,
    );
    let result = claim(&name, &ids.installation)?;
    assert!(
        matches!(&result, Err(error) if is_end_of_file(error)),
        "{result:?}"
    );
    refused
        .recv_timeout(super::peer::KILL_SWITCH)
        .map_err(io::Error::other)?;
    claim(&name, &ids.installation)?.map_err(io::Error::other)?;
    let (owner, refusals) = join(owner)?;
    owner.map_err(io::Error::other)?;
    assert!(
        matches!(refusals.as_slice(), [ClaimRefusal::PinMismatch]),
        "{refusals:?}"
    );
    Ok(())
}

/// §7 codec row: a one-field mutation of `KELD-AA1` is refused, for each of
/// its seven fields at the spec table's offsets; each refused claimant reads
/// end of file, no `KELD-AR1`, and the same instance then accepts the real
/// candidate.
#[test]
fn a_one_field_mutation_of_the_acknowledgement_is_refused_at_the_pipe() -> io::Result<()> {
    const FIELDS: [(&str, usize); 7] = [
        ("installation ID", 8),
        ("attempt ID", 40),
        ("health-channel ID", 72),
        ("client nonce", 104),
        ("server nonce", 136),
        ("client PID", 168),
        ("server PID", 172),
    ];
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let pin = TestPin::own()?;
    let (owner, refused) = spawn_admitting_owner(endpoint, &pin);
    for (field, offset) in FIELDS {
        let mut client = raw_client(&name)?;
        let sent = raw_claim(ids.installation, std::process::id())?;
        write(&mut client, AttemptRecord::Claim(sent))?;
        let AttemptRecord::Challenge(challenge) =
            read(&mut client, AttemptReadPosition::CandidateChallenge).map_err(io::Error::other)?
        else {
            panic!("the candidate's first position admits only KELD-AC1");
        };
        let transcript =
            AttemptTranscript::for_claimant(&sent, &challenge, &name, std::process::id())
                .map_err(io::Error::other)?;
        let mut acknowledgement = Vec::new();
        AttemptRecord::Acknowledgement(transcript)
            .write_to(&mut acknowledgement)
            .map_err(io::Error::other)?;
        acknowledgement[offset] ^= 0x80;
        std::io::Write::write_all(&mut client, &acknowledgement)?;
        expect_end_of_file(&mut client)
            .map_err(|error| io::Error::other(format!("{field}: {error}")))?;
        refused
            .recv_timeout(super::peer::KILL_SWITCH)
            .map_err(io::Error::other)?;
    }
    claim(&name, &ids.installation)?.map_err(io::Error::other)?;
    let (owner, refusals) = join(owner)?;
    owner.map_err(io::Error::other)?;
    assert_eq!(refusals.len(), FIELDS.len());
    for refusal in &refusals {
        assert!(
            matches!(
                refusal,
                ClaimRefusal::Record(AttemptRecordError::TranscriptMismatch)
            ),
            "{refusal:?}"
        );
    }
    Ok(())
}

/// A claimant whose pinned process exits after `KELD-AC1` and before
/// `KELD-AA1` is refused before the one-shot is consumed: it reads end of
/// file where `KELD-AR1` was due, and the same instance accepts the next.
#[test]
fn a_claimant_that_exits_before_its_acknowledgement_is_refused() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let exiting = TestPin::own()?;
    let mut answers = vec![TestPin::own()?, exiting.clone()];
    let check = move |_: u32, _: u32, _: &crate::WindowsPeerTokenFacts| answers.pop();
    let (refusals, refused) = mpsc::channel();
    let owner = spawn_owner(
        endpoint,
        kill_switch(),
        APP_LINK_IO_DEADLINE,
        check,
        pipe_token,
        refusals,
    );
    let mut client = raw_client(&name)?;
    let sent = raw_claim(ids.installation, std::process::id())?;
    write(&mut client, AttemptRecord::Claim(sent))?;
    let AttemptRecord::Challenge(challenge) =
        read(&mut client, AttemptReadPosition::CandidateChallenge).map_err(io::Error::other)?
    else {
        panic!("the candidate's first position admits only KELD-AC1");
    };
    exiting.report_exit();
    let transcript = AttemptTranscript::for_claimant(&sent, &challenge, &name, std::process::id())
        .map_err(io::Error::other)?;
    write(&mut client, AttemptRecord::Acknowledgement(transcript))?;
    expect_end_of_file(&mut client)?;
    refused
        .recv_timeout(super::peer::KILL_SWITCH)
        .map_err(io::Error::other)?;
    claim(&name, &ids.installation)?.map_err(io::Error::other)?;
    let (owner, refusals) = join(owner)?;
    owner.map_err(io::Error::other)?;
    assert!(
        matches!(refusals.as_slice(), [ClaimRefusal::ClaimantExited]),
        "{refusals:?}"
    );
    Ok(())
}

/// Claimant-binding row: a failed `RevertToSelf` terminates the owner
/// (seam-injected). In a child process the owner's thread makes its reverts
/// count as failed and runs the shipped claim against the shipped claimant;
/// the revert after it reads the claim writer's token aborts the child before
/// any claim is accepted. The markers prove the child reached the claim and
/// that nothing ran after the revert.
#[test]
fn a_failed_revert_after_the_token_read_terminates_the_owner() -> io::Result<()> {
    const CHILD: &str = "KELD_TEST_ATTEMPT_REVERT_FAILURE_CHILD";
    const REACHED: &str = "KELD_TEST_ATTEMPT_CLAIM_STARTED";
    const ACCEPTED: &str = "KELD_TEST_ATTEMPT_CLAIM_ACCEPTED";
    /// `std::process::abort` on Windows: `__fastfail`, `STATUS_STACK_BUFFER_OVERRUN`.
    const ABORTED: u32 = 0xC000_0409;
    if std::env::var_os(CHILD).is_some() {
        let ids = fresh_ids()?;
        let endpoint = connect_back_endpoint(ids)?;
        let name = endpoint.endpoint().to_owned();
        let pin = TestPin::own()?;
        let owner = std::thread::spawn(move || {
            crate::windows_named_pipe::inject_revert_failure_on_this_thread();
            endpoint.accept_claim(kill_switch(), admit_own(&pin))
        });
        println!("{REACHED}");
        let candidate = claim(&name, &ids.installation)?;
        let owner = owner.join().map(|accepted| accepted.is_ok());
        if candidate.is_ok() && owner.is_ok_and(|accepted| accepted) {
            println!("{ACCEPTED}");
        }
        return Ok(());
    }
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "attempt::tests::claim::a_failed_revert_after_the_token_read_terminates_the_owner",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(REACHED),
        "the child never reached the claim: {stdout}"
    );
    assert!(
        !stdout.contains(ACCEPTED),
        "a claim was accepted after a failed revert: {stdout}"
    );
    assert_eq!(
        output.status.code(),
        Some(ABORTED.cast_signed()),
        "the owner must abort, not fail or panic: {:?}",
        output.status
    );
    Ok(())
}
