//! The claim's deadlines on real Windows pipes (KEL-53 §4 "Candidate
//! connect-back", *Acceptance* and *Refusal*; the deadline cells of §7 "8
//! (claimant binding)"): a connector that sends nothing is dropped at its
//! per-connection deadline and the same instance then accepts the candidate,
//! and refusals never extend the claim deadline, at which the endpoint closes.
//!
//! Oracles: end of file observed on the dropped connector's own end, the
//! owner's reported refusal, the elapsed time against the landed
//! per-connection deadline (a kill-switch bound, never a synchronization), and
//! the closed name, probed independently.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;

use super::oracle::probe_exists;
use super::peer::{
    TestPin, admit_own, claim, connect_back_endpoint, expect_end_of_file, fresh_ids, is_deadline,
    is_end_of_file, join, kill_switch, pipe_token, raw_claim, raw_client, refused_claimant,
    spawn_owner, write,
};
use crate::APP_LINK_IO_DEADLINE;
use crate::attempt::WindowsAttemptExchangeError;
use crate::attempt::claim::ClaimRefusal;
use crate::attempt::records::AttemptRecord;
use crate::bootstrap::WindowsLifecyclePeerPin;
use crate::token::SessionToken;

/// Claimant-binding row: a connector that sends nothing is dropped at its
/// per-connection deadline (shortened here), and the same instance then
/// accepts the real candidate. The dropped connector observes end of file.
#[test]
fn a_silent_connector_is_dropped_at_its_per_connection_deadline() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let pin = TestPin::own()?;
    let (refusals, refused) = mpsc::channel();
    let owner = spawn_owner(
        endpoint,
        kill_switch(),
        Duration::from_millis(300),
        admit_own(&pin),
        pipe_token,
        refusals,
    );
    let mut silent = raw_client(&name)?;
    let connected = Instant::now();
    expect_end_of_file(&mut silent)?;
    let dropped_after = connected.elapsed();
    refused
        .recv_timeout(super::peer::KILL_SWITCH)
        .map_err(io::Error::other)?;
    claim(&name, &ids.installation)?.map_err(io::Error::other)?;
    let (owner, refusals) = join(owner)?;
    owner.map_err(io::Error::other)?;
    assert!(
        matches!(refusals.as_slice(), [ClaimRefusal::Record(error)] if is_deadline(error)),
        "the silent connector is refused at its deadline: {refusals:?}"
    );
    assert!(
        dropped_after < Duration::from_secs(10),
        "dropped at the claim deadline, not its own: {dropped_after:?}"
    );
    Ok(())
}

/// Claimant-binding row: refusals never extend the claim deadline. With the
/// landed five-second per-connection deadline and a one-second claim
/// deadline, a refused claimant and then a silent connector leave the owner
/// returning `ClaimDeadline` after one second, not after any per-connection
/// deadline, with its endpoint closed.
#[test]
fn refusals_never_extend_the_claim_deadline() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let pin = TestPin::own()?;
    let started = Instant::now();
    let deadline = started + Duration::from_secs(1);
    let (refusals, refused) = mpsc::channel();
    let owner = spawn_owner(
        endpoint,
        deadline,
        APP_LINK_IO_DEADLINE,
        admit_own(&pin),
        pipe_token,
        refusals,
    );
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
    let silent = raw_client(&name)?;
    let (outcome, _) = join(owner)?;
    let elapsed = started.elapsed();
    drop(silent);
    assert!(
        matches!(outcome, Err(WindowsAttemptExchangeError::ClaimDeadline)),
        "{outcome:?}"
    );
    assert!(
        elapsed >= Duration::from_secs(1),
        "returned before its deadline: {elapsed:?}"
    );
    assert!(
        elapsed < APP_LINK_IO_DEADLINE,
        "a refusal or the silent connector extended the deadline: {elapsed:?}"
    );
    let absent = probe_exists(&name).expect_err("the endpoint is closed at its deadline");
    assert_eq!(
        absent.raw_os_error(),
        Some(ERROR_FILE_NOT_FOUND.cast_signed())
    );
    Ok(())
}

/// A claimant pin whose second liveness query (the one after `KELD-AA1`
/// matched, just before the one-shot is consumed) returns only once the claim
/// deadline has passed, as a slow caller's pin can.
#[derive(Debug, Clone)]
struct SlowPin {
    inner: TestPin,
    calls: Arc<AtomicUsize>,
    deadline: Instant,
}

impl WindowsLifecyclePeerPin for SlowPin {
    fn process_id(&self) -> u32 {
        self.inner.process_id
    }

    fn session_id(&self) -> u32 {
        self.inner.session_id
    }

    fn has_exited(&self) -> io::Result<bool> {
        if self.calls.fetch_add(1, Ordering::AcqRel) == 1 {
            while Instant::now() < self.deadline {
                std::thread::yield_now();
            }
        }
        Ok(false)
    }
}

/// The claim deadline that passes after `KELD-AA1` matched, while the
/// caller's pin is still being asked, is re-checked before the one-shot is
/// consumed: the owner returns `ClaimDeadline` and sends no `KELD-AR1`, so
/// the claimant reads end of file. Without the re-check the one-shot is
/// consumed and the `KELD-AR1` write fails on the expired absolute deadline.
#[test]
fn a_deadline_that_passes_after_aa1_is_not_accepted() -> io::Result<()> {
    let ids = fresh_ids()?;
    let endpoint = connect_back_endpoint(ids)?;
    let name = endpoint.endpoint().to_owned();
    let deadline = Instant::now() + Duration::from_secs(3);
    let pin = SlowPin {
        inner: TestPin::own()?,
        calls: Arc::new(AtomicUsize::new(0)),
        deadline,
    };
    let calls = Arc::clone(&pin.calls);
    let owner = std::thread::spawn(move || {
        endpoint
            .accept_claim(
                deadline,
                move |_: u32, _: u32, _: &crate::WindowsPeerTokenFacts| Some(pin.clone()),
            )
            .map(drop)
    });
    let candidate = claim(&name, &ids.installation)?;
    let outcome = owner
        .join()
        .map_err(|_| io::Error::other("owner thread panicked"))?;
    assert_eq!(
        calls.load(Ordering::Acquire),
        2,
        "both liveness queries ran"
    );
    assert!(
        matches!(&candidate, Err(error) if is_end_of_file(error)),
        "{candidate:?}"
    );
    assert!(
        matches!(outcome, Err(WindowsAttemptExchangeError::ClaimDeadline)),
        "{outcome:?}"
    );
    Ok(())
}
