use super::{
    AppWindowCommand, CancelDisposition, CorrelationId, DialogFault, DialogOwner, DialogReplyV1,
    DialogSession, DialogUiOutcome, MessageOptions, PrimaryLinkIdentity, WebviewId,
};
use keld_ipc::window::{WindowGeneration, WindowId, WindowRef};
use sha2::{Digest as _, Sha256};
use std::sync::mpsc;

#[cfg(target_os = "macos")]
#[path = "publication_tests.rs"]
mod publication;

fn with_manifest(
    source: &str,
) -> (
    DialogSession,
    mpsc::Receiver<AppWindowCommand>,
    tempfile::TempDir,
) {
    let temp = tempfile::tempdir().expect("private immutable manifest");
    let path = temp.path().join("keld.permissions.jsonc");
    std::fs::write(&path, source).expect("source bytes");
    let digest: [u8; 32] = Sha256::digest(source.as_bytes()).into();
    let verified = keld_guard::verified_manifest::load_verified_manifest(
        std::fs::File::open(&path).expect("retained source"),
        path,
        digest,
    )
    .expect("verified snapshot");
    let (wake, receive) = mpsc::channel();
    (DialogSession::new(verified, wake), receive, temp)
}

fn original(correlation: u32, nonce: u64) -> DialogOwner {
    DialogOwner {
        identity: PrimaryLinkIdentity::Fixture,
        attempt: 1,
        correlation: CorrelationId(correlation),
        nonce,
        parent: Some(WindowRef {
            id: WindowId(1),
            incarnation: WindowGeneration(1),
        }),
    }
}

fn options() -> MessageOptions {
    MessageOptions {
        message: "Unsaved changes".to_owned(),
        kind: keld_native::dialog::MessageKind::Question,
        buttons: vec!["Save".to_owned(), "Cancel".to_owned(), "Discard".to_owned()],
        default_id: Some(0),
        cancel_id: Some(1),
        normalize_access_keys: true,
        title: Some("Confirm".to_owned()),
        detail: None,
        no_link: true,
        checkbox_label: None,
        checkbox_checked: None,
    }
}

