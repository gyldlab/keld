use super::{
    AppWindowCommand, CancelDisposition, CorrelationId, DialogFault, DialogOwner, DialogReplyV1,
    DialogSession, MacDialogUiOutcome, MessageOptions, PrimaryLinkIdentity, WebviewId,
};
use keld_ipc::window::{WindowGeneration, WindowId, WindowRef};
use sha2::{Digest as _, Sha256};
use std::sync::mpsc;

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
    assert_eq!(wake.try_recv(), Ok(AppWindowCommand::MacDialogWake));
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
    assert_eq!(wake.try_recv(), Ok(AppWindowCommand::MacDialogWake));
    assert!(session.take_work().expect("bounded pending work").is_some());
    assert!(!session.may_present(first).expect("tombstone"));
    session
        .record_native_outcome(first, MacDialogUiOutcome::Dismissed, None)
        .expect("OS callback observed after cancellation");
    let terminal = session
        .complete(1, 9, MacDialogUiOutcome::Dismissed)
        .expect("OS callback modeled as already physically drained");
    assert!(terminal == Some((first, Err(DialogFault::Cancelled))));
    assert!(session.fully_drained());
    assert!(session.has_capacity());
    assert!(
        session
            .complete(
                1,
                9,
                MacDialogUiOutcome::Selected {
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
            MacDialogUiOutcome::Selected {
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
            MacDialogUiOutcome::Selected {
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
            .complete(1, 17, MacDialogUiOutcome::Dismissed)
            .unwrap()
            .is_none()
    );
}

#[test]
fn appkit_observed_native_click_is_terminal_before_event_coordinator_receives_it() {
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
            MacDialogUiOutcome::Selected {
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
            MacDialogUiOutcome::Selected {
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
