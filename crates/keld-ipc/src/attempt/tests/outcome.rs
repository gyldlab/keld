//! The health result and the close, on real Windows pipes (KEL-53 §4
//! "Candidate connect-back", *Health sequence*; §7 "8 (health sequence)"):
//! `KELD-AK1` accepted is followed by the owner's wait for the candidate's end
//! of file, and a disconnect right after it leaves the candidate unarmed; the
//! candidate arms only on `KELD-AK1` accepted; the rollback writes `KELD-AK1`
//! rolled back once without waiting for the candidate to read it.
//!
//! Oracles: the spec's `KELD-AK1` bytes, a peer that reads, withholds or
//! writes bytes itself, the owner's own observation of the close, and end of
//! file as the candidate observes it.

use std::io::{self, Write as _};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::peer::{
    DIGEST, KILL_SWITCH, SHORT_WINDOW, TestPin, is_end_of_file, kill_switch, ready_pair,
};
use crate::APP_LINK_IO_DEADLINE;
use crate::attempt::records::{AttemptHealthResult, AttemptRecord, AttemptRecordError};
use crate::attempt::{
    WindowsAttemptClaimantChannel, WindowsAttemptCloseWait, WindowsAttemptExchangeError,
    WindowsAttemptOwnerChannel, WindowsAttemptRollBack,
};

fn healthy_pair() -> io::Result<(
    WindowsAttemptOwnerChannel<TestPin>,
    WindowsAttemptClaimantChannel,
    TestPin,
)> {
    let (mut owner, candidate, pin) = ready_pair(&DIGEST)?;
    owner
        .await_window_of(SHORT_WINDOW)
        .map_err(io::Error::other)?;
    Ok((owner, candidate, pin))
}

/// The owner reads `KELD-AB1` and `KELD-AY1` by position, a connection that
/// stays open and silent through the window with the launched process
/// running passes, and `KELD-AK1` accepted arms the candidate.
#[test]
fn boot_ready_and_a_silent_window_accept_health() -> io::Result<()> {
    let (owner, candidate, _) = healthy_pair()?;
    let reader = thread::spawn(move || candidate.await_health_accepted(kill_switch()));
    let observed = owner.accept(kill_switch()).map_err(io::Error::other)?;
    assert!(
        matches!(observed, WindowsAttemptCloseWait::CandidateClosed),
        "{observed:?}"
    );
    reader
        .join()
        .map_err(|_| io::Error::other("candidate thread panicked"))?
        .map_err(io::Error::other)
}

/// §7 "8 (health sequence)": the owner disconnects only after the
/// candidate's end of file. The candidate reads `KELD-AK1` only after the
/// owner wrote it and is either blocked reading for end of file or has
/// returned, and is armed; the owner then observes its close. An owner that
/// disconnected right after the write would have returned, and the candidate
/// would read end of file instead.
#[test]
fn the_owner_waits_for_the_candidates_end_of_file_after_ak1_accepted() -> io::Result<()> {
    let (mut owner, candidate, _) = healthy_pair()?;
    let (entered, written) = mpsc::channel();
    owner.install_close_wait_witness(entered);
    let view = owner.stream_mut().try_clone()?;
    let owner = thread::spawn(move || owner.accept(kill_switch()));
    written
        .recv_timeout(KILL_SWITCH)
        .map_err(|_| io::Error::other("the owner never wrote KELD-AK1"))?;
    let started = Instant::now();
    while !(view.has_active_io() || owner.is_finished()) {
        if started.elapsed() > KILL_SWITCH {
            return Err(io::Error::other("the owner neither waited nor returned"));
        }
        thread::yield_now();
    }
    let armed = candidate.await_health_accepted(kill_switch());
    let observed = owner
        .join()
        .map_err(|_| io::Error::other("owner thread panicked"))?
        .map_err(io::Error::other)?;
    drop(view);
    armed.map_err(io::Error::other)?;
    assert!(
        matches!(observed, WindowsAttemptCloseWait::CandidateClosed),
        "{observed:?}"
    );
    Ok(())
}

