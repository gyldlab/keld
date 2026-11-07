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

// Exercise the actual primary reader and constructor-owned native sessions.
// Removing either the channel registration or its reader dispatch arm must fail
// the correlated Guard ERR; direct handler tests cannot prove that connection.
// The paired-stream fixture proves routing, not Runtime admission or native UI.
fn native_guard_denial_through_primary_reader(channel: keld_ipc::ChannelId) {
    use crate::app_session as app;
    use keld_ipc::codec::{decode, encode};
    use keld_ipc::echo::{EchoRequest, EchoResponse};
    use keld_ipc::frame::FrameKind;
    use keld_ipc::link::{AppLinkDeadlines as _, read_frame, write_frame};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let (source, _source_wake, _temp) = with_manifest(r#"{"app":{"system":[]}}"#);
    let snapshot = app::GuardSnapshot::prepare(source.verified.clone())
        .expect("prepare the existing immutable Guard snapshot");
    let (server, mut peer) = UnixStream::pair().expect("actual primary socket pair");
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
        .expect("bounded wire observations");
    let (window_tx, window_rx) = mpsc::channel();
    let (guardian_tx, _guardian_rx) = mpsc::channel();
    let router = app::PrimaryRouter::start_with_fs_test_gate(
        server,
        window_tx,
        app::PlatformPrimaryOwnerHandle {
            command_tx: guardian_tx,
        },
        app::SessionShutdownState::new(),
        Some(&snapshot),
        None,
    )
    .expect("start the actual guarded primary reader");

    let observed = (|| {
        // Guard must refuse before either native decoder sees this malformed
        // body. A fabricated success or generic decoder error is not the oracle.
        write_frame(
            &mut peer,
            FrameKind::Call,
            0,
            channel,
            CorrelationId(47),
            &[255],
        )?;
        let denied = read_frame(&mut peer)?;
        let echo = encode(&EchoRequest {
            message: "healthy after native Guard denial".to_owned(),
            count: 9,
        })?;
        write_frame(
            &mut peer,
            FrameKind::Call,
            0,
            keld_ipc::ECHO_CHANNEL,
            CorrelationId(48),
            &echo,
        )?;
        Ok::<_, keld_ipc::IpcError>((denied, read_frame(&mut peer)?))
    })();
    // Join the real reader/FS worker before asserting, including for a missing
    // route mutation which causes a bounded read failure instead of a reply.
    let cleanup = router.shutdown();
    let ((denied_header, denied_body), (echo_header, echo_body)) =
        observed.expect("native CALL must reach its Guard and preserve the primary link");
    cleanup.expect("join the actual primary reader and filesystem worker");
    assert_eq!(
        (
            denied_header.kind,
            denied_header.channel,
            denied_header.corr
        ),
        (FrameKind::Err, channel, CorrelationId(47))
    );
    assert_eq!(
        decode::<keld_ipc::CallError>(&denied_body)
            .expect("canonical typed Guard refusal")
            .code,
        "KELD-GUARD001"
    );
    assert_eq!(
        (echo_header.kind, echo_header.channel, echo_header.corr),
        (FrameKind::Reply, keld_ipc::ECHO_CHANNEL, CorrelationId(48))
    );
    assert_eq!(
        decode::<EchoResponse>(&echo_body).expect("healthy follow-up response"),
        EchoResponse {
            message: "healthy after native Guard denial".to_owned(),
            count: 9,
        }
    );
    assert!(
        window_rx.try_recv().is_err(),
        "denial must not wake native UI"
    );
}

#[test]
fn primary_reader_routes_dialog_guard_denial_and_keeps_link_healthy() {
    native_guard_denial_through_primary_reader(keld_ipc::channel_table::DIALOG.id());
}

#[test]
fn primary_reader_routes_menu_guard_denial_and_keeps_link_healthy() {
    native_guard_denial_through_primary_reader(keld_ipc::channel_table::MENU.id());
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
    assert!(matches!(
        wake.try_recv(),
        Ok(AppWindowCommand::MacDialogWake)
    ));
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
    assert!(matches!(
        wake.try_recv(),
        Ok(AppWindowCommand::MacDialogWake)
    ));
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