#[test]
fn dialog_guard_denial_never_allocates_or_wakes_native_owner() {
    let (session, receive, _temp) = with_manifest(r#"{"app":{"system":[]}}"#);
    assert_eq!(session.authorize().unwrap_err().code, "KELD-GUARD001");
    assert!(session.has_capacity());
    assert!(session.fully_drained());
    assert!(receive.try_recv().is_err());
}

#[test]
fn retiring_fs_only_generation_without_a_dialog_must_not_enqueue_native_ui_wake() {
    let (session, native, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let identity = PrimaryLinkIdentity::Fixture;
    session.retire(identity, 1);
    assert!(
        native.try_recv().is_err(),
        "no dialog was reserved, so no native work exists"
    );
    assert!(session.fully_drained());
    assert!(session.has_capacity());
}

#[test]
fn dialog_cancel_at_capacity_keeps_distinct_original_and_cancel_correlations() {
    let (session, wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    session.authorize().expect("verified exact dialog grant");
    let first = original(7, 9);
    session
        .reserve(first, options(), WebviewId(3))
        .expect("one reservation");
    assert_eq!(wake.try_recv(), Ok(AppWindowCommand::DialogWake));
    assert_eq!(
        session.pending_original_for(first.identity, first.attempt),
        Some(CorrelationId(7))
    );
    assert_eq!(
        session.reserve(original(8, 10), options(), WebviewId(3)),
        Err(DialogFault::Busy)
    );
    assert_eq!(
        session.cancel(first.identity, 1, 9, CorrelationId(7)),
        Err(DialogFault::InvalidRequest),
        "a Cancel may not overwrite its original correlation",
    );
    assert_eq!(
        session.cancel(first.identity, 1, 9, CorrelationId(8)),
        Ok(CancelDisposition::Requested),
    );
    assert_eq!(wake.try_recv(), Ok(AppWindowCommand::DialogWake));
    assert!(session.take_work().expect("bounded pending work").is_some());
    assert!(!session.may_present(first).expect("tombstone"));
    session
        .record_native_outcome(first, DialogUiOutcome::Dismissed, None)
        .expect("OS callback observed after cancellation");
    let terminal = session
        .complete(1, 9, DialogUiOutcome::Dismissed)
        .expect("OS callback modeled as already physically drained");
    assert!(terminal == Some((first, Err(DialogFault::Cancelled))));
    assert!(session.fully_drained());
    assert!(session.has_capacity());
    assert!(
        session
            .complete(
                1,
                9,
                DialogUiOutcome::Selected {
                    response: 0,
                    checkbox_checked: false,
                }
            )
            .expect("late native event")
            .is_none()
    );
}

#[test]
fn dialog_committed_native_result_wins_before_late_cancel_and_role_retirement() {
    let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let first = original(15, 17);
    session
        .reserve(first, options(), WebviewId(2))
        .expect("reserve");
    session.take_work().expect("one work");
    session
        .record_native_outcome(
            first,
            DialogUiOutcome::Selected {
                response: 2,
                checkbox_checked: true,
            },
            None,
        )
        .expect("real native click committed before event dispatch");
    let reply = session
        .complete(
            1,
            17,
            DialogUiOutcome::Selected {
                response: 2,
                checkbox_checked: true,
            },
        )
        .expect("observed physical native completion");
    assert!(
        reply
            == Some((
                first,
                Ok(DialogReplyV1::Message {
                    response: 2,
                    checkbox_checked: true,
                })
            ))
    );
    assert_eq!(
        session.cancel(first.identity, 1, 17, CorrelationId(99)),
        Ok(CancelDisposition::AlreadyCompleted),
    );
    session.retire(first.identity, first.attempt);
    assert!(
        session
            .complete(1, 17, DialogUiOutcome::Dismissed)
            .unwrap()
            .is_none()
    );
}

#[test]
fn native_click_is_terminal_before_event_coordinator_receives_it() {
    let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let first = original(22, 25);
    session
        .reserve(first, options(), WebviewId(3))
        .expect("reserve one effect");
    session.take_work().expect("Tao owns one physical panel");
    // The Tao thread observed callback + invisibility first; its event
    // packet has not reached Core yet. A later Cancel must not supersede
    // an already observed real click across this scheduling gap.
    session
        .record_native_outcome(
            first,
            DialogUiOutcome::Selected {
                response: 2,
                checkbox_checked: false,
            },
            None,
        )
        .expect("UI callback wins at physical observation");
    assert_eq!(
        session.cancel(
            first.identity,
            first.attempt,
            first.nonce,
            CorrelationId(66)
        ),
        Ok(CancelDisposition::AlreadyCompleted),
    );
    let result = session
        .complete(
            1,
            25,
            DialogUiOutcome::Selected {
                response: 2,
                checkbox_checked: false,
            },
        )
        .expect("coordinator receives already committed native outcome");
    assert!(matches!(result, Some((owner, Ok(DialogReplyV1::Message {
            response: 2, checkbox_checked: false,
        }))) if owner == first));
    assert!(session.fully_drained());
}

#[test]
fn delayed_poll_preserves_only_pre_cutoff_native_completion() {
    use std::time::{Duration, Instant};
    let cutoff = Instant::now();
    let tick = Duration::from_millis(1);
    for (completed_at, expected_expiry) in [
        (
            Some(
                cutoff
                    .checked_sub(tick)
                    .expect("fixture completion instant before cutoff"),
            ),
            false,
        ),
        (Some(cutoff), true),
        (Some(cutoff + tick), true),
        (None, true),
    ] {
        let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
        let owner = original(1, 1);
        session.reserve(owner, options(), WebviewId(3)).unwrap();
        assert_eq!(
            session
                .expire_if_due(owner, cutoff, completed_at, cutoff + tick)
                .unwrap(),
            expected_expiry
        );
        // Even a timely callback must not retain physical custody indefinitely.
        let cleanup = cutoff + std::time::Duration::from_secs(10);
        assert_eq!(
            session.state.lock().unwrap().cleanup_deadline,
            Some(cleanup)
        );
        assert!(session.check_cleanup(owner, || cleanup).is_err());
        assert!(!session.has_capacity());
        let outcome = DialogUiOutcome::Selected {
            response: 0,
            checkbox_checked: false,
        };
        session.record_native_outcome(owner, outcome, None).unwrap();
        let terminal = session
            .complete(owner.attempt, owner.nonce, outcome)
            .unwrap()
            .unwrap()
            .1;
        if expected_expiry {
            assert_eq!(terminal, Err(DialogFault::Expired));
        } else {
            assert_eq!(
                terminal,
                Ok(DialogReplyV1::Message {
                    response: 0,
                    checkbox_checked: false
                })
            );
        }
    }
}

#[test]
fn timely_open_result_survives_delayed_service_and_allows_next_save() {
    use super::DialogWorkOptions;
    use keld_native::dialog::{OpenDialogOptions, OpenProperty, SaveDialogOptions};
    use std::time::Duration;

    let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let open = original(1, 1);
    session
        .reserve_file(
            open,
            DialogWorkOptions::Open(OpenDialogOptions {
                default_path: None,
                filters: vec![],
                properties: vec![OpenProperty::OpenFile],
            }),
            WebviewId(3),
        )
        .unwrap();
    let work = session.take_work().unwrap().unwrap();
    assert!(work.owner == open);
    let tick = Duration::from_millis(1);
    let callback_at = work
        .deadline
        .checked_sub(tick)
        .expect("fixture callback instant before cutoff");
    let serviced_at = work.deadline + tick;
    assert!(
        !session
            .expire_if_due(open, work.deadline, Some(callback_at), serviced_at)
            .unwrap()
    );
    assert!(!session.has_capacity());
    let opened = DialogReplyV1::Open {
        canceled: false,
        file_paths: vec!["/tmp/keld-picker-selected.txt".to_owned()],
    };
    // A fixture observation of callback plus drain, not a real OS panel.
    let observed = session
        .poll_before_cleanup_deadline(open, || serviced_at, || Ok(Some(opened.clone())))
        .unwrap();
    session
        .record_native_outcome(open, DialogUiOutcome::FileResult, observed)
        .unwrap();
    let (owner, result) = session
        .complete(open.attempt, open.nonce, DialogUiOutcome::FileResult)
        .unwrap()
        .expect("original open terminal");
    assert!(owner == open);
    assert_eq!(result, Ok(opened));
    assert!(session.fully_drained());

    let save = original(2, 2);
    session
        .reserve_file(
            save,
            DialogWorkOptions::Save(SaveDialogOptions {
                default_path: None,
                filters: vec![],
                security_scoped_bookmarks: false,
            }),
            WebviewId(3),
        )
        .unwrap();
    let work = session.take_work().unwrap().unwrap();
    assert!(work.owner == save);
    let saved = DialogReplyV1::Save {
        canceled: false,
        file_path: "/tmp/keld-picker-destination.txt".to_owned(),
    };
    session
        .record_native_outcome(save, DialogUiOutcome::FileResult, Some(saved.clone()))
        .unwrap();
    let (owner, result) = session
        .complete(save.attempt, save.nonce, DialogUiOutcome::FileResult)
        .unwrap()
        .expect("original save terminal");
    assert!(owner == save);
    assert_eq!(result, Ok(saved));
    assert!(session.fully_drained());
}

#[test]
fn deadline_arbitration_preserves_prior_cancel_and_retirement() {
    use std::time::Instant;
    for retired in [false, true] {
        let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
        let owner = original(1, 1);
        session.reserve(owner, options(), WebviewId(3)).unwrap();
        assert!(session.take_work().unwrap().is_some());
        let cutoff = Instant::now();
        if retired {
            session.retire(owner.identity, owner.attempt);
        } else {
            session
                .cancel(owner.identity, owner.attempt, owner.nonce, CorrelationId(2))
                .unwrap();
        }
        assert!(
            session
                .expire_if_due(owner, cutoff, Some(cutoff), cutoff)
                .unwrap()
        );
        let outcome = DialogUiOutcome::Selected {
            response: 0,
            checkbox_checked: false,
        };
        session.record_native_outcome(owner, outcome, None).unwrap();
        let result = session
            .complete(owner.attempt, owner.nonce, outcome)
            .unwrap();
        if retired {
            assert!(result.is_none());
        } else {
            assert!(result == Some((owner, Err(DialogFault::Cancelled))));
        }
        assert!(session.fully_drained());
    }
}

#[test]
fn cleanup_deadline_fails_without_releasing_custody_and_never_restarts() {
    use std::time::{Duration, Instant};
    let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let owner = original(1, 1);
    session.reserve(owner, options(), WebviewId(3)).unwrap();
    assert!(session.take_work().unwrap().is_some());
    let started = Instant::now();
    let cutoff = started + Duration::from_secs(10);
    session.begin_cleanup(owner, cutoff).unwrap();
    session
        .begin_cleanup(owner, cutoff + Duration::from_secs(10))
        .unwrap();
    assert!(
        session
            .check_cleanup(owner, || cutoff
                .checked_sub(Duration::from_nanos(1))
                .expect("fixture cleanup instant before cutoff"))
            .is_ok()
    );
    assert!(session.check_cleanup(owner, || cutoff).is_err());
    assert!(
        !session.has_capacity(),
        "failed drain must retain physical custody"
    );
    assert!(!session.fully_drained());
    let retirement_cutoff = started + Duration::from_secs(5);
    session.begin_cleanup(owner, retirement_cutoff).unwrap();
    assert!(session.check_cleanup(owner, || retirement_cutoff).is_err());
    // Genuine callback + invisibility is represented by complete, not by the timer.
    session.expire(owner).unwrap();
    session
        .complete(owner.attempt, owner.nonce, DialogUiOutcome::Dismissed)
        .unwrap();
    assert!(session.fully_drained());
    let next = original(2, 2);
    session.reserve(next, options(), WebviewId(3)).unwrap();
    assert!(
        session
            .check_cleanup(next, || cutoff + Duration::from_secs(100))
            .is_ok(),
        "old timer cannot poison healthy next dialog"
    );
}

#[test]
fn cancel_and_retirement_start_their_owned_cleanup_clocks() {
    use std::time::{Duration, Instant};
    let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let owner = original(1, 1);
    session.reserve(owner, options(), WebviewId(3)).unwrap();
    let before = Instant::now();
    session
        .cancel(owner.identity, owner.attempt, owner.nonce, CorrelationId(2))
        .unwrap();
    let cancel_deadline = session.state.lock().unwrap().cleanup_deadline.unwrap();
    assert!(cancel_deadline >= before + Duration::from_secs(10));
    assert!(cancel_deadline <= Instant::now() + Duration::from_secs(10));
    session
        .cancel(owner.identity, owner.attempt, owner.nonce, CorrelationId(3))
        .unwrap();
    assert_eq!(
        session.state.lock().unwrap().cleanup_deadline,
        Some(cancel_deadline)
    );
    let before_retire = Instant::now();
    session.retire(owner.identity, owner.attempt);
    let retire_deadline = session.state.lock().unwrap().cleanup_deadline.unwrap();
    assert!(retire_deadline <= cancel_deadline);
    assert!(retire_deadline >= (before_retire + Duration::from_secs(5)).min(cancel_deadline));
    assert!(retire_deadline <= Instant::now() + Duration::from_secs(5));
    session.retire(owner.identity, owner.attempt);
    assert_eq!(
        session.state.lock().unwrap().cleanup_deadline,
        Some(retire_deadline)
    );
}

#[test]
fn late_cleanup_poll_does_not_consume_newly_available_drain() {
    use std::cell::Cell;
    use std::time::{Duration, Instant};
    let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let owner = original(1, 1);
    session.reserve(owner, options(), WebviewId(3)).unwrap();
    let cutoff = Instant::now() + Duration::from_secs(10);
    session.begin_cleanup(owner, cutoff).unwrap();
    for now in [cutoff, cutoff + Duration::from_secs(1)] {
        let polled = Cell::new(false);
        let result = session.poll_before_cleanup_deadline(
            owner,
            || now,
            || {
                polled.set(true);
                Ok(Some(DialogUiOutcome::Dismissed))
            },
        );
        assert!(result.is_err());
        assert!(
            !polled.get(),
            "expired owner must retain its native proof and handles"
        );
        assert!(!session.has_capacity());
    }
    let polled = Cell::new(false);
    let result = session.poll_before_cleanup_deadline(
        owner,
        || {
            cutoff
                .checked_sub(Duration::from_nanos(1))
                .expect("fixture cleanup instant before cutoff")
        },
        || {
            polled.set(true);
            Ok(Some(DialogUiOutcome::Dismissed))
        },
    );
    assert!(result.unwrap().is_some());
    assert!(polled.get());
}

#[test]
fn poll_crossing_cleanup_cutoff_cannot_release_owner() {
    use std::cell::Cell;
    use std::time::{Duration, Instant};
    let (session, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let owner = original(1, 1);
    session.reserve(owner, options(), WebviewId(3)).unwrap();
    let cutoff = Instant::now() + Duration::from_secs(10);
    session.begin_cleanup(owner, cutoff).unwrap();
    let observed = Cell::new(false);
    let result = session.poll_before_cleanup_deadline(
        owner,
        || {
            if observed.get() {
                cutoff
            } else {
                cutoff
                    .checked_sub(Duration::from_nanos(1))
                    .expect("fixture cleanup instant before cutoff")
            }
        },
        || {
            observed.set(true);
            Ok(Some(DialogUiOutcome::Dismissed))
        },
    );
    assert!(observed.get());
    assert!(
        result.is_err(),
        "proof first returned at cutoff cannot release custody"
    );
    assert!(!session.has_capacity());
    assert!(!session.fully_drained());
}

#[cfg(target_os = "macos")]
mod transport;