/// §7 "8 (health sequence)" negative control: an owner that disconnects
/// right after writing `KELD-AK1` accepted, before the candidate reads it,
/// leaves the candidate unarmed: the disconnect discards the unread record.
#[test]
fn disconnecting_right_after_ak1_accepted_leaves_the_candidate_unarmed() -> io::Result<()> {
    let (mut owner, candidate, _) = healthy_pair()?;
    owner
        .stream_mut()
        .set_absolute_deadline(Some(kill_switch()));
    AttemptRecord::HealthResult(AttemptHealthResult::Accepted)
        .write_to(owner.stream_mut())
        .map_err(io::Error::other)?;
    owner.stream_mut().shutdown()?;
    let result = candidate.await_health_accepted(kill_switch());
    assert!(
        matches!(&result, Err(error) if is_end_of_file(error)),
        "{result:?}"
    );
    Ok(())
}

/// The owner closes its end at its deadline when the candidate reads
/// `KELD-AK1` accepted (raw, spec bytes) but never closes.
#[test]
fn the_owner_closes_at_its_deadline_when_the_candidate_never_closes() -> io::Result<()> {
    let (owner, mut candidate, _) = healthy_pair()?;
    let owner = thread::spawn(move || owner.accept(Instant::now() + Duration::from_millis(500)));
    let mut record = [0_u8; 9];
    io::Read::read_exact(candidate.stream_mut(), &mut record)?;
    assert_eq!(&record, b"KELD-AK1\x01");
    let observed = owner
        .join()
        .map_err(|_| io::Error::other("owner thread panicked"))?
        .map_err(io::Error::other)?;
    assert!(
        matches!(observed, WindowsAttemptCloseWait::DeadlineElapsed),
        "{observed:?}"
    );
    Ok(())
}

/// §7 "8 (health sequence)": a `KELD-AK1` that the candidate loses to end
/// of file, its deadline, a malformed record or `KELD-AK1` rolled back
/// leaves it unarmed: only `KELD-AK1` accepted returns `Ok`.
#[test]
fn the_candidate_arms_only_on_ak1_accepted() -> io::Result<()> {
    let (owner, candidate, _) = ready_pair(&DIGEST)?;
    drop(owner);
    let lost = candidate.await_health_accepted(kill_switch());
    assert!(
        matches!(&lost, Err(error) if is_end_of_file(error)),
        "{lost:?}"
    );

    let (_owner, candidate, _) = ready_pair(&DIGEST)?;
    let late = candidate.await_health_accepted(Instant::now() + Duration::from_millis(200));
    assert!(
        matches!(&late, Err(WindowsAttemptExchangeError::Record(error)) if super::peer::is_deadline(error)),
        "{late:?}"
    );

    for result in [0_u8, 3, 0xff] {
        let (mut owner, candidate, _) = ready_pair(&DIGEST)?;
        owner
            .stream_mut()
            .write_all(&[b'K', b'E', b'L', b'D', b'-', b'A', b'K', b'1', result])?;
        let malformed = candidate.await_health_accepted(kill_switch());
        assert!(
            matches!(
                malformed,
                Err(WindowsAttemptExchangeError::Record(AttemptRecordError::ValueOutOfSet {
                    value, ..
                })) if value == result
            ),
            "{result}: {malformed:?}"
        );
    }

    let (owner, candidate, _) = ready_pair(&DIGEST)?;
    assert!(matches!(owner.roll_back(), WindowsAttemptRollBack::Sent));
    let rolled_back = candidate.await_health_accepted(kill_switch());
    assert!(
        matches!(
            rolled_back,
            Err(WindowsAttemptExchangeError::HealthRolledBack)
        ),
        "{rolled_back:?}"
    );
    Ok(())
}

/// §7 "8 (health sequence)": on rollback `KELD-AK1` rolled back is written
/// once while the connection is open, and the rollback completes although
/// the candidate never reads it.
#[test]
fn the_rollback_completes_when_the_candidate_never_reads() -> io::Result<()> {
    let (owner, candidate, _) = ready_pair(&DIGEST)?;
    let started = Instant::now();
    assert!(matches!(owner.roll_back(), WindowsAttemptRollBack::Sent));
    assert!(
        started.elapsed() < APP_LINK_IO_DEADLINE,
        "the rollback waited for the candidate"
    );
    drop(candidate);
    Ok(())
}
