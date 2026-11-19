//! Actual Core routing and socket faults, not native `AppKit` qualification.
use super::{DialogUiOutcome, WebviewId, options, original, with_manifest};
use crate::app_session::{GuardianOwnerCommand, PrimaryCallerContext};
use keld_ipc::link::{AppLinkDeadlines as _, read_frame};
use keld_ipc::{CorrelationId, codec::encode};
use keld_native::dialog::DialogRequestV1;
use std::io::Read as _;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn reused_original_correlation_closes_without_a_competing_terminal() {
    let payloads = [
        vec![0xff],
        encode(&DialogRequestV1::Cancel { nonce: 9 }).unwrap(),
        encode(&DialogRequestV1::Cancel { nonce: 99 }).unwrap(),
        encode(&DialogRequestV1::Message {
            nonce: 10,
            parent: None,
            options: options(),
        })
        .unwrap(),
    ];
    for payload in payloads {
        let (mut handle, mut peer, owner_rx, ui_rx) =
            crate::app_session::tests::initial_ready_router_with_window_loop();
        peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
            .unwrap();
        let (dialog, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
        let dialog = Arc::new(dialog);
        handle.dialog = Some(Arc::clone(&dialog));
        let owner = original(7, 9);
        dialog.reserve(owner, options(), WebviewId(3)).unwrap();
        dialog.take_work().unwrap();
        let caller = {
            let current = handle.current.lock().unwrap();
            let active = current.as_ref().unwrap();
            PrimaryCallerContext {
                identity: active.identity,
                attempt: active.attempt,
                reader_stop: Arc::clone(&active.reader_stop),
            }
        };
        let observer = handle.clone();
        let acknowledge = std::thread::spawn(move || {
            let GuardianOwnerCommand::FailGeneration(attempt, reply) =
                owner_rx.recv_timeout(Duration::from_secs(3)).unwrap()
            else {
                panic!("wrong owner command")
            };
            assert_eq!(attempt, 1);
            assert!(observer.current.try_lock().unwrap().is_none());
            drop(observer.shutdown.transition.try_lock().unwrap());
            reply.send(Ok(())).unwrap();
        });
        let error = handle
            .dispatch_dialog(&caller, CorrelationId(7), &payload)
            .unwrap_err();
        acknowledge.join().unwrap();
        assert!(error.to_string().contains("KELD-IPC-005"), "{error}");
        let mut byte = [0_u8; 1];
        match peer.read(&mut byte) {
            Ok(0) => {}
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
            other => panic!("expected EOF/reset without any correlated frame, got {other:?}"),
        }
        assert!(
            ui_rx.try_recv().is_err(),
            "accepted recovery is not HostFatal"
        );
        assert!(
            !dialog.fully_drained(),
            "retirement is not native quiescence"
        );
        handle
            .dialog_native_finished(1, 9, DialogUiOutcome::Dismissed)
            .unwrap();
        assert!(
            dialog.fully_drained(),
            "late callback still drains retired custody"
        );
    }
}

#[test]
fn terminal_writer_loss_retires_before_unlocked_owner_notification() {
    for owner_succeeds in [true, false] {
        let (mut handle, _peer, owner_rx, ui_rx) =
            crate::app_session::tests::initial_ready_router_with_window_loop();
        let (dialog, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
        let dialog = Arc::new(dialog);
        handle.dialog = Some(Arc::clone(&dialog));
        let mut owner = original(7, 9);
        owner.parent = None; // Isolate transport from parent-registry acceptance.
        dialog.reserve(owner, options(), WebviewId(3)).unwrap();
        dialog.take_work().unwrap();
        let outcome = DialogUiOutcome::Selected {
            response: 0,
            checkbox_checked: false,
        };
        dialog.record_native_outcome(owner, outcome, None).unwrap();
        handle
            .current
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .writer
            .shutdown_app_link()
            .unwrap();
        let observer = handle.clone();
        let acknowledge = std::thread::spawn(move || {
            let GuardianOwnerCommand::FailGeneration(attempt, reply) =
                owner_rx.recv_timeout(Duration::from_secs(3)).unwrap()
            else {
                panic!("wrong owner command")
            };
            assert_eq!(attempt, 1);
            assert!(observer.current.try_lock().unwrap().is_none());
            drop(observer.shutdown.transition.try_lock().unwrap());
            reply
                .send(if owner_succeeds {
                    Ok(())
                } else {
                    Err("injected owner failure".to_owned())
                })
                .unwrap();
        });
        let result = handle.dialog_native_finished(1, 9, outcome);
        acknowledge.join().unwrap();
        if owner_succeeds {
            result.unwrap();
        } else {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("dialog correlated reply"), "{error}");
            assert!(error.contains("injected owner failure"), "{error}");
        }
        assert!(ui_rx.try_recv().is_err());
        assert!(dialog.fully_drained());
    }
}

#[test]
fn fresh_cancel_keeps_its_ack_separate_from_the_original_terminal() {
    let (mut handle, mut peer, owner_rx, _ui_rx) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let (dialog, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let dialog = Arc::new(dialog);
    handle.dialog = Some(Arc::clone(&dialog));
    let mut owner = original(7, 9);
    owner.parent = None;
    dialog.reserve(owner, options(), WebviewId(3)).unwrap();
    dialog.take_work().unwrap();
    let caller = {
        let current = handle.current.lock().unwrap();
        let active = current.as_ref().unwrap();
        PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    peer.set_app_link_deadlines(Some(Duration::from_secs(3)))
        .unwrap();
    handle
        .dispatch_dialog(
            &caller,
            CorrelationId(8),
            &encode(&DialogRequestV1::Cancel { nonce: 9 }).unwrap(),
        )
        .unwrap();
    let (header, payload) = read_frame(&mut peer).unwrap();
    assert_eq!(
        (header.kind, header.corr),
        (keld_ipc::FrameKind::Reply, CorrelationId(8))
    );
    assert_eq!(
        keld_ipc::codec::decode::<keld_native::dialog::DialogReplyV1>(&payload).unwrap(),
        keld_native::dialog::DialogReplyV1::Cancel {
            disposition: keld_native::dialog::CancelDisposition::Requested
        }
    );
    dialog
        .record_native_outcome(owner, DialogUiOutcome::Dismissed, None)
        .unwrap();
    handle
        .dialog_native_finished(1, 9, DialogUiOutcome::Dismissed)
        .unwrap();
    let (header, _) = read_frame(&mut peer).unwrap();
    assert_eq!(
        (header.kind, header.corr),
        (keld_ipc::FrameKind::Err, CorrelationId(7))
    );
    assert!(dialog.fully_drained());
    assert!(handle.current.lock().unwrap().is_some());
    assert!(owner_rx.try_recv().is_err());
}

#[test]
fn cli_lease_loss_retires_pending_dialog_custody() {
    let (mut handle, _peer, owner_rx, _ui_rx) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let (dialog, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let dialog = Arc::new(dialog);
    handle.dialog = Some(Arc::clone(&dialog));
    let owner = original(7, 9);
    dialog.reserve(owner, options(), WebviewId(3)).unwrap();
    dialog.take_work().unwrap();
    assert!(!dialog.fully_drained());
    assert!(handle.shutdown.reserve_cli_lease_lost());
    let acknowledge = std::thread::spawn(move || {
        for _ in 0..2 {
            let command = owner_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("guardian owner command");
            let (GuardianOwnerCommand::PrepareAcceptedShutdown(reply)
            | GuardianOwnerCommand::Shutdown(reply)) = command
            else {
                panic!("unexpected guardian owner command");
            };
            reply.send(Ok(())).expect("guardian owner receipt");
        }
    });
    handle.cli_lease_lost().expect("forced CLI lease tail");
    acknowledge.join().unwrap();
    // Retirement tombstones the acknowledged original; only a genuine native
    // callback and physical invisibility may release the retained owner.
    assert_eq!(
        dialog.pending_original_for(owner.identity, owner.attempt),
        None
    );
    assert!(!dialog.fully_drained());
    handle
        .dialog_native_finished(1, 9, DialogUiOutcome::Dismissed)
        .unwrap();
    assert!(dialog.fully_drained());
}

#[test]
fn lifecycle_quit_retires_pending_dialog_custody() {
    let (mut handle, _peer, owner_rx, _ui_rx) =
        crate::app_session::tests::initial_ready_router_with_window_loop();
    let (dialog, _wake, _temp) = with_manifest(r#"{"app":{"system":["dialog"]}}"#);
    let dialog = Arc::new(dialog);
    handle.dialog = Some(Arc::clone(&dialog));
    let owner = original(7, 9);
    dialog.reserve(owner, options(), WebviewId(3)).unwrap();
    dialog.take_work().unwrap();
    assert!(!dialog.fully_drained());
    let acknowledge = std::thread::spawn(move || {
        let GuardianOwnerCommand::Shutdown(reply) = owner_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("guardian shutdown command")
        else {
            panic!("unexpected guardian owner command");
        };
        reply.send(Ok(())).expect("guardian shutdown receipt");
    });
    handle
        .finish_quit_link_and_tail(Ok(()))
        .expect("lifecycle Quit tail");
    acknowledge.join().unwrap();
    assert_eq!(
        dialog.pending_original_for(owner.identity, owner.attempt),
        None
    );
    assert!(!dialog.fully_drained());
    handle
        .dialog_native_finished(1, 9, DialogUiOutcome::Dismissed)
        .unwrap();
    assert!(dialog.fully_drained());
}

mod failure_ordering;
